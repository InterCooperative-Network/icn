#!/usr/bin/env bash
# Tests for the capability-discovery property.
#
# The claim this whole layer makes is:
#
#   "adding a supported agent capability in its canonical location makes it discoverable
#    through the standard agent runtime, without teaching any launcher or prompt about it"
#
# A test that only checks the manifest file exists proves none of that. These tests add a REAL
# capability in its canonical place, prove it appears, and prove CI fails if you forget to
# regenerate. They also prove the manifest cannot lie about the runtime, because the MCP surface
# is introspected from the live server rather than grepped from source.
#
# Usage: bash scripts/test-agent-capability-discovery.sh

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/docs/reference/project-index/generated/agent-capabilities.json"
GEN="python3 $ROOT/scripts/generate-agent-capabilities.py --repo-root $ROOT"
CHECK="python3 $ROOT/scripts/check-agent-capabilities.py --repo-root $ROOT"

# NOTE: `set -o pipefail` is on, and the checker exits 1 BY DESIGN when it finds drift. So
# `$CHECK | grep -q ...` would inherit the checker's exit code, not grep's, and every
# "does the message mention X" assertion would fail spuriously. Capture, then match.
run_check() { $CHECK 2>&1; }

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  FAIL  %s\n     -> %s\n' "$1" "${2:-}" >&2; }

# This suite perturbs TRACKED files to prove drift is detected. Restoring them with
# `git checkout --` would DISCARD a developer's uncommitted work in those files, so we refuse
# to start if they are already dirty and restore from byte-exact copies instead.
FIXTURE_FILES=("docs/reference/project-index/generated/agent-capabilities.json" "ops/mcp/src/tools/health.ts")
for f in "${FIXTURE_FILES[@]}"; do
  if [ -n "$(git -C "$ROOT" status --porcelain -- "$f")" ]; then
    echo "REFUSING TO RUN: $f has uncommitted changes." >&2
    echo "This suite perturbs it and would restore over your work. Commit or stash first." >&2
    exit 2
  fi
done

BACKUP="$(mktemp)"; cp "$MANIFEST" "$BACKUP"
HEALTH_BACKUP="$(mktemp)"; cp "$ROOT/ops/mcp/src/tools/health.ts" "$HEALTH_BACKUP"
PROBE_HELPER="$ROOT/ops/scripts/icn-capability-probe"
cleanup() {
  cp "$BACKUP" "$MANIFEST"
  cp "$HEALTH_BACKUP" "$ROOT/ops/mcp/src/tools/health.ts"
  rm -f "$BACKUP" "$HEALTH_BACKUP" "$PROBE_HELPER"
}
trap cleanup EXIT

echo "capability discovery tests"
echo

# ── 0. baseline ──────────────────────────────────────────────────────────────
$CHECK >/dev/null 2>&1 && ok "committed manifest starts true" || bad "baseline drift" "manifest already stale"

# ── 1. a NEW helper in its canonical place becomes discoverable ───────────────
echo
echo "1. adding a capability in its canonical location"
cat > "$PROBE_HELPER" <<'EOS'
#!/usr/bin/env bash
#: capability: capability-probe
#: summary: Temporary fixture proving canonical additions are discovered.
#: safety: read_only
exit 0
EOS
chmod +x "$PROBE_HELPER"

$CHECK >/dev/null 2>&1
if [ $? -ne 0 ]; then
  ok "forgetting to regenerate FAILS the drift check (this is what CI catches)"
else
  bad "drift check should fail after adding a helper" "it passed"
fi

OUT="$(run_check)"
case "$OUT" in
  *capability-probe*) ok "the failure names the specific capability that drifted" ;;
  *) bad "drift message should name the new capability" "$(printf '%s' "$OUT" | head -3)" ;;
esac

$GEN --write >/dev/null 2>&1
if grep -q '"capability": "capability-probe"' "$MANIFEST"; then
  ok "regenerating discovers it with NO launcher, adapter or prompt edited"
else
  bad "new helper missing from regenerated manifest"
fi
$CHECK >/dev/null 2>&1 && ok "drift check passes again after regeneration" || bad "still drifting"

rm -f "$PROBE_HELPER"
$GEN --write >/dev/null 2>&1
grep -q "capability-probe" "$MANIFEST" && bad "removed helper still listed" || ok "removing it removes it from discovery"

# ── 2. the manifest cannot lie about the MCP surface ──────────────────────────
echo
echo "2. the MCP surface is runtime truth, not source text"
python3 - "$MANIFEST" <<'EOS'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["mcp_tools"].append({"name": "icn_ops_imaginary_tool", "summary": "does not exist"})
json.dump(d, open(p, "w"), indent=2)
EOS
OUT="$(run_check)"
case "$OUT" in
  *icn_ops_imaginary_tool*) ok "a tool the runtime cannot expose is rejected" ;;
  *) bad "invented tool not detected" "$(printf '%s' "$OUT" | head -3)" ;;
esac
cp "$BACKUP" "$MANIFEST"

# A tool really registered in the server must appear via live tools/list.
python3 - "$ROOT/ops/mcp/src/tools/health.ts" <<'EOS'
import re, sys
p = sys.argv[1]
t = open(p).read()
anchor = "export function registerHealthTools("
i = t.index("{", t.index(anchor)) + 1
t = t[:i] + '''
  server.tool(
    "icn_ops_probe_tool_fixture",
    "Temporary fixture tool used by capability-discovery tests.",
    {},
    async () => ({ content: [{ type: "text" as const, text: "ok" }] })
  );
''' + t[i:]
open(p, "w").write(t)
EOS
if npm --prefix "$ROOT/ops/mcp" run build >/dev/null 2>&1; then
  OUT="$(run_check)"
  case "$OUT" in
    *icn_ops_probe_tool_fixture*) ok "a newly registered MCP tool is detected via live tools/list" ;;
    *) bad "new MCP tool not detected by introspection" "$(printf '%s' "$OUT" | head -3)" ;;
  esac
else
  bad "fixture build failed" "could not compile the probe tool"
fi
cp "$HEALTH_BACKUP" "$ROOT/ops/mcp/src/tools/health.ts"
npm --prefix "$ROOT/ops/mcp" run build >/dev/null 2>&1

# ── 3. startup context points at the canonical sources ────────────────────────
echo
echo "3. startup context routes rather than duplicates"
HOOK="$ROOT/.claude/hooks/session-lifecycle.sh"
for needle in "agent-capabilities.json" "icn_ops_agent_runtime" "ops/state/truth/sources.json" "AGENTS.md"; do
  grep -q "$needle" "$HOOK" && ok "startup context points at $needle" || bad "startup context missing $needle"
done
# It must stay a router: a startup banner that dumps documentation is the failure mode.
# Anchor on the exact heading so the DEGRADED banner (which shares the prefix) is measured
# separately rather than being folded into the same range.
LINES=$(sed -n '/^## ICN agent runtime$/,/^CTX$/p' "$HOOK" | wc -l)
if [ "$LINES" -lt 30 ]; then ok "startup context stays small (${LINES} lines)"; else bad "startup context too large" "${LINES} lines"; fi

# The degraded banner must ALSO stay small, and must say plainly that tracking is off.
DLINES=$(sed -n '/^## ICN agent runtime — DEGRADED$/,/^BANNER$/p' "$HOOK" | wc -l)
if [ "$DLINES" -gt 0 ] && [ "$DLINES" -lt 20 ]; then
  ok "degraded banner stays small (${DLINES} lines)"
else
  bad "degraded banner missing or too large" "${DLINES} lines"
fi
if sed -n '/^## ICN agent runtime — DEGRADED$/,/^BANNER$/p' "$HOOK" | grep -q "NOT active"; then
  ok "degraded banner states plainly that lifecycle tracking is NOT active"
else
  bad "degraded banner must not imply tracking is working"
fi

$CHECK >/dev/null 2>&1 && ok "manifest restored and true at end of run" || bad "manifest left dirty"

echo
printf 'passed: %d  failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
