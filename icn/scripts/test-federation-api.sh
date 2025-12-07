#!/bin/bash
# Federation API Test Script
# Usage: ./test-federation-api.sh [host] [token]
#
# Prerequisites:
# 1. Gateway running: icnd --gateway-enable --gateway-bind 127.0.0.1:8080 --gateway-jwt-secret "test-secret"
# 2. JWT token from /v1/auth/verify flow

set -e

HOST="${1:-http://localhost:8080}"
TOKEN="${2:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() { echo -e "${GREEN}PASS${NC} $1"; }
fail() { echo -e "${RED}FAIL${NC} $1"; exit 1; }
skip() { echo -e "${YELLOW}SKIP${NC} $1"; }

echo "=================================="
echo "Federation API Test Suite"
echo "Host: $HOST"
echo "=================================="
echo ""

# Health check (no auth)
echo "Testing: GET /v1/health"
response=$(curl -s -w "\n%{http_code}" "$HOST/v1/health")
code=$(echo "$response" | tail -n1)
body=$(echo "$response" | head -n-1)
if [ "$code" = "200" ]; then
    pass "Health check"
else
    fail "Health check returned $code"
fi

# If no token provided, skip authenticated tests
if [ -z "$TOKEN" ]; then
    skip "Authenticated tests (no token provided)"
    echo ""
    echo "To run authenticated tests:"
    echo "  1. Get a challenge: curl -X POST $HOST/v1/auth/challenge -H 'Content-Type: application/json' -d '{\"did\":\"your-did\"}'"
    echo "  2. Sign the challenge with your DID key"
    echo "  3. Verify: curl -X POST $HOST/v1/auth/verify -H 'Content-Type: application/json' -d '{...}'"
    echo "  4. Re-run: $0 $HOST <token>"
    exit 0
fi

AUTH="Authorization: Bearer $TOKEN"

# Federation status
echo "Testing: GET /v1/federation/status"
response=$(curl -s -w "\n%{http_code}" -H "$AUTH" "$HOST/v1/federation/status")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ]; then
    pass "Federation status"
else
    fail "Federation status returned $code"
fi

# Initialize federation
echo "Testing: POST /v1/federation/init"
response=$(curl -s -w "\n%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"coop_id":"test-coop","name":"Test Cooperative","gateway_endpoint":"http://localhost:8080"}' \
    "$HOST/v1/federation/init")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ] || [ "$code" = "400" ]; then  # 400 if already initialized
    pass "Federation init"
else
    fail "Federation init returned $code"
fi

# List cooperatives
echo "Testing: GET /v1/federation/coops"
response=$(curl -s -w "\n%{http_code}" -H "$AUTH" "$HOST/v1/federation/coops")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ]; then
    pass "List cooperatives"
else
    fail "List cooperatives returned $code"
fi

# Register cooperative
echo "Testing: POST /v1/federation/coops"
response=$(curl -s -w "\n%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"coop_id":"partner-coop","name":"Partner Cooperative","public_did":"did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK","gateway_endpoints":["http://partner.example.com:8080"],"capabilities":["compute"]}' \
    "$HOST/v1/federation/coops")
code=$(echo "$response" | tail -n1)
if [ "$code" = "201" ] || [ "$code" = "400" ]; then  # 400 if already exists
    pass "Register cooperative"
else
    fail "Register cooperative returned $code"
fi

# Get specific cooperative
echo "Testing: GET /v1/federation/coops/{id}"
response=$(curl -s -w "\n%{http_code}" -H "$AUTH" "$HOST/v1/federation/coops/partner-coop")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ] || [ "$code" = "404" ]; then
    pass "Get cooperative"
else
    fail "Get cooperative returned $code"
fi

# Get vouches
echo "Testing: GET /v1/federation/coops/{id}/vouches"
response=$(curl -s -w "\n%{http_code}" -H "$AUTH" "$HOST/v1/federation/coops/partner-coop/vouches")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ]; then
    pass "Get vouches"
else
    fail "Get vouches returned $code"
fi

# Issue vouch with trust score
echo "Testing: POST /v1/federation/coops/{id}/vouch"
response=$(curl -s -w "\n%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"target_coop_id":"partner-coop","trust_score":0.75,"expires_in_days":365}' \
    "$HOST/v1/federation/coops/partner-coop/vouch")
code=$(echo "$response" | tail -n1)
body=$(echo "$response" | head -n-1)
if [ "$code" = "201" ]; then
    # Verify trust score is in response
    if echo "$body" | grep -q '"trust_score":0.75'; then
        pass "Issue vouch (trust_score verified)"
    else
        fail "Issue vouch missing trust_score in response"
    fi
else
    fail "Issue vouch returned $code"
fi

# Issue attestation
echo "Testing: POST /v1/federation/attestations"
response=$(curl -s -w "\n%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"member_did":"did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK","trust_score":0.85,"context":"economic","validity_days":90}' \
    "$HOST/v1/federation/attestations")
code=$(echo "$response" | tail -n1)
if [ "$code" = "201" ]; then
    pass "Issue attestation"
else
    fail "Issue attestation returned $code"
fi

# Get attestations
echo "Testing: GET /v1/federation/attestations/{did}"
response=$(curl -s -w "\n%{http_code}" -H "$AUTH" \
    "$HOST/v1/federation/attestations/did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ]; then
    pass "Get attestations"
else
    fail "Get attestations returned $code"
fi

# List clearing agreements
echo "Testing: GET /v1/federation/clearing"
response=$(curl -s -w "\n%{http_code}" -H "$AUTH" "$HOST/v1/federation/clearing")
code=$(echo "$response" | tail -n1)
if [ "$code" = "200" ]; then
    pass "List clearing agreements"
else
    fail "List clearing agreements returned $code"
fi

# Create clearing agreement
echo "Testing: POST /v1/federation/clearing"
response=$(curl -s -w "\n%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"agreement_id":"test-partner-001","partner_coop_id":"partner-coop","partner_did":"did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK","max_imbalance":10000,"settlement":"weekly"}' \
    "$HOST/v1/federation/clearing")
code=$(echo "$response" | tail -n1)
if [ "$code" = "201" ] || [ "$code" = "400" ]; then  # 400 if already exists
    pass "Create clearing agreement"
else
    fail "Create clearing agreement returned $code"
fi

# Error case: Invalid trust score
echo "Testing: Invalid trust score (should fail)"
response=$(curl -s -w "\n%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
    -d '{"target_coop_id":"partner-coop","trust_score":1.5,"expires_in_days":365}' \
    "$HOST/v1/federation/coops/partner-coop/vouch")
code=$(echo "$response" | tail -n1)
if [ "$code" = "400" ]; then
    pass "Invalid trust score rejected"
else
    fail "Invalid trust score should return 400, got $code"
fi

# Error case: Missing auth
echo "Testing: Missing auth (should fail)"
response=$(curl -s -w "\n%{http_code}" "$HOST/v1/federation/status")
code=$(echo "$response" | tail -n1)
if [ "$code" = "401" ]; then
    pass "Missing auth rejected"
else
    fail "Missing auth should return 401, got $code"
fi

echo ""
echo "=================================="
echo -e "${GREEN}All tests passed!${NC}"
echo "=================================="
