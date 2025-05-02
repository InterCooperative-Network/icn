#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}==================================${NC}"
echo -e "${YELLOW}   ICN Wallet Integration Tests   ${NC}"
echo -e "${YELLOW}==================================${NC}"

# Install Node.js dependencies if needed
if [ ! -d "node_modules" ]; then
    echo -e "${YELLOW}Installing Node.js dependencies...${NC}"
    npm install
fi

# Setup test environment
echo -e "${YELLOW}Setting up test environment...${NC}"
bash setup_test_env.sh

# Start AgoraNet server in background
echo -e "${YELLOW}Starting mock AgoraNet server...${NC}"
node mock_agoranet.js &
AGORANET_PID=$!

# Give it time to start
sleep 2

# Function to run a test with proper output formatting
run_test() {
    TEST_NAME=$1
    echo -e "${YELLOW}Running $TEST_NAME...${NC}"
    
    if cargo test --test $TEST_NAME -- --nocapture; then
        echo -e "${GREEN}✓ $TEST_NAME passed${NC}"
        return 0
    else
        echo -e "${RED}✗ $TEST_NAME failed${NC}"
        return 1
    fi
}

# Track failures
FAILURES=0

# Run individual tests
run_test "agoranet_integration_test" || FAILURES=$((FAILURES+1))
run_test "runtime_integration_test" || FAILURES=$((FAILURES+1))
run_test "e2e_workflow_test" || FAILURES=$((FAILURES+1))

# Clean up
echo -e "${YELLOW}Cleaning up...${NC}"
kill $AGORANET_PID || true

# Final report
echo -e "${YELLOW}==================================${NC}"
if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}$FAILURES test(s) failed.${NC}"
    exit 1
fi 