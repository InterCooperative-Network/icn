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
# What it does: on `POST /v1/dev/demo/session` it runs the existing
# `icn-demo-seed --json` (the same safe, fictional, dev-gated seed an operator
# could run by hand) and returns its JSON: a short-lived DEV/DEMO session
# credential, the seeded item/domain/did, and the standing note. The shell
# holds the credential in page memory only (it is never persisted, never put
# in a URL). This service adds no privilege the tunnel-holding operator does
# not already have (they have SSH + sudo on this throwaway VM).
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

# Serialize seeding: ThreadingHTTPServer can dispatch concurrent POSTs, and
# overlapping icn-demo-seed runs would race on node state. Only one seed runs
# at a time; a second request blocks until the first releases.
_SEED_LOCK = threading.Lock()

BIND_HOST = "127.0.0.1"
BIND_PORT = int(os.environ.get("ICN_DEMO_SESSION_PORT", "8091"))
SEED_CMD = ["/usr/local/sbin/icn-demo-seed", "--json"]

# The demo shell is loaded over a loopback tunnel on the operator's machine.
# Its host port is configurable — open-proxmox-demo.sh documents
# ICN_DEMO_SHELL_PORT for conflict avoidance — so we cannot hard-code one port:
# accept ANY http loopback origin (localhost / 127.0.0.1, any port) and refuse
# everything else. Refusing all non-loopback origins is what stops a cross-site
# page from triggering a reseed; the specific port is not the boundary.
_LOOPBACK_ORIGIN = re.compile(r"^http://(localhost|127\.0\.0\.1):([0-9]{1,5})$")


def safe_origin(origin):
    """If `origin` is an http loopback origin, return a SAFE echo value built
    only from string literals + an int-parsed port — no request bytes reach the
    response header, so a tainted Origin can never be reflected (defeats HTTP
    response-splitting; CodeQL-clean). Returns None for any other origin."""
    m = _LOOPBACK_ORIGIN.match(origin or "")
    if not m:
        return None
    host, port = m.group(1), int(m.group(2))
    if not (1 <= port <= 65535):
        return None
    host_literal = "localhost" if host == "localhost" else "127.0.0.1"
    return "http://%s:%d" % (host_literal, port)


def demo_gated():
    """True only when both dev gates are on (mirrors icn-demo-seed / gateway)."""
    admin = os.environ.get("ICN_ENABLE_ADMIN_ENDPOINTS", "false").lower() == "true"
    production = os.environ.get("ICN_GOVERNANCE_BUILD_MODE", "").lower() == "production"
    return admin and not production


def log(msg):
    # stdout -> journald via the unit. Never include the credential here.
    print("[demo-session] %s" % msg, flush=True)


class Handler(BaseHTTPRequestHandler):
    server_version = "icn-demo-session/0"

    def _cors(self, origin):
        # Echo only a server-reconstructed loopback origin (literals + an
        # int-parsed port), never the raw request value — so a tainted Origin
        # header can never be reflected into a response (avoids HTTP
        # response-splitting). Non-loopback origins get no CORS headers.
        safe = safe_origin(origin)
        if safe is not None:
            self.send_header("Access-Control-Allow-Origin", safe)
            self.send_header("Vary", "Origin")
            self.send_header("Access-Control-Allow-Methods", "POST, OPTIONS")
            self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors(self.headers.get("Origin", ""))
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        origin = self.headers.get("Origin", "")
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
        # Serialize: never run two seeds at once (see _SEED_LOCK).
        with _SEED_LOCK:
            try:
                out = subprocess.run(SEED_CMD, capture_output=True, text=True, timeout=120)
            except Exception as e:  # noqa: BLE001 - report any spawn failure plainly
                log("seed spawn failed: %s" % e)
                return self._json(500, {"error": "seed failed to start"}, origin)
            if out.returncode != 0:
                # stderr may name the failure; it does not contain the credential.
                log("seed exit %d: %s" % (out.returncode, (out.stderr or "").strip()[:200]))
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
            log("session seeded ok (item %s) — credential returned to caller, not logged"
                % session.get("item_id", "?"))
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
