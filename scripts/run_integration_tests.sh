#!/bin/bash

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}ICN Runtime Integration Tests${NC}"
echo -e "${BLUE}============================${NC}"
echo ""

# Function to run tests and report results
run_test_suite() {
    local suite_name=$1
    local command=$2
    
    echo -e "${BLUE}Running $suite_name...${NC}"
    
    if $command; then
        echo -e "${GREEN}✓ $suite_name passed${NC}"
        return 0
    else
        echo -e "${RED}✗ $suite_name failed${NC}"
        return 1
    fi
}

# Keep track of failures
FAILURES=0

# Run governance kernel tests
if ! run_test_suite "Governance Event/Credential Emission Tests" "cargo test --test integration_tests --package icn-governance-kernel"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Run federation tests 
if ! run_test_suite "Federation TrustBundle Sync Tests" "cargo test --test trustbundle_tests --package icn-federation"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Run core VM tests
if ! run_test_suite "Core VM Execution Tests" "cargo test --test execution_tests --package icn-core-vm"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Run whole system integration tests
if ! run_test_suite "Wallet Integration Flow Tests" "cargo test --test integration_tests"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Run state consistency tests
if ! run_test_suite "State Consistency Tests" "cargo test --test state_consistency_tests"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Run stress tests (new)
if ! run_test_suite "Runtime Stress Tests" "cargo test --test stress_tests -- --nocapture"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Run performance metrics tests (new)
if ! run_test_suite "Performance Metrics Tests" "cargo test --test metrics_tests --package icn-core-vm"; then
    FAILURES=$((FAILURES+1))
fi

echo ""

# Report overall results
if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}All integration tests passed!${NC}"
    exit 0
else
    echo -e "${RED}$FAILURES test suite(s) failed${NC}"
    exit 1
fi 