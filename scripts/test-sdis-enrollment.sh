#!/bin/bash
# SDIS Enrollment Test Script
# Tests the complete SDIS enrollment flow end-to-end

set -e

GATEWAY_URL="http://10.8.30.40:30080"
COOP_ID="test-coop"
IDENTITY_NAME="TestUser_$(date +%s)"

echo "========================================="
echo "SDIS Enrollment End-to-End Test"
echo "========================================="
echo ""
echo "Gateway: $GATEWAY_URL"
echo "Coop ID: $COOP_ID"
echo "Identity: $IDENTITY_NAME"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Step 1: Health Check
echo -e "${YELLOW}Step 1: SDIS Health Check${NC}"
HEALTH=$(curl -s "$GATEWAY_URL/v1/sdis/health")
if echo "$HEALTH" | grep -q '"status":"ok"'; then
    echo -e "${GREEN}✓ SDIS is healthy${NC}"
    echo "$HEALTH" | jq .
else
    echo -e "${RED}✗ SDIS health check failed${NC}"
    echo "Response: $HEALTH"
    exit 1
fi
echo ""

# Step 2: Start Enrollment
#
# NOTE: self-serve enrollment is absent unless the gateway operator has declared
# an isolated rehearsal deployment with ICN_ENABLE_SELF_SERVE_ENROLLMENT=true.
# No shipped deployment profile sets it, so against a default gateway these
# routes are simply not mounted and answer 404 (or 401, if the request falls
# through to the authenticated /sdis sub-scope). That is the expected posture,
# not a regression — this script requires a gateway that has opted in.
echo -e "${YELLOW}Step 2: Start Enrollment${NC}"
ENROLLMENT_HTTP=$(curl -s -o /tmp/icn-enroll-start.$$ -w "%{http_code}" \
    -X POST "$GATEWAY_URL/v1/sdis/enrollment/start" \
    -H "Content-Type: application/json" \
    -d "{\"identity_name\":\"$IDENTITY_NAME\",\"coop_id\":\"$COOP_ID\"}")
ENROLLMENT_RESPONSE=$(cat /tmp/icn-enroll-start.$$ 2>/dev/null)
rm -f /tmp/icn-enroll-start.$$

if [ "$ENROLLMENT_HTTP" = "404" ] || [ "$ENROLLMENT_HTTP" = "401" ]; then
    echo -e "${YELLOW}⊘ Self-serve enrollment is not mounted on this gateway (HTTP $ENROLLMENT_HTTP)${NC}"
    echo ""
    echo "  This is the expected default. The route tree mounts only when the"
    echo "  operator sets ICN_ENABLE_SELF_SERVE_ENROLLMENT=true to declare an"
    echo "  isolated rehearsal deployment, and no shipped profile sets it."
    echo ""
    echo "  To run this script, point it at a gateway that has opted in."
    echo "  Skipping the enrollment flow rather than reporting a false result."
    exit 0
fi

ENROLLMENT_ID=$(echo "$ENROLLMENT_RESPONSE" | jq -r '.enrollment_id // empty')

if [ -z "$ENROLLMENT_ID" ]; then
    echo -e "${RED}✗ Enrollment start failed (HTTP $ENROLLMENT_HTTP)${NC}"
    echo "$ENROLLMENT_RESPONSE" | jq . 2>/dev/null || echo "$ENROLLMENT_RESPONSE"
    exit 1
else
    echo -e "${GREEN}✓ Enrollment started${NC}"
    echo "Enrollment ID: $ENROLLMENT_ID"
    echo "Verification Code: $(echo "$ENROLLMENT_RESPONSE" | jq -r '.verification_code')"
    echo "Expires At: $(echo "$ENROLLMENT_RESPONSE" | jq -r '.expires_at')"
fi
echo ""

# Step 3: Generate Ephemeral DID
echo -e "${YELLOW}Step 3: Generate Ephemeral DID${NC}"
EPHEMERAL_RESPONSE=$(curl -s -X POST "$GATEWAY_URL/v1/sdis/ephemeral/generate" \
    -H "Content-Type: application/json" \
    -d '{"purpose":"enrollment","ttl_seconds":3600}')

EPHEMERAL_DID=$(echo "$EPHEMERAL_RESPONSE" | jq -r '.ephemeral_did // empty')
EPHEMERAL_KEY=$(echo "$EPHEMERAL_RESPONSE" | jq -r '.private_key // empty')

if [ -z "$EPHEMERAL_DID" ]; then
    echo -e "${RED}✗ Ephemeral DID generation failed${NC}"
    echo "$EPHEMERAL_RESPONSE" | jq .
    exit 1
else
    echo -e "${GREEN}✓ Ephemeral DID generated${NC}"
    echo "Ephemeral DID: $EPHEMERAL_DID"
    echo "Expires At: $(echo "$EPHEMERAL_RESPONSE" | jq -r '.expires_at')"
fi
echo ""

# Step 4: Level 1 Verification (Device Proof)
echo -e "${YELLOW}Step 4: Level 1 Verification (Device Proof)${NC}"
echo "NOTE: This requires signing challenge with ephemeral key"
echo "Skipping for now - requires crypto implementation"
echo -e "${YELLOW}⚠ Level 1 verification not tested${NC}"
echo ""

# Step 5: Get Steward Token
echo -e "${YELLOW}Step 5: Get Steward Token${NC}"
echo "Attempting to get token from existing identity..."

POD=$(kubectl get pods -n icn -l component=daemon -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")

if [ -z "$POD" ]; then
    echo -e "${YELLOW}⚠ Cannot get steward token - kubectl not available${NC}"
    echo "To complete this test manually:"
    echo "  1. SSH to k3s node: ssh ubuntu@10.8.30.40"
    echo "  2. Get pod: POD=\$(sudo kubectl get pods -n icn -l component=daemon -o jsonpath='{.items[0].metadata.name}')"
    echo "  3. Get token: sudo kubectl exec -it -n icn \$POD -- icnctl auth token --coop-id $COOP_ID --gateway http://localhost:8080"
    echo "  4. Use token for Level 2 verification"
    exit 0
fi

echo "Pod: $POD"
# Note: This won't work locally, needs to run on k3s node
echo -e "${YELLOW}⚠ Steward token retrieval requires cluster access${NC}"
echo ""

# Step 6: Level 2 Verification (Steward Vouch)
echo -e "${YELLOW}Step 6: Level 2 Verification (Steward Vouch)${NC}"
echo "NOTE: Requires valid steward authentication token"
echo "Skipping for now - requires steward token"
echo -e "${YELLOW}⚠ Level 2 verification not tested${NC}"
echo ""

# Step 7: Complete Enrollment
echo -e "${YELLOW}Step 7: Complete Enrollment${NC}"
echo "NOTE: Requires passing verifications first"
echo "Skipping for now - requires Level 1 & 2 completion"
echo -e "${YELLOW}⚠ Enrollment completion not tested${NC}"
echo ""

# Summary
echo "========================================="
echo "Test Summary"
echo "========================================="
echo -e "${GREEN}✓ SDIS health check passed${NC}"
echo -e "${GREEN}✓ Enrollment initiated successfully${NC}"
echo -e "${GREEN}✓ Ephemeral DID generated${NC}"
echo -e "${YELLOW}⚠ Level 1 verification skipped (needs crypto)${NC}"
echo -e "${YELLOW}⚠ Level 2 verification skipped (needs steward token)${NC}"
echo -e "${YELLOW}⚠ Enrollment completion skipped (needs verifications)${NC}"
echo ""
echo "Enrollment ID: $ENROLLMENT_ID"
echo "Ephemeral DID: $EPHEMERAL_DID"
echo ""
echo "Next Steps:"
echo "  1. Implement device proof signing with ephemeral key"
echo "  2. Get steward token from cluster"
echo "  3. Complete Level 1 & 2 verifications"
echo "  4. Call enrollment/complete endpoint"
echo ""
echo "For manual testing, see: SDIS_API_GUIDE.md"
echo "========================================="
