#!/usr/bin/env python3
"""generate-agent-capabilities.py — derive what the agent runtime can actually offer.

WHY GENERATED
    A hand-maintained list of tools and skills is wrong the day someone adds one. The property
    this file exists to provide is: adding a capability in its CANONICAL location makes it
    discoverable, without editing any launcher, provider adapter or prompt.

WHY THE MCP SURFACE IS INTROSPECTED, NOT GREPPED
    Regexing `server.tool("...")` out of TypeScript answers "what does the source appear to
    say", which is not the invariant we care about. The invariant is "what can the runtime
    REALLY expose". So this speaks MCP to the built server over stdio and asks it — the same
    question a client asks. A tool that fails to register, is gated behind a condition, or
    exists only in an unbuilt source file is then correctly absent.

    If the server cannot be started or introspected, this FAILS rather than silently falling
    back to a regex: a manifest that quietly degrades to a guess is worse than no manifest.

SOURCES (each capability comes from the place that already owns it)
    MCP tools     live `tools/list` from ops/mcp/dist/index.js
    skills        ops/state/truth/skills.json      (the registry that already owns them)
    hooks         .claude/settings.json
    helpers       ops/scripts/* declaring `#: capability:` headers
    truth domains ops/state/truth/sources.json

Usage:
    python3 scripts/generate-agent-capabilities.py [--write] [--repo-root PATH]

Exit 0 = generated (or unchanged). Exit 1 = could not determine the real surface.
Refs icn#2653.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import threading
import sys
import tempfile
from pathlib import Path

OUTPUT = "docs/reference/project-index/generated/agent-capabilities.json"
SCHEMA = "icn.agent-capabilities.v1"


# ── MCP introspection ─────────────────────────────────────────────────────────

# Overridable so the failure-mode tests can exercise the SAME watchdog path in seconds rather
# than sitting through the production budget three times. Not a behaviour switch: the default
# is what runs in CI, and the code path is identical either way.
MCP_INTROSPECT_TIMEOUT = float(os.environ.get("ICN_CAPABILITY_MCP_TIMEOUT") or 90.0)


def introspect_mcp_tools(root: Path, timeout: float = MCP_INTROSPECT_TIMEOUT) -> list[dict]:
    """Ask the built MCP server what tools it actually exposes.

    Speaks newline-delimited JSON-RPC over stdio, which is what the MCP stdio transport uses.
    Runs against a throwaway database so introspection never touches real ops state.
    """
    entry = root / "ops" / "mcp" / "dist" / "index.js"
    if not entry.is_file():
        raise SystemExit(
            f"MCP server not built: {entry} missing.\n"
            f"Run: npm --prefix {root / 'ops' / 'mcp'} run build"
        )

    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ)
        env["ICN_OPS_DB"] = str(Path(tmp) / "introspect.db")
        env["ICN_ROOT"] = str(root)
        proc = subprocess.Popen(
            ["node", str(entry)],
            cwd=str(root),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
            text=True,
            bufsize=1,
        )
        # ENFORCE the declared timeout. It was a parameter and nothing else: read_reply()
        # blocked in a bare readline() with no deadline, so a wedged MCP server hung this
        # generator forever — and this generator runs inside the drift gate on EVERY pull
        # request. Measured: a stub server that reads stdin and never answers held the gate
        # until an external timeout(1) killed it at 100s.
        #
        # A watchdog that kills the child, rather than select() on proc.stdout, because the
        # stream is buffered text: select() can report "not ready" while a complete line
        # already sits in Python's buffer, which would produce false timeouts.
        timed_out = threading.Event()

        def _kill_on_timeout() -> None:
            timed_out.set()
            proc.kill()

        watchdog = threading.Timer(timeout, _kill_on_timeout)
        watchdog.daemon = True
        watchdog.start()
        try:
            def send(msg: dict) -> None:
                assert proc.stdin
                proc.stdin.write(json.dumps(msg) + "\n")
                proc.stdin.flush()

            def read_reply(want_id: int) -> dict:
                assert proc.stdout
                while True:
                    line = proc.stdout.readline()
                    if not line:
                        if timed_out.is_set():
                            raise SystemExit(
                                f"MCP server did not answer within {timeout:.0f}s during "
                                "introspection; refusing to hang the drift gate. Is the server "
                                "wedged, or waiting on something it cannot get?"
                            )
                        raise SystemExit("MCP server closed stdout during introspection")
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        msg = json.loads(line)
                    except json.JSONDecodeError:
                        continue  # server log noise on stdout
                    if msg.get("id") == want_id:
                        return msg

            send({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "icn-capability-generator", "version": "1"},
                },
            })
            read_reply(1)
            send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
            reply = read_reply(2)
        finally:
            watchdog.cancel()
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()

    tools = reply.get("result", {}).get("tools")
    if not isinstance(tools, list) or not tools:
        raise SystemExit(f"MCP tools/list returned no tools: {json.dumps(reply)[:400]}")

    return sorted(
        (
            {
                "name": t["name"],
                # First sentence only: the manifest is an index, not documentation.
                "summary": first_sentence(t.get("description", "")),
            }
            for t in tools
            if isinstance(t, dict) and t.get("name")
        ),
        key=lambda t: t["name"],
    )


def first_sentence(text: str, limit: int = 180) -> str:
    text = " ".join(text.split())
    m = re.search(r"(?<=[.!?])\s", text)
    if m:
        text = text[: m.start() + 1]
    return text[:limit].rstrip()


# ── other canonical sources ───────────────────────────────────────────────────

def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def collect_skills(root: Path) -> list[dict]:
    """Skills come from the registry that already owns them — never from a directory walk."""
    reg = read_json(root / "ops" / "state" / "truth" / "skills.json")
    out: list[dict] = []
    for group, entries in (reg.get("skills") or {}).items():
        for e in entries or []:
            if not isinstance(e, dict) or not e.get("name"):
                continue
            out.append({
                "name": e["name"],
                "group": group,
                "canonical_path": e.get("canonical_path"),
                "summary": first_sentence(e.get("description", "") or ""),
            })
    return sorted(out, key=lambda s: (s["group"], s["name"]))


def collect_hooks(root: Path) -> list[dict]:
    settings = read_json(root / ".claude" / "settings.json")
    out: list[dict] = []
    for event, entries in (settings.get("hooks") or {}).items():
        for entry in entries or []:
            for h in entry.get("hooks", []) or []:
                cmd = h.get("command", "")
                out.append({
                    "event": event,
                    "matcher": entry.get("matcher", ""),
                    "command": cmd.replace(str(root), "${REPO}"),
                })
    return sorted(out, key=lambda h: (h["event"], h["matcher"], h["command"]))


HEADER_RE = re.compile(r"^#:\s*(\w+)\s*:\s*(.+?)\s*$")


def collect_helpers(root: Path) -> list[dict]:
    """Repo helper scripts opt in by declaring `#: capability:` in their header.

    Opt-in rather than a directory listing: ops/scripts also holds internals that are not
    agent-facing, and a manifest that advertises everything advertises nothing.
    """
    out: list[dict] = []
    scripts_dir = root / "ops" / "scripts"
    if not scripts_dir.is_dir():
        return out
    # Editor/backup artefacts are not capabilities. A mode-preserving `.bak` in ops/scripts
    # was published and flipped the drift gate red.
    IGNORED_SUFFIXES = (".bak", ".orig", ".rej", ".tmp", ".swp", "~")
    for path in sorted(scripts_dir.iterdir()):
        if not path.is_file() or not os.access(path, os.X_OK):
            continue
        if path.name.endswith(IGNORED_SUFFIXES) or path.name.startswith("."):
            continue
        meta: dict[str, str] = {}
        try:
            with path.open(encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    if not line.startswith("#"):
                        if meta:
                            break
                        continue
                    m = HEADER_RE.match(line.rstrip("\n"))
                    if m:
                        meta[m.group(1)] = m.group(2)
        except OSError:
            continue
        if "capability" in meta:
            out.append({
                "name": path.name,
                "path": str(path.relative_to(root)),
                "capability": meta["capability"],
                "summary": meta.get("summary", ""),
                "safety": meta.get("safety", "unknown"),
            })
    return out


def collect_truth_domains(root: Path) -> list[dict]:
    src = read_json(root / "ops" / "state" / "truth" / "sources.json")
    return sorted(
        (
            {"domain": name, "owner": (spec or {}).get("owner")}
            for name, spec in (src.get("domains") or {}).items()
            if isinstance(spec, dict)
        ),
        key=lambda d: d["domain"],
    )


def build(root: Path) -> dict:
    return {
        "schema": SCHEMA,
        "description": (
            "Generated index of agent-facing capabilities. Do not edit by hand: run "
            "scripts/generate-agent-capabilities.py --write. MCP tools are read from the live "
            "server via tools/list, so this reflects what the runtime can really expose."
        ),
        "generated_by": "scripts/generate-agent-capabilities.py",
        "verified_against": {
            "mcp_tools": "live tools/list from ops/mcp/dist/index.js",
            "skills": "ops/state/truth/skills.json",
            "hooks": ".claude/settings.json",
            "helpers": "ops/scripts/* with #: capability: headers",
            "truth_domains": "ops/state/truth/sources.json",
        },
        "mcp_tools": introspect_mcp_tools(root),
        "skills": collect_skills(root),
        "hooks": collect_hooks(root),
        "helpers": collect_helpers(root),
        "truth_domains": collect_truth_domains(root),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="write the manifest to disk")
    ap.add_argument("--repo-root", default=None)
    args = ap.parse_args()

    root = Path(args.repo_root) if args.repo_root else Path(
        subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                       text=True, check=True).stdout.strip()
    )

    manifest = build(root)
    rendered = json.dumps(manifest, indent=2, sort_keys=False) + "\n"
    out = root / OUTPUT

    if args.write:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(rendered, encoding="utf-8")
        print(
            f"wrote {OUTPUT}: {len(manifest['mcp_tools'])} MCP tools, "
            f"{len(manifest['skills'])} skills, {len(manifest['hooks'])} hooks, "
            f"{len(manifest['helpers'])} helpers, {len(manifest['truth_domains'])} truth domains"
        )
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    sys.exit(main())
