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

# Skill scan scope is DATA, not a list in this script. ops/state/truth/skills.json owns which
# trees are canonical and which are provider-facing; adding a tree or a skill must never require
# editing a shell array here (icn#2633).
REGISTRY="${REPO_ROOT}/ops/state/truth/skills.json"
mapfile -t SKILL_TREES < <(python3 -c "
import json,sys
try: d=json.load(open('${REGISTRY}'))
except Exception: sys.exit(0)
s=d.get('enforcement',{}).get('scan_scope',{})
for t in s.get('canonical_trees',[])+s.get('provider_trees',[]):
    if t.strip(): print(t.strip())
" 2>/dev/null || true)
if [[ "${#SKILL_TREES[@]}" -eq 0 ]]; then
  fail "could not derive skill scan scope from ops/state/truth/skills.json#enforcement.scan_scope"
  SKILL_TREES=(".agents/skills" ".claude/skills" "ops/automation/skills")
fi

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

# ─── Check 1b: Agent capability manifest is present ─────────────────────────
#
# Full verification introspects the live MCP server and lives in
# scripts/check-agent-capabilities.py (run in CI). Here we only assert the generated manifest
# EXISTS and parses, because drift-check.sh must stay fast.
#
# NOTE: this script is no longer strictly dependency-free. Check 8 below runs
# scripts/check-agent-registry.py, which requires PyYAML -- YAML parse validity is owned by a
# YAML parser rather than reimplemented (icn#2632). Install with
# `python3 -m pip install -r scripts/requirements.txt`. The statement above was true when
# written and is corrected here rather than left to mislead.

CAP_MANIFEST="${REPO_ROOT}/docs/reference/project-index/generated/agent-capabilities.json"
if [[ ! -f "${CAP_MANIFEST}" ]]; then
  fail "Generated capability manifest missing: docs/reference/project-index/generated/agent-capabilities.json (run: python3 scripts/generate-agent-capabilities.py --write)"
elif ! python3 -c "import json,sys; json.load(open(sys.argv[1]))" "${CAP_MANIFEST}" 2>/dev/null; then
  fail "Capability manifest is not valid JSON: docs/reference/project-index/generated/agent-capabilities.json"
else
  ok "Capability manifest present and parses"
fi

# ─── Check 2: Optional machine-local skill symlinks ──────────────────────────
#
# ops/state/truth/skills.json declares that the ops-automation skills have NO provider-facing
# copy in this repository. ops/scripts/setup-skill-symlinks.sh can create convenience links for
# a developer who wants them exposed to a provider; those links live OUTSIDE the repository, are
# untracked, and are not a claim of the registry — so a problem with them is a WARN, never a FAIL.
#
# icn#2633 removed a second, undeclared model from this check (it asserted that three ICN-level
# skills must also be symlinked at that out-of-repo path, which no registry ever said).
# In-repo skill ownership is enforced by Check 2b below.

PROJECT_SKILLS="${REPO_ROOT}/../.claude/skills"
CANONICAL_SKILLS="${REPO_ROOT}/ops/automation/skills"

if [[ -d "${PROJECT_SKILLS}" ]]; then
  mapfile -t SYMLINK_SKILLS < <(python3 -c "
import json,sys
try: d=json.load(open('${REGISTRY}'))
except Exception: sys.exit(0)
print('\n'.join(e['name'] for e in d['skills'].get('ops_automation_canonical', [])))
" 2>/dev/null || true)

  for skill in "${SYMLINK_SKILLS[@]}"; do
    link="${PROJECT_SKILLS}/${skill}"
    canonical="${CANONICAL_SKILLS}/${skill}"
    if [[ -L "${link}" ]]; then
      resolved="$(readlink -f "${link}" 2>/dev/null || echo "BROKEN")"
      if [[ "${resolved}" == "${canonical}" ]]; then
        ok "machine-local symlink valid: ${skill}"
      else
        warn "machine-local symlink points elsewhere: ${link} -> ${resolved} (expected ${canonical})"
      fi
    elif [[ -d "${link}" ]]; then
      warn "machine-local copy is a plain directory, not a symlink: ${link} — it can drift from ${canonical}"
    fi
  done
else
  ok "no machine-local project skills directory at ${PROJECT_SKILLS} — nothing to verify"
fi

# ─── Check 2b: In-repo skill ownership must be mechanically true ─────────────
#
# The registry declares a canonical owner and an exact mirror relationship for every skill in
# its declared scope. scripts/check-skill-registry.py is the enforcement; it derives scope,
# mirror policy and per-skill assertions from the registry, so nothing is listed twice.

SKILL_REGISTRY_CHECK="${REPO_ROOT}/scripts/check-skill-registry.py"
if [[ -f "${SKILL_REGISTRY_CHECK}" ]]; then
  if skill_out="$(python3 "${SKILL_REGISTRY_CHECK}" --repo-root "${REPO_ROOT}" 2>&1)"; then
    ok "skill registry ownership: ${skill_out}"
  else
    while IFS= read -r line; do
      [[ -n "${line}" ]] && echo "  ${line}" >&2
    done <<< "${skill_out}"
    fail "skill registry ownership is not mechanically true (see check-skill-registry output above)"
  fi
else
  fail "scripts/check-skill-registry.py missing — in-repo skill ownership is unenforced"
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
  "${SKILL_TREES[@]}"
  ".claude/agents"
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
  "${SKILL_TREES[@]}"
  ".claude/agents"
)

for dir in "${MACHINE_PATH_SCAN[@]}"; do
  full="${REPO_ROOT}/${dir}"
  if [[ ! -e "${full}" ]]; then continue; fi

  # Follow symlinks so we scan the canonical files, not just symlink metadata
  hits=$(find -L "${full}" -name "*.md" -exec grep -ln "/home/ubuntu" {} \; 2>/dev/null \
    | while read -r f; do
        # Allow paths after ubuntu@ (SSH targets - legitimate)
        # Allow lines marked as examples/illustrative
        # NOTE: there is deliberately no Markdown-table exemption. It used to exist and it
        # failed open — a "repo topology" table kept /home/ubuntu/projects/icn alive in a
        # skill through a green gate for months (icn#2633).
        grep -n "/home/ubuntu" "$f" \
          | grep -v "ubuntu@" \
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
  "${SKILL_TREES[@]}"
  ".claude/agents"
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

# ─── Check 6: the agent registry is true about every provider surface ───────

# Was: a bash loop over .claude/agents/ only, one direction, comparing basenames. It could
# not see .github/agents/ (21 live Copilot definitions) or the icn-agent-pack plugin tree,
# and it never validated a registered path. Delegated to the checker that does both
# directions across every declared surface (Refs icn#2632).

# A missing checker must FAIL, not skip. This script is a documented standalone entrypoint,
# so a conditional skip would let it report success while enforcement had disappeared --
# fail-open on the gate whose whole purpose is failing closed.
if [[ -f "${REPO_ROOT}/scripts/check-agent-registry.py" ]]; then
  if agent_registry_out=$(python3 "${REPO_ROOT}/scripts/check-agent-registry.py" 2>&1); then
    ok "Agent registry consistent across all declared provider surfaces"
  else
    while IFS= read -r line; do
      [[ -n "${line}" ]] && fail "agent registry: ${line}"
    done <<< "${agent_registry_out}"
  fi
else
  fail "scripts/check-agent-registry.py is missing — agent-registry enforcement would be silently absent"
fi

# ─── Check 7: All SKILL.md files must have truth_contract ────────────────────

# truth_contract is required in every skill — it makes drift detectable.
# Missing it means the skill can silently encode volatile state.

SKILL_TRUTH_DIRS=("${SKILL_TREES[@]}")

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
