#!/bin/bash
# Deployment Verification Script for ICN Invite System

set -e

GATEWAY_URL="${GATEWAY_URL:-http://localhost:8080}"
UI_URL="${UI_URL:-http://localhost:3000}"

echo "🔍 ICN Invite System Deployment Verification"
echo "============================================="
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

check_test() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $1"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗${NC} $1"
        ((TESTS_FAILED++))
    fi
}

echo "1. Checking Gateway Health..."
curl -s -f "${GATEWAY_URL}/v1/health" > /dev/null
check_test "Gateway health endpoint"

echo ""
echo "2. Checking Gateway API..."
curl -s -f "${GATEWAY_URL}/v1/health" | grep -q "ok"
check_test "Gateway status is ok"

echo ""
echo "3. Checking Pilot UI..."
curl -s -f "${UI_URL}/" > /dev/null
check_test "Pilot UI accessible"

echo ""
echo "4. Verifying Invite UI Components..."
curl -s "${UI_URL}/index.html" | grep -q "join-screen"
check_test "Join screen present"

curl -s "${UI_URL}/index.html" | grep -q "create-invite-modal"
check_test "Invite creation modal present"

curl -s "${UI_URL}/app.js" | grep -q "loadInvites"
check_test "Invite JavaScript loaded"

curl -s "${UI_URL}/style.css" | grep -q "invite-list"
check_test "Invite CSS loaded"

echo ""
echo "5. Checking Build Artifacts..."
if [ -f "/home/matt/projects/icn/icn/target/release/icnd" ]; then
    echo -e "${GREEN}✓${NC} Gateway binary exists"
    ((TESTS_PASSED++))
else
    echo -e "${RED}✗${NC} Gateway binary exists"
    ((TESTS_FAILED++))
fi

if [ -f "/home/matt/projects/icn/icn/target/release/icnctl" ]; then
    echo -e "${GREEN}✓${NC} CLI binary exists"
    ((TESTS_PASSED++))
else
    echo -e "${RED}✗${NC} CLI binary exists"
    ((TESTS_FAILED++))
fi

echo ""
echo "6. Checking Docker Images..."
docker images | grep -q "icn-pilot-ui"
check_test "Pilot UI Docker image exists"

echo ""
echo "7. Git Verification..."
cd /home/matt/projects/icn
git status --porcelain | wc -l | grep -q "^0$"
check_test "No uncommitted changes"

git log --oneline -1 | grep -q "invite"
check_test "Invite system committed"

echo ""
echo "============================================="
echo -e "Results: ${GREEN}${TESTS_PASSED} passed${NC}, ${RED}${TESTS_FAILED} failed${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}🎉 All checks passed! Ready for deployment.${NC}"
    exit 0
else
    echo -e "${YELLOW}⚠️  Some checks failed. Review issues above.${NC}"
    exit 1
fi
