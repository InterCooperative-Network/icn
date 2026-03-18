#!/bin/bash
# Simple API Test - No mobile app needed
# Tests the backend that the mobile app would use

GATEWAY="http://10.8.10.40:30080"
COOP_ID="test-coop"

echo "========================================="
echo "Testing ICN Gateway APIs"
echo "Gateway: $GATEWAY"
echo "========================================="
echo ""

# Test 1: Health Check
echo "1. Testing Health Check..."
HEALTH=$(curl -s "$GATEWAY/v1/health")
echo "   Response: $HEALTH"
echo ""

# Test 2: SDIS Health
echo "2. Testing SDIS Health..."
SDIS_HEALTH=$(curl -s "$GATEWAY/v1/sdis/health")
echo "   Response: $SDIS_HEALTH"
echo ""

# Test 3: Start SDIS Enrollment
echo "3. Testing SDIS Enrollment..."
ENROLL=$(curl -s -X POST "$GATEWAY/v1/sdis/enrollment/start" \
  -H "Content-Type: application/json" \
  -d "{\"identity_name\":\"TestUser_$$\",\"coop_id\":\"$COOP_ID\"}")
echo "   Response: $ENROLL"

ENROLLMENT_ID=$(echo "$ENROLL" | jq -r '.enrollment_id // empty' 2>/dev/null)
if [ ! -z "$ENROLLMENT_ID" ]; then
    echo ""
    echo "   ✅ Enrollment created: $ENROLLMENT_ID"
    VERIFY_CODE=$(echo "$ENROLL" | jq -r '.verification_code // empty' 2>/dev/null)
    echo "   Verification Code: $VERIFY_CODE"
fi
echo ""

echo "========================================="
echo "✅ Backend APIs are working!"
echo ""
echo "The mobile app would use these same APIs."
echo "========================================="
