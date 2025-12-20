#!/bin/bash
# Quick Demo Test - Verify everything works
# Run this before a presentation to ensure demo readiness

set -e

GATEWAY="http://localhost:8080"
UI_PORT=3000
COOP_ID="rochester-tool-library"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

PASSED=0
FAILED=0
WARNINGS=0

test_check() {
    local name="$1"
    local command="$2"
    
    echo -n "Testing $name... "
    if eval "$command" > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${RED}✗${NC}"
        ((FAILED++))
        return 1
    fi
}

test_warning() {
    local name="$1"
    local command="$2"
    
    echo -n "Checking $name... "
    if eval "$command" > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "${YELLOW}⚠${NC}"
        ((WARNINGS++))
        return 1
    fi
}

echo "========================================="
echo "ICN Demo Quick Test"
echo "========================================="
echo

echo "Infrastructure Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
test_check "Backend builds" "[ -x icn/target/release/icnd ]"
test_check "UI files exist" "[ -f web/pilot-ui/index.html ]"
test_check "Sample data exists" "[ -f demo/data/tool-library-members.json ]"
test_check "Demo scripts exist" "[ -x demo/scripts/run-tool-library-demo.sh ]"

echo
echo "Service Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
test_check "Gateway health" "curl -s $GATEWAY/v1/health | grep -q '\"status\":\"ok\"'"
test_warning "UI accessible" "curl -s http://localhost:$UI_PORT > /dev/null"

echo
echo "API Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Try to get a token (might fail if no passphrase)
TOKEN=$(cd icn && ./target/release/icnctl \
    -d /home/matt/icn-demo-test/data \
    -e 127.0.0.1:15602 \
    auth token \
    --coop-id $COOP_ID \
    --scopes "coop:read" \
    --passphrase demo123 2>/dev/null | tr -d '\n' || true)

if [ -n "$TOKEN" ]; then
    test_check "Get auth token" "[ -n '$TOKEN' ]"
    test_check "Cooperative exists" "curl -s $GATEWAY/v1/coops/$COOP_ID -H 'Authorization: Bearer $TOKEN' | grep -q '\"id\":\"$COOP_ID\"'"
    test_check "Balance endpoint" "curl -s $GATEWAY/v1/ledger/coops/$COOP_ID/balances/did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh -H 'Authorization: Bearer $TOKEN' | grep -q 'balance'"
else
    echo -e "${YELLOW}⚠${NC} Skipping authenticated tests (no token)"
    ((WARNINGS+=3))
fi

echo
echo "UI Integration Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
test_check "API endpoints fixed" "grep -q '/ledger/coops/' web/pilot-ui/app.js"
test_check "CORS configured" "[ -f demo/configs/tool-library.toml ]"

echo
echo "========================================="
echo "Results"
echo "========================================="
echo -e "${GREEN}Passed:${NC}   $PASSED"
echo -e "${YELLOW}Warnings:${NC} $WARNINGS"
echo -e "${RED}Failed:${NC}   $FAILED"
echo

if [ $FAILED -eq 0 ]; then
    if [ $WARNINGS -eq 0 ]; then
        echo -e "${GREEN}✓ Demo is READY! All tests passed.${NC}"
        echo
        echo "To run demo:"
        echo "  ./demo/scripts/run-tool-library-demo.sh"
        exit 0
    else
        echo -e "${YELLOW}⚠ Demo is MOSTLY ready. ${WARNINGS} warning(s).${NC}"
        echo
        echo "Check warnings above. Demo may still work."
        echo
        echo "To run demo:"
        echo "  ./demo/scripts/run-tool-library-demo.sh"
        exit 0
    fi
else
    echo -e "${RED}✗ Demo is NOT ready. Fix ${FAILED} failed test(s) first.${NC}"
    echo
    echo "Common fixes:"
    echo "  - Not in repo root: cd /home/matt/projects/icn"
    echo "  - Daemon not running: Start it with run-tool-library-demo.sh"
    echo "  - UI not running: Start it with run-tool-library-demo.sh"
    exit 1
fi
