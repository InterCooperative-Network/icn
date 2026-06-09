#!/usr/bin/env bash
# ============================================================================
# local_receipt_chain_13of13_rehearsal.sh
# ----------------------------------------------------------------------------
# One-command local 13/13 governed receipt-chain rehearsal (icn#1992).
#
# Thin wrapper over scripts/local_economic_receipt_chain_demo.sh. That script is
# the real proof: it drives a governed proposal -> vote -> close -> allocation
# over a fresh, JWT-secured, dev-gated LOCAL gateway and asserts that
# `icnctl audit verify --token --json` reports a complete (13/13) receipt chain.
# This wrapper makes the proof boring to rerun and adds the rehearsal framing:
#
#   1. runs the real proof (fresh node/gateway, no network, no production);
#   2. asserts every receipt-chain check passes (13/13);
#   3. emits a repo-safe evidence packet conforming to
#      urn:icn:contract:rehearsal-evidence-export:v1 and validates it;
#   4. prints what is real, dev-gated, fixture-only, and NOT production.
#
# All outputs land under demo/output/receipt-chain-13of13/ (gitignored). The
# evidence packet is repo-safe by construction (no names, tokens, person DIDs, or
# private paths); the raw transcript/verify.json are diagnostic and may contain
# the ephemeral local node DID, so they stay in the gitignored output dir.
#
# Cautious framing: governed coordination / allocation / contribution receipts,
# provenance chain, audit chain. No payment / wallet / balance / currency /
# bank framing. Fictional data only. Ephemeral loopback gateway, torn down on
# exit. Not production, not a pilot, not live federation.
#
# Usage:   ./scripts/local_receipt_chain_13of13_rehearsal.sh [--fresh] [--keep] [--no-build]
# Flags:
#   --fresh     Explicit: each run uses a fresh ephemeral node/gateway (default).
#   --keep      Keep the inner run's temporary /tmp dir for post-mortem.
#   --no-build  Fail instead of building icnd/icnctl if they are missing.
# Env:     ICN_GW_PORT (default 18080); ICND / ICNCTL (auto-detected from target/)
# ============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INNER="$ROOT/scripts/local_economic_receipt_chain_demo.sh"
VALIDATOR="$ROOT/docs/scripts/validate-rehearsal-evidence.py"
OUTDIR="$ROOT/demo/output/receipt-chain-13of13"

KEEP_RUNDIR=0
NOBUILD=0
for arg in "$@"; do
  case "$arg" in
    --fresh)    : ;; # default behavior; accepted for an explicit one-command call
    --keep)     KEEP_RUNDIR=1 ;;
    --no-build) NOBUILD=1 ;;
    -h|--help)  sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg (try --help)" >&2; exit 2 ;;
  esac
done

log()   { echo "[$(date -u +%H:%M:%SZ)] $*"; }
fatal() { echo "FATAL: $*" >&2; exit 1; }

[ -f "$INNER" ]     || fatal "inner proof script not found: $INNER"
[ -f "$VALIDATOR" ] || fatal "evidence validator not found: $VALIDATOR"
command -v python3 >/dev/null || fatal "python3 is required"

# --- binaries: the inner proof needs icnd + icnctl in icn/target/{release,debug}
find_bin() { for d in release debug; do p="$ROOT/icn/target/$d/$1"; [ -x "$p" ] && { echo "$p"; return 0; }; done; return 1; }
if ! find_bin icnd >/dev/null || ! find_bin icnctl >/dev/null; then
  if [ "$NOBUILD" = "1" ]; then
    fatal "icnd/icnctl not built and --no-build set. Build: (cd icn && cargo build --release -p icnd -p icnctl)"
  fi
  log "icnd/icnctl not found — building (cargo build --release -p icnd -p icnctl; first run is slow)..."
  ( cd "$ROOT/icn" && cargo build --release -p icnd -p icnctl ) || fatal "build of icnd/icnctl failed"
fi

mkdir -p "$OUTDIR"
TRANSCRIPT="$OUTDIR/transcript.txt"
VERIFY_COPY="$OUTDIR/audit-verify.json"
PACKET="$OUTDIR/rehearsal-evidence-export.json"
: >"$TRANSCRIPT"

# --- run the real governed receipt-chain proof (fresh local gateway) ----------
# KEEP=1 makes the inner script preserve its temp run dir so we can read the
# authoritative verify.json it wrote; we tidy that dir afterward.
log "running governed receipt-chain proof against a fresh local gateway..."
echo "### local_economic_receipt_chain_demo.sh transcript ($(date -u +%Y-%m-%dT%H:%M:%SZ)) ###" >>"$TRANSCRIPT"
KEEP=1 bash "$INNER" 2>&1 | tee -a "$TRANSCRIPT"
INNER_RC=${PIPESTATUS[0]}

# --- locate the preserved run dir + its authoritative verify.json -------------
RUNDIR="$(grep -oE '/tmp/icn-econ-chain-demo-[A-Za-z0-9]+' "$TRANSCRIPT" | tail -1)"
if [ -n "$RUNDIR" ] && [ -f "$RUNDIR/verify.json" ]; then
  cp "$RUNDIR/verify.json" "$VERIFY_COPY"
fi
if [ -n "$RUNDIR" ] && [ "$KEEP_RUNDIR" = "0" ]; then rm -rf "$RUNDIR" 2>/dev/null; fi

if [ "$INNER_RC" -ne 0 ] || [ ! -f "$VERIFY_COPY" ]; then
  echo
  fatal "governed receipt-chain proof did not complete (inner exit $INNER_RC). See $TRANSCRIPT. No evidence packet emitted."
fi

# --- read the authoritative summary (decisionHash + checks) -------------------
read_summary() { python3 -c 'import sys,json
try:
    d=json.load(open(sys.argv[1]))
except Exception:
    print("|||"); sys.exit(0)
s=d.get("summary",{})
print("|".join([str(d.get("decisionHash","")), str(s.get("passed","")), str(s.get("total","")), str(s.get("verified",""))]))' "$1"; }

IFS='|' read -r DHASH PASSED TOTAL VERIFIED < <(read_summary "$VERIFY_COPY")
if [ "$VERIFIED" != "True" ] || [ -z "$PASSED" ] || [ "$PASSED" != "$TOTAL" ]; then
  echo
  echo "  Failing checks:"
  python3 -c 'import sys,json
try:
    d=json.load(open(sys.argv[1]))
    for c in d.get("checks",[]):
        if not c.get("passed"): print("    [FAIL]",c.get("name"),"-",c.get("detail"))
except Exception as e:
    print("    (could not parse verify output:",e,")")' "$VERIFY_COPY"
  fatal "audit verify did NOT report a complete chain (verified=$VERIFIED, $PASSED/$TOTAL). No evidence packet emitted. See $VERIFY_COPY."
fi
log "audit verify: $PASSED/$TOTAL receipt-chain checks PASS"
log "decision_hash: $DHASH"
[ "$TOTAL" = "13" ] || log "NOTE: expected 13 checks; gateway reported $TOTAL (audit-chain contract may have changed)."

# --- emit the repo-safe evidence packet ---------------------------------------
RECORDED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
python3 - "$PACKET" "$DHASH" "$PASSED" "$TOTAL" "$RECORDED_AT" <<'PY'
import sys, json
packet_path, dhash, passed, total, recorded_at = sys.argv[1:6]
packet = {
    "schema_version": "0.1.0",
    "recorded_at": recorded_at,
    "rehearsal_label": f"Local governed receipt-chain rehearsal: fictional demo coordination coop ({passed}/{total} audit-verified)",
    "rehearsal_mode": "local-execute",
    "audience_categories": ["operator"],
    "source_material": [
        {
            "kind": "example-snippet",
            "basename": "q1-coordination-allocation.proposal.json",
            "summary": "Fictional governed allocation proposal (compute-hours to infrastructure upkeep) authored inline and executed against a local non-production gateway.",
        }
    ],
    "workflow_steps_completed": ["start", "decide", "confirm", "surface-result", "export-evidence"],
    "decision_outcomes": [
        {
            "category": "approved",
            "plain_summary": "The fictional governed allocation proposal was opened, voted For, and closed Accepted; execution produced a complete provenance chain (governance proof -> allocation/contribution receipt -> settlement intent -> execution record -> ledger journal entry).",
        }
    ],
    "preview_review_boundary": {
        "enforced": False,
        "notes": "Backend governed-execution rehearsal, not an organizer preview/review-then-mutate UI walk; mutation was gated by dev-only flags (admin endpoints + test governance posture). The review-first UI boundary is exercised by the organizer-shell rehearsal track, not here.",
    },
    "mutation_boundary": {
        "executed": True,
        "target": "local-gateway",
        "notes": "Governed proposal executed against an ephemeral local non-production gateway on loopback, torn down on exit. No external network egress.",
    },
    "proof_loop_references": [
        {"kind": "governance-decision-receipt", "public_id_or_category": dhash, "status": "closed"},
        {"kind": "provenance-summary", "public_id_or_category": "governance->allocation->intent->execution->journal", "status": "closed"},
        {"kind": "validator-output-category", "public_id_or_category": f"icnctl-audit-verify-{passed}-of-{total}-pass", "status": "closed"},
    ],
    "warnings": [
        "Run with dev-only gates enabled (admin endpoints, test governance posture, dev self-trust) against an ephemeral loopback gateway; these must never be set in production.",
        "Member standing was established via the dev-gated bootstrap endpoint (POST /v1/commons/dev/bootstrap-standing), not the production standing path.",
    ],
    "follow_ups": [
        {
            "title": "Organizer-facing rehearsal of this proof via the no-CLI workflow",
            "url": "https://github.com/InterCooperative-Network/icn/issues/1746",
        }
    ],
    "privacy_review": {
        "reviewer_role": "operator",
        "status": "reviewed-clean",
        "notes": "Packet contains only fictional demo data and an opaque decision hash; no names, contacts, tokens, person DIDs, or private paths. The ephemeral local node DID and JWT are excluded.",
    },
    "export_safety_classification": "repo-safe",
    "non_claims": [
        "Not a production deployment.",
        "Not a formally committed cooperative pilot or NYCN pilot.",
        "Not a live federation, live Google Drive / Groups / Sheets sync, or any K3s / DNS / Forgejo mutation.",
        "Not private-data handling; all material is fictional and repo-safe by construction.",
        "Not a payment, wallet, balance, currency, or money-transmission flow; this is governed coordination allocation with provenance receipts.",
    ],
}
with open(packet_path, "w", encoding="utf-8") as fh:
    json.dump(packet, fh, indent=2)
    fh.write("\n")
PY
[ -f "$PACKET" ] || fatal "failed to write evidence packet"

# --- validate the packet against the v1 contract ------------------------------
log "validating evidence packet against urn:icn:contract:rehearsal-evidence-export:v1..."
python3 "$VALIDATOR" "$PACKET" || fatal "evidence packet failed schema validation: $PACKET"

# --- clarity matrix: what is real / dev-gated / fixture / not-production -------
cat <<EOF

================ 13/13 RECEIPT-CHAIN REHEARSAL — CLARITY ================
REAL (exercised end-to-end against the live local gateway path):
  - fresh local icnd node + JWT-secured gateway
  - governed proposal -> open -> vote(for) -> close (Accepted)
  - allocation execution -> settlement intent -> execution record -> journal
  - decision hash captured from the governance proof
  - icnctl audit verify --token --json => $PASSED/$TOTAL checks PASS

DEV-GATED (local-only; never set in production):
  - member standing via POST /v1/commons/dev/bootstrap-standing
  - ICN_ENABLE_ADMIN_ENDPOINTS=true, ICN_GOVERNANCE_BUILD_MODE=test
  - ICN_DEV_SELF_TRUST=1 (node roots its own trust web)
  - ephemeral random JWT secret; 1% quorum/approval demo domain

FIXTURE / DEMO-ONLY:
  - all data is fictional (demo coordination coop, compute-hours)

NOT PRODUCTION:
  - ephemeral loopback gateway, torn down on exit; no external network
  - not a pilot, not live federation, not a payment/wallet/balance/currency flow
========================================================================

Evidence packet (repo-safe, schema-valid): $PACKET
Diagnostic transcript / verify.json (gitignored):
  $TRANSCRIPT
  $VERIFY_COPY

RESULT: PASS — local governed receipt chain audit-verified $PASSED/$TOTAL.
EOF
exit 0
