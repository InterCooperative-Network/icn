#!/bin/bash

# Stress Testing Script for ICN Runtime
# This script performs comprehensive stress tests on different components
# of the ICN Runtime

set -e

# Colors for output formatting
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}ICN Runtime Stress Testing Suite${NC}"
echo "========================================"
echo

# Function to run a specific test
run_test() {
    local test_name=$1
    local test_func=$2
    
    echo -e "${YELLOW}Running test: ${test_name}${NC}"
    echo "----------------------------------------"
    
    # Run the test with cargo test
    # We use --nocapture to see all output and --test to specify the test file
    RUST_BACKTRACE=1 cargo test --test stress_tests $test_func -- --nocapture
    
    # Check the exit status
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ ${test_name} completed successfully${NC}"
    else
        echo -e "${RED}✗ ${test_name} failed${NC}"
        exit 1
    fi
    
    echo
}

# Check if specific tests were requested
if [ $# -gt 0 ]; then
    for test in "$@"; do
        case $test in
            governance)
                run_test "Governance Stress Test" test_governance_stress
                ;;
            federation)
                run_test "Federation Stress Test" test_federation_stress
                ;;
            dag)
                run_test "DAG Stress Test" test_dag_stress
                ;;
            concurrent)
                run_test "Concurrent Operations Stress Test" test_concurrent_stress
                ;;
            resources)
                run_test "Resource Utilization Test" test_resource_utilization
                ;;
            *)
                echo -e "${RED}Unknown test: $test${NC}"
                echo "Available tests: governance, federation, dag, concurrent, resources"
                exit 1
                ;;
        esac
    done
else
    # No specific tests requested, run all tests
    echo "Running all stress tests. This may take a while..."
    echo
    
    run_test "Governance Stress Test" test_governance_stress
    run_test "Federation Stress Test" test_federation_stress
    run_test "DAG Stress Test" test_dag_stress
    run_test "Concurrent Operations Stress Test" test_concurrent_stress
    run_test "Resource Utilization Test" test_resource_utilization
fi

echo -e "${GREEN}All stress tests completed successfully!${NC}" 