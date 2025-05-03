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

# Check if Python is installed
if ! command -v python3 &> /dev/null; then
    echo -e "${RED}Python 3 is required but not installed. Please install Python 3 and try again.${NC}"
    exit 1
fi

# Install dependencies if needed
if [ ! -d "venv" ]; then
    echo -e "${YELLOW}Creating Python virtual environment...${NC}"
    python3 -m venv venv
    
    echo -e "${YELLOW}Installing dependencies...${NC}"
    venv/bin/pip install -r requirements.txt
fi

# Activate virtual environment
source venv/bin/activate

# Run the integration tests
echo -e "${YELLOW}Running integration tests...${NC}"
python3 test_orchestrator.py "$@"

# Get the exit code
EXIT_CODE=$?

# Print final status
if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}All integration tests passed!${NC}"
else
    echo -e "${RED}Some integration tests failed. Check the logs above for details.${NC}"
fi

# Deactivate virtual environment
deactivate

exit $EXIT_CODE 