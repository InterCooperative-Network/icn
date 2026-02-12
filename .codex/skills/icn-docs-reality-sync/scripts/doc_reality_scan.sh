#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-.}"
cd "$ROOT"

echo "== ICN Doc Reality Scan =="
echo "root: $(pwd)"

echo
echo "[1/3] Broken relative markdown links in docs/"
missing=0
while IFS= read -r file; do
  while IFS= read -r link; do
    target="${link#*](}"
    target="${target%)}"
    target="${target%%#*}"

    case "$target" in
      ""|http://*|https://*|mailto:*) continue ;;
    esac

    base_dir="$(dirname "$file")"
    resolved="$base_dir/$target"

    if [[ ! -e "$resolved" ]]; then
      echo "MISSING_LINK|$file|$target"
      missing=$((missing + 1))
    fi
  done < <(rg -No '\[[^]]+\]\(([^)]+)\)' "$file" || true)
done < <(find docs \
  -path 'docs/archive' -prune -o \
  -path 'docs/development/sessions' -prune -o \
  -path 'docs/templates' -prune -o \
  -type f -name '*.md' -print | sort)

if [[ "$missing" -eq 0 ]]; then
  echo "No broken relative markdown links found in docs/."
fi

echo
echo "[2/3] Known stale path patterns"
rg -n "docs/dev-journal/ROADMAP.md|\./dev-journal/|\./decisions/|\(KERNEL_APP_SEPARATION.md\)" docs || true

echo
echo "[3/3] App topology drift markers"
rg -n "icn/apps|apps/" docs/AGENTS.md AGENTS.md docs/ARCHITECTURE.md docs/architecture/KERNEL_APP_SEPARATION.md 2>/dev/null || true

echo
echo "Scan complete."
