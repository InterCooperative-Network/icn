#!/usr/bin/env python3
"""check-agent-runtime-adoption.py — the lifecycle contract must not silently disappear.

A runtime that only one manually-launched session uses is not integrated. The lifecycle hooks
live in .claude/settings.json, which is edited by hand and by tooling, so nothing stops a
future change from dropping an event and quietly turning lifecycle tracking off for every
session. This checker makes that a build failure instead of a mystery.

It also reports launcher COVERAGE honestly, including the launchers that cannot support the
contract today. An unsupported launcher listed explicitly is a known gap; an unsupported
launcher nobody wrote down is a false claim of integration.

Usage:
    python3 scripts/check-agent-runtime-adoption.py [--repo-root PATH] [--verbose]

Exit 0 = the contract is wired. Exit 1 = it is not. Refs icn#2653.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import sys
from pathlib import Path

HOOK = ".claude/hooks/session-lifecycle.sh"

# Every event the lifecycle depends on, and what breaks if it goes missing. The message is the
# point: a bare "missing hook" tells a future maintainer nothing about the consequence.
REQUIRED_EVENTS = {
    "SessionStart": "sessions would never register; the whole registry goes dark",
    "PostToolUse": "progress would never advance; every live lane would look stalled",
    "Stop": "interaction/liveness would never be reported between tool calls",
    "SessionEnd": "sessions would never release; every lane would leak until TTL expiry",
}

REQUIRED_EXECUTABLES = [
    "ops/scripts/icn-agent-session",
    "ops/scripts/icn-wait",
    HOOK,
]

# Provider adapters that must keep pointing at the ops MCP server. They do not get hooks (see
# COVERAGE below), so the MCP surface is the only capability route they have.
PROVIDER_MCP_CONFIGS = [
    (".mcp.json", ["mcpServers", "icn-ops"]),
    (".cursor/mcp.json", ["mcpServers", "icn-ops"]),
]

COVERAGE = {
    "supported": {
        "claude-code (icn-start)": "project .claude/settings.json hooks; auto-registers",
        "claude-code (icn-claude, ssh)": "same hooks; lands in the mcp-host worktree",
        "claude-code (remote/ccd-cli)": "same hooks (--setting-sources includes project)",
        "any Claude Code session opened in the repo": "project settings are inherited",
    },
    "partial": {
        ".codex / .cursor / .opencode adapters":
            "reach the ops MCP (so icn_ops_agent_runtime and the session tools work), but do "
            "not execute Claude Code hooks — they must call register_session explicitly",
    },
    "unsupported": {
        "Claude Code subagents (Agent tool)":
            "run in-process: no SessionStart event, no separate pid, no MCP client of their "
            "own, so they cannot be auto-registered. Provider limitation, not an omission.",
        "CI agents (.github/workflows)":
            "ephemeral, no worktree, no persistent registry — deliberately out of scope.",
    },
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", default=None)
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    root = Path(args.repo_root) if args.repo_root else Path(
        subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                       text=True, check=True).stdout.strip()
    )

    failures: list[str] = []
    checked = 0

    def ok(msg: str) -> None:
        nonlocal checked
        checked += 1
        if args.verbose:
            print(f"  ok    {msg}")

    # 1. every required lifecycle event is wired to the hook
    settings_path = root / ".claude" / "settings.json"
    try:
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        failures.append(f".claude/settings.json unreadable: {exc}")
        settings = {}

    hooks = settings.get("hooks") or {}
    for event, consequence in REQUIRED_EVENTS.items():
        commands = [
            h.get("command", "")
            for entry in (hooks.get(event) or [])
            for h in (entry.get("hooks") or [])
        ]
        if any(HOOK in c for c in commands):
            ok(f"{event} -> {HOOK}")
        else:
            failures.append(
                f"{event} does not invoke {HOOK} — {consequence}"
            )

    # 2. the things the hooks call actually exist and can be executed
    for rel in REQUIRED_EXECUTABLES:
        path = root / rel
        if not path.is_file():
            failures.append(f"missing: {rel}")
        elif not os.access(path, os.X_OK):
            failures.append(f"not executable: {rel} (chmod +x)")
        else:
            ok(f"executable: {rel}")

    # 3. provider adapters still reach the ops MCP
    for rel, keypath in PROVIDER_MCP_CONFIGS:
        path = root / rel
        if not path.is_file():
            continue  # optional adapter
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except ValueError as exc:
            failures.append(f"{rel} is not valid JSON: {exc}")
            continue
        node = data
        for key in keypath:
            node = node.get(key) if isinstance(node, dict) else None
        if node is None:
            failures.append(
                f"{rel} no longer declares {'.'.join(keypath)} — that provider would lose the "
                "ops MCP surface, which is its ONLY capability route (it gets no hooks)"
            )
        else:
            ok(f"{rel} declares {'.'.join(keypath)}")

    # 4. the hook must be resilient: it may never hard-fail a session.
    #
    # This used to grep for the literal string "exit 0" and then PRINT "exits 0 on every path".
    # A hook whose only `exit 0` was unreachable passed while hard-failing every session. Run
    # it instead, with the payload shapes that actually reach it, and check the exit status.
    hook_path = root / HOOK
    if hook_path.is_file() and os.access(hook_path, os.X_OK):
        probes = [
            ("SessionStart", '{"hook_event_name":"SessionStart","session_id":"probe","cwd":"%s"}' % root),
            ("PostToolUse", '{"hook_event_name":"PostToolUse","session_id":"probe","cwd":"%s","tool_name":"Bash"}' % root),
            ("SessionEnd", '{"hook_event_name":"SessionEnd","session_id":"probe","cwd":"%s","reason":"clear"}' % root),
            ("malformed", "not json at all"),
            ("empty", ""),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ)
            env["ICN_OPS_DB"] = str(Path(tmp) / "probe.db")
            env["ICN_ROOT"] = tmp
            env["CLAUDE_PROJECT_DIR"] = str(root)
            for label, payload in probes:
                try:
                    r = subprocess.run(
                        ["bash", str(hook_path)], input=payload, capture_output=True,
                        text=True, env=env, timeout=30,
                    )
                except subprocess.TimeoutExpired:
                    failures.append(f"{HOOK} timed out on a {label} payload; it must never block a session")
                    continue
                if r.returncode != 0:
                    failures.append(
                        f"{HOOK} exited {r.returncode} on a {label} payload; a lifecycle hook "
                        "must never fail a session"
                    )
                else:
                    ok(f"{HOOK} exits 0 on a {label} payload")

    print(f"agent-runtime adoption: {checked} check(s) passed, {len(failures)} failure(s)")
    if args.verbose or failures:
        print("\nLauncher coverage:")
        for status, entries in COVERAGE.items():
            print(f"  {status.upper()}:")
            for name, note in entries.items():
                print(f"    - {name}: {note}")

    if failures:
        print("\nFAILURES:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
