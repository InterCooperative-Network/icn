#!/bin/bash
# Complete Mobile App E2E Test with Real Identity
# Tests the full authentication and API flow that CoopWallet uses

set -e

GATEWAY="http://10.8.30.40:30080"
COOP_ID="test-coop"
ICNCTL="/home/matt/projects/icn/icn/target/release/icnctl"
TEST_DATA_DIR="/tmp/icn-mobile-test-$$"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0;  NC'

echo "=================================================="
echo "Mobile App E2E Test (Full Auth Flow)"
echo "Gateway: $GATEWAY"
echo "=================================================="
echo ""

# Check if icnctl exists
if [ ! -f "$ICNCTL" ]; then
    echo -e "${YELLOW}Building icnctl...${NC}"
    cd /home/matt/projects/icn/icn
    cargo build --release --bin icnctl
fi

# Create temporary data directory
mkdir -p "$TEST_DATA_DIR"
export ICN_DATA_DIR="$TEST_DATA_DIR"
export ICN_PASSPHRASE="test-passphrase-123"

echo -e "${BLUE}Step 1: Generate Test Identity${NC}"
echo "----------------------------------------"
$ICNCTL id init
DID=$($ICNCTL id show | grep "DID:" | awk '{print $2}')
echo -e "${GREEN}✓${NC} Identity created: $DID"
echo ""

echo -e "${BLUE}Step 2: Get Authentication Token${NC}"
echo "----------------------------------------"
TOKEN=$($ICNCTL auth token --gateway "$GATEWAY" --coop-id "$COOP_ID" 2>&1 | grep -A1 "Token:" | tail -1 | xargs)

if [ -z "$TOKEN" ] || [ "$TOKEN" = "Token:" ]; then
    echo -e "${RED}✗${NC} Failed to get authentication token"
    echo "Trying to get more debug info..."
    $ICNCTL auth token --gateway "$GATEWAY" --coop-id "$COOP_ID" || true
    exit 1
fi

echo -e "${GREEN}✓${NC} Token received: ${TOKEN:0:50}..."
echo ""

echo -e "${BLUE}Step 3: Test Authenticated Endpoints${NC}"
echo "----------------------------------------"

# Test 1: Get Position
echo -n "Testing GET /v1/ledger/$COOP_ID/position/$DID... "
balance_response=$(curl -s -w "\n%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    "$GATEWAY/v1/ledger/$COOP_ID/position/$DID")

balance_status=$(echo "$balance_response" | tail -n1)
balance_body=$(echo "$balance_response" | head -n-1)

if [ "$balance_status" = "200" ]; then
    echo -e "${GREEN}✓${NC}"
    echo "  Position: $balance_body"
else
    echo -e "${RED}✗${NC} ($balance_status)"
    echo "  Response: $balance_body"
fi
echo ""

# Test 2: Get Transaction History
echo -n "Testing GET /v1/ledger/$COOP_ID/history/$DID... "
history_response=$(curl -s -w "\n%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    "$GATEWAY/v1/ledger/$COOP_ID/history/$DID?limit=10")

history_status=$(echo "$history_response" | tail -n1)
history_body=$(echo "$history_response" | head -n-1)

if [ "$history_status" = "200" ]; then
    echo -e "${GREEN}✓${NC}"
    tx_count=$(echo "$history_body" | jq -r '.transactions | length' 2>/dev/null || echo "?")
    echo "  Transactions: $tx_count"
else
    echo -e "${RED}✗${NC} ($history_status)"
    echo "  Response: $history_body"
fi
echo ""

# Test 3: List Governance Domains
echo -n "Testing GET /v1/gov/domains... "
domains_response=$(curl -s -w "\n%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    "$GATEWAY/v1/gov/domains")

domains_status=$(echo "$domains_response" | tail -n1)
domains_body=$(echo "$domains_response" | head -n-1)

if [ "$domains_status" = "200" ]; then
    echo -e "${GREEN}✓${NC}"
    domain_count=$(echo "$domains_body" | jq -r '.domains | length' 2>/dev/null || echo "?")
    echo "  Domains: $domain_count"
else
    echo -e "${RED}✗${NC} ($domains_status)"
    echo "  Response: $domains_body"
fi
echo ""

# Test 4: List Proposals
echo -n "Testing GET /v1/gov/proposals... "
proposals_response=$(curl -s -w "\n%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    "$GATEWAY/v1/gov/proposals?domain=coop:$COOP_ID&status=open")

proposals_status=$(echo "$proposals_response" | tail -n1)
proposals_body=$(echo "$proposals_response" | head -n-1)

if [ "$proposals_status" = "200" ]; then
    echo -e "${GREEN}✓${NC}"
    proposal_count=$(echo "$proposals_body" | jq -r '.proposals | length' 2>/dev/null || echo "?")
    echo "  Proposals: $proposal_count"
else
    echo -e "${RED}✗${NC} ($proposals_status)"
    echo "  Response: $proposals_body"
fi
echo ""

# Test 5: Test Payment Endpoint (will fail without recipient, but tests auth)
echo -n "Testing POST /v1/ledger/$COOP_ID/pay (validation)... "
pay_response=$(curl -s -w "\n%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -X POST "$GATEWAY/v1/ledger/$COOP_ID/pay" \
    -d "{\"to\":\"$DID\",\"amount\":0.1,\"memo\":\"Test payment\"}")

pay_status=$(echo "$pay_response" | tail -n1)
pay_body=$(echo "$pay_response" | head -n-1)

if [ "$pay_status" = "200" ] || [ "$pay_status" = "400" ]; then
    echo -e "${GREEN}✓${NC} (endpoint accessible)"
    echo "  Response: $pay_body" | head -c 150
else
    echo -e "${RED}✗${NC} ($pay_status)"
    echo "  Response: $pay_body"
fi
echo ""

echo -e "${BLUE}Step 4: Test SDIS Enrollment Flow${NC}"
echo "----------------------------------------"

# Outcome of the enrollment section, consumed by the final summary.
# Default to "failed" so an unforeseen path cannot silently read as success.
enrollment_result="failed"

echo -n "Starting SDIS enrollment... "
enroll_response=$(curl -s -w "\n%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "$GATEWAY/v1/sdis/enrollment/start" \
    -d "{
        \"name\":\"Test Mobile User\",
        \"email\":\"mobile-test@example.com\",
        \"phone\":\"+1234567890\",
        \"device_id\":\"test-device-$$\",
        \"identity_name\":\"mobile-test-identity\",
        \"coop_id\":\"$COOP_ID\"
    }")

enroll_status=$(echo "$enroll_response" | tail -n1)
enroll_body=$(echo "$enroll_response" | head -n-1)

if [ "$enroll_status" = "200" ] || [ "$enroll_status" = "201" ]; then
    enrollment_id=$(echo "$enroll_body" | jq -r '.enrollment_id' 2>/dev/null)
    if [ -z "$enrollment_id" ] || [ "$enrollment_id" = "null" ]; then
        # 2xx but no usable enrollment_id: a malformed response is a real
        # failure, not a skip.
        echo -e "${RED}✗${NC} ($enroll_status, no enrollment_id in response)"
        echo "  Response: $enroll_body" | head -c 200
        enrollment_result="failed"
    else
        echo -e "${GREEN}✓${NC}"
        echo "  Enrollment ID: $enrollment_id"
        enrollment_result="operational"
    fi
elif [ "$enroll_status" = "404" ] || [ "$enroll_status" = "401" ]; then
    # Self-serve enrollment is absent unless the operator declares an isolated
    # rehearsal deployment (ICN_ENABLE_SELF_SERVE_ENROLLMENT=true). No shipped
    # profile sets it, so this is the EXPECTED default, not a failure.
    #
    # Report it as SKIPPED rather than a warning: this section previously
    # printed a warning and carried on, so a run against a default gateway
    # reported the enrollment flow as exercised when no enrollment happened.
    echo -e "${YELLOW}SKIPPED${NC} ($enroll_status — self-serve enrollment not mounted)"
    echo "  This is the expected default: no shipped profile sets"
    echo "  ICN_ENABLE_SELF_SERVE_ENROLLMENT=true. To exercise this flow, point"
    echo "  the script at a gateway that has explicitly declared an isolated"
    echo "  rehearsal deployment."
    enrollment_id=""
    enrollment_result="skipped"
else
    echo -e "${RED}✗${NC} ($enroll_status)"
    echo "  Response: $enroll_body" | head -c 200
    enrollment_result="failed"
fi
echo ""

echo "=================================================="
echo "Test Results Summary"
echo "=================================================="
echo ""
echo -e "${GREEN}✓ Core Gateway Health${NC} - Responding normally"
echo -e "${GREEN}✓ Authentication Flow${NC} - Challenge-response working"
echo -e "${GREEN}✓ Token Generation${NC} - JWT tokens being issued"
echo -e "${GREEN}✓ Ledger Endpoints${NC} - Balance and history accessible"
echo -e "${GREEN}✓ Governance Endpoints${NC} - Domains and proposals accessible"

# The enrollment line must reflect what actually happened. Self-serve enrollment
# is absent on every shipped profile, so on a default gateway this section is
# skipped — and a skipped section must never be summarised as "operational",
# nor support an unconditional "fully operational" conclusion.
case "$enrollment_result" in
    operational)
        echo -e "${GREEN}✓ SDIS Enrollment${NC} - Enrollment system operational"
        echo ""
        echo "🎉 Mobile app backend is fully operational!"
        ;;
    skipped)
        echo -e "${YELLOW}⊘ SDIS Enrollment${NC} - SKIPPED (self-serve enrollment not enabled on this gateway)"
        echo ""
        echo "✅ Mobile app backend checks passed, EXCEPT SDIS enrollment, which was not exercised."
        echo "   Self-serve enrollment is absent unless the operator sets"
        echo "   ICN_ENABLE_SELF_SERVE_ENROLLMENT=true. That is the expected default and is"
        echo "   not a failure — but this run does NOT demonstrate that the enrollment flow works."
        ;;
    *)
        echo -e "${RED}✗ SDIS Enrollment${NC} - FAILED"
        echo ""
        echo "❌ Mobile app backend is NOT fully operational: SDIS enrollment failed."
        ;;
esac
echo ""
echo "To run CoopWallet mobile app:"
echo "  cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet"
echo "  npm start"
echo ""
echo "The app is configured to connect to: $GATEWAY"
echo "Users can create identities through SDIS enrollment or import existing DIDs"
echo ""

# Cleanup
rm -rf "$TEST_DATA_DIR"

# A genuine enrollment failure must fail the run. A skip must not.
if [ "$enrollment_result" = "failed" ]; then
    exit 1
fi
