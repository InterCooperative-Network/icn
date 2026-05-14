#!/usr/bin/env bash
# Non-destructive Prettier check restricted to website files changed vs a base ref.
#
# Default base ref: origin/main. Override with BASE_REF env or first positional arg.
#
# Usage:
#   bash scripts/format-check-changed.sh
#   bash scripts/format-check-changed.sh origin/some-branch
#   BASE_REF=HEAD~1 bash scripts/format-check-changed.sh
#
# Exits 0 when no website files changed under the configured extensions
# (so it is safe to call unconditionally from CI or pre-push hooks).

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root/website"

base_ref="${1:-${BASE_REF:-origin/main}}"

if ! git rev-parse --verify --quiet "$base_ref" >/dev/null; then
  if [ "$base_ref" = "origin/main" ] && git rev-parse --verify --quiet main >/dev/null; then
    base_ref="main"
  else
    echo "format-check-changed: base ref '$base_ref' not found; skipping (no-op)."
    exit 0
  fi
fi

# Prettier-handled extensions present under website/. Keep narrow on purpose —
# anything else falls through to a deliberate follow-up rather than silently
# expanding the script's surface area.
allowed_re='\.(astro|css|ts|tsx|js|jsx|mjs|cjs|json|md|yml|yaml)$'

changed_files="$(git -C "$repo_root" diff --name-only --diff-filter=d "${base_ref}"...HEAD -- 'website/' \
  | sed 's#^website/##' \
  | grep -E "$allowed_re" || true)"

if [ -z "$changed_files" ]; then
  echo "format-check-changed: no website files to check (base: $base_ref)."
  exit 0
fi

file_count="$(printf '%s\n' "$changed_files" | wc -l | tr -d ' ')"
echo "format-check-changed: checking ${file_count} file(s) against base '${base_ref}'..."

printf '%s\n' "$changed_files" \
  | xargs npx prettier --check --plugin=prettier-plugin-astro
