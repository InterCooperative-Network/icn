#!/usr/bin/env bash
# setup-skill-symlinks.sh — optional, MACHINE-LOCAL convenience links for the ops-automation skills.
#
# Scope, and what this is NOT (icn#2633):
#   ops/state/truth/skills.json declares the ops-automation skills canonical under
#   ops/automation/skills/ with NO provider-facing copy inside this repository. This script does
#   not change that. It links them into a project-level .claude/skills/ directory that lives
#   OUTSIDE the repository root, for a developer who wants them exposed to a provider tool.
#
#   Those links are untracked, machine-local, and are not a claim of the registry. Nothing in CI
#   depends on them, and their absence is not drift. In-repo skill ownership is enforced by
#   scripts/check-skill-registry.py, never by this script.
#
# The skill names are read from the registry — do not add a name here.
#
# Usage:
#   bash ops/scripts/setup-skill-symlinks.sh [--check]
#
# --check: verify only, do not create (exits 1 if any link is missing/wrong)

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SKILLS_DIR="${REPO_ROOT}/../.claude/skills"
CANONICAL_DIR="${REPO_ROOT}/ops/automation/skills"

mapfile -t SKILLS < <(python3 -c "
import json,sys
d=json.load(open('${REPO_ROOT}/ops/state/truth/skills.json'))
print('\n'.join(e['name'] for e in d['skills']['ops_automation_canonical']))
")
if [[ "${#SKILLS[@]}" -eq 0 ]]; then
  echo "ERROR: no ops-automation skills registered in ops/state/truth/skills.json" >&2
  exit 1
fi

CHECK_ONLY=false
if [[ "${1:-}" == "--check" ]]; then
  CHECK_ONLY=true
fi

ERRORS=0

for skill in "${SKILLS[@]}"; do
  target_link="${SKILLS_DIR}/${skill}"
  canonical="${CANONICAL_DIR}/${skill}"

  if [[ ! -d "${canonical}" ]]; then
    echo "ERROR: Canonical source missing: ${canonical}"
    ERRORS=$((ERRORS + 1))
    continue
  fi

  if [[ -L "${target_link}" ]]; then
    # It's already a symlink — verify it resolves correctly
    resolved="$(readlink -f "${target_link}" 2>/dev/null || echo "BROKEN")"
    if [[ "${resolved}" == "${canonical}" ]]; then
      echo "OK: ${skill} → symlink correct"
    else
      echo "WRONG: ${skill} symlink points to ${resolved}, expected ${canonical}"
      if [[ "${CHECK_ONLY}" == "false" ]]; then
        rm "${target_link}"
        ln -s "${canonical}" "${target_link}"
        echo "FIXED: recreated symlink for ${skill}"
      else
        ERRORS=$((ERRORS + 1))
      fi
    fi
  elif [[ -d "${target_link}" ]]; then
    if [[ "${CHECK_ONLY}" == "true" ]]; then
      echo "DRIFT: ${skill} is a plain directory, not a symlink (divergence risk)"
      ERRORS=$((ERRORS + 1))
    else
      echo "REPLACING: removing plain directory and creating symlink for ${skill}"
      rm -rf "${target_link}"
      ln -s "${canonical}" "${target_link}"
      echo "CREATED: ${skill} → ${canonical}"
    fi
  else
    if [[ "${CHECK_ONLY}" == "true" ]]; then
      echo "MISSING: ${skill} symlink does not exist"
      ERRORS=$((ERRORS + 1))
    else
      mkdir -p "${SKILLS_DIR}"
      ln -s "${canonical}" "${target_link}"
      echo "CREATED: ${skill} → ${canonical}"
    fi
  fi
done

if [[ "${ERRORS}" -gt 0 ]]; then
  echo ""
  echo "FAIL: ${ERRORS} skill symlink problem(s) found."
  echo "Run: bash ops/scripts/setup-skill-symlinks.sh"
  exit 1
fi

echo ""
echo "All ${#SKILLS[@]} skill symlinks OK."
