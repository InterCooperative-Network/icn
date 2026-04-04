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

fail() { echo "FAIL: $*" >&2; ERRORS=$((ERRORS + 1)); }
warn() { echo "WARN: $*" >&2; WARNINGS=$((WARNINGS + 1)); }
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
#
# PROJECT_SKILLS lives outside the repo (project-level .claude/skills on dev machines).
# This directory does not exist in CI checkouts — the check is skipped when absent.
# To verify symlinks locally: bash ops/scripts/setup-skill-symlinks.sh

PROJECT_SKILLS="${REPO_ROOT}/../.claude/skills"
CANONICAL_SKILLS="${REPO_ROOT}/ops/automation/skills"
ICN_SKILLS="${REPO_ROOT}/.claude/skills"

if [[ -d "${PROJECT_SKILLS}" ]]; then
  # ops/automation/skills canonical skills — must be symlinked from project-level
  SYMLINK_SKILLS=("status" "sync-and-build" "worktree")

  # icn-level canonical skills that also exist in project-level — must be symlinks there
  declare -A ICN_SYMLINK_SKILLS
  ICN_SYMLINK_SKILLS["fix-ci"]="${ICN_SKILLS}/fix-ci"
  ICN_SYMLINK_SKILLS["icn-preflight"]="${ICN_SKILLS}/icn-preflight"
  ICN_SYMLINK_SKILLS["merge-prs"]="${ICN_SKILLS}/merge-prs"

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
      fail "Skill is plain directory, not symlink (duplicate writable source): .claude/skills/${skill} — fix: rm -rf ${link} && ln -s ${canonical} ${link}"
    else
      fail "Skill symlink missing: .claude/skills/${skill} — fix: ln -s ${canonical} ${link}"
    fi
  done

  # Check icn-level skill symlinks at project level
  for skill in "${!ICN_SYMLINK_SKILLS[@]}"; do
    link="${PROJECT_SKILLS}/${skill}"
    canonical="${ICN_SYMLINK_SKILLS[$skill]}"

    if [[ -L "${link}" ]]; then
      resolved="$(readlink -f "${link}" 2>/dev/null || echo "BROKEN")"
      if [[ "${resolved}" == "${canonical}" ]]; then
        ok "Symlink valid: .claude/skills/${skill} → icn/.claude/skills/${skill}"
      else
        fail "Symlink wrong target: .claude/skills/${skill} → ${resolved} (expected ${canonical})"
      fi
    elif [[ -d "${link}" ]]; then
      fail "Duplicate writable skill: .claude/skills/${skill} is a directory (must be symlink to ${canonical}) — fix: rm -rf ${link} && ln -s ${canonical} ${link}"
    else
      fail "Skill symlink missing: .claude/skills/${skill} — fix: ln -s ${canonical} ${link}"
    fi
  done
else
  ok "PROJECT_SKILLS (${PROJECT_SKILLS}) not present — skipping project-level symlink check (CI-only path)"
fi

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

# ─── Check 4: Machine-specific absolute paths in agent/skill files ───────────

# .mcp.json is exempted (intentionally machine-tied).
# Agent and skill files must not contain /home/ubuntu or similar machine paths
# in executable code blocks — only in examples clearly marked as illustrative.

MACHINE_PATH_SCAN=(
  ".claude/agents"
  ".claude/skills"
  "ops/automation/skills"
)

for dir in "${MACHINE_PATH_SCAN[@]}"; do
  full="${REPO_ROOT}/${dir}"
  if [[ ! -e "${full}" ]]; then continue; fi

  # Follow symlinks so we scan the canonical files, not just symlink metadata
  hits=$(find -L "${full}" -name "*.md" -exec grep -ln "/home/ubuntu" {} \; 2>/dev/null \
    | while read -r f; do
        # Allow paths in Markdown table rows (lines starting with |)
        # Allow paths after ubuntu@ (SSH targets - legitimate)
        # Allow lines marked as examples/illustrative
        grep -n "/home/ubuntu" "$f" \
          | grep -v "ubuntu@" \
          | grep -Pv "^[^:]*:[[:space:]]*\|" \
          | grep -v "# example\|# machine\|# illustrative\|# topology\|examples_only" \
          && true || true
      done || true)
  if [[ -n "${hits}" ]]; then
    while IFS= read -r line; do
      fail "Machine-specific path in skill/agent file: ${dir}: ${line} — use REPO_ROOT=\$(git rev-parse --show-toplevel) instead"
    done <<< "${hits}"
  else
    ok "No machine-specific paths in ${dir}"
  fi
done

# ─── Check 4b: HARD BAN on hardcoded required-check sets ─────────────────────

# NO skill may contain 'required = {' in code sections. Required checks must ALWAYS
# be loaded from ops/state/truth/policy.json at runtime. This prevents stale-policy bugs.
#
# Pattern banned: required = {   (in any SKILL.md, in any executable context)
# Correct pattern: load from policy.json via python3 or jq

HARDCODE_BAN_DIRS=(
  ".claude/skills"
  ".claude/agents"
  "ops/automation/skills"
)

for dir in "${HARDCODE_BAN_DIRS[@]}"; do
  full="${REPO_ROOT}/${dir}"
  if [[ ! -e "${full}" ]]; then continue; fi

  while IFS= read -r -d '' f; do
    # Detect hardcoded required-check set assignment in code sections
    # The pattern 'required = {' or 'required = [' with CI check names is banned
    # Allow in truth_contract YAML sections only (they document, not execute)
    # Ban 1: Python dict/set literal with check names
    banned_lines=$(grep -n "required\s*=\s*{" "$f" 2>/dev/null \
      | grep -v "truth_contract:\|canonical_sources:\|live_load_required:\|never_hardcode:\|examples_only:" \
      | grep -v "^[[:space:]]*#" \
      || true)
    if [[ -n "${banned_lines}" ]]; then
      while IFS= read -r bline; do
        fail "Hardcoded required-check set (required = {) in $(basename $(dirname $f))/$(basename $f): ${bline} — load from policy.json"
      done <<< "${banned_lines}"
    fi

    # Ban 2: jq IN() with hardcoded "Build Release" (catches the other form of check set hardcoding)
    banned_jq=$(grep -n 'IN("Build Release"' "$f" 2>/dev/null \
      | grep -v "^[[:space:]]*#\|truth_contract:\|examples_only:\|<!-- examples_only" \
      || true)
    if [[ -n "${banned_jq}" ]]; then
      while IFS= read -r bline; do
        fail "Hardcoded required-check set (jq IN pattern) in $(basename $(dirname $f))/$(basename $f): ${bline} — use dynamic REQUIRED var from policy.json"
      done <<< "${banned_jq}"
    fi
  done < <(find -L "${full}" -name "*.md" -print0 2>/dev/null)
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

  if [[ "${REQUIRED_CHECK_COUNT}" -ge 11 ]]; then
    ok "policy.json has ${REQUIRED_CHECK_COUNT} required checks"
  else
    fail "policy.json has fewer than 11 required checks (found ${REQUIRED_CHECK_COUNT}) — verify merge policy is complete (must include Agent Tooling Drift Check)"
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
      fail "Agent not in registry: ${agent_name} (add to ops/state/truth/agents.json)"
    else
      ok "Agent registered: ${agent_name}"
    fi
  done
fi

# ─── Check 7: All SKILL.md files must have truth_contract ────────────────────

# truth_contract is required in every skill — it makes drift detectable.
# Missing it means the skill can silently encode volatile state.

SKILL_TRUTH_DIRS=(
  ".claude/skills"
  "ops/automation/skills"
)

for dir in "${SKILL_TRUTH_DIRS[@]}"; do
  full="${REPO_ROOT}/${dir}"
  if [[ ! -e "${full}" ]]; then continue; fi

  while IFS= read -r -d '' f; do
    if ! grep -q "^truth_contract:" "$f"; then
      fail "Missing truth_contract in $(basename $(dirname $f))/$(basename $f)"
    else
      ok "truth_contract present: $(basename $(dirname $f))/$(basename $f)"
    fi
  done < <(find -L "${full}" -name "SKILL.md" -print0 2>/dev/null)
done

# (Check 8 merged into Check 4b — hardcoded check sets now caught by the required = { ban)

# ─── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "Drift check complete: ${ERRORS} error(s), ${WARNINGS} warning(s)"

if [[ "${ERRORS}" -gt 0 ]]; then
  echo "STATUS: FAIL"
  exit 1
fi

echo "STATUS: PASS"
exit 0
