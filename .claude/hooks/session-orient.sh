#!/bin/bash
# Hook: SessionStart (startup) — minimal, truthful session orientation.
#
# Prints branch, sprint-cadence state, available skills, and the five invariants.
# It ORIENTS; it must not dump project state into every context.
#
# Truth discipline (Refs icn#2634):
#   - The sprint line is resolved through the REGISTERED OWNER: this script reads
#     ops/state/truth/sources.json -> domains.sprint_state.owner, then reads that
#     file. The owner path is never hardcoded here.
#   - It reports "no active sprint" plainly when the cadence is dormant. It never
#     synthesises current work from planning prose, never hardcodes a sprint
#     number, and never names a path that does not exist.
#   - "What is being worked on now" is NOT answered here: that is a live query
#     (live_issue_state / live_pr_state). This line answers only "is a sprint
#     running", which is all sprint_state owns.
#   - Every failure path degrades to a neutral, honest message. It never invents
#     an answer and never exits non-zero (a startup banner must not block a session).

set -u

ROOT="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[[ -z "${ROOT}" ]] && ROOT="$(pwd)"

BRANCH="$(git -C "${ROOT}" branch --show-current 2>/dev/null)"
[[ -z "${BRANCH}" ]] && BRANCH="detached"

# Resolve the sprint_state owner from the truth spine, then read it. All output
# on stdout; diagnostics are folded into the returned string, never a raw error.
SPRINT="$(
  ROOT="${ROOT}" python3 - <<'PY' 2>/dev/null || echo "unresolved (python3 unavailable)"
import json, os, sys

root = os.environ["ROOT"]

def out(msg):
    print(msg)
    sys.exit(0)

try:
    with open(os.path.join(root, "ops/state/truth/sources.json"), encoding="utf-8") as fh:
        sources = json.load(fh)
except (OSError, ValueError):
    out("unresolved (truth spine unreadable)")

domain = (sources.get("domains") or {}).get("sprint_state")
if not isinstance(domain, dict) or not domain.get("owner"):
    out("unresolved (no sprint_state owner registered)")

owner = str(domain["owner"]).split("#", 1)[0]
try:
    with open(os.path.join(root, owner), encoding="utf-8") as fh:
        state = json.load(fh)
except FileNotFoundError:
    out(f"unresolved (registered owner {owner} is missing)")
except (OSError, ValueError):
    out(f"unresolved (registered owner {owner} is unreadable/malformed)")

if not isinstance(state, dict):
    out(f"unresolved (registered owner {owner} is not an object)")

# Dormant is a first-class, truthful answer — not a defect to paper over.
if state.get("cadence") == "dormant" or state.get("active_sprint") is None:
    out("none active (cadence dormant)")

active = state.get("active_sprint")
status = state.get("status", "unknown")
out(f"{active} ({status})")
PY
)"

echo "ICN Session | branch: ${BRANCH} | sprint: ${SPRINT}"

SKILLS_DIR="${ROOT}/.claude/skills"
if [[ -d "${SKILLS_DIR}" ]]; then
  echo "Skills: $(ls "${SKILLS_DIR}" | tr '\n' ' ')"
fi

echo 'Invariants: adversarial-by-default | determinism | canonical-encodings | no-panics | kernel/app-boundaries'
echo 'Current work is a LIVE query (gh issue/pr list) — not a sprint file, not a handoff.'

exit 0
