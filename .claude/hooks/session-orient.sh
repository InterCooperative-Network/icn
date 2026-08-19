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
#   - Dormancy must be DECLARED, never inferred from silence. An owner that says
#     nothing about its cadence has not said "dormant"; an owner whose cadence and
#     active_sprint contradict each other has not said anything resolvable. Both
#     resolve to "unresolved", because guessing either way states a fact the hook
#     has not established. This mirrors declares_dormancy() in
#     scripts/check-truth-spine.py, which refuses dormancy-by-omission for the
#     same reason; scripts/tests/test_sprint_state_invariants.py pins the two
#     vocabularies together.
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
  ROOT="${ROOT}" python3 - <<'PY' 2>/dev/null || echo "unresolved (sprint resolver unavailable)"
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

if not isinstance(sources, dict):
    out("unresolved (truth spine is not an object)")

domains = sources.get("domains")
domain = domains.get("sprint_state") if isinstance(domains, dict) else None
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

# Vocabulary shared with declares_dormancy() in scripts/check-truth-spine.py.
DORMANT_CADENCES = {"dormant", "inactive", "none", "paused"}
ACTIVE_CADENCES = {"active", "running"}

has_active_key = "active_sprint" in state
active = state.get("active_sprint")
status = state.get("status", "unknown")

# Dormancy is a first-class, truthful answer — but only when the owner DECLARES
# it. Absence is not a declaration, and a self-contradictory owner is not one
# either; both are reported as unresolved rather than resolved in either
# direction.
if "cadence" in state:
    cadence = str(state["cadence"]).strip().lower()
    if cadence in DORMANT_CADENCES:
        if has_active_key and active is not None:
            out(f"unresolved (owner {owner} contradicts itself: "
                f"cadence={cadence!r} but active_sprint={active!r})")
        out("none active (cadence dormant)")
    if cadence in ACTIVE_CADENCES:
        if not has_active_key or active is None:
            out(f"unresolved (owner {owner} contradicts itself: "
                f"cadence={cadence!r} but no active_sprint)")
        out(f"{active} ({status})")
    out(f"unresolved (owner {owner} declares unrecognised cadence {cadence!r})")

# No cadence key. `active_sprint: null` is the other accepted spelling of
# dormancy, but the key must be PRESENT — an absent key is silence, not null,
# and reporting silence as "dormant" is the fail-open this hook exists to avoid.
if not has_active_key:
    out(f"unresolved (owner {owner} declares neither cadence nor active_sprint)")
if active is None:
    out("none active (active_sprint: null)")
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
