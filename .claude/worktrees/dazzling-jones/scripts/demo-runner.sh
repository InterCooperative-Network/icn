#!/usr/bin/env bash
# ICN Demo Runner
# Runs all three pilot flows against a running devnet
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GATEWAY="${ICN_GATEWAY:-http://localhost:8000}"
TIMEOUT="${ICN_DEMO_TIMEOUT:-30}"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BOLD}${CYAN}"
echo "╔══════════════════════════════════════╗"
echo "║     ICN Pilot Demo Runner            ║"
echo "║     Four Flows End-to-End            ║"
echo "╚══════════════════════════════════════╝"
echo -e "${NC}"

# Wait for gateway health
echo -e "${YELLOW}Waiting for gateway at $GATEWAY...${NC}"
for i in $(seq 1 "$TIMEOUT"); do
  if curl -sf "$GATEWAY/v1/health" > /dev/null 2>&1; then
    echo -e "${GREEN}Gateway is healthy${NC}"
    break
  fi
  if [ "$i" = "$TIMEOUT" ]; then
    echo -e "${RED}Gateway not ready after ${TIMEOUT}s. Is the devnet running?${NC}"
    echo "  Start with: cd deploy/devnet && make up"
    exit 1
  fi
  sleep 1
done

PASSED=0
FAILED=0
RESULTS=""

run_flow() {
  local name="$1"
  local script="$2"

  echo ""
  echo -e "${BOLD}━━━ $name ━━━${NC}"

  if bash "$script"; then
    PASSED=$((PASSED + 1))
    RESULTS="${RESULTS}\n  ${GREEN}✓${NC} $name"
  else
    FAILED=$((FAILED + 1))
    RESULTS="${RESULTS}\n  ${RED}✗${NC} $name"
  fi
}

# Run all three flows
run_flow "Flow A: WASM Distribution" "$SCRIPT_DIR/demo-flow-a.sh"
run_flow "Flow B: Service Discovery"  "$SCRIPT_DIR/demo-flow-b.sh"
run_flow "Flow C: Treasury Governance" "$SCRIPT_DIR/demo-flow-c.sh"
run_flow "Flow D: Tool Library Cooperative" "$SCRIPT_DIR/demo-flow-d.sh"

# Summary
TOTAL=$((PASSED + FAILED))
echo ""
echo -e "${BOLD}━━━ Summary ━━━${NC}"
echo -e "$RESULTS"
echo ""

if [ "$FAILED" -eq 0 ]; then
  echo -e "${GREEN}${BOLD}All $TOTAL flows passed${NC}"
  exit 0
else
  echo -e "${RED}${BOLD}$FAILED of $TOTAL flows failed${NC}"
  exit 1
fi
