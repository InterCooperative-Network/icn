#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}==================================${NC}"
echo -e "${YELLOW}   ICN Wallet Integration Tests   ${NC}"
echo -e "${YELLOW}==================================${NC}"

# Start in the correct directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
cd "$SCRIPT_DIR"

# Function to handle errors
handle_error() {
    echo -e "${RED}Test failed with error code $1${NC}"
    exit $1
}

# Build the CLI first
echo -e "${YELLOW}Building the Wallet CLI...${NC}"
cargo build --release --bin icn-wallet-cli || handle_error $?

# 1. Run CLI JSON format tests
echo -e "\n${YELLOW}Running CLI JSON output format tests...${NC}"
chmod +x integration_tests/test_cli_json.sh
integration_tests/test_cli_json.sh || handle_error $?

# 2. Run orchestrated integration tests with Docker
echo -e "\n${YELLOW}Running full ecosystem integration tests...${NC}"
chmod +x integration_tests/run_tests.sh
cd integration_tests
./run_tests.sh "$@" || handle_error $?

# If we get here, all tests passed
echo -e "\n${GREEN}All integration tests passed!${NC}"
exit 0 