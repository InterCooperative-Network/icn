#!/usr/bin/env python3
# ============================================================================
# icn-demo-session — DEV/DEMO-only local session endpoint for the member-shell.
# ----------------------------------------------------------------------------
# DEMO PROFILE ONLY. Installed and enabled by build-image.sh when
# ICN_APPLIANCE_DEMO_PROFILE=1. Runs as a small loopback HTTP service inside
# the appliance VM (icn-demo-session.service), 127.0.0.1:8091.
#
# Why this exists: the member-shell's live mode needs a gateway address and an
# access credential. Typing the gateway and pasting a JWT by hand is an
# unacceptable human-demo step. This endpoint lets the shell's "Start local
# demo" button obtain a fresh DEV/DEMO session with one click — no copy/paste,
# no JWT in any URL.
#
# What it does: on `POST /v1/dev/demo/session` it reads a CLOSED role intent
# from the JSON body (`{"role":"organizer"|"member"}`, default member), maps it
# to one of two FIXED `icn-demo-seed --session <role> --json` commands (the same
# safe, fictional, dev-gated seed an operator could run by hand), and returns its
# JSON: a short-lived least-privilege per-role session credential, the domain/did,
# and the standing note. The role only SELECTS a constant command — no request
# bytes are ever interpolated into it. The shell holds the credential in page
# memory only (never persisted, never in a URL). A role transition (organizer ->
# member) calls back here for a FRESH least-privilege session — it never upgrades
# a credential. This service adds no privilege the tunnel-holding operator does not
# already have (they have SSH + sudo on this throwaway VM).
#
# Safety:
#   * Binds 127.0.0.1 ONLY (loopback) — reachable solely through the operator's
#     own SSH tunnel, never on the VLAN.
#   * Double dev-gated: refuses unless ICN_ENABLE_ADMIN_ENDPOINTS=true AND the
#     governance posture is non-Production (the same gates icn-demo-seed and
#     the gateway's dev bootstrap require). Returns 403 otherwise.
#   * Fixed command — no request data influences what is run; no injection.
#   * CORS accepts only http loopback origins (localhost / 127.0.0.1, any
#     port — the demo shell, however the operator tunnels it). Every
#     non-loopback origin is refused, server-side, before any side effect.
#   * NEVER logs the credential. Logs are redacted by construction.
#
# This is NOT a production auth surface. It exists only on demo-profile images.
# ============================================================================
import json
import os
import re
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def log(msg):
    # stdout -> journald via the unit. Never include the credential here.
    # Defined up front so module-level origin validation below can warn.
    print("[demo-session] %s" % msg, flush=True)


# Serialize seeding: ThreadingHTTPServer can dispatch concurrent POSTs, and
# overlapping icn-demo-seed runs would race on node state. Only one seed runs
# at a time; a second request blocks until the first releases.
_SEED_LOCK = threading.Lock()

BIND_HOST = "127.0.0.1"
BIND_PORT = int(os.environ.get("ICN_DEMO_SESSION_PORT", "8091"))
# Two FIXED commands, one per role. The request selects one from this CLOSED set
# (see resolve_seed_cmd); no request bytes are ever interpolated into a command,
# so there is no injection surface. Each --session role mints a least-privilege
# per-role browser credential (organizer = read+review+confirm; member = read+complete)
# for the Rehearsal Node loop (#2386). The role transition in the browser calls
# back here for a FRESH least-privilege session — it never upgrades a credential.
SEED_CMD_ORGANIZER = ["/usr/local/sbin/icn-demo-seed", "--session", "organizer", "--json"]
SEED_CMD_MEMBER = ["/usr/local/sbin/icn-demo-seed", "--session", "member", "--json"]
# "Start a NEW rehearsal": same fixed organizer seed plus --fresh, which makes
# the seed reset the workspace (an organizer act: prior un-completed fictional
# items are retired, recorded receipts remain permanent process facts) before
# minting the organizer session. Still a constant list — the flag only SELECTS
# this third command.
SEED_CMD_ORGANIZER_FRESH = [
    "/usr/local/sbin/icn-demo-seed", "--session", "organizer", "--fresh", "--json",
]
# Read-only sanitized status summary for the landing page (GET
# /v1/dev/demo/status): gateway health + workspace/receipt COUNTS only. The
# script mints an internal governance:read credential and never returns it.
STATUS_CMD = ["/usr/local/sbin/icn-demo-status", "--json"]


def resolve_seed_cmd(body_bytes):
    """Map a CLOSED role intent (plus the organizer-only `fresh` flag) to one
    of the FIXED commands above. Returns (cmd, role) on success, or
    (None, error_message) on a bad body/role. The role only SELECTS a constant
    list — it is never spliced into a command."""
    role = "member"  # default: the member session
    fresh = False
    if body_bytes:
        try:
            body = json.loads(body_bytes.decode("utf-8"))
            role = body.get("role") or "member"
            fresh = body.get("fresh") is True
        except Exception:  # noqa: BLE001
            return (None, "request body is not valid JSON")
    if role == "organizer":
        return (SEED_CMD_ORGANIZER_FRESH if fresh else SEED_CMD_ORGANIZER, "organizer")
    if role == "member":
        if fresh:
            # Reset is an organizer act; a member session must not trigger it.
            return (None, "fresh requires the organizer role")
        return (SEED_CMD_MEMBER, "member")
    return (None, "unknown role (allowed: organizer, member)")

# The demo shell is served at 127.0.0.1:8090 in the VM and reached over the
# operator's tunnel at localhost:18090 (a FIXED host port — open-proxmox-demo.sh
# offers no shell-port override). This endpoint mints a DEV/DEMO JWT, so it must
# only answer the demo shell's own origins — NOT any loopback page (e.g. an
# unrelated http://localhost:3000 dev server, which could otherwise obtain a
# JWT). The allow-list below is therefore the exact, fixed set, identical to the
# gateway's ICN_CORS_ORIGINS. Non-listed origins are refused.
#
# LAN single-origin posture (appliance LAN profile): this endpoint STAYS bound
# to loopback; an in-VM reverse proxy serves the shell and forwards
# /v1/dev/demo/session here, so the browser's Origin header carries the
# deployment's public origin. That exact origin is added — at build/deploy
# time, by the operator, via ICN_DEMO_SESSION_EXTRA_ORIGINS (comma-separated,
# exact `scheme://host[:port]` values; no wildcards) — never from request
# bytes. An empty/unset variable changes nothing.
#
# Each configured value is validated to a strict origin shape before it can
# join the allowlist. safe_origin() reflects an allowlist member verbatim into
# Access-Control-Allow-Origin, so a value containing whitespace, control
# characters, or CR/LF could otherwise become a header-injection / response-
# splitting vector. A malformed entry is dropped with a startup warning rather
# than trusted.
_ORIGIN_RE = re.compile(r"^https?://[A-Za-z0-9.-]+(:[0-9]{1,5})?$")


def _valid_configured_origins(raw):
    out = []
    for o in raw.split(","):
        o = o.strip()
        if not o:
            continue
        if _ORIGIN_RE.match(o):
            out.append(o)
        else:
            log("ignoring malformed ICN_DEMO_SESSION_EXTRA_ORIGINS entry: %r" % o[:80])
    return tuple(out)


_EXTRA_ORIGINS = _valid_configured_origins(
    os.environ.get("ICN_DEMO_SESSION_EXTRA_ORIGINS", "")
)
ALLOWED_ORIGINS = (
    "http://localhost:8090", "http://127.0.0.1:8090",
    "http://localhost:18090", "http://127.0.0.1:18090",
) + _EXTRA_ORIGINS


def safe_origin(origin):
    """Return the matching allow-list member (an exact constant) if `origin` is a
    demo shell origin, else None. Echoing only a hard-coded constant keeps the
    response header free of any request bytes (no HTTP response-splitting;
    CodeQL-clean), and the set matches the gateway's CORS allow-list exactly."""
    for allowed in ALLOWED_ORIGINS:
        if origin == allowed:
            return allowed
    return None


def demo_gated():
    """True only when both dev gates are on (mirrors icn-demo-seed / gateway)."""
    admin = os.environ.get("ICN_ENABLE_ADMIN_ENDPOINTS", "false").lower() == "true"
    production = os.environ.get("ICN_GOVERNANCE_BUILD_MODE", "").lower() == "production"
    return admin and not production


class Handler(BaseHTTPRequestHandler):
    server_version = "icn-demo-session/0"

    def _cors(self, origin):
        # Echo only an exact allowlist member (a stored constant), never the raw
        # request value — so a tainted Origin header can never be reflected into
        # a response (avoids HTTP response-splitting). Non-listed origins get no
        # CORS headers. GET is advertised alongside POST because the status
        # route (GET /v1/dev/demo/status) is served from this same handler.
        safe = safe_origin(origin)
        if safe is not None:
            self.send_header("Access-Control-Allow-Origin", safe)
            self.send_header("Vary", "Origin")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors(self.headers.get("Origin", ""))
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        # Read-only sanitized status for the landing page. No credential is
        # minted INTO the response (icn-demo-status returns counts only), so
        # unlike the POST path this does not demand an Origin header — a
        # same-origin GET may legitimately omit one. The dev gates still apply.
        origin = self.headers.get("Origin", "")
        if self.path.rstrip("/") != "/v1/dev/demo/status":
            return self._json(404, {"error": "not found"}, origin)
        if not demo_gated():
            return self._json(403, {"error": "demo status disabled (not a DEV/DEMO posture)"}, origin)
        try:
            out = subprocess.run(STATUS_CMD, capture_output=True, text=True, timeout=60)
        except Exception as e:  # noqa: BLE001
            log("status spawn failed: %s" % e)
            return self._json(500, {"error": "status failed to start"}, origin)
        if out.returncode != 0:
            log("status exit %d: %s" % (out.returncode, (out.stderr or "").strip()[:200]))
            return self._json(500, {"error": "status failed"}, origin)
        try:
            summary = json.loads(out.stdout)
        except Exception:  # noqa: BLE001
            log("status produced non-JSON output")
            return self._json(500, {"error": "status output not JSON"}, origin)
        return self._json(200, summary, origin)

    def do_POST(self):
        origin = self.headers.get("Origin", "")
        # Read and bound any request body up front so the socket is drained. The
        # body may carry a role intent; it is validated against a CLOSED set
        # (resolve_seed_cmd) and never interpolated into a command.
        try:
            length = int(self.headers.get("Content-Length", "0") or "0")
        except ValueError:
            length = 0
        body_bytes = self.rfile.read(min(length, 4096)) if length > 0 else b""
        if self.path.rstrip("/") != "/v1/dev/demo/session":
            return self._json(404, {"error": "not found"}, origin)
        # SERVER-SIDE origin check, before any side effect. CORS response
        # headers only control whether the BROWSER reveals the response; they
        # do not stop a cross-site page from reaching this loopback port and
        # triggering a reseed. Reject any non-loopback (or absent) Origin up
        # front so only a local demo shell can cause a reseed.
        if safe_origin(origin) is None:
            log("refused: origin not an allowed loopback origin")
            return self._json(403, {"error": "origin not allowed"}, origin)
        if not demo_gated():
            log("refused: dev gates off (ICN_ENABLE_ADMIN_ENDPOINTS / non-production)")
            return self._json(403, {"error": "demo session disabled (not a DEV/DEMO posture)"}, origin)
        cmd, role = resolve_seed_cmd(body_bytes)
        if cmd is None:
            log("refused: %s" % role)  # `role` holds the error string here
            return self._json(400, {"error": role}, origin)
        # Serialize: never run two seeds at once (see _SEED_LOCK).
        with _SEED_LOCK:
            try:
                out = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
            except Exception as e:  # noqa: BLE001 - report any spawn failure plainly
                log("seed spawn failed: %s" % e)
                return self._json(500, {"error": "seed failed to start"}, origin)
            if out.returncode != 0:
                # stderr may name the failure; it does not contain the credential.
                log("seed exit %d (%s): %s" % (out.returncode, role, (out.stderr or "").strip()[:200]))
                return self._json(500, {"error": "seed failed"}, origin)
            try:
                session = json.loads(out.stdout)
            except Exception:  # noqa: BLE001
                log("seed produced non-JSON output")
                return self._json(500, {"error": "seed output not JSON"}, origin)
            note = session.get("standing_note", "")
            if note != "bootstrap-standing: ok":
                log("seed standing degraded: %s" % note)
                return self._json(500, {"error": "standing bootstrap degraded", "standing_note": note}, origin)
            log("%s session seeded ok — credential returned to caller, not logged" % role)
            # Return the seed JSON verbatim (it carries the short-lived
            # credential under "jwt"); the shell keeps it in page memory only.
            return self._json(200, session, origin)

    def _json(self, code, obj, origin):
        body = json.dumps(obj).encode("utf-8")
        self.send_response(code)
        self._cors(origin)
        self.send_header("Content-Type", "application/json")
        # The body may carry the short-lived credential — never let any cache
        # (browser or intermediary) retain it.
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):  # silence default access log (could echo paths)
        return


def main():
    if not demo_gated():
        # Start anyway (so the unit stays up) but it will 403; log the posture.
        log("WARNING: starting without DEV/DEMO posture — endpoint will 403 until "
            "ICN_ENABLE_ADMIN_ENDPOINTS=true and non-production governance mode")
    httpd = ThreadingHTTPServer((BIND_HOST, BIND_PORT), Handler)
    log("listening on http://%s:%d  (POST /v1/dev/demo/session, loopback only)" % (BIND_HOST, BIND_PORT))
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()


if __name__ == "__main__":
    sys.exit(main())
