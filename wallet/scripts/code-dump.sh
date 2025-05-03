#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

# === Config ===
PROJECT_NAME=$(basename "$(pwd)")
TIMESTAMP=$(date +"%Y-%m-%d_%H-%M")
DEFAULT_OUT="${PROJECT_NAME}_dump_${TIMESTAMP}.txt"
TMP_FILE=".tmp_icn_dump.txt"
INCLUDE_DEPS=false
EXCLUDE_PATTERNS=(
  "./.git/*"
  "./target/*"
  "./node_modules/*"
  "./deps/*"
  "./.cursor/*"
  "./.wallet/*"
  "./docker/*"
  "./scripts/tmp/*"
  "./.github/workflows/*"
  "./*.lock"
  "./*.dump.txt"
  "./${PROJECT_NAME}_dump_*.txt"
)
INCLUDE_REPOS=()

# === Parse Args ===
OUT_FILE="$DEFAULT_OUT"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      OUT_FILE="$2"
      shift 2
      ;;
    --include-deps)
      INCLUDE_DEPS=true
      shift
      ;;
    --exclude)
      EXCLUDE_PATTERNS+=("$2")
      shift 2
      ;;
    --include-repo)
      INCLUDE_REPOS+=("$2")
      shift 2
      ;;
    --help)
      echo "Usage: $0 [options]"
      echo "Options:"
      echo "  --out FILE              Output file name (default: auto-generated with timestamp)"
      echo "  --include-deps          Include projects in ./deps/"
      echo "  --include-repo DIR      Include specific repo (requires --include-deps)"
      echo "  --exclude PATTERN       Add additional exclusion patterns"
      echo "  --help                  Show help and exit"
      exit 0
      ;;
    *)
      echo "❌ Unknown argument: $1"
      exit 1
      ;;
  esac
done

# Delete any old dumps matching the current project
find . -maxdepth 1 -type f -name "${PROJECT_NAME}_dump_*.txt" -exec rm -f {} +

echo "📦 Dumping '$PROJECT_NAME' to: $OUT_FILE"
echo "🧩 Excluding patterns: ${EXCLUDE_PATTERNS[*]}"
echo "📎 Including deps: $INCLUDE_DEPS"

# === Start Dump ===
echo "# Code Dump for $PROJECT_NAME" > "$TMP_FILE"
echo "# Generated: $(date)" >> "$TMP_FILE"
echo "# ----------------------------------------" >> "$TMP_FILE"

# === Find Main Project Files ===
FIND_EXCLUDES=()
for pattern in "${EXCLUDE_PATTERNS[@]}"; do
  FIND_EXCLUDES+=(-not -path "$pattern")
done

find . -type f \
  "${FIND_EXCLUDES[@]}" \
  ! -name "$TMP_FILE" \
  ! -name "$OUT_FILE" \
  | sort | while read -r file; do
    echo -e "\n--- FILE: $file ---" >> "$TMP_FILE"
    cat "$file" >> "$TMP_FILE"
done

# === Optional: Dump ./deps/ Repos ===
if [ "$INCLUDE_DEPS" = true ]; then
  echo -e "\n\n# --- DEPENDENCIES ---" >> "$TMP_FILE"
  if [ ${#INCLUDE_REPOS[@]} -gt 0 ]; then
    for repo in "${INCLUDE_REPOS[@]}"; do
      if [ -d "$repo" ]; then
        echo -e "\n\n## Repo: $repo" >> "$TMP_FILE"
        find "$repo" -type f -not -path "*/\.*" | sort | while read -r f; do
          echo -e "\n--- FILE: $f ---" >> "$TMP_FILE"
          cat "$f" >> "$TMP_FILE"
        done
      fi
    done
  else
    for dep in deps/*; do
      if [ -d "$dep" ] && [ -f "$dep/Cargo.toml" ]; then
        echo -e "\n\n## Repo: $dep" >> "$TMP_FILE"
        find "$dep" -type f -not -path "*/\.*" | sort | while read -r f; do
          echo -e "\n--- FILE: $f ---" >> "$TMP_FILE"
          cat "$f" >> "$TMP_FILE"
        done
      fi
    done
  fi
fi

# === Finalize ===
mv "$TMP_FILE" "$OUT_FILE"
echo "✅ Dump complete: $OUT_FILE"
echo "📏 Size: $(du -h "$OUT_FILE" | cut -f1)"

# === Update .gitignore ===
if [ -f .gitignore ]; then
  echo "📄 Updating .gitignore..."
  grep -qxF "*.dump.txt" .gitignore || echo "*.dump.txt" >> .gitignore
  grep -qxF "${PROJECT_NAME}_dump_*.txt" .gitignore || echo "${PROJECT_NAME}_dump_*.txt" >> .gitignore
  echo "✅ .gitignore updated."
fi
