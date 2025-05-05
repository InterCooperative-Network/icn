#!/bin/bash
set -e

# Colors for terminal output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}ICN Federation Genesis Snapshot Validator${NC}"
echo "========================================"
echo ""

# Parse arguments
ANCHOR_CID=$1
TRUST_BUNDLE_FILE=$2

if [ -z "$ANCHOR_CID" ] && [ -z "$TRUST_BUNDLE_FILE" ]; then
    echo -e "${RED}Error: Either an anchor CID or trust bundle file must be provided.${NC}"
    echo "Usage: $0 <anchor_cid> [trust_bundle_file]"
    echo "   or: $0 --bundle <trust_bundle_file>"
    exit 1
fi

# Directory to store validated data
VALIDATION_DIR="./genesis_validation"
mkdir -p "$VALIDATION_DIR"

echo -e "${YELLOW}Starting validation process...${NC}"

# If a CID is provided, fetch the trust bundle from the DAG
if [ -n "$ANCHOR_CID" ]; then
    echo -e "${YELLOW}Fetching trust bundle from DAG using CID: $ANCHOR_CID${NC}"
    
    # Retrieve the trust bundle using the DAG tool
    cargo run --bin icn-dag-tool -- fetch \
        --cid "$ANCHOR_CID" \
        --output "$VALIDATION_DIR/fetched_trust_bundle.json"
    
    TRUST_BUNDLE_FILE="$VALIDATION_DIR/fetched_trust_bundle.json"
    
    echo -e "${GREEN}Trust bundle retrieved from DAG.${NC}"
fi

# Ensure we now have a trust bundle file
if [ ! -f "$TRUST_BUNDLE_FILE" ]; then
    echo -e "${RED}Error: Trust bundle file does not exist: $TRUST_BUNDLE_FILE${NC}"
    exit 1
fi

echo -e "${YELLOW}Verifying trust bundle signatures...${NC}"

# Verify the signatures on the trust bundle
VERIFY_RESULT=$(cargo run --bin icn-bundle-tool -- verify \
    --input "$TRUST_BUNDLE_FILE" \
    --output-result)

if [ "$VERIFY_RESULT" != "valid" ]; then
    echo -e "${RED}Error: Trust bundle signature verification failed!${NC}"
    echo "Result: $VERIFY_RESULT"
    exit 1
fi

echo -e "${GREEN}Trust bundle signatures verified successfully.${NC}"

# Extract the genesis bundle content
echo -e "${YELLOW}Extracting genesis bundle content...${NC}"
cargo run --bin icn-bundle-tool -- extract \
    --input "$TRUST_BUNDLE_FILE" \
    --output "$VALIDATION_DIR/extracted_genesis.json"

# Parse and display federation information
FEDERATION_INFO=$(jq -r '.federation | "Name: \(.name)\nID: \(.id)\nCreated: \(.created)"' "$VALIDATION_DIR/extracted_genesis.json")
echo -e "${YELLOW}Federation Information:${NC}"
echo -e "$FEDERATION_INFO"

# Display policy information
POLICY_INFO=$(jq -r '.policies.governance | "Voting Period: \(.votingPeriodHours) hours\nThreshold: \(.threshold)\nMin Voters: \(.minVoters)"' "$VALIDATION_DIR/extracted_genesis.json")
echo -e "${YELLOW}\nGovernance Policies:${NC}"
echo -e "$POLICY_INFO"

# Display initial members
MEMBER_COUNT=$(jq -r '.initialState.members | length' "$VALIDATION_DIR/extracted_genesis.json")
echo -e "${YELLOW}\nInitial Members (${MEMBER_COUNT}):${NC}"
jq -r '.initialState.members[] | "  - DID: \(.did)\n    Role: \(.role)\n    Joined: \(.joined)"' "$VALIDATION_DIR/extracted_genesis.json"

echo -e "\n${GREEN}Federation genesis snapshot validated successfully!${NC}"
echo ""
echo -e "${YELLOW}To initialize a federation node with this genesis:${NC}"
echo "icn-federation-node --genesis $TRUST_BUNDLE_FILE"
echo ""
echo -e "${YELLOW}To import this genesis into a wallet:${NC}"
echo "icn-wallet import-federation --genesis $TRUST_BUNDLE_FILE" 