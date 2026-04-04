#!/usr/bin/env bash
# setup-skill-symlinks.sh — Create/verify symlinks from project-level .claude/skills/ to canonical ops/automation/skills/
#
# Run this once after cloning or when symlinks are missing.
# The project-level .claude/ is not tracked by git, so symlinks must be created manually or via this script.
#
# Usage:
#   bash ops/scripts/setup-skill-symlinks.sh [--check]
#
# --check: verify only, do not create (exits 1 if any symlink is missing/wrong)

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SKILLS_DIR="${REPO_ROOT}/../.claude/skills"
CANONICAL_DIR="${REPO_ROOT}/ops/automation/skills"

SKILLS=("status" "sync-and-build" "worktree")

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
    ((ERRORS++))
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
        ((ERRORS++))
      fi
    fi
  elif [[ -d "${target_link}" ]]; then
    if [[ "${CHECK_ONLY}" == "true" ]]; then
      echo "DRIFT: ${skill} is a plain directory, not a symlink (divergence risk)"
      ((ERRORS++))
    else
      echo "REPLACING: removing plain directory and creating symlink for ${skill}"
      rm -rf "${target_link}"
      ln -s "${canonical}" "${target_link}"
      echo "CREATED: ${skill} → ${canonical}"
    fi
  else
    if [[ "${CHECK_ONLY}" == "true" ]]; then
      echo "MISSING: ${skill} symlink does not exist"
      ((ERRORS++))
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
