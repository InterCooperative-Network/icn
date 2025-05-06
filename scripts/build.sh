#!/bin/bash
set -e

# Build script for the ICN project
# Usage: ./scripts/build.sh [component] [--release] [--test]

RELEASE_FLAG=""
COMPONENT=""
RUN_TESTS=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --release)
            RELEASE_FLAG="--release"
            ;;
        --test)
            RUN_TESTS=true
            ;;
        runtime|wallet|agoranet|mesh|common)
            COMPONENT="$arg"
            ;;
        *)
            echo "Unknown argument: $arg"
            exit 1
            ;;
    esac
done

# Build specified component or all if none specified
if [ -n "$COMPONENT" ]; then
    echo "Building $COMPONENT component..."
    if [ "$COMPONENT" = "runtime" ]; then
        cargo build $RELEASE_FLAG --workspace --package 'crates/runtime/*'
    elif [ "$COMPONENT" = "wallet" ]; then
        cargo build $RELEASE_FLAG --workspace --package 'crates/wallet/*'
    elif [ "$COMPONENT" = "agoranet" ]; then
        cargo build $RELEASE_FLAG --workspace --package 'crates/agoranet/*'
    elif [ "$COMPONENT" = "mesh" ]; then
        cargo build $RELEASE_FLAG --workspace --package 'crates/mesh/*'
    elif [ "$COMPONENT" = "common" ]; then
        cargo build $RELEASE_FLAG --workspace --package 'crates/common/*'
    fi
else
    echo "Building all components..."
    cargo build $RELEASE_FLAG --workspace
fi

# Run tests if requested
if [ "$RUN_TESTS" = true ]; then
    if [ -n "$COMPONENT" ]; then
        echo "Running tests for $COMPONENT component..."
        if [ "$COMPONENT" = "runtime" ]; then
            cargo test $RELEASE_FLAG --workspace --package 'crates/runtime/*'
        elif [ "$COMPONENT" = "wallet" ]; then
            cargo test $RELEASE_FLAG --workspace --package 'crates/wallet/*'
        elif [ "$COMPONENT" = "agoranet" ]; then
            cargo test $RELEASE_FLAG --workspace --package 'crates/agoranet/*'
        elif [ "$COMPONENT" = "mesh" ]; then
            cargo test $RELEASE_FLAG --workspace --package 'crates/mesh/*'
        elif [ "$COMPONENT" = "common" ]; then
            cargo test $RELEASE_FLAG --workspace --package 'crates/common/*'
        fi
    else
        echo "Running tests for all components..."
        cargo test $RELEASE_FLAG --workspace
    fi
fi

echo "Build completed successfully!" 