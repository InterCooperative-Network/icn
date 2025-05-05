#!/bin/bash
set -e

# Function to run tests for a specific package or workspace
run_tests() {
    local package=$1
    local target_dir=$2
    local features=$3

    echo "Running tests for $package..."
    
    if [ -n "$target_dir" ]; then
        cd "$target_dir"
    fi
    
    if [ -n "$features" ]; then
        cargo test --package "$package" --features "$features" -- --nocapture
    else
        cargo test --package "$package" -- --nocapture
    fi
    
    if [ -n "$target_dir" ]; then
        cd - > /dev/null # Return to previous directory
    fi
    
    echo "Tests for $package completed."
    echo "========================================"
}

# Function to run tests for AgoraNet with SQLx offline mode
run_agoranet_tests() {
    echo "Running AgoraNet tests with SQLx offline mode..."
    
    # Set SQLx offline mode
    export SQLX_OFFLINE=true
    
    # Run AgoraNet tests
    cd /home/matt/dev/icn
    cargo test --package agoranet
    
    # Unset SQLx offline mode
    unset SQLX_OFFLINE
    
    echo "AgoraNet tests completed."
    echo "========================================"
}

# Main script execution
echo "====== ICN Project Test Runner ======"

# Run wallet crate tests
run_tests "icn-wallet-storage" "/home/matt/dev/icn/icn-wallet-root"
run_tests "icn-wallet-identity" "/home/matt/dev/icn/icn-wallet-root"
run_tests "icn-wallet-actions" "/home/matt/dev/icn/icn-wallet-root"
run_tests "icn-wallet-sync" "/home/matt/dev/icn/icn-wallet-root"
run_tests "icn-wallet-api" "/home/matt/dev/icn/icn-wallet-root"
run_tests "icn-wallet-core" "/home/matt/dev/icn/icn-wallet-root"

# Run runtime crate tests
run_tests "icn-identity" "/home/matt/dev/icn/icn-runtime-root"
run_tests "icn-storage" "/home/matt/dev/icn/icn-runtime-root"
run_tests "icn-dag" "/home/matt/dev/icn/icn-runtime-root"

# Run AgoraNet tests with SQLx offline mode
run_agoranet_tests

echo "All tests completed successfully!" 