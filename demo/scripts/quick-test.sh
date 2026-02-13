#!/bin/bash
# Quick Demo Test - Verify everything works
# Run this before a presentation to ensure demo readiness.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ICN_DIR="${REPO_ROOT}/icn"

GATEWAY_HOST="${ICN_DEMO_GATEWAY_HOST:-127.0.0.1}"
GATEWAY_PORT="${ICN_DEMO_GATEWAY_PORT:-8080}"
GATEWAY="http://${GATEWAY_HOST}:${GATEWAY_PORT}"
UI_PORT="${ICN_DEMO_UI_PORT:-3000}"
COOP_ID="${ICN_DEMO_COOP_ID:-rochester-tool-library}"
DATA_DIR="${ICN_DEMO_DATA_DIR:-${REPO_ROOT}/.demo-data/tool-library}"
RPC_ENDPOINT="${ICN_DEMO_RPC_ENDPOINT:-127.0.0.1:15602}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

PASSED=0
FAILED=0
WARNINGS=0

run_check() {
    local name="$1"
    local command="$2"

    echo -n "Testing $name... "
    if eval "$command" >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
        ((PASSED+=1))
        return 0
    fi

    echo -e "${RED}✗${NC}"
    ((FAILED+=1))
    return 1
}

run_warning() {
    local name="$1"
    local command="$2"

    echo -n "Checking $name... "
    if eval "$command" >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
        ((PASSED+=1))
        return 0
    fi

    echo -e "${YELLOW}⚠${NC}"
    ((WARNINGS+=1))
    return 1
}

echo "========================================="
echo "ICN Demo Quick Test"
echo "========================================="
echo
echo "Using:"
echo "  Repo root: $REPO_ROOT"
echo "  Gateway:   $GATEWAY"
echo "  Data dir:  $DATA_DIR"
echo

echo "Infrastructure Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
run_check "Backend binary exists" "[ -x '$ICN_DIR/target/release/icnd' ]"
run_check "CLI binary exists" "[ -x '$ICN_DIR/target/release/icnctl' ]"
run_check "UI files exist" "[ -f '$REPO_ROOT/web/pilot-ui/index.html' ]"
run_check "Sample data exists" "[ -f '$REPO_ROOT/demo/data/tool-library-members.json' ]"
run_check "Demo scripts exist" "[ -x '$REPO_ROOT/demo/scripts/run-tool-library-demo.sh' ]"

echo
echo "Service Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
run_check "Gateway health" "curl -fsS '$GATEWAY/v1/health' | grep -q '\"status\":\"ok\"'"
run_warning "UI accessible" "curl -fsS 'http://localhost:$UI_PORT'"

echo
echo "API Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

CURRENT_DID=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 ./target/release/icnctl -d "$DATA_DIR" id show 2>/dev/null | grep -oE 'did:icn:[A-Za-z0-9]+' | head -1 || true)

TOKEN=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 ./target/release/icnctl \
    -d "$DATA_DIR" \
    -e "$RPC_ENDPOINT" \
    auth token \
    --coop-id "$COOP_ID" \
    --scopes "coop:read,ledger:read" \
    2>/dev/null | grep -oE 'eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+' | head -1 || true)

if [ -n "$TOKEN" ] && [ -n "$CURRENT_DID" ]; then
    run_check "Get auth token" "[ -n '$TOKEN' ]"
    run_check "Cooperative exists" "curl -fsS '$GATEWAY/v1/coops/$COOP_ID' -H 'Authorization: Bearer $TOKEN' | grep -q '\"id\":\"$COOP_ID\"'"
    run_check "Balance endpoint" "curl -fsS '$GATEWAY/v1/ledger/$COOP_ID/balance/$CURRENT_DID' -H 'Authorization: Bearer $TOKEN' | grep -q 'balances'"
else
    echo -e "${YELLOW}⚠${NC} Skipping authenticated API tests (missing token or DID)"
    ((WARNINGS+=3))
fi

echo
echo "UI Integration Tests:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
run_check "UI uses canonical ledger path" "grep -q '/ledger/\${state.coopId}/balance/' '$REPO_ROOT/web/pilot-ui/app.js'"
run_check "CORS config exists" "[ -f '$REPO_ROOT/demo/configs/tool-library.toml' ]"

echo
echo "========================================="
echo "Results"
echo "========================================="
echo -e "${GREEN}Passed:${NC}   $PASSED"
echo -e "${YELLOW}Warnings:${NC} $WARNINGS"
echo -e "${RED}Failed:${NC}   $FAILED"
echo

if [ "$FAILED" -eq 0 ]; then
    if [ "$WARNINGS" -eq 0 ]; then
        echo -e "${GREEN}✓ Demo is READY! All tests passed.${NC}"
        echo "Run: ./demo/scripts/run-tool-library-demo.sh"
        exit 0
    fi

    echo -e "${YELLOW}⚠ Demo is MOSTLY ready. $WARNINGS warning(s).${NC}"
    echo "Run: ./demo/scripts/run-tool-library-demo.sh"
    exit 0
fi

echo -e "${RED}✗ Demo is NOT ready. Fix $FAILED failed test(s) first.${NC}"
echo "Common fixes:"
echo "  - Build release binaries: cd icn && cargo build --release -p icnd -p icnctl"
echo "  - Start services: ./demo/scripts/run-tool-library-demo.sh"
exit 1
