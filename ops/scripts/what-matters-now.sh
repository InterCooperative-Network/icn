#!/usr/bin/env bash
# what-matters-now.sh — Live operational truth synthesis for ICN
#
# Answers: "What is true right now?" for agents and humans alike.
# All output is derived from live sources — no hardcoded state.
#
# Usage:
#   bash ops/scripts/what-matters-now.sh          # human-readable
#   bash ops/scripts/what-matters-now.sh --json   # machine-readable JSON
#   bash ops/scripts/what-matters-now.sh --drift  # drift check only (exit 1 on drift)
#
# This script is the canonical preflight for ICN agent sessions.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

MODE="${1:-human}"
DRIFT_ERRORS=0

# ─── helpers ────────────────────────────────────────────────────────────────

json_escape() { python3 -c "import sys,json; print(json.dumps(sys.stdin.read().strip()))" 2>/dev/null || cat; }

check_file() {
  local f="$1"
  if [[ ! -f "${REPO_ROOT}/${f}" ]]; then
    echo "MISSING: ${f}" >&2
    ((DRIFT_ERRORS++))
    return 1
  fi
  return 0
}

section() { echo ""; echo "── $* ──"; }

# ─── Phase 1: canonical file existence ──────────────────────────────────────

TRUTH_FILES=(
  "ops/state/truth/sources.json"
  "ops/state/truth/policy.json"
  "ops/state/truth/agents.json"
  "ops/state/truth/skills.json"
  "ops/state/config/repo-map.json"
  "ops/state/sprint/current.json"
)

TRUTH_OK=true
for f in "${TRUTH_FILES[@]}"; do
  if [[ ! -f "${REPO_ROOT}/${f}" ]]; then
    TRUTH_OK=false
    ((DRIFT_ERRORS++))
  fi
done

# ─── Phase 2: symlink verification ──────────────────────────────────────────
# PROJECT_SKILLS is the project-level .claude/skills (outside the repo checkout).
# It only exists on developer machines, not in CI. Skip when absent.

PROJECT_SKILLS="${REPO_ROOT}/../.claude/skills"
SYMLINK_SKILLS=("status" "sync-and-build" "worktree")
SYMLINKS_OK=true
SYMLINK_WARNINGS=""

if [[ -d "${PROJECT_SKILLS}" ]]; then
  for skill in "${SYMLINK_SKILLS[@]}"; do
    link="${PROJECT_SKILLS}/${skill}"
    if [[ -L "${link}" ]]; then
      resolved="$(readlink -f "${link}" 2>/dev/null || echo "BROKEN")"
      canonical="${REPO_ROOT}/ops/automation/skills/${skill}"
      if [[ "${resolved}" != "${canonical}" ]]; then
        SYMLINKS_OK=false
        SYMLINK_WARNINGS+="  WRONG TARGET: .claude/skills/${skill} → ${resolved}\n"
        ((DRIFT_ERRORS++))
      fi
    elif [[ -d "${link}" ]]; then
      SYMLINKS_OK=false
      SYMLINK_WARNINGS+="  NOT SYMLINK: .claude/skills/${skill} is a plain directory (run ops/scripts/setup-skill-symlinks.sh)\n"
      ((DRIFT_ERRORS++))
    else
      SYMLINKS_OK=false
      SYMLINK_WARNINGS+="  MISSING: .claude/skills/${skill} (run ops/scripts/setup-skill-symlinks.sh)\n"
      ((DRIFT_ERRORS++))
    fi
  done
fi

# ─── Phase 3: stale path detection ──────────────────────────────────────────

STALE_PATTERNS=(
  "icn-ops/state"
  "sync-from-icn.sh"
  "10\.8\.10\.4[012]"
)

STALE_FILES=(
  "ops/automation/skills"
  ".claude/agents"
  ".claude/skills"
  "ops/CLAUDE.md"
)

STALE_HITS=""
for pattern in "${STALE_PATTERNS[@]}"; do
  for dir in "${STALE_FILES[@]}"; do
    full_dir="${REPO_ROOT}/${dir}"
    if [[ -e "${full_dir}" ]]; then
      hits=$(grep -rn "${pattern}" "${full_dir}" 2>/dev/null | grep -v ".pyc" | grep -v "Binary" || true)
      if [[ -n "${hits}" ]]; then
        STALE_HITS+="  [${pattern}] in ${dir}:\n"
        while IFS= read -r line; do
          STALE_HITS+="    ${line}\n"
        done <<< "${hits}"
        ((DRIFT_ERRORS++))
      fi
    fi
  done
done

# ─── Phase 4: live git state ─────────────────────────────────────────────────

BRANCH=$(git -C "${REPO_ROOT}" branch --show-current 2>/dev/null || echo "unknown")
DIRTY=$(git -C "${REPO_ROOT}" status --porcelain 2>/dev/null | wc -l | tr -d ' ')
DIRTY_STATUS=$([[ "${DIRTY}" == "0" ]] && echo "clean" || echo "${DIRTY} uncommitted change(s)")

WORKTREES=$(git -C "${REPO_ROOT}" worktree list 2>/dev/null | awk '{print $1, $3}' || echo "unavailable")

# ─── Phase 5: sprint state ──────────────────────────────────────────────────

# sprint_state owns the CADENCE (is a sprint running), never "what is being worked
# on now" — that is a live issue/PR query (Refs icn#2634). A dormant cadence is a
# truthful answer and must be reported as such, not as a stale sprint number.
SPRINT_FILE="${REPO_ROOT}/ops/state/sprint/current.json"
SPRINT_SUMMARY=$(python3 -c "
import json
d = json.load(open('${SPRINT_FILE}'))
if d.get('cadence') == 'dormant' or d.get('active_sprint') is None:
    print('no active sprint (cadence dormant)')
else:
    print('Sprint %s (%s)' % (d.get('active_sprint'), d.get('status','?')))
" 2>/dev/null || echo "unresolved (sprint owner unreadable)")
# Emitted as JSON, not as Python source. The --json payload consumes this with json.loads;
# repr() was only ever correct because the payload used to be built by splicing shell values
# into Python source, which is the defect icn#2638 names.
SPRINT_ACTIVE_JSON=$(python3 -c "import json; d=json.load(open('${SPRINT_FILE}')); v=d.get('active_sprint'); print(json.dumps(v if isinstance(v,(int,type(None))) else str(v)))" 2>/dev/null || echo "null")
SPRINT_STATUS=$(python3 -c "import json; d=json.load(open('${SPRINT_FILE}')); print(d.get('status','?'))" 2>/dev/null || echo "?")
SPRINT_TASKS=$(python3 -c "
import json
d = json.load(open('${SPRINT_FILE}'))
tasks = d.get('tasks', [])
from collections import Counter
counts = Counter(t.get('status','unknown') for t in tasks)
print(', '.join(f'{v} {k}' for k,v in sorted(counts.items())) or 'none')
" 2>/dev/null || echo "unavailable")

# ─── Phase 6: open PRs (if gh available) ────────────────────────────────────

PR_STATE="unavailable (gh not authenticated or offline)"
if command -v gh &>/dev/null && gh auth status &>/dev/null 2>&1; then
  PR_COUNT=$(gh pr list --repo InterCooperative-Network/icn --json number 2>/dev/null | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")
  if [[ "${PR_COUNT}" == "0" ]]; then
    PR_STATE="none open"
  elif [[ "${PR_COUNT}" =~ ^[0-9]+$ ]]; then
    PR_STATE="${PR_COUNT} open"
    PR_LIST=$(gh pr list --repo InterCooperative-Network/icn --json number,title --jq '.[] | "  #\(.number) \(.title)"' 2>/dev/null || echo "  (details unavailable)")
  fi
fi

# ─── Phase 7: canonical paths ────────────────────────────────────────────────

POLICY_CHECKS=$(python3 -c "
import json
d = json.load(open('${REPO_ROOT}/ops/state/truth/policy.json'))
checks = d['merge']['required_checks']
print(str(len(checks)) + ' required checks')
" 2>/dev/null || echo "?")

# ─── Output ──────────────────────────────────────────────────────────────────

if [[ "${MODE}" == "--json" ]]; then
  # Values reach Python as ENVIRONMENT, never as interpolated Python source (Refs icn#2638).
  # The previous form spliced shell values into the heredoc body, so every field had to be
  # valid *Python source*: the shell booleans below arrived as the bare names `true`/`false`
  # and the mode died with NameError before json.dumps ever ran. Passing by environment
  # removes the entire class of defect -- a value can no longer be syntax.
  WMN_REPO_ROOT="${REPO_ROOT}" \
  WMN_BRANCH="${BRANCH}" \
  WMN_DIRTY_STATUS="${DIRTY_STATUS}" \
  WMN_SPRINT_SUMMARY="${SPRINT_SUMMARY}" \
  WMN_SPRINT_ACTIVE_JSON="${SPRINT_ACTIVE_JSON}" \
  WMN_SPRINT_STATUS="${SPRINT_STATUS}" \
  WMN_SPRINT_TASKS="${SPRINT_TASKS}" \
  WMN_PR_STATE="${PR_STATE}" \
  WMN_POLICY_CHECKS="${POLICY_CHECKS}" \
  WMN_DRIFT_ERRORS="${DRIFT_ERRORS}" \
  WMN_TRUTH_OK="${TRUTH_OK}" \
  WMN_SYMLINKS_OK="${SYMLINKS_OK}" \
  python3 - <<'PY_JSON_EOF'
import json, os


def env(name):
    return os.environ.get(name, "")


def flag(name):
    """The shell sets these to the literal strings "true"/"false"."""
    return env(name) == "true"


repo_root = env("WMN_REPO_ROOT")

# active_sprint is genuinely nullable: a dormant cadence is a truthful answer, not a gap.
try:
    sprint_active = json.loads(env("WMN_SPRINT_ACTIVE_JSON"))
except ValueError:
    sprint_active = None

data = {
    "repo_root": repo_root,
    "workspace_root": repo_root + "/icn",
    "branch": env("WMN_BRANCH"),
    "working_tree": env("WMN_DIRTY_STATUS"),
    "sprint": {
        "summary": env("WMN_SPRINT_SUMMARY"),
        "active_sprint": sprint_active,
        "status": env("WMN_SPRINT_STATUS"),
        "tasks": env("WMN_SPRINT_TASKS"),
    },
    "current_work_owner": (
        "live issue/PR query \u2014 see live_issue_state / live_pr_state "
        "in ops/state/truth/sources.json"
    ),
    "open_prs": env("WMN_PR_STATE"),
    "merge_policy": env("WMN_POLICY_CHECKS") + " (read ops/state/truth/policy.json)",
    "drift_errors": int(env("WMN_DRIFT_ERRORS") or 0),
    "truth_files_ok": flag("WMN_TRUTH_OK"),
    "symlinks_ok": flag("WMN_SYMLINKS_OK"),
    "canonical_truth": "ops/state/truth/sources.json",
    "canonical_policy": "ops/state/truth/policy.json",
    "canonical_agents": "ops/state/truth/agents.json",
    "canonical_skills": "ops/state/truth/skills.json",
}
print(json.dumps(data, indent=2))
PY_JSON_EOF
  exit ${DRIFT_ERRORS}
fi

if [[ "${MODE}" == "--drift" ]]; then
  if [[ ${DRIFT_ERRORS} -gt 0 ]]; then
    echo "DRIFT DETECTED: ${DRIFT_ERRORS} problem(s)"
    [[ "${TRUTH_OK}" == "false" ]] && echo "  Missing canonical truth files"
    [[ "${SYMLINKS_OK}" == "false" ]] && printf "${SYMLINK_WARNINGS}"
    [[ -n "${STALE_HITS}" ]] && printf "Stale paths:\n${STALE_HITS}"
    exit 1
  fi
  echo "OK: no drift detected"
  exit 0
fi

# ─── Human-readable output ───────────────────────────────────────────────────

echo "╔══════════════════════════════════════════════════════╗"
echo "║          ICN — What Matters Now                      ║"
echo "╚══════════════════════════════════════════════════════╝"

section "Repo"
echo "  Root:      ${REPO_ROOT}"
echo "  Workspace: ${REPO_ROOT}/icn  (cargo commands run from here)"
echo "  Branch:    ${BRANCH}"
echo "  State:     ${DIRTY_STATUS}"

section "Worktrees"
while IFS= read -r line; do
  echo "  ${line}"
done <<< "${WORKTREES}"

section "Sprint cadence"
echo "  ${SPRINT_SUMMARY}"
echo "  Tasks on board: ${SPRINT_TASKS}"
echo "  Cadence record: ops/state/sprint/current.json"
echo "  Current work is NOT here — query live issues/PRs (live_issue_state, live_pr_state)"

section "Open PRs"
echo "  ${PR_STATE}"
if [[ -n "${PR_LIST:-}" ]]; then
  echo "${PR_LIST}"
fi

section "Merge Policy"
echo "  ${POLICY_CHECKS}  (squash-by-default, admin bypass for queue-stalled only)"
echo "  Source: ops/state/truth/policy.json"

section "Canonical Truth Files"
if [[ "${TRUTH_OK}" == "true" ]]; then
  echo "  ✓ All present (sources.json, policy.json, agents.json, skills.json)"
else
  echo "  ✗ Some canonical files missing — run from repo root"
fi

section "Skill Symlinks"
if [[ "${SYMLINKS_OK}" == "true" ]]; then
  echo "  ✓ status, sync-and-build, worktree → ops/automation/skills/"
else
  printf "  ✗ Problems:\n${SYMLINK_WARNINGS}"
  echo "  Fix: bash ops/scripts/setup-skill-symlinks.sh"
fi

section "Drift Check"
if [[ ${DRIFT_ERRORS} -eq 0 ]]; then
  echo "  ✓ No stale paths detected"
elif [[ -n "${STALE_HITS}" ]]; then
  printf "  ✗ Stale path hits:\n${STALE_HITS}"
fi

echo ""
echo "Drift errors: ${DRIFT_ERRORS}"
if [[ ${DRIFT_ERRORS} -gt 0 ]]; then
  exit 1
fi
