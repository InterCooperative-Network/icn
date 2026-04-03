#!/usr/bin/env bash
# drift-check.sh — CI-safe drift detection for ICN agent tooling
#
# Run in CI or locally to detect operational drift in agent files.
# Exits 0 = clean, 1 = drift detected.
#
# Usage:
#   bash ops/scripts/drift-check.sh
#   bash ops/scripts/drift-check.sh --verbose

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
VERBOSE="${1:-}"
ERRORS=0
WARNINGS=0

fail() { echo "FAIL: $*" >&2; ((ERRORS++)); }
warn() { echo "WARN: $*" >&2; ((WARNINGS++)); }
ok()   { [[ "${VERBOSE}" == "--verbose" ]] && echo "OK:   $*" || true; }

# ─── Check 1: Canonical truth files must exist ──────────────────────────────

TRUTH_FILES=(
  "ops/state/truth/sources.json"
  "ops/state/truth/policy.json"
  "ops/state/truth/agents.json"
  "ops/state/truth/skills.json"
  "ops/state/config/repo-map.json"
  "ops/state/sprint/current.json"
)

for f in "${TRUTH_FILES[@]}"; do
  if [[ ! -f "${REPO_ROOT}/${f}" ]]; then
    fail "Canonical truth file missing: ${f}"
  else
    ok "Truth file present: ${f}"
  fi
done

# ─── Check 2: Skill symlinks must be valid ───────────────────────────────────

PROJECT_SKILLS="${REPO_ROOT}/../.claude/skills"
CANONICAL_SKILLS="${REPO_ROOT}/ops/automation/skills"
SYMLINK_SKILLS=("status" "sync-and-build" "worktree")

for skill in "${SYMLINK_SKILLS[@]}"; do
  link="${PROJECT_SKILLS}/${skill}"
  canonical="${CANONICAL_SKILLS}/${skill}"

  if [[ -L "${link}" ]]; then
    resolved="$(readlink -f "${link}" 2>/dev/null || echo "BROKEN")"
    if [[ "${resolved}" == "${canonical}" ]]; then
      ok "Symlink valid: .claude/skills/${skill}"
    else
      fail "Symlink wrong target: .claude/skills/${skill} → ${resolved} (expected ${canonical})"
    fi
  elif [[ -d "${link}" ]]; then
    fail "Skill is plain directory, not symlink (drift risk): .claude/skills/${skill} — run: bash ops/scripts/setup-skill-symlinks.sh"
  else
    warn "Skill symlink missing (run setup-skill-symlinks.sh): .claude/skills/${skill}"
  fi
done

# ─── Check 3: Stale path patterns must not appear in agent tooling files ─────

# These patterns have historically caused drift. Any hit is a FAIL.
declare -A STALE_PATTERNS
STALE_PATTERNS["icn-ops/state"]="Stale path — canonical is ops/state/ (inside monorepo)"
STALE_PATTERNS["scripts/sync-from-icn"]="Dead script — website reads docs directly via path.resolve"

# These patterns are WARNs (may appear in comments or historical docs)
declare -A WARN_PATTERNS
WARN_PATTERNS["10\\.8\\.10\\.[4][012]"]="Old VLAN 10 K3s IP — post-Feb-2026 cluster is on 10.8.30.x"

# Files to scan (tracked in git, agent-facing)
SCAN_DIRS=(
  ".claude/agents"
  ".claude/skills"
  "ops/automation/skills"
  "ops/CLAUDE.md"
)

for dir in "${SCAN_DIRS[@]}"; do
  full="${REPO_ROOT}/${dir}"
  if [[ ! -e "${full}" ]]; then
    continue
  fi

  for pattern in "${!STALE_PATTERNS[@]}"; do
    hits=$(grep -rlP "${pattern}" "${full}" 2>/dev/null | grep -v ".pyc" | grep -v "Binary" || true)
    if [[ -n "${hits}" ]]; then
      fail "${STALE_PATTERNS[$pattern]} | Pattern '${pattern}' found in: ${hits}"
    else
      ok "No '${pattern}' in ${dir}"
    fi
  done

  for pattern in "${!WARN_PATTERNS[@]}"; do
    hits=$(grep -rlP "${pattern}" "${full}" 2>/dev/null | grep -v ".pyc" | grep -v "Binary" || true)
    if [[ -n "${hits}" ]]; then
      warn "${WARN_PATTERNS[$pattern]} | Pattern '${pattern}' found in: ${hits}"
    fi
  done
done

# ─── Check 4: Machine-specific absolute paths in agent configs ───────────────

# .mcp.json is exempted (intentionally machine-tied).
# Agent/skill files must not contain /home/ubuntu or similar machine paths.

AGENT_FILES=(
  ".claude/agents"
  "ops/automation/skills"
)

for dir in "${AGENT_FILES[@]}"; do
  full="${REPO_ROOT}/${dir}"
  if [[ ! -e "${full}" ]]; then continue; fi

  # Exemptions: examples in icn-preflight skill (it explicitly shows paths)
  hits=$(grep -rn "/home/ubuntu" "${full}" 2>/dev/null | grep -v ".pyc" | grep -v "Binary" \
    | grep -v "# example\|# machine\|10\.8\.\|ubuntu@" || true)
  if [[ -n "${hits}" ]]; then
    while IFS= read -r line; do
      warn "Machine-specific path in agent file: ${line}"
    done <<< "${hits}"
  fi
done

# ─── Check 5: Required CI policy fields present in policy.json ───────────────

POLICY_FILE="${REPO_ROOT}/ops/state/truth/policy.json"
if [[ -f "${POLICY_FILE}" ]]; then
  REQUIRED_CHECK_COUNT=$(python3 -c "
import json, sys
try:
    d = json.load(open('${POLICY_FILE}'))
    checks = d['merge']['required_checks']
    print(len(checks))
except Exception as e:
    print(0)
" 2>/dev/null || echo "0")

  if [[ "${REQUIRED_CHECK_COUNT}" -ge 10 ]]; then
    ok "policy.json has ${REQUIRED_CHECK_COUNT} required checks"
  else
    fail "policy.json has fewer than 10 required checks (found ${REQUIRED_CHECK_COUNT}) — verify merge policy is complete"
  fi
fi

# ─── Check 6: agents.json lists all agents in .claude/agents/ ───────────────

AGENTS_FILE="${REPO_ROOT}/ops/state/truth/agents.json"
AGENTS_DIR="${REPO_ROOT}/.claude/agents"

if [[ -f "${AGENTS_FILE}" ]] && [[ -d "${AGENTS_DIR}" ]]; then
  registered=$(python3 -c "
import json
d = json.load(open('${AGENTS_FILE}'))
names = {a['name'] for a in d['agents']}
print('\n'.join(sorted(names)))
" 2>/dev/null || echo "")

  for agent_file in "${AGENTS_DIR}"/*.md; do
    agent_name=$(basename "${agent_file}" .md)
    if ! echo "${registered}" | grep -qx "${agent_name}"; then
      warn "Agent not in registry: ${agent_name} (add to ops/state/truth/agents.json)"
    else
      ok "Agent registered: ${agent_name}"
    fi
  done
fi

# ─── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "Drift check complete: ${ERRORS} error(s), ${WARNINGS} warning(s)"

if [[ "${ERRORS}" -gt 0 ]]; then
  echo "STATUS: FAIL"
  exit 1
fi

echo "STATUS: PASS"
exit 0
