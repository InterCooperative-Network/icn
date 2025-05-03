#!/bin/bash

# ICN Runtime Integration Setup Script
# This script prepares and launches the ICN Runtime for integration
# with AgoraNet and ICN Wallet

set -e

# Colors for output formatting
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${GREEN}ICN Runtime Integration Setup${NC}"
echo "====================================="
echo

# Check if configuration exists
CONFIG_FILE="config/runtime-config-integration.toml"
if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${RED}Error: Configuration file not found at ${CONFIG_FILE}${NC}"
    echo "Please run this script from the project root directory."
    exit 1
fi

# Step 1: Create necessary directories
echo -e "${YELLOW}Step 1: Creating directory structure...${NC}"
mkdir -p ./data/storage
mkdir -p ./data/blobs
mkdir -p ./data/metadata
mkdir -p ./logs
echo -e "${GREEN}✓ Created required directories${NC}"

# Step 2: Generate key if needed
echo -e "${YELLOW}Step 2: Checking for node key...${NC}"
KEY_FILE=$(grep "key_file" ${CONFIG_FILE} | sed 's/key_file\s*=\s*"\(.*\)"/\1/')
KEY_DIR=$(dirname "$KEY_FILE")

if [ -f "$KEY_FILE" ]; then
    echo -e "${GREEN}✓ Key already exists: ${KEY_FILE}${NC}"
else
    echo "Key not found, generating new key at: ${KEY_FILE}"
    # Create directory if it doesn't exist
    mkdir -p "${KEY_DIR}"
    
    # Generate Ed25519 key with OpenSSL
    openssl genpkey -algorithm Ed25519 -out "${KEY_FILE}"
    chmod 600 "${KEY_FILE}"
    
    echo -e "${GREEN}✓ Generated new Ed25519 key: ${KEY_FILE}${NC}"
fi

# Step 3: Build the runtime (if needed)
echo -e "${YELLOW}Step 3: Checking runtime binary...${NC}"
if [ ! -f "./target/release/icn-runtime" ]; then
    echo "Runtime binary not found. Building now..."
    cargo build --release
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Build successful${NC}"
    else
        echo -e "${RED}✗ Build failed${NC}"
        exit 1
    fi
else
    echo -e "${GREEN}✓ Runtime binary already exists${NC}"
    echo "  If you want to rebuild, run: cargo build --release"
fi

# Step 4: Launch the runtime
echo -e "${YELLOW}Step 4: Launching ICN Runtime...${NC}"
echo
echo -e "${BLUE}Runtime will start in integration mode.${NC}"
echo -e "${BLUE}Press Ctrl+C to stop.${NC}"
echo

# Start with increased log output for debugging
RUST_LOG=debug ./target/release/icn-runtime --config ${CONFIG_FILE}

# The script will end here when the runtime is stopped (Ctrl+C)
echo -e "${YELLOW}Runtime stopped.${NC}" 