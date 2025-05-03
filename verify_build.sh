#!/bin/bash
set -e

# Colors for better output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Verifying monorepo build status...${NC}"

# Clean previous build artifacts
echo -e "${YELLOW}Cleaning previous build artifacts...${NC}"
cargo clean

# Build runtime components
echo -e "${YELLOW}Building runtime components...${NC}"
components=(
    "icn-runtime-root"
    "icn-core-vm"
    "icn-governance-kernel"
    "icn-dag"
    "icn-identity"
    "icn-economics"
    "icn-federation"
    "icn-storage"
    "icn-agoranet-integration"
    "icn-execution-tools"
    "icn-ccl-compiler"
)

success_count=0
fail_count=0

for component in "${components[@]}"; do
    echo -e "${YELLOW}Building ${component}...${NC}"
    if cargo build -p ${component}; then
        echo -e "${GREEN}✓ ${component} built successfully!${NC}"
        ((success_count++))
    else
        echo -e "${RED}✗ ${component} build failed!${NC}"
        ((fail_count++))
    fi
done

# Build wallet components
echo -e "${YELLOW}Building wallet components...${NC}"
components=(
    "icn-wallet-root"
    "wallet-core"
    "wallet-agent"
    "wallet-types"
    "wallet-ui-api"
)

for component in "${components[@]}"; do
    echo -e "${YELLOW}Building ${component}...${NC}"
    if cargo build -p ${component}; then
        echo -e "${GREEN}✓ ${component} built successfully!${NC}"
        ((success_count++))
    else
        echo -e "${RED}✗ ${component} build failed!${NC}"
        ((fail_count++))
    fi
done

# Build AgoraNet
echo -e "${YELLOW}Building AgoraNet...${NC}"
if SQLX_OFFLINE=true cargo build -p icn-agoranet; then
    echo -e "${GREEN}✓ AgoraNet built successfully!${NC}"
    ((success_count++))
else
    echo -e "${RED}✗ AgoraNet build failed!${NC}"
    echo -e "${YELLOW}To set up the database for AgoraNet, run ./setup_agoranet_db.sh${NC}"
    ((fail_count++))
fi

# Build all workspace
echo -e "${YELLOW}Building entire workspace...${NC}"
if SQLX_OFFLINE=true cargo build --workspace; then
    echo -e "${GREEN}✓ Entire workspace built successfully!${NC}"
else
    echo -e "${RED}✗ Workspace build failed!${NC}"
fi

# Print summary
echo -e "${GREEN}Build Summary:${NC}"
echo -e "${GREEN}✓ ${success_count} components built successfully!${NC}"
echo -e "${RED}✗ ${fail_count} components failed to build!${NC}"

if [ $fail_count -gt 0 ]; then
    echo -e "${YELLOW}Next Steps:${NC}"
    echo -e "1. For database issues with AgoraNet, run: ./setup_agoranet_db.sh"
    echo -e "2. For wallet-sync issues, it's currently excluded from the workspace. To re-enable it, remove the exclude section from Cargo.toml"
    echo -e "3. Run this script again to verify: ./verify_build.sh"
fi 