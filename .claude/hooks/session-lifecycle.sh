#!/usr/bin/env bash
# Hook: agent session lifecycle — register / progress / release.
#
# This is the narrowest seam every interactive Claude Code session passes through, so it is
# where the ICN session registry is wired in. One script handles every lifecycle event; it
# dispatches on hook_event_name so there is a single place to reason about the contract.
#
# EVENT -> SIGNAL MAPPING (Refs docs/architecture/AGENT_RUNTIME.md §4)
#   SessionStart      register + emit the standard startup context
#   PostToolUse Edit  progress:file_edit   — a file actually changed
#   PostToolUse Bash  progress:command     — a command actually completed
#   Stop              progress:turn        — an agent turn actually finished
#   UserPromptSubmit  heartbeat            — liveness only; a human interacting is not progress
#   SessionEnd        release
#
# DELIBERATELY ABSENT: a periodic heartbeat timer. A timer is exactly the mechanism that makes
# a deadlocked session look healthy forever. Because every signal here is edge-triggered by a
# real runtime event, a session blocked inside a non-terminating Bash call emits nothing at all
# — which is the correct and detectable signature, not a false green.
#
# Never blocks and never fails a session: all paths exit 0.

set -u

INPUT="$(cat 2>/dev/null || true)"

field() {
  printf '%s' "$INPUT" | python3 -c "
import json,sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
v = d
for k in sys.argv[1].split('.'):
    if not isinstance(v, dict):
        sys.exit(0)
    v = v.get(k)
print(v if isinstance(v, str) else '')
" "$1" 2>/dev/null
}

EVENT="$(field hook_event_name)"
KEY="$(field session_id)"
CWD="$(field cwd)"
TOOL="$(field tool_name)"

ROOT="${CLAUDE_PROJECT_DIR:-${CWD:-$PWD}}"
SESSION_BIN="$ROOT/ops/scripts/icn-agent-session"
[ -x "$SESSION_BIN" ] || exit 0
[ -n "$KEY" ] || exit 0

run() { "$SESSION_BIN" "$@" >/dev/null 2>>"${TMPDIR:-/tmp}/icn-agent-session.log"; }

case "$EVENT" in
  SessionStart)
    # Registration output is discarded; the startup context below is what the agent sees.
    run register --harness-key "$KEY" --cwd "${CWD:-$ROOT}" --pid "$PPID" --provider claude-code

    STATUS="$("$SESSION_BIN" status --harness-key "$KEY" 2>/dev/null || echo '{}')"
    if printf '%s' "$STATUS" | grep -q '"registered": *true'; then
      LIFECYCLE="registered (heartbeat+progress tracked; release on SessionEnd)"
    else
      LIFECYCLE="DEGRADED — NOT registered. Lifecycle tracking is NOT active for this session."
    fi

    BRANCH="$(git -C "$ROOT" branch --show-current 2>/dev/null || echo detached)"
    WT="$(basename "$(git -C "$ROOT" rev-parse --show-toplevel 2>/dev/null || echo "$ROOT")")"

    # Standard startup context. It is a ROUTER, not a briefing: it names where authoritative
    # information lives and stops. Anything longer belongs behind a tool call.
    cat <<CTX
## ICN agent runtime
worktree: ${WT} | branch: ${BRANCH} | session: ${LIFECYCLE}

Capabilities (tools, skills, hooks, helpers) are GENERATED, never hand-listed:
  - MCP: icn_ops_agent_runtime  (start here — it also reports your session identity)
  - file: docs/reference/project-index/generated/agent-capabilities.json

Authoritative sources — resolve the owner, do not guess:
  - truth domains: ops/state/truth/sources.json
  - operating contract: AGENTS.md
  - current work: a LIVE query (gh issue/pr list), never a sprint file or handoff
  - agent runtime: docs/architecture/AGENT_RUNTIME.md

Standing rules: work only in your own worktree; never merge without explicit
per-PR authorization; wait for things with 'ops/scripts/icn-wait', never an
ad-hoc 'until ! pgrep -f ...' loop (it self-matches and never exits).
Report completion per AGENTS.md §9.
CTX
    ;;

  PostToolUse)
    case "$TOOL" in
      Edit|Write|MultiEdit|NotebookEdit)
        run progress --harness-key "$KEY" --kind file_edit --activity "edit: $TOOL" ;;
      Bash)
        run progress --harness-key "$KEY" --kind command --activity "bash" ;;
      *) : ;;
    esac
    ;;

  Stop|SubagentStop)
    run progress --harness-key "$KEY" --kind turn --activity "turn complete" ;;

  UserPromptSubmit)
    run heartbeat --harness-key "$KEY" ;;

  SessionEnd)
    REASON="$(field reason)"
    run release --harness-key "$KEY" --reason "${REASON:-completed}" ;;
esac

exit 0
