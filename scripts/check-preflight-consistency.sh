#!/usr/bin/env bash
# check-preflight-consistency.sh — guard against agent-foundation/root guidance drift.
#
# Companion to scripts/icn-preflight.sh. CI-safe: repo-content checks always run;
# environment checks run only where the relevant paths/vars exist and never FAIL.
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

# ── 3. Doc-control command forms have ONE provider-neutral owner ──────────────
#
# AGENTS.md is the provider-neutral operating contract. Provider adapters such as
# CLAUDE.md must point to it instead of duplicating the command doctrine. This is
# deliberately stronger than the old check, which required the same four strings in
# both root files and therefore made drift inevitable.
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

missing=0
for c in "${DOC_COMMANDS[@]}"; do
  grep -qF "$c" AGENTS.md || { missing=1; fail "AGENTS.md lost the canonical doc-control command form: $c"; }
done
[ "$missing" -eq 0 ] && pass "AGENTS.md owns all four universal doc-control command forms"

# Every ACTIVE provider adapter must route to AGENTS.md rather than restate the command
# doctrine. Historical/archive documents are deliberately not scanned: they record what was
# true when written and are not active doctrine (icn#2633).
ADAPTERS=(CLAUDE.md ops/CLAUDE.md .claude/project_rules.md docs/ai/CODEX_WORKFLOW.md)
adapter_dupes=0
for f in "${ADAPTERS[@]}"; do
  if [ ! -f "$f" ]; then
    fail "active provider adapter missing: $f"
    continue
  fi
  if ! grep -q 'AGENTS.md' "$f"; then
    fail "$f must route provider-neutral operating guidance to AGENTS.md"
  fi
  for c in "${DOC_COMMANDS[@]}"; do
    if grep -qF "$c" "$f"; then
      adapter_dupes=1
      fail "$f duplicates provider-neutral doc-control command owned by AGENTS.md: $c"
    fi
  done
done
[ "$adapter_dupes" -eq 0 ] && pass "no active provider adapter duplicates AGENTS.md doc-control command doctrine"

# ── 4. Registries must point at files that exist and foundation owners must bind ─
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

sources = json.load(open("ops/state/truth/sources.json"))
expected = {
    "agent_operating_contract": "AGENTS.md",
    "agent_reasoning_constitution": "docs/ai/ICN_CONSTITUTIONAL_CORE.md",
    "agent_workflow": "docs/ai/WORKFLOW_ARCHITECTURE.md",
    "invariants": "AGENTS.md",
}
for domain, owner in expected.items():
    actual = sources.get("domains", {}).get(domain, {}).get("owner")
    if actual != owner:
        bad.append(f'sources.json: {domain} owner={actual!r}, expected {owner!r}')
    elif not os.path.exists(owner.split("#", 1)[0]):
        bad.append(f'sources.json: {domain} -> {owner} (missing)')

if bad:
    print("  FAIL  agent registry/truth-owner consistency:")
    for b in bad:
        print(f"        {b}")
    sys.exit(1)
print("  ok  agent/skill registry paths exist and foundation truth owners are registered")
PYEOF

# ── 5. Skill ownership is enforced from the registry, not from a name list here ─
#
# icn#2633: this used to diff three hardcoded skill names and only warn, so twelve
# divergences and one unregistered skill were invisible. Scope, mirror policy and
# per-skill assertions are now data in ops/state/truth/skills.json; the checker below
# derives everything from it and FAILS. Adding a skill never edits this file.
if [ -f scripts/check-skill-registry.py ]; then
  if python3 scripts/check-skill-registry.py; then
    pass "skill registry: canonical/mirror ownership is mechanically true"
  else
    fail "skill registry: canonical/mirror ownership is not true (see check-skill-registry output above)"
  fi
else
  fail "scripts/check-skill-registry.py missing — skill ownership is unenforced"
fi

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

# ── result ────────────────────────────────────────────────────────────────────
if [ "$FAILS" -gt 0 ]; then
  echo -e "${RED}check-preflight-consistency: ${FAILS} failure(s)${NC}"
  exit 1
fi
echo -e "${GREEN}check-preflight-consistency: clean${NC}"
