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
import re
import shlex
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

# Matchers that must be covered, not merely present. Keyed by event.
REQUIRED_MATCHER_TOKENS = {
    "SessionStart": ["startup", "resume"],
    "PostToolUse": ["Bash"],
}


# Stands in for a `$` the shell would NOT expand. Printable on purpose: a NUL byte makes
# Path.resolve() raise ValueError rather than simply failing to match.
LITERAL_DOLLAR = "__ICN_LITERAL_DOLLAR__"


def _mask_unexpanded_dollars(command: str) -> str:
    """
    Neutralise every `$` a shell would NOT expand.

    A shell expands `$VAR` when it is bare or inside DOUBLE quotes, and leaves it literal inside
    SINGLE quotes or after a backslash. `shlex.split` strips quoting before we ever see it, so
    `'$CLAUDE_PROJECT_DIR'/.claude/hooks/session-lifecycle.sh` — which a shell runs as a literal
    path and fails with exit 127, lifecycle tracking entirely off — arrived here looking exactly
    like the expanded form and resolved to the real hook file. The gate reported
    "25 check(s) passed, 0 failure(s)" for a configuration that provably never runs.

    Masking first means those forms keep their literal `$`, fail to resolve, and are rejected —
    the same outcome as the nonexistent-path case this gate already catches.
    """
    out: list[str] = []
    i, n, in_single = 0, len(command), False
    while i < n:
        c = command[i]
        if c == "'":
            in_single = not in_single
            out.append(c)
        elif c == "\\" and not in_single and i + 1 < n:
            # A backslash-escaped $ is literal too. Keep both characters, masked.
            out.append(c)
            out.append(LITERAL_DOLLAR if command[i + 1] == "$" else command[i + 1])
            i += 2
            continue
        elif c == "$" and in_single:
            out.append(LITERAL_DOLLAR)
        else:
            out.append(c)
        i += 1
    return "".join(out)


def _invokes_hook(command: str, root: Path) -> bool:
    """
    The hook must be THE COMMAND — the program actually executed — not merely the last word
    on the line.

    This used to be `stripped.endswith(HOOK)`, which any string ENDING in the path satisfies.
    Rewriting all five hooks as `true "$CLAUDE_PROJECT_DIR"/.claude/hooks/session-lifecycle.sh`
    — lifecycle tracking completely off, nothing registered, nothing released — left this gate
    reporting "25 check(s) passed, 0 failure(s)". A gate whose entire purpose is to prove the
    runtime is wired must not accept a command that provably never runs it.
    """
    # Mask literal `$` BEFORE shlex removes the quoting that decides whether it expands.
    stripped = _mask_unexpanded_dollars(command.split("#", 1)[0].strip())

    # NO SHELL OPERATORS. Everything after argv0 used to be ignored, so appending ` </dev/null`
    # to each hook left the gate at 25/0 while the hook received no payload at all and answered
    # "DEGRADED — hook payload unparseable" on every single event: no register, no progress, no
    # release. A redirection, a pipe or a second command can change what runs or what it reads,
    # and none of them belong in a hook invocation.
    if re.search(r"[<>;&|`$(){}]", stripped.replace("$CLAUDE_PROJECT_DIR", "")):
        return False

    try:
        tokens = shlex.split(stripped)
    except ValueError:
        return False
    # Leading VAR=value environment assignments are legitimate and are not the program.
    idx = 0
    while idx < len(tokens) and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", tokens[idx]):
        idx += 1
    if idx >= len(tokens):
        return False
    argv0 = tokens[idx]
    # An explicit interpreter is legitimate (`bash <hook>`), but then the hook must be its
    # FIRST argument — not something appended after an unrelated program.
    if os.path.basename(argv0) in {"bash", "sh", "zsh"} and idx + 1 < len(tokens):
        argv0 = tokens[idx + 1]

    # IT MUST BE THE HOOK FILE, not a string that merely ends like it. Pointing every command
    # at `.claude/hooks/DISABLED/session-lifecycle.sh` — a path that does not exist, so the hook
    # exits 127 and lifecycle tracking is entirely off — satisfied `endswith()` and left the
    # gate reporting 25 checks passed. Resolve the path and compare it to the file the gate
    # separately execs.
    resolved = argv0.replace("$CLAUDE_PROJECT_DIR", str(root)).replace(
        "${CLAUDE_PROJECT_DIR}", str(root)
    )
    candidate = Path(resolved)
    if not candidate.is_absolute():
        candidate = root / resolved
    try:
        return candidate.resolve() == (root / HOOK).resolve() and candidate.is_file()
    except (OSError, ValueError):
        # An unresolvable path is not the hook. Fail CLOSED.
        return False


REQUIRED_EXECUTABLES = [
    "ops/scripts/icn-agent-session",
    "ops/scripts/icn-wait",
    HOOK,
]

# Provider adapters that must keep pointing at the ops MCP server. They do not get hooks (see
# COVERAGE below), so the MCP surface is the only capability route they have.
# How many checks a COMPLETE run performs. Asserted, not decorative — see the floor in main().
EXPECTED_CHECKS = 25

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
        ".cursor adapter":
            "declares the ops MCP in .cursor/mcp.json, so icn_ops_agent_runtime and the "
            "session tools work — but it does not execute Claude Code hooks, so it must call "
            "register_session explicitly",
        ".codex / .opencode adapters":
            "do NOT declare the ops MCP at all (.codex/mcp/servers.example.json is an example "
            "that omits it and points at the retired ~/projects/icn path; .opencode/opencode.json "
            "has no MCP block). They get the capability manifest as a file and nothing else.",
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
        # A SUBSTRING test passed for `true  # .claude/hooks/session-lifecycle.sh`, i.e. the
        # runtime fully off while the gate printed ok. Require the hook to be the command.
        entries = [
            (entry.get("matcher", ""), h.get("command", ""))
            for entry in (hooks.get(event) or [])
            for h in (entry.get("hooks") or [])
        ]
        invoking = [m for m, c in entries if _invokes_hook(c, root)]
        if not invoking:
            failures.append(f"{event} does not invoke {HOOK} — {consequence}")
            continue

        # MATCHERS WERE NEVER INSPECTED. Narrowing PostToolUse to `NotebookEdit`, or
        # SessionStart to `fork`, left the gate green while producing exactly the consequence
        # the gate itself prints.
        required = REQUIRED_MATCHER_TOKENS.get(event)
        if required:
            covered = any(
                all(re.search(rf"(^|\|){re.escape(tok)}($|\|)", m) for tok in required)
                for m in invoking
            )
            if not covered:
                failures.append(
                    f"{event} invokes {HOOK} but its matcher {invoking!r} does not cover "
                    f"{required} — {consequence}"
                )
                continue
        ok(f"{event} -> {HOOK} (matcher {invoking[0]!r})")

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
            # NOT "continue # optional adapter". A missing file is the MOST likely way to lose
            # a provider's only capability route, and skipping it silently dropped the check
            # while COVERAGE below went on printing that the adapter "declares the ops MCP in
            # .cursor/mcp.json" — a claim about a file that did not exist. Deleting
            # .cursor/mcp.json passed 24/0; deleting .mcp.json passed 24/0. Both now fail.
            failures.append(
                f"missing: {rel} — the gate claims this adapter reaches the ops MCP, so its "
                "absence is a coverage failure, not an optional extra"
            )
            continue
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
            ("Stop", '{"hook_event_name":"Stop","session_id":"probe","cwd":"%s"}' % root),
            ("UserPromptSubmit", '{"hook_event_name":"UserPromptSubmit","session_id":"probe","cwd":"%s"}' % root),
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
                        # The declared budget in settings.json, not an arbitrary 30s: a 12s
                        # hook passed this gate and was then killed live by the harness.
                        text=True, env=env, timeout=(10 if label == "SessionStart" else 5),
                    )
                except subprocess.TimeoutExpired:
                    failures.append(f"{HOOK} timed out on a {label} payload; it must never block a session")
                    continue
                if r.returncode != 0:
                    failures.append(
                        f"{HOOK} exited {r.returncode} on a {label} payload; a lifecycle hook "
                        "must never fail a session"
                    )
                    continue
                ok(f"{HOOK} exits 0 on a {label} payload")

                # Exit status alone proved nothing: the hook exits 0 on every path BY DESIGN,
                # so eight behavioural mutations (SessionStart printing nothing, the banner
                # suppressed, the registry call removed) all passed. Assert the OUTPUT.
                if label == "SessionStart":
                    out = r.stdout
                    if "ICN agent runtime" not in out:
                        failures.append(
                            f"{HOOK} produced no startup context on SessionStart; an agent "
                            "would never learn the runtime exists"
                        )
                    elif "DEGRADED" not in out and "session:" not in out:
                        failures.append(
                            f"{HOOK} startup context does not state the session's lifecycle "
                            "status; it must never leave that ambiguous"
                        )
                    else:
                        ok(f"{HOOK} emits startup context naming its lifecycle status")
                elif label in ("PostToolUse", "Stop", "UserPromptSubmit", "SessionEnd"):
                    # These must stay SILENT with a well-formed payload — a banner after every
                    # tool call was a real regression.
                    if r.stdout.strip():
                        failures.append(
                            f"{HOOK} printed to stdout on {label}; only SessionStart and an "
                            "unknown-event degrade may emit context"
                        )
                    else:
                        ok(f"{HOOK} stays silent on {label}")

    # 2b. THE DEGRADED PATH, probed in a degraded fixture.
    #
    # The healthy probes above cannot see the banner at all: with a working helper the hook
    # emits the normal context and the banner never fires. So suppressing the banner entirely,
    # or printing it after every tool call, both passed. Recreate the hook in a scratch tree
    # with NO helper and assert both halves of the contract.
    if hook_path.is_file():
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "fakeroot"
            (fake / ".claude" / "hooks").mkdir(parents=True)
            (fake / ".claude" / "hooks" / "session-lifecycle.sh").write_text(
                hook_path.read_text(encoding="utf-8"), encoding="utf-8"
            )
            env = dict(os.environ)
            env["CLAUDE_PROJECT_DIR"] = str(fake)   # no ops/scripts/icn-agent-session here
            env["ICN_OPS_DB"] = str(fake / "x.db")
            env["ICN_ROOT"] = str(fake)
            probes = {
                "SessionStart": ('{"hook_event_name":"SessionStart","session_id":"p","cwd":"%s"}' % fake, True),
                "PostToolUse": ('{"hook_event_name":"PostToolUse","session_id":"p","cwd":"%s","tool_name":"Bash"}' % fake, False),
                "Stop": ('{"hook_event_name":"Stop","session_id":"p","cwd":"%s"}' % fake, False),
            }
            for label, (payload, expect_banner) in probes.items():
                r = subprocess.run(
                    ["bash", str(fake / ".claude" / "hooks" / "session-lifecycle.sh")],
                    input=payload, capture_output=True, text=True, env=env, timeout=15,
                )
                got_banner = "DEGRADED" in r.stdout
                if expect_banner and not got_banner:
                    failures.append(
                        f"{HOOK} did not announce DEGRADED on {label} with the helper missing; "
                        "the runtime must never be disabled silently"
                    )
                elif not expect_banner and got_banner:
                    failures.append(
                        f"{HOOK} printed the DEGRADED banner on {label}; only SessionStart and "
                        "an unknown event may emit it, or it repeats after every tool call"
                    )
                else:
                    ok(f"{HOOK} degraded-path behaviour correct on {label}")

    # 4b. THE HOOK MUST ACTUALLY WRITE TO THE REGISTRY.
    #
    # stdout probes cannot see this: neutering the helper-invoking wrapper leaves the startup
    # context intact while progress, interaction and release silently never happen. Drive a
    # real SessionStart + PostToolUse against a scratch registry and assert the row moved.
    if hook_path.is_file() and (root / "ops" / "mcp" / "dist" / "cli" / "session.js").is_file():
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ)
            env["ICN_OPS_DB"] = str(Path(tmp) / "probe.db")
            env["ICN_ROOT"] = tmp
            env["CLAUDE_PROJECT_DIR"] = str(root)
            sid = "adoption-probe-session"
            for payload in (
                '{"hook_event_name":"SessionStart","session_id":"%s","cwd":"%s"}' % (sid, root),
                '{"hook_event_name":"PostToolUse","session_id":"%s","cwd":"%s","tool_name":"Bash"}' % (sid, root),
            ):
                subprocess.run(["bash", str(hook_path)], input=payload, capture_output=True,
                               text=True, env=env, timeout=20)
            status = subprocess.run(
                [str(root / "ops" / "scripts" / "icn-agent-session"), "status",
                 "--harness-key", sid],
                capture_output=True, text=True, env=env, timeout=20,
            )
            try:
                row = json.loads(status.stdout or "{}")
            except ValueError:
                row = {}
            if not row.get("registered"):
                failures.append(
                    f"{HOOK} did not register a session in the registry; the startup context "
                    "would claim tracking is active while nothing is recorded"
                )
            elif not row.get("progress_count"):
                failures.append(
                    f"{HOOK} registered a session but PostToolUse recorded no progress; "
                    "every lane would look stalled"
                )
            else:
                ok(f"{HOOK} writes registration and progress to the registry")


    # A CHECK THAT VANISHES MUST NOT READ AS A CHECK THAT PASSED.
    #
    # Several blocks below skip themselves when their input is absent, so the total silently
    # dropped to 24 without ops/mcp/dist (which is gitignored), 24 without .mcp.json, and 23
    # without both — every one of them exiting 0. The strongest check in the file, the registry
    # write-through, was one of the ones that disappeared. CI happens to build first today, so
    # this was latent rather than live; a floor makes it neither.
    # EXACT, not a floor. `checked < EXPECTED_CHECKS` let a spurious extra ok() report
    # "26 check(s) passed" and exit 0 — which is how a duplicated check masks a lost one.
    if checked != EXPECTED_CHECKS:
        failures.append(
            f"{checked} checks ran, expected exactly {EXPECTED_CHECKS} — a check that skipped "
            "itself is "
            "not a check that passed. Something the gate depends on is missing (an unbuilt "
            "ops/mcp/dist, a deleted adapter); fix that rather than lowering EXPECTED_CHECKS"
        )

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
