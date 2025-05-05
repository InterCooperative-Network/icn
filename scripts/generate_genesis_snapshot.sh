#!/bin/bash
set -e

# Colors for terminal output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}ICN Federation Genesis Snapshot Generator${NC}"
echo "======================================"
echo ""

# Configuration variables
OUTPUT_DIR="./genesis_data"
FEDERATION_NAME=${1:-"TestFederation"}
FEDERATION_DESCRIPTION=${2:-"A test federation for ICN"}
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check for required tools
if ! command -v jq &> /dev/null; then
    echo -e "${RED}Error: jq is required but not installed.${NC}"
    echo "Please install jq: https://stedolan.github.io/jq/download/"
    exit 1
fi

echo -e "${YELLOW}Generating federation identity...${NC}"

# Generate federation keypair
FEDERATION_KEYPAIR_FILE="$OUTPUT_DIR/federation_keypair.json"
cargo run --bin icn-identity-tool -- generate-keypair --output "$FEDERATION_KEYPAIR_FILE" --type ed25519

# Extract federation DID from keypair
FEDERATION_DID=$(cargo run --bin icn-identity-tool -- extract-did --input "$FEDERATION_KEYPAIR_FILE" --scope community)
echo -e "${GREEN}Federation DID: $FEDERATION_DID${NC}"

echo -e "${YELLOW}Generating genesis bundle...${NC}"

# Create genesis bundle content
cat > "$OUTPUT_DIR/genesis_bundle.json" << EOF
{
  "id": "$(uuidgen)",
  "type": "FederationGenesisBundle",
  "created": "$TIMESTAMP",
  "federation": {
    "id": "$FEDERATION_DID",
    "name": "$FEDERATION_NAME",
    "description": "$FEDERATION_DESCRIPTION",
    "created": "$TIMESTAMP",
    "version": "1"
  },
  "policies": {
    "governance": {
      "votingPeriodHours": 48,
      "threshold": 0.5,
      "minVoters": 1
    },
    "membership": {
      "initialAdmins": ["$FEDERATION_DID"],
      "invitationRequired": true
    }
  },
  "initialState": {
    "members": [
      {
        "did": "$FEDERATION_DID",
        "role": "admin",
        "joined": "$TIMESTAMP"
      }
    ],
    "dag": {
      "root": null
    }
  }
}
EOF

echo -e "${YELLOW}Signing trust bundle...${NC}"

# Sign the genesis bundle to create the trust bundle
cargo run --bin icn-bundle-tool -- sign \
  --input "$OUTPUT_DIR/genesis_bundle.json" \
  --key "$FEDERATION_KEYPAIR_FILE" \
  --output "$OUTPUT_DIR/trust_bundle.json"

echo -e "${YELLOW}Anchoring to DAG...${NC}"

# Anchor the trust bundle to the DAG and get the CID
ANCHOR_CID=$(cargo run --bin icn-dag-tool -- anchor \
  --content "$OUTPUT_DIR/trust_bundle.json" \
  --type "FederationGenesis" \
  --signer "$FEDERATION_KEYPAIR_FILE")

# Save the anchor CID to a file
echo "$ANCHOR_CID" > "$OUTPUT_DIR/anchor_cid.txt"

echo -e "${GREEN}Genesis snapshot created successfully!${NC}"
echo "Trust bundle: $OUTPUT_DIR/trust_bundle.json"
echo "Genesis bundle: $OUTPUT_DIR/genesis_bundle.json"
echo "Anchor CID: $ANCHOR_CID"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Distribute the trust bundle and anchor CID to federation members"
echo "2. Run 'scripts/replay_genesis.sh $ANCHOR_CID' to validate the bundle"
echo "3. Initialize federation nodes with 'icn-federation-node --genesis $OUTPUT_DIR/trust_bundle.json'" 