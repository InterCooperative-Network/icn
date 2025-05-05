#!/bin/bash
set -e

echo "ICN Project Linting and Fix Script"
echo "=================================="

# Colors for better output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to run Clippy on a specific package or workspace
run_clippy() {
    local target=$1
    local fix=${2:-false}

    echo -e "${YELLOW}Running Clippy on ${target}...${NC}"
    
    if [ "$fix" = true ]; then
        cargo clippy --package "$target" --fix --allow-dirty -- -D warnings
    else
        cargo clippy --package "$target" -- -D warnings
    fi
    
    echo -e "${GREEN}Clippy completed for ${target}${NC}"
    echo ""
}

# Function to run Clippy on the entire workspace
run_workspace_clippy() {
    local fix=${1:-false}

    echo -e "${YELLOW}Running Clippy on the entire workspace...${NC}"
    
    exclude_args=""
    
    # Add packages to exclude if they're causing problems
    #exclude_args="--exclude problematic-package"
    
    if [ "$fix" = true ]; then
        cargo clippy --workspace $exclude_args --all-targets --all-features --fix --allow-dirty -- -D warnings
    else
        cargo clippy --workspace $exclude_args --all-targets --all-features -- -D warnings
    fi
    
    echo -e "${GREEN}Workspace Clippy completed${NC}"
    echo ""
}

# Main script execution
ICN_ROOT="/home/matt/dev/icn"
cd "$ICN_ROOT"

# Parse command line arguments
FIX_MODE=false
TARGET=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --fix)
            FIX_MODE=true
            shift
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        *)
            echo -e "${RED}Unknown argument: $1${NC}"
            echo "Usage: $0 [--fix] [--target PACKAGE_NAME]"
            exit 1
            ;;
    esac
done

if [ -n "$TARGET" ]; then
    # Run Clippy on the specified target
    run_clippy "$TARGET" "$FIX_MODE"
else
    # Or run it on specific packages in a controlled order
    echo -e "${YELLOW}Running Clippy on specific packages...${NC}"
    
    # Wallet crates
    run_clippy "icn-wallet-types" "$FIX_MODE"
    run_clippy "icn-wallet-storage" "$FIX_MODE"
    run_clippy "icn-wallet-identity" "$FIX_MODE"
    run_clippy "icn-wallet-actions" "$FIX_MODE"
    run_clippy "icn-wallet-sync" "$FIX_MODE"
    run_clippy "icn-wallet-api" "$FIX_MODE"
    run_clippy "icn-wallet-core" "$FIX_MODE"
    
    # Runtime crates
    run_clippy "icn-identity" "$FIX_MODE"
    run_clippy "icn-storage" "$FIX_MODE"
    run_clippy "icn-dag" "$FIX_MODE"
    run_clippy "icn-governance-kernel" "$FIX_MODE"
    run_clippy "icn-ccl-compiler" "$FIX_MODE"
    run_clippy "icn-core-vm" "$FIX_MODE"
    
    # Finally run on entire workspace if all individual packages pass
    echo -e "${YELLOW}Final workspace check...${NC}"
    run_workspace_clippy "$FIX_MODE"
fi

echo -e "${GREEN}All linting tasks completed!${NC}"

if [ "$FIX_MODE" = true ]; then
    echo -e "${YELLOW}Some issues were automatically fixed. Please review the changes.${NC}"
    echo -e "${YELLOW}Run 'git diff' to see the changes made.${NC}"
else
    echo -e "${YELLOW}To automatically fix issues, run:${NC}"
    if [ -n "$TARGET" ]; then
        echo -e "  $0 --fix --target $TARGET"
    else
        echo -e "  $0 --fix"
    fi
fi 