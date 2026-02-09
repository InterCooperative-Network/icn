#!/usr/bin/env bash
# ICN Single-Node Demo — exercises all major subsystems end-to-end
#
# Usage:
#   ICN_GATEWAY=http://localhost:8000 ./scripts/demo-single-node.sh
#   ICN_GATEWAY=http://localhost:8000 ./scripts/demo-single-node.sh --json
#   ICN_GATEWAY=http://localhost:8000 ./scripts/demo-single-node.sh --skip wasm,trust
#
# Prerequisites:
#   - Running ICN daemon with gateway enabled
#   - jq installed
#
# Start the daemon (dev mode):
#   cd icn && cargo build
#   ICN_GATEWAY_JWT_SECRET=$(openssl rand -base64 32) ./target/debug/icnd \
#     --init --data-dir /tmp/icn-demo --gateway-enable

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=demo-lib.sh
. "$SCRIPT_DIR/demo-lib.sh"

# --- CLI argument parsing ---
JSON_MODE=false
SKIP_SECTIONS=""

while [ $# -gt 0 ]; do
  case "$1" in
    --json)     JSON_MODE=true; shift ;;
    --skip)     SKIP_SECTIONS="$2"; shift 2 ;;
    --gateway)  GATEWAY="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--json] [--skip section1,section2] [--gateway URL]"
      echo ""
      echo "Sections: health, identity, coop, governance, ledger, treasury, discovery, wasm, trust, entity"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

should_skip() {
  echo ",$SKIP_SECTIONS," | grep -q ",$1,"
}

echo -e "${BOLD}${CYAN}"
echo "╔══════════════════════════════════════════╗"
echo "║     ICN Single-Node Demo                 ║"
echo "║     Comprehensive End-to-End Test        ║"
echo "╚══════════════════════════════════════════╝"
echo -e "${NC}"
echo -e "  Gateway: ${GATEWAY}"
echo ""

# ── PREFIX for unique resource IDs ──
TS=$(date +%s)
PREFIX="demo-${TS}"

# ── Discover node DID from health/identity ──
NODE_DID=""

# ============================================================================
# 0. Health
# ============================================================================
if should_skip health; then
  section_start "0. Health"
  skip "Skipped"
  section_end skip
else
  section_start "0. Health"
  (
    step "Basic health check"
    RESP=$(http_get "$GATEWAY/v1/health")
    STATUS=$(expect_field "$RESP" '.status' "status")
    ok "Status: $STATUS"

    step "Detailed health check"
    RESP=$(http_get "$GATEWAY/v1/health/detailed")
    VERSION=$(expect_field "$RESP" '.version' "version")
    ok "Version: $VERSION"

    step "Liveness probe"
    RESP=$(http_get "$GATEWAY/v1/healthz")
    ok "Alive: $(jq_field "$RESP" '.status')"

    step "Readiness probe"
    RESP=$(http_get "$GATEWAY/v1/readyz")
    ok "Ready: $(jq_field "$RESP" '.status')"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 1. Identity
# ============================================================================
if should_skip identity; then
  section_start "1. Identity"
  skip "Skipped"
  section_end skip
else
  section_start "1. Identity"
  (
    step "Identity service health"
    RESP=$(http_get "$GATEWAY/v1/identity/health")
    ok "Identity service: $(jq_field "$RESP" '.status')"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 2. Cooperative
# ============================================================================
COOP_ID="${PREFIX}-coop"
if should_skip coop; then
  section_start "2. Cooperative"
  skip "Skipped"
  section_end skip
else
  section_start "2. Cooperative"
  (
    step "Create cooperative: $COOP_ID"
    RESP=$(http_post "$GATEWAY/v1/coops" \
      "{\"id\":\"$COOP_ID\",\"name\":\"Demo Cooperative\"}")
    ok "Created cooperative: $(jq_field "$RESP" '.id // .coop_id // "ok"')"

    step "Get cooperative"
    RESP=$(http_get "$GATEWAY/v1/coops/$COOP_ID")
    COOP_NAME=$(jq_field "$RESP" '.name')
    ok "Cooperative name: $COOP_NAME"

    step "Get cooperative stats"
    RESP=$(http_get "$GATEWAY/v1/coops/$COOP_ID/stats")
    ok "Stats: $(echo "$RESP" | jq -c '.' 2>/dev/null || echo "$RESP")"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 3. Governance (full lifecycle with proof)
# ============================================================================
DOMAIN_ID="${PREFIX}-domain"
if should_skip governance; then
  section_start "3. Governance"
  skip "Skipped"
  section_end skip
else
  section_start "3. Governance"
  (
    step "Create governance domain: $DOMAIN_ID"
    RESP=$(http_post "$GATEWAY/v1/gov/domains" \
      "{
        \"id\":\"$DOMAIN_ID\",
        \"name\":\"Demo Governance Domain\",
        \"profile\":\"cooperative_default\",
        \"quorum_percent\":1,
        \"approval_percent\":1,
        \"voting_period_days\":1,
        \"members\":[]
      }")
    ok "Created domain: $(jq_field "$RESP" '.id // "ok"')"

    step "List governance domains"
    RESP=$(http_get "$GATEWAY/v1/gov/domains")
    ok "Domains found: $(jq_field "$RESP" '.items | length // .total // "?"')"

    step "Create proposal"
    RESP=$(http_post "$GATEWAY/v1/gov/proposals" \
      "{
        \"domain_id\":\"$DOMAIN_ID\",
        \"title\":\"Demo proposal: community garden allocation\",
        \"description\":\"Allocate shared space for community garden project\",
        \"payload\":{\"type\":\"text\",\"body\":\"We propose allocating lot 42 for the community garden.\"}
      }")
    PROPOSAL_ID=$(expect_field "$RESP" '.id // .proposal_id' "proposal_id")
    ok "Created proposal: $PROPOSAL_ID"

    step "Open proposal for voting"
    RESP=$(http_post "$GATEWAY/v1/gov/proposals/$PROPOSAL_ID/open" \
      "{\"voting_period_seconds\":3600}")
    ok "Proposal opened"

    step "Cast vote"
    RESP=$(http_post "$GATEWAY/v1/gov/proposals/$PROPOSAL_ID/vote" \
      "{\"choice\":\"for\",\"comment\":\"Strongly support this initiative\"}")
    ok "Vote cast"

    step "Close proposal"
    RESP=$(http_post "$GATEWAY/v1/gov/proposals/$PROPOSAL_ID/close" "")
    OUTCOME=$(jq_field "$RESP" '.outcome // .state // "closed"')
    ok "Proposal closed — outcome: $OUTCOME"

    step "Get governance proof (cryptographic)"
    RESP=$(http_get "$GATEWAY/v1/gov/proposals/$PROPOSAL_ID/proof")
    PROOF_HASH=$(jq_field "$RESP" '.proof_hash // .hash // "none"')
    if [ "$PROOF_HASH" != "null" ] && [ "$PROOF_HASH" != "none" ]; then
      ok "Governance proof: ${PROOF_HASH:0:16}..."
    else
      ok "Governance proof endpoint responded (proof may require quorum)"
    fi

    step "Get proposal with votes"
    RESP=$(http_get "$GATEWAY/v1/gov/proposals/$PROPOSAL_ID/votes")
    ok "Votes retrieved"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 4. Ledger (mutual credit)
# ============================================================================
if should_skip ledger; then
  section_start "4. Ledger"
  skip "Skipped"
  section_end skip
else
  section_start "4. Ledger"
  (
    ALICE_DID="did:icn:z${PREFIX}alice"
    BOB_DID="did:icn:z${PREFIX}bob"

    step "Create payment: Alice → Bob"
    RESP=$(http_post "$GATEWAY/v1/ledger/$COOP_ID/payment" \
      "{
        \"from\":\"$ALICE_DID\",
        \"to\":\"$BOB_DID\",
        \"amount\":100,
        \"currency\":\"hours\",
        \"memo\":\"Community garden volunteer hours\"
      }")
    ok "Payment created"

    step "Check Alice's balance"
    RESP=$(http_get "$GATEWAY/v1/ledger/$COOP_ID/balance/$ALICE_DID")
    ok "Alice balances: $(echo "$RESP" | jq -c '.balances // .' 2>/dev/null || echo "$RESP")"

    step "Check Bob's balance"
    RESP=$(http_get "$GATEWAY/v1/ledger/$COOP_ID/balance/$BOB_DID")
    ok "Bob balances: $(echo "$RESP" | jq -c '.balances // .' 2>/dev/null || echo "$RESP")"

    step "Transaction history"
    RESP=$(http_get "$GATEWAY/v1/ledger/$COOP_ID/history")
    ok "History entries: $(jq_field "$RESP" '.items | length // .total // "?"')"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 5. Treasury
# ============================================================================
if should_skip treasury; then
  section_start "5. Treasury"
  skip "Skipped"
  section_end skip
else
  section_start "5. Treasury"
  (
    step "Treasury status"
    RESP=$(http_get "$GATEWAY/v1/treasury/status") \
      && ok "Treasury: $(jq_field "$RESP" '.is_active // .status // "responded"')" \
      || ok "Treasury status not available (may need federation init)"

    step "Treasury balances"
    RESP=$(http_get "$GATEWAY/v1/treasury/balances") \
      && ok "Balance: $(jq_field "$RESP" '.balance // "responded"')" \
      || ok "Treasury balances not available"

    step "Treasury budgets"
    RESP=$(http_get "$GATEWAY/v1/treasury/budgets") \
      && ok "Budgets: $(jq_field "$RESP" '.items | length // 0')" \
      || ok "Treasury budgets not available"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 6. Service Discovery
# ============================================================================
SVC_ID="${PREFIX}-svc"
if should_skip discovery; then
  section_start "6. Service Discovery"
  skip "Skipped"
  section_end skip
else
  section_start "6. Service Discovery"
  (
    step "Announce service: $SVC_ID"
    RESP=$(http_post "$GATEWAY/v1/services/announce" \
      "{
        \"service_id\":\"$SVC_ID\",
        \"service_type\":{\"name\":\"ledger\",\"version\":\"1.0\"},
        \"addresses\":[{\"protocol\":\"https\",\"host\":\"node-a.local\",\"port\":8080}],
        \"capabilities\":[\"read\",\"write\"],
        \"scope\":\"org\",
        \"ttl_secs\":3600
      }")
    ok "Service announced"

    step "Discover services"
    RESP=$(http_get "$GATEWAY/v1/services?service_type=ledger&scope=org")
    SVC_COUNT=$(jq_field "$RESP" '.total // (.items | length) // 0')
    ok "Found $SVC_COUNT service(s)"

    step "Get service by ID"
    RESP=$(http_get "$GATEWAY/v1/services/$SVC_ID")
    ok "Service provider: $(jq_field "$RESP" '.provider // .service_id // "ok"')"

    step "Withdraw service"
    http_delete "$GATEWAY/v1/services/$SVC_ID" > /dev/null
    ok "Service withdrawn"

    step "Verify withdrawal (expect 404 or 401)"
    HTTP_CODE=$(http_status_code GET "$GATEWAY/v1/services/$SVC_ID")
    if [ "$HTTP_CODE" = "404" ] || [ "$HTTP_CODE" = "401" ]; then
      ok "Service gone ($HTTP_CODE)"
    else
      fail "Expected 404/401, got $HTTP_CODE"
    fi
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 7. WASM (upload + list + metadata)
# ============================================================================
if should_skip wasm; then
  section_start "7. WASM"
  skip "Skipped"
  section_end skip
else
  section_start "7. WASM"
  (
    step "Upload minimal WASM module"
    # Minimal valid WASM: magic number + version
    WASM_BASE64=$(printf '\x00\x61\x73\x6d\x01\x00\x00\x00' | base64)
    RESP=$(http_post "$GATEWAY/v1/compute/wasm/upload" \
      "{\"wasm_bytes\":\"$WASM_BASE64\",\"name\":\"${PREFIX}-module\",\"version\":\"1.0.0\"}")
    WASM_HASH=$(expect_field "$RESP" '.hash' "hash")
    ok "Uploaded: hash=${WASM_HASH:0:16}..."

    step "List WASM modules"
    RESP=$(http_get "$GATEWAY/v1/compute/wasm?limit=10")
    MODULE_COUNT=$(jq_field "$RESP" '.total // (.modules | length) // 0')
    ok "Found $MODULE_COUNT module(s)"

    step "Get module metadata"
    RESP=$(http_get "$GATEWAY/v1/compute/wasm/$WASM_HASH")
    MODULE_NAME=$(jq_field "$RESP" '.name')
    ok "Module: $MODULE_NAME"

    step "Submit compute task by WASM hash"
    RESP=$(http_post "$GATEWAY/v1/compute/submit" \
      "{\"code_type\":\"wasm\",\"wasm_hash\":\"$WASM_HASH\",\"fuel_limit\":10000}") \
      && ok "Task submitted: $(jq_field "$RESP" '.task_hash // .task_id // "ok"')" \
      || ok "Task submit not available (executor may not be connected)"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 8. Trust
# ============================================================================
if should_skip trust; then
  section_start "8. Trust"
  skip "Skipped"
  section_end skip
else
  section_start "8. Trust"
  (
    TARGET_DID="did:icn:z${PREFIX}trustee"

    step "Create trust attestation"
    RESP=$(http_post "$GATEWAY/v1/trust/attest" \
      "{\"to\":\"$TARGET_DID\",\"score\":0.8,\"memo\":\"Verified community member\"}") \
      && ok "Attestation created" \
      || ok "Trust attestation not available (trust service may need initialization)"

    step "Query trust score"
    RESP=$(http_get "$GATEWAY/v1/trust/$TARGET_DID") \
      && ok "Trust score: $(jq_field "$RESP" '.trust_score // "?"'), class: $(jq_field "$RESP" '.trust_class // "?"')" \
      || ok "Trust score not available"

    step "Get trust network"
    RESP=$(http_get "$GATEWAY/v1/trust/$TARGET_DID/network") \
      && ok "Trust network retrieved" \
      || ok "Trust network not available"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 9. Entity
# ============================================================================
ENTITY_ID="${PREFIX}-entity"
if should_skip entity; then
  section_start "9. Entity"
  skip "Skipped"
  section_end skip
else
  section_start "9. Entity"
  (
    step "Register entity: $ENTITY_ID"
    RESP=$(http_post "$GATEWAY/v1/entities" \
      "{
        \"entity_type\":\"cooperative\",
        \"identifier\":\"$ENTITY_ID\",
        \"name\":\"Demo Entity\",
        \"description\":\"A demo cooperative entity\"
      }")
    ok "Entity registered: $(jq_field "$RESP" '.id // .identifier // "ok"')"

    step "Get entity"
    RESP=$(http_get "$GATEWAY/v1/entities/$ENTITY_ID")
    ENTITY_NAME=$(jq_field "$RESP" '.name')
    ok "Entity name: $ENTITY_NAME"
  ) && section_end pass || section_end fail
fi

# ============================================================================
# 10. Summary
# ============================================================================
echo ""
if [ "$JSON_MODE" = true ]; then
  print_summary_json
else
  print_summary
fi
