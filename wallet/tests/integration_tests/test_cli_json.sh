#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}==================================${NC}"
echo -e "${YELLOW}   Testing CLI JSON Output Options   ${NC}"
echo -e "${YELLOW}==================================${NC}"

# Location of the icn-wallet-cli binary
CLI_BIN="${CLI_BIN:-../../target/release/icn-wallet-cli}"

# Test directory
TEST_DIR="./cli_test_data"
mkdir -p "$TEST_DIR"

# Clean up function
cleanup() {
    if [ -d "$TEST_DIR" ]; then
        echo -e "${YELLOW}Cleaning up test data...${NC}"
        rm -rf "$TEST_DIR"
    fi
}

# Register the cleanup function to run on script exit
trap cleanup EXIT

# Function to validate JSON
is_valid_json() {
    echo "$1" | jq . >/dev/null 2>&1
    return $?
}

# Test cases
test_create_identity() {
    echo -e "${YELLOW}Testing 'create' command with JSON output...${NC}"
    
    # Test with --format json
    RESULT=$("$CLI_BIN" --data-dir "$TEST_DIR" create --scope personal --format json)
    
    # Validate JSON output
    if is_valid_json "$RESULT"; then
        echo -e "${GREEN}✓ Create identity returned valid JSON${NC}"
        
        # Extract and verify required fields
        ID=$(echo "$RESULT" | jq -r '.id')
        DID=$(echo "$RESULT" | jq -r '.did')
        
        if [ -n "$ID" ] && [ -n "$DID" ]; then
            echo -e "${GREEN}✓ JSON contains required fields (id, did)${NC}"
            return 0
        else
            echo -e "${RED}✗ JSON missing required fields${NC}"
            return 1
        fi
    else
        echo -e "${RED}✗ Create identity did not return valid JSON${NC}"
        echo "Output: $RESULT"
        return 1
    fi
}

test_list_identities() {
    echo -e "${YELLOW}Testing listing identities with JSON output...${NC}"
    
    # Create a test identity first
    "$CLI_BIN" --data-dir "$TEST_DIR" create --scope personal >/dev/null
    
    # Test list with --format json
    RESULT=$("$CLI_BIN" --data-dir "$TEST_DIR" list --format json)
    
    # Validate JSON output
    if is_valid_json "$RESULT"; then
        echo -e "${GREEN}✓ List identities returned valid JSON${NC}"
        
        # Verify it's an array
        COUNT=$(echo "$RESULT" | jq 'length')
        
        if [ "$COUNT" -gt 0 ]; then
            echo -e "${GREEN}✓ JSON contains identity array with $COUNT items${NC}"
            return 0
        else
            echo -e "${RED}✗ JSON doesn't contain any identities${NC}"
            return 1
        fi
    else
        echo -e "${RED}✗ List identities did not return valid JSON${NC}"
        echo "Output: $RESULT"
        return 1
    fi
}

test_agoranet_threads() {
    echo -e "${YELLOW}Testing AgoraNet threads list with JSON output...${NC}"
    
    # Create a test identity
    ID=$("$CLI_BIN" --data-dir "$TEST_DIR" create --scope personal --format json | jq -r '.id')
    
    # Test AgoraNet command with --format json (note: this will use a mock)
    RESULT=$("$CLI_BIN" --data-dir "$TEST_DIR" AgoraNet -i "$TEST_DIR/identities/$ID.json" ListThreads --format json)
    
    # Validate JSON output
    if is_valid_json "$RESULT"; then
        echo -e "${GREEN}✓ AgoraNet threads returned valid JSON${NC}"
        return 0
    else
        echo -e "${RED}✗ AgoraNet threads did not return valid JSON${NC}"
        echo "Output: $RESULT"
        return 1
    fi
}

# Run all test cases
FAILURES=0

test_create_identity || FAILURES=$((FAILURES+1))
test_list_identities || FAILURES=$((FAILURES+1))
test_agoranet_threads || FAILURES=$((FAILURES+1))

# Final report
echo -e "${YELLOW}==================================${NC}"
if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}All CLI JSON tests passed!${NC}"
    exit 0
else
    echo -e "${RED}$FAILURES test(s) failed.${NC}"
    exit 1
fi 