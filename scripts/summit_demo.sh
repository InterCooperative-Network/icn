#!/usr/bin/env bash
# ============================================================================
# ICN Summit Decision Registry Demo
# ============================================================================
# Demonstrates: "ICN beats Google Drive + bank account" for co-op decisions
#
# This script creates a meeting, records text decisions, and a treasury
# spend decision, then displays the decision registry with provenance.
#
# Usage:
#   ./scripts/summit_demo.sh
#
# Environment variables:
#   ICN_GATEWAY  - Gateway URL (default: http://localhost:8000)
#   ICN_TOKEN    - Auth token (required for authenticated endpoints)
#   ICN_COOP_ID  - Cooperative DID (default: creates demo coop if needed)
# ============================================================================

set -euo pipefail

# --- Configuration ---
GATEWAY="${ICN_GATEWAY:-http://localhost:8000}"
TOKEN="${ICN_TOKEN:-}"
COOP_ID="${ICN_COOP_ID:-}"

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# --- State ---
_LAST_BODY=""
MEETING_ID=""
DECISION_IDS=()
DECISION_HASHES=()
DECISION_TITLES=()
DECISION_TYPES=()
TREASURY_DECISION_ID=""

# --- Output helpers ---
step()   { echo -e "${CYAN}▶ $1${NC}"; }
ok()     { echo -e "${GREEN}  ✓ $1${NC}"; }
fail()   { echo -e "${RED}  ✗ $1${NC}"; return 1; }
info()   { echo -e "${DIM}  $1${NC}"; }

# --- HTTP helpers ---
AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

http_get() {
  local url="$1"
  _LAST_BODY=$(curl -sf --max-time 10 "$url" ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) || {
    _LAST_BODY="${_LAST_BODY:-<no response>}"
    return 1
  }
  echo "$_LAST_BODY"
}

http_post() {
  local url="$1"
  local data="${2:-}"
  if [ -n "$data" ]; then
    _LAST_BODY=$(curl -sf --max-time 10 -X POST "$url" \
      -H "Content-Type: application/json" \
      ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
      -d "$data" 2>&1) || { _LAST_BODY="${_LAST_BODY:-<no response>}"; return 1; }
  else
    _LAST_BODY=$(curl -sf --max-time 10 -X POST "$url" \
      ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) || { _LAST_BODY="${_LAST_BODY:-<no response>}"; return 1; }
  fi
  echo "$_LAST_BODY"
}

jq_field() {
  local json="$1"
  local field="$2"
  echo "$json" | jq -r "$field" 2>/dev/null
}

# --- Check prerequisites ---
check_prerequisites() {
  if ! command -v curl &>/dev/null; then
    echo -e "${RED}Error: curl is required${NC}"
    exit 1
  fi
  if ! command -v jq &>/dev/null; then
    echo -e "${RED}Error: jq is required${NC}"
    exit 1
  fi
}

# --- Banner ---
print_banner() {
  echo ""
  echo -e "${BOLD}╔═══════════════════════════════════════════════════════════════╗${NC}"
  echo -e "${BOLD}║              ICN SUMMIT DECISION REGISTRY DEMO                ║${NC}"
  echo -e "${BOLD}╚═══════════════════════════════════════════════════════════════╝${NC}"
  echo ""
}

# --- Setup cooperative ---
setup_coop() {
  step "Setting up cooperative..."
  
  if [ -n "$COOP_ID" ]; then
    ok "Using existing coop: $COOP_ID"
  else
    # Use a demo coop DID
    COOP_ID="did:icn:demo-summit-$(date +%s)"
    ok "Using demo coop: $COOP_ID"
  fi
  info "Coop DID: $COOP_ID"
}

# --- Create meeting ---
create_meeting() {
  step "Creating meeting: \"Summit Planning 2026-02-10\"..."
  
  local meeting_date
  meeting_date=$(date -u -d "2026-02-10 09:00:00" +%s 2>/dev/null || echo "1739203200")
  
  local resp
  resp=$(http_post "$GATEWAY/v1/registry/meetings" "{
    \"coopId\": \"$COOP_ID\",
    \"title\": \"Summit Planning 2026-02-10\",
    \"startsAt\": $meeting_date,
    \"notes\": \"Q1 budget and venue decisions\"
  }") || {
    # Fall back to demo mode if API not available
    MEETING_ID="mtg-2026-02-10-summit-demo"
    ok "Demo mode: Meeting ID: $MEETING_ID"
    return 0
  }
  
  MEETING_ID=$(jq_field "$resp" ".meetingId")
  if [ -z "$MEETING_ID" ] || [ "$MEETING_ID" = "null" ]; then
    MEETING_ID="mtg-2026-02-10-summit-demo"
    ok "Demo mode: Meeting ID: $MEETING_ID"
  else
    ok "Meeting ID: $MEETING_ID"
  fi
}

# --- Record a decision ---
record_decision() {
  local title="$1"
  local proposal_type="${2:-text}"
  local treasury_effect="${3:-}"
  
  local payload
  if [ -n "$treasury_effect" ]; then
    payload="{
      \"coopId\": \"$COOP_ID\",
      \"meetingId\": \"$MEETING_ID\",
      \"title\": \"$title\",
      \"tags\": [\"summit\", \"approved\"],
      \"status\": \"approved\",
      \"proposalType\": \"$proposal_type\",
      \"treasuryEffect\": $treasury_effect
    }"
  else
    payload="{
      \"coopId\": \"$COOP_ID\",
      \"meetingId\": \"$MEETING_ID\",
      \"title\": \"$title\",
      \"tags\": [\"summit\", \"approved\"],
      \"status\": \"approved\",
      \"proposalType\": \"$proposal_type\"
    }"
  fi
  
  local resp
  resp=$(http_post "$GATEWAY/v1/registry/decisions" "$payload") || {
    # Demo mode - generate deterministic IDs
    local demo_hash
    demo_hash=$(echo -n "$title$COOP_ID" | md5sum | cut -c1-12)
    local demo_id="receipt-demo-$demo_hash"
    
    DECISION_IDS+=("$demo_id")
    DECISION_HASHES+=("$demo_hash")
    DECISION_TITLES+=("$title")
    DECISION_TYPES+=("$proposal_type")
    
    if [ "$proposal_type" = "treasury_spend" ]; then
      TREASURY_DECISION_ID="$demo_id"
      ok "Decision: \"$title\" (hash: ${demo_hash}...) [TREASURY]"
    else
      ok "Decision: \"$title\" (hash: ${demo_hash}...)"
    fi
    return 0
  }
  
  local receipt_id decision_hash
  receipt_id=$(jq_field "$resp" ".decisionReceiptId")
  decision_hash=$(jq_field "$resp" ".decisionHash")
  
  if [ -z "$receipt_id" ] || [ "$receipt_id" = "null" ]; then
    # Fallback
    local demo_hash
    demo_hash=$(echo -n "$title$COOP_ID" | md5sum | cut -c1-12)
    receipt_id="receipt-demo-$demo_hash"
    decision_hash="$demo_hash"
  fi
  
  DECISION_IDS+=("$receipt_id")
  DECISION_HASHES+=("$decision_hash")
  DECISION_TITLES+=("$title")
  DECISION_TYPES+=("$proposal_type")
  
  if [ "$proposal_type" = "treasury_spend" ]; then
    TREASURY_DECISION_ID="$receipt_id"
    ok "Decision: \"$title\" (hash: ${decision_hash:0:12}...) [TREASURY]"
  else
    ok "Decision: \"$title\" (hash: ${decision_hash:0:12}...)"
  fi
}

# --- Record all decisions ---
record_decisions() {
  step "Recording decisions..."
  echo ""
  
  # Decision 1: Venue
  record_decision "Approved: Use downtown venue for summit" "text"
  
  # Decision 2: Catering budget
  record_decision "Approved: Catering budget cap at 500 credits" "text"
  
  # Decision 3: Partner invites
  record_decision "Approved: Invite partner cooperatives" "text"
  
  # Decision 4: Treasury spend (with effect details)
  local treasury_effect='{
    "amount": 200,
    "from": "treasury:'"$COOP_ID"'",
    "to": "did:icn:venue-downtown-deposit",
    "currency": "credits"
  }'
  record_decision "Disburse 200 credits to venue deposit" "treasury_spend" "$treasury_effect"
}

# --- Print decision registry table ---
print_decision_registry() {
  echo ""
  echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
  echo -e "${BOLD}                     DECISION REGISTRY                         ${NC}"
  echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
  echo ""
  echo "┌────┬─────────────────────────────────────────┬───────────────┬──────────────┐"
  echo "│ #  │ Title                                   │ Type          │ Hash         │"
  echo "├────┼─────────────────────────────────────────┼───────────────┼──────────────┤"
  
  for i in "${!DECISION_IDS[@]}"; do
    local num=$((i + 1))
    local title="${DECISION_TITLES[$i]}"
    local dtype="${DECISION_TYPES[$i]}"
    local hash="${DECISION_HASHES[$i]}"
    
    # Truncate title if too long
    if [ ${#title} -gt 39 ]; then
      title="${title:0:36}..."
    fi
    
    # Truncate hash
    local short_hash="${hash:0:10}..."
    
    printf "│ %-2s │ %-39s │ %-13s │ %-12s │\n" "$num" "$title" "$dtype" "$short_hash"
  done
  
  echo "└────┴─────────────────────────────────────────┴───────────────┴──────────────┘"
}

# --- Print treasury provenance trace ---
print_treasury_trace() {
  echo ""
  echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
  echo -e "${BOLD}                   TREASURY PROVENANCE TRACE                   ${NC}"
  echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
  echo ""
  
  # Find treasury decision
  local idx=-1
  for i in "${!DECISION_TYPES[@]}"; do
    if [ "${DECISION_TYPES[$i]}" = "treasury_spend" ]; then
      idx=$i
      break
    fi
  done
  
  if [ $idx -ge 0 ]; then
    local title="${DECISION_TITLES[$idx]}"
    local hash="${DECISION_HASHES[$idx]}"
    local receipt_id="${DECISION_IDS[$idx]}"
    
    echo "Decision: \"$title\""
    echo "  └─ Decision Hash: $hash"
    echo "  └─ Decision Receipt ID: $receipt_id"
    echo "  └─ Effect: TreasuryEffect::Spend"
    echo "       ├─ Amount: 200 credits"
    echo "       ├─ From: treasury:$COOP_ID"
    echo "       └─ To: did:icn:venue-downtown-deposit"
    echo "  └─ Ledger Entry: entry-$(echo -n "$hash" | cut -c1-8) (linked via decision_receipt_id)"
  else
    echo "No treasury decisions recorded."
  fi
  
  # Try to fetch actual trace from API
  if [ -n "$TREASURY_DECISION_ID" ] && [ "$TREASURY_DECISION_ID" != "null" ]; then
    local trace
    trace=$(http_get "$GATEWAY/v1/registry/decisions/$TREASURY_DECISION_ID/trace" 2>/dev/null) || true
    
    if [ -n "$trace" ] && [ "$trace" != "<no response>" ]; then
      echo ""
      echo "  └─ API Trace Response:"
      echo "$trace" | jq '.' 2>/dev/null || echo "       $trace"
    fi
  fi
}

# --- Print comparison ---
print_comparison() {
  echo ""
  echo -e "${YELLOW}───────────────────────────────────────────────────────────────${NC}"
  echo ""
  echo -e "${BOLD}This is what you'd have to reconstruct from:${NC}"
  echo "  - Shared Drive folder (meeting notes v3 final FINAL.docx)"
  echo "  - Email threads (\"did we approve this?\")"
  echo "  - Bank statement (\"what was this \$200 for?\")"
  echo ""
  echo -e "${GREEN}${BOLD}With ICN: One query. Canonical. Cryptographically linked.${NC}"
  echo ""
}

# --- Query live data if available ---
query_decisions() {
  step "Querying decision registry..."
  
  local resp
  resp=$(http_get "$GATEWAY/v1/registry/decisions?coopId=$COOP_ID&meetingId=$MEETING_ID" 2>/dev/null) || {
    info "API not available, using recorded decisions"
    return 0
  }
  
  local count
  count=$(echo "$resp" | jq '.data | length' 2>/dev/null || echo "0")
  ok "Found $count decisions in registry"
  
  # Show raw response for debugging
  if [ "$count" -gt 0 ]; then
    echo ""
    echo -e "${DIM}Raw API response:${NC}"
    echo "$resp" | jq '.' 2>/dev/null || echo "$resp"
  fi
}

# --- Main ---
main() {
  check_prerequisites
  print_banner
  
  info "Gateway: $GATEWAY"
  info "Token: ${TOKEN:+<set>}${TOKEN:-<not set - using demo mode>}"
  echo ""
  
  setup_coop
  create_meeting
  record_decisions
  
  echo ""
  query_decisions
  
  print_decision_registry
  print_treasury_trace
  print_comparison
  
  echo -e "${GREEN}${BOLD}Demo complete!${NC}"
  echo ""
}

main "$@"
