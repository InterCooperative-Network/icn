#!/usr/bin/env bash
# check-preflight-consistency.sh — guard against root/path guidance drift (Refs #2140).
#
# Companion to scripts/icn-preflight.sh (the documented preflight path is: run both).
# CI-safe: repo-content checks always run; environment checks (stale legacy checkout,
# MCP-root mismatch) run only where the relevant paths/vars exist and never FAIL.
#
# Exit 1 on FAIL (live guidance regressed), exit 0 otherwise (warnings allowed).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

FAILS=0
pass() { echo -e "${GREEN}  ok${NC}  $1"; }
warn() { echo -e "${YELLOW}  !!${NC}  $1"; }
fail() { echo -e "${RED}  FAIL${NC}  $1"; FAILS=$((FAILS + 1)); }

echo "check-preflight-consistency"

# ── 1. Live guidance files must not present legacy roots as current truth ──────
# `~/projects/icn` (standalone clone) and `../icn-wt` (repo-adjacent worktrees) are
# retired. Mentions are allowed only on lines that mark them as legacy.
GUIDANCE_FILES=(CLAUDE.md ops/CLAUDE.md AGENTS.md)
LEGACY_MARKER='legacy|older|retired|not truth|never treat'

for f in "${GUIDANCE_FILES[@]}"; do
  if [ ! -f "$f" ]; then
    fail "guidance file missing: $f"
    continue
  fi
  bad=$(grep -nE 'projects/icn|icn-wt' "$f" | grep -viE "${LEGACY_MARKER}" || true)
  if [ -n "$bad" ]; then
    fail "$f presents a legacy root without a legacy marker:"
    echo "$bad" | sed 's/^/        /'
  else
    pass "$f: legacy roots only appear with legacy framing"
  fi
done

# ── 2. MCP source must not hardcode the legacy worktree root ───────────────────
# Sanctioned places: the clearly-named fallback in ops/mcp/src/paths.ts, and the
# tests that pin that fallback's behavior (ops/mcp/src/tests/).
mcp_hits=$(grep -rn 'icn-wt' ops/mcp/src/ --include='*.ts' 2>/dev/null | grep -v -e 'ops/mcp/src/paths.ts' -e 'ops/mcp/src/tests/' || true)
if [ -n "$mcp_hits" ]; then
  fail "hardcoded legacy worktree root in MCP source (use resolveWorktreeRoot from paths.ts):"
  echo "$mcp_hits" | sed 's/^/        /'
else
  pass "ops/mcp/src: no legacy worktree-root hardcoding outside paths.ts"
fi

# ── 3. Doc-control command forms: scripts exist, exact forms documented ────────
DOC_SCRIPTS=(
  "docs/scripts/doc_control_check.py"
  "docs/scripts/freshness-check.py"
  ".github/scripts/compliance_linter.py"
  ".github/scripts/readiness_overclaim_linter.py"
)
for s in "${DOC_SCRIPTS[@]}"; do
  [ -f "$s" ] && pass "doc-control script present: $s" || fail "doc-control script missing: $s"
done

DOC_COMMANDS=(
  'python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml'
  'python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .'
  'python3 .github/scripts/compliance_linter.py --repo-root .'
  'python3 .github/scripts/readiness_overclaim_linter.py --repo-root .'
)
for doc in CLAUDE.md AGENTS.md; do
  missing=0
  for c in "${DOC_COMMANDS[@]}"; do
    grep -qF "$c" "$doc" || { missing=1; fail "$doc lost the documented command form: $c"; }
  done
  [ "$missing" -eq 0 ] && pass "$doc documents all four doc-control command forms"
done

# ── 4. Registries must point at files that exist (agents.json / skills.json) ───
python3 - <<'PYEOF' || FAILS=$((FAILS + 1))
import json, os, sys
bad = []
agents = json.load(open("ops/state/truth/agents.json"))
for a in agents["agents"]:
    if not os.path.exists(a["path"]):
        bad.append(f'agents.json: {a["name"]} -> {a["path"]} (missing)')
skills = json.load(open("ops/state/truth/skills.json"))
for e in skills["skills"]["ops_automation_canonical"]:
    if not os.path.exists(e["canonical_path"]):
        bad.append(f'skills.json: {e["name"]} -> {e["canonical_path"]} (missing)')
if bad:
    print("  FAIL  registry entries point at missing files:")
    for b in bad:
        print(f"        {b}")
    sys.exit(1)
print("  ok  agents.json and skills.json registry paths all exist on disk")
PYEOF

# ── 5. Preflight skill twins: warn when .agents/ and .claude/ copies diverge ───
# skills.json declares the .agents/skills/ copy authoritative for icn-level skills.
for skill in preflight icn-preflight; do
  a=".agents/skills/${skill}/SKILL.md"
  c=".claude/skills/${skill}/SKILL.md"
  if [ -f "$a" ] && [ -f "$c" ]; then
    if ! diff -q "$a" "$c" >/dev/null 2>&1; then
      warn "skill copies diverge: $a vs $c (skills.json says .agents/ is authoritative; reconcile deliberately)"
    else
      pass "skill copies in sync: ${skill}"
    fi
  fi
done

# ── 6. Environment checks (dev-VM only; skipped silently where paths absent) ───
LEGACY_CLONE="${HOME}/projects/icn"
if [ -d "${LEGACY_CLONE}/.git" ]; then
  legacy_head=$(git -C "${LEGACY_CLONE}" rev-parse HEAD 2>/dev/null || echo "")
  if [ -n "$legacy_head" ]; then
    behind=$(git rev-list --count "${legacy_head}..origin/main" 2>/dev/null || echo "?")
    warn "legacy checkout exists: ${LEGACY_CLONE} (${behind} commits behind origin/main) — never use it as current truth"
  else
    warn "legacy checkout exists: ${LEGACY_CLONE} — never use it as current truth"
  fi
fi

TOPLEVEL=$(git rev-parse --show-toplevel 2>/dev/null || echo "")
if [ -n "${ICN_ROOT:-}" ] && [ -n "$TOPLEVEL" ]; then
  if [ "$(readlink -f "${ICN_ROOT}")" != "$(readlink -f "${TOPLEVEL}")" ]; then
    warn "MCP root mismatch: ICN_ROOT=${ICN_ROOT} but this checkout is ${TOPLEVEL} — MCP repo/worktree answers describe ICN_ROOT, not this checkout"
  else
    pass "ICN_ROOT matches this checkout"
  fi
elif [ -z "${ICN_ROOT:-}" ] && [ -d "${HOME}/icn-dev/worktrees/icn/mcp-host" ] && [ -n "$TOPLEVEL" ]; then
  if [ "$(readlink -f "${HOME}/icn-dev/worktrees/icn/mcp-host")" != "$(readlink -f "${TOPLEVEL}")" ]; then
    warn "icn-ops MCP is rooted at ~/icn-dev/worktrees/icn/mcp-host; its repo/worktree answers describe that checkout, not this one (${TOPLEVEL})"
  fi
fi

# ── result ──────────────────────────────────────────────────────────────────────
if [ "$FAILS" -gt 0 ]; then
  echo -e "${RED}check-preflight-consistency: ${FAILS} failure(s)${NC}"
  exit 1
fi
echo -e "${GREEN}check-preflight-consistency: clean${NC}"
