#!/bin/bash
# Test UI -> Gateway API Integration

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ICN_DIR="${REPO_ROOT}/icn"
UI_DIR="${REPO_ROOT}/web/pilot-ui"

GATEWAY_HOST="${ICN_DEMO_GATEWAY_HOST:-127.0.0.1}"
GATEWAY_PORT="${ICN_DEMO_GATEWAY_PORT:-8080}"
GATEWAY="http://${GATEWAY_HOST}:${GATEWAY_PORT}"
UI_PORT="${ICN_DEMO_UI_PORT:-3000}"

echo "========================================="
echo "ICN Demo - UI Integration Test"
echo "========================================="
echo

echo "Step 1: Verify UI uses canonical ledger routes"
echo

grep -nE "/ledger/\\\$\{state\.coopId\}/(balance|history|payment)" "$UI_DIR/app.js" || {
    echo "Route patterns not found in app.js"
    exit 1
}

echo
echo "Step 2: Verify gateway health"
echo

if curl -fsS "$GATEWAY/v1/health" >/dev/null 2>&1; then
    echo "Gateway healthy at $GATEWAY"
else
    echo "Gateway not running at $GATEWAY"
    echo "Start with: ./demo/scripts/run-tool-library-demo.sh"
    exit 1
fi

echo
echo "Step 3: Manual smoke flow"
echo "  1. Open http://localhost:$UI_PORT"
echo "  2. Sign in with credentials printed by run-tool-library-demo.sh"
echo "  3. Confirm balance loads"
echo "  4. Confirm history loads"
echo "  5. Create a payment and confirm dashboard refresh"
echo

echo "========================================="
echo "Integration checks passed"
echo "========================================="
