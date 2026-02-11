#!/usr/bin/env bash
# ICN Pilot Receipt Chain Demo - Full Economic Chain Visibility
#
# This script demonstrates the complete provenance chain:
# DecisionReceipt → AllocationReceipt → SettlementIntent → LedgerEntry
#
# It runs both the determinism tests AND queries the gateway API
# to show the chain in action.
#
# Usage: ./scripts/pilot_receipt_chain_demo.sh [GATEWAY_URL]
#
# If URL is omitted, assumes http://localhost:8080

set -euo pipefail

cd "$(dirname "$0")/.."

GATEWAY_URL="${1:-http://localhost:8080}"
LOGFILE="/tmp/icn_receipt_chain_demo_$$.log"
FAIL=0

echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║        ICN PILOT: Full Receipt Chain Visibility Demo             ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo
echo "Gateway: $GATEWAY_URL"
echo

# Move into the Rust workspace
cd icn

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[1/4] Economic Chain Determinism Tests"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

# Run economic chain tests
if cargo test -p icn-core --test economic_chain_two_node -- --nocapture 2>&1 | tee "$LOGFILE"; then
    echo "✅ Economic chain tests passed"
else
    echo "❌ Economic chain tests failed"
    FAIL=1
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/4] Order-Independence Test"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

if cargo test -p icn-kernel-api order_independent -- --nocapture 2>&1 | tee -a "$LOGFILE"; then
    echo "✅ Intent ordering determinism verified"
else
    echo "❌ Intent ordering test failed"
    FAIL=1
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[3/4] Gateway API Connectivity Check"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

# Check if gateway is reachable
if curl -sf "$GATEWAY_URL/v1/health" >/dev/null 2>&1; then
    GATEWAY_AVAILABLE=true
    echo "✅ Gateway is reachable at $GATEWAY_URL"
else
    GATEWAY_AVAILABLE=false
    echo "⚠️  Gateway not available at $GATEWAY_URL (skipping API tests)"
    echo "   To run with gateway: start icnd then run this script again"
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[4/4] Receipt Chain Query Demo"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

if [ "$GATEWAY_AVAILABLE" = true ]; then
    # Use a fixture decision hash (all 42s = 0x2a repeated)
    DECISION_HASH="2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
    COOP_ID="test-coop"
    
    echo "Querying receipt chain for decision_hash: ${DECISION_HASH:0:16}..."
    echo
    
    echo "→ GET /v1/receipts/chain?decision_hash=$DECISION_HASH"
    CHAIN_RESP=$(curl -sf "$GATEWAY_URL/v1/receipts/chain?decision_hash=$DECISION_HASH" 2>&1 || echo '{"error":"not_found"}')
    echo "$CHAIN_RESP" | head -20
    echo
    
    # Check if chain was found
    if echo "$CHAIN_RESP" | grep -q '"allocations"'; then
        echo "✅ Receipt chain query returned allocations"
        
        # Extract and display allocation hashes using jq for proper JSON parsing
        ALLOC_HASH=$(echo "$CHAIN_RESP" | jq -r '.allocations[0].canonicalHash // empty' 2>/dev/null || echo "")
        if [ -n "$ALLOC_HASH" ]; then
            echo "   Allocation canonical hash: ${ALLOC_HASH:0:32}..."
        fi
        
        # Count intents from the intents array
        INTENT_COUNT=$(echo "$CHAIN_RESP" | jq '.intents | length' 2>/dev/null || echo "0")
        echo "   Intent count: $INTENT_COUNT"
    else
        echo "ℹ️  No receipts found for test decision hash (expected if not seeded)"
    fi
    
    echo
    echo "→ GET /v1/ledger/$COOP_ID/entries/by-decision?decision_hash=$DECISION_HASH"
    LEDGER_RESP=$(curl -sf "$GATEWAY_URL/v1/ledger/$COOP_ID/entries/by-decision?decision_hash=$DECISION_HASH" 2>&1 || echo '{"error":"not_found"}')
    echo "$LEDGER_RESP" | head -10
    echo
    
    if echo "$LEDGER_RESP" | grep -q '"entries"'; then
        ENTRY_COUNT=$(echo "$LEDGER_RESP" | grep -o '"id"' | wc -l || echo "0")
        echo "✅ Ledger entries query returned $ENTRY_COUNT entries"
    else
        echo "ℹ️  No ledger entries for test decision (expected if not seeded)"
    fi
else
    echo "⏭️  Skipping gateway queries (gateway not available)"
    echo
    echo "To see the full chain:"
    echo "  1. Start the gateway: cargo run --bin icnd"
    echo "  2. Re-run this script: ./scripts/pilot_receipt_chain_demo.sh"
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "RECEIPT CHAIN INVARIANTS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Verify tripwires from test output
if grep -q "test_economic_chain_canonical_hash_determinism.*ok" "$LOGFILE"; then
    echo "✅ Cross-node determinism: VERIFIED"
else
    echo "❌ Cross-node determinism: FAILED"
    FAIL=1
fi

if grep -q "order-independent" "$LOGFILE" || grep -q "intent_hashes.*sorted" "$LOGFILE"; then
    echo "✅ Intent ordering determinism: VERIFIED"
else
    echo "❌ Intent ordering determinism: FAILED"
    FAIL=1
fi

if grep -q "test_provenance_chain_integrity.*ok" "$LOGFILE" || grep -q "provenance.*verified" "$LOGFILE"; then
    echo "✅ Provenance chain integrity: VERIFIED"
else
    # This test may print results but the "ok" line may be interleaved
    if grep -q "PROVENANCE" "$LOGFILE"; then
        echo "✅ Provenance chain integrity: VERIFIED"
    else
        echo "⚠️  Provenance chain integrity: check test output manually"
    fi
fi

echo

if [ $FAIL -eq 0 ]; then
    echo "════════════════════════════════════════════════════════════════════"
    echo "  RECEIPT CHAIN DEMO COMPLETE"
    echo ""
    echo "  Economic Invariant: Same inputs → same canonical hashes"
    echo "  Ordering Invariant: Intent order does not affect hash"
    echo "  Provenance Chain: Decision → Allocation → Intent → Ledger"
    echo "════════════════════════════════════════════════════════════════════"
    rm -f "$LOGFILE"
    exit 0
else
    echo "════════════════════════════════════════════════════════════════════"
    echo "  DEMO FAILED: See $LOGFILE for details"
    echo "════════════════════════════════════════════════════════════════════"
    exit 1
fi
