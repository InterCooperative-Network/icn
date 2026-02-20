#!/usr/bin/env bash
# sync-from-icn.sh — Pull docs and stats from the main ICN repo at build time.
# Expects the icn repo checkout at ../icn (sibling directory).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SITE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ICN_ROOT="${ICN_REPO:-$(cd "${SITE_ROOT}/../icn" 2>/dev/null && pwd || echo "")}"

if [ -z "$ICN_ROOT" ] || [ ! -d "$ICN_ROOT" ]; then
    echo "ERROR: ICN repo not found. Set ICN_REPO or place it at ../icn"
    exit 1
fi

echo "=== ICN Website Content Sync ==="
echo "  ICN repo : $ICN_ROOT"
echo "  Site root: $SITE_ROOT"
echo

# ─── 1. Sync selected docs ────────────────────────────────────────────

DOCS_DEST="${SITE_ROOT}/src/content/docs"
mkdir -p "$DOCS_DEST"

# Clear previous synced content
rm -rf "${DOCS_DEST:?}"/*

echo "Syncing docs..."

# Core docs (top-level)
for doc in README.md VISION.md CONTRIBUTING.md; do
    if [ -f "${ICN_ROOT}/${doc}" ]; then
        cp "${ICN_ROOT}/${doc}" "${DOCS_DEST}/${doc}"
        echo "  ✓ ${doc}"
    fi
done

# Selected docs/ files
SYNC_DOCS=(
    "ARCHITECTURE.md"
    "GETTING_STARTED.md"
    "INDEX.md"
    "glossary.md"
    "STATE.md"
    "PHASE_HISTORY.md"
)

for doc in "${SYNC_DOCS[@]}"; do
    if [ -f "${ICN_ROOT}/docs/${doc}" ]; then
        cp "${ICN_ROOT}/docs/${doc}" "${DOCS_DEST}/${doc}"
        echo "  ✓ docs/${doc}"
    fi
done

# Sync entire guide directories
SYNC_DIRS=(
    "guides/user"
    "guides/developer"
    "guides/operations"
    "api"
    "design/economics"
    "design/governance"
)

for dir in "${SYNC_DIRS[@]}"; do
    src="${ICN_ROOT}/docs/${dir}"
    dest="${DOCS_DEST}/${dir}"
    if [ -d "$src" ]; then
        mkdir -p "$dest"
        cp -r "$src"/* "$dest"/ 2>/dev/null || true
        count=$(find "$dest" -name "*.md" | wc -l)
        echo "  ✓ docs/${dir}/ (${count} files)"
    fi
done

# ─── 2. Generate stats.json ───────────────────────────────────────────

echo
echo "Generating stats..."

# Count workspace crates
CRATE_COUNT=0
if [ -f "${ICN_ROOT}/icn/Cargo.toml" ]; then
    CRATE_COUNT=$(grep -c '^\s*"' "${ICN_ROOT}/icn/Cargo.toml" 2>/dev/null || echo "0")
    # Fallback: count crate directories
    if [ "$CRATE_COUNT" -eq 0 ] && [ -d "${ICN_ROOT}/icn/crates" ]; then
        CRATE_COUNT=$(find "${ICN_ROOT}/icn/crates" -maxdepth 1 -mindepth 1 -type d | wc -l)
    fi
fi

# Count app crates
APP_COUNT=0
if [ -d "${ICN_ROOT}/apps" ]; then
    APP_COUNT=$(find "${ICN_ROOT}/apps" -maxdepth 1 -mindepth 1 -type d | wc -l)
fi

# Count binaries
BIN_COUNT=0
if [ -d "${ICN_ROOT}/icn/bins" ]; then
    BIN_COUNT=$(find "${ICN_ROOT}/icn/bins" -maxdepth 1 -mindepth 1 -type d | wc -l)
fi

# Count Rust LoC (approximate, fast)
RUST_LOC="0"
if command -v tokei &>/dev/null; then
    RUST_LOC=$(tokei "${ICN_ROOT}/icn" -t Rust -o json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('Rust',{}).get('code',0))" 2>/dev/null || echo "0")
elif command -v wc &>/dev/null; then
    RUST_LOC=$(find "${ICN_ROOT}/icn" -name "*.rs" -exec cat {} + 2>/dev/null | wc -l || echo "0")
fi

# Count tests (approximate from cargo test output or grep)
TEST_COUNT=$(grep -r "#\[test\]" "${ICN_ROOT}/icn" --include="*.rs" 2>/dev/null | wc -l || echo "0")

# Count doc files
DOC_COUNT=$(find "${ICN_ROOT}/docs" -name "*.md" 2>/dev/null | wc -l || echo "0")

# Count GitHub Actions workflows
WORKFLOW_COUNT=$(find "${ICN_ROOT}/.github/workflows" -name "*.yml" 2>/dev/null | wc -l || echo "0")

# Count open branches
BRANCH_COUNT=$(git -C "$ICN_ROOT" branch -r --format='%(refname:short)' 2>/dev/null | wc -l || echo "0")

# Get latest commit info
LATEST_COMMIT=$(git -C "$ICN_ROOT" log -1 --format='%H' 2>/dev/null || echo "unknown")
LATEST_COMMIT_SHORT=$(git -C "$ICN_ROOT" log -1 --format='%h' 2>/dev/null || echo "unknown")
LATEST_COMMIT_DATE=$(git -C "$ICN_ROOT" log -1 --format='%ci' 2>/dev/null || echo "unknown")
LATEST_COMMIT_MSG=$(git -C "$ICN_ROOT" log -1 --format='%s' 2>/dev/null || echo "unknown")

# Total PRs merged (approximate from merge commit count)
PR_COUNT=$(git -C "$ICN_ROOT" log --oneline --merges 2>/dev/null | wc -l || echo "0")

STATS_FILE="${SITE_ROOT}/src/data/stats.json"
mkdir -p "$(dirname "$STATS_FILE")"

cat > "$STATS_FILE" <<EOF
{
  "crates": ${CRATE_COUNT},
  "appCrates": ${APP_COUNT},
  "binaries": ${BIN_COUNT},
  "rustLinesOfCode": ${RUST_LOC},
  "testCount": ${TEST_COUNT},
  "docFiles": ${DOC_COUNT},
  "ciWorkflows": ${WORKFLOW_COUNT},
  "activeBranches": ${BRANCH_COUNT},
  "mergedPRs": ${PR_COUNT},
  "latestCommit": "${LATEST_COMMIT_SHORT}",
  "latestCommitFull": "${LATEST_COMMIT}",
  "latestCommitDate": "${LATEST_COMMIT_DATE}",
  "latestCommitMessage": "${LATEST_COMMIT_MSG}",
  "syncedAt": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

echo "  ✓ stats.json written"
echo
echo "=== Sync complete ==="
echo "  Docs synced to: ${DOCS_DEST}"
echo "  Stats written to: ${STATS_FILE}"
cat "$STATS_FILE"
