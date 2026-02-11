#!/usr/bin/env bash
# ICN Demo Recording Script
#
# This script produces deterministic output suitable for demo recording.
# Run with: ./scripts/demo_recording.sh | tee demo_output.txt
#
# Designed for ~50 second narration:
# - 10s: What a DecisionReceipt is
# - 10s: Allocation + intents hash deterministically
# - 10s: Ledger entries queryable by decision hash
# - 10s: UI shows the chain
# - 10s: "This is government with receipts"

set -euo pipefail

cd "$(dirname "$0")/.."

# Colors for terminal output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo ""
echo -e "${BLUE}╔══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║            ICN: Government With Receipts Demo                     ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Section 1: What is a DecisionReceipt?
echo -e "${YELLOW}[1/5] What is a DecisionReceipt?${NC}"
echo ""
echo "In ICN, every governance decision produces a receipt with:"
echo "  • A canonical hash (same on every node)"
echo "  • A decision_receipt_id (node-local reference)"
echo "  • Provenance links to upstream decisions"
echo ""
echo "This receipt is the starting point for all economic effects."
echo ""
sleep 1

# Section 2: Allocation + Intents hash deterministically
echo -e "${YELLOW}[2/5] Economic intents hash deterministically${NC}"
echo ""
echo "When a decision authorizes economic activity:"
echo ""
echo "  DecisionReceipt"
echo "       │"
echo "       ▼"
echo "  AllocationReceipt (canonical hash: sorted intents)"
echo "       │"
echo "       ▼"
echo "  SettlementIntent(s) (each with canonical hash)"
echo ""

cd icn
echo "Running determinism proof..."
cargo test -p icn-kernel-api order_independent -- --nocapture 2>&1 | grep -E "^(✅|test.*ok)"
echo ""
sleep 1

# Section 3: Ledger entries queryable by decision hash
echo -e "${YELLOW}[3/5] Ledger entries are queryable by decision${NC}"
echo ""
echo "Every ledger entry from economic intents carries provenance:"
echo ""
echo "  decision_receipt_id: links to the governance decision"
echo "  decision_hash: canonical anchor for cross-node verification"
echo ""
echo "Query endpoints:"
echo "  GET /v1/receipts/chain?decision_hash=..."
echo "  GET /v1/ledger/{coop}/entries/by-decision?decision_hash=..."
echo ""
sleep 1

# Section 4: UI shows the chain
echo -e "${YELLOW}[4/5] Pilot UI visualizes the chain${NC}"
echo ""
echo "In the Pilot UI Receipts tab:"
echo ""
echo "  1. Enter decision hash (64 hex chars)"
echo "  2. Click 'Query Chain'"
echo "  3. View:"
echo "     • Allocation receipts with ✓ canonicalized badge"
echo "     • Settlement intents with amounts/accounts"
echo "     • Ledger entries with provenance links"
echo "  4. Click 'Copy' for bug report summary"
echo ""
sleep 1

# Section 5: This is government with receipts
echo -e "${YELLOW}[5/5] This is government with receipts${NC}"
echo ""
echo "ICN's promise:"
echo ""
echo -e "  ${GREEN}✓${NC} Same inputs → same canonical hashes (cross-node)"
echo -e "  ${GREEN}✓${NC} Intent order doesn't affect AllocationReceipt hash"
echo -e "  ${GREEN}✓${NC} Every ledger entry links back to its decision"
echo -e "  ${GREEN}✓${NC} Anyone can verify the chain independently"
echo ""
echo "This isn't 'trust us' governance."
echo "This is 'verify it yourself' governance."
echo ""

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║         ICN: Executable Consent with Provenance                   ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════════╝${NC}"
echo ""

echo "Demo complete. Run './scripts/pilot_receipt_chain_demo.sh' for full verification."
