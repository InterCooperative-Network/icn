#!/bin/bash
# verify_debug_api.sh
#
# This script verifies that the debug API endpoints are working correctly.
# It can be used to validate the ICN Runtime deployment is ready for integration testing.

set -e

# Default values
API_HOST="localhost"
API_PORT="8080"
API_BASE_PATH="/api/v1/debug"
VERBOSE=false
EXIT_ON_ERROR=true

# Color codes for pretty output
RED="\033[0;31m"
GREEN="\033[0;32m"
YELLOW="\033[0;33m"
NC="\033[0m" # No Color

# Function to display usage
show_usage() {
  echo "Usage: $0 [options]"
  echo "Verify ICN Runtime debug API endpoints are working"
  echo ""
  echo "Options:"
  echo "  -h, --help               Display this help message"
  echo "  -H, --host <hostname>    API host (default: localhost)"
  echo "  -p, --port <port>        API port (default: 8080)"
  echo "  -b, --base-path <path>   Base API path (default: /api/v1/debug)"
  echo "  -v, --verbose            Show verbose output"
  echo "  -c, --continue           Continue on error (don't exit)"
  echo ""
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      show_usage
      exit 0
      ;;
    -H|--host)
      API_HOST="$2"
      shift 2
      ;;
    -p|--port)
      API_PORT="$2"
      shift 2
      ;;
    -b|--base-path)
      API_BASE_PATH="$2"
      shift 2
      ;;
    -v|--verbose)
      VERBOSE=true
      shift
      ;;
    -c|--continue)
      EXIT_ON_ERROR=false
      shift
      ;;
    *)
      echo -e "${RED}Unknown option: $1${NC}"
      show_usage
      exit 1
      ;;
  esac
done

# Construct the base URL
API_URL="http://${API_HOST}:${API_PORT}${API_BASE_PATH}"

# Check for curl
if ! command -v curl &> /dev/null; then
  echo -e "${RED}Error: curl is required but not installed${NC}"
  exit 1
fi

# Check for jq
if ! command -v jq &> /dev/null; then
  echo -e "${YELLOW}Warning: jq is not installed. JSON responses will not be pretty-printed${NC}"
fi

# Function to make API call and check result
check_endpoint() {
  local endpoint="$1"
  local description="$2"
  
  echo -e "Testing endpoint: ${YELLOW}${endpoint}${NC} (${description})"
  
  local full_url="${API_URL}${endpoint}"
  local response
  local status_code
  
  # Make the API call and capture both status code and response
  if $VERBOSE; then
    echo -e "Request URL: ${full_url}"
  fi
  
  response=$(curl -s -w "\n%{http_code}" "${full_url}")
  status_code=$(echo "$response" | tail -n1)
  response_body=$(echo "$response" | sed '$ d')
  
  # Check if the status code is 2xx (success)
  if [[ $status_code =~ ^2[0-9][0-9]$ ]]; then
    echo -e "${GREEN}✓ Success:${NC} Status code: ${status_code}"
    
    # Try to parse and display the response if verbose
    if $VERBOSE; then
      if command -v jq &> /dev/null; then
        echo "Response:"
        echo "$response_body" | jq '.'
      else
        echo "Response: $response_body"
      fi
    fi
    
    return 0
  else
    echo -e "${RED}✗ Failed:${NC} Status code: ${status_code}"
    echo "Response: $response_body"
    
    if $EXIT_ON_ERROR; then
      echo -e "${RED}Exiting due to failed endpoint check${NC}"
      exit 1
    fi
    
    return 1
  fi
}

# Main testing sequence
echo "Verifying ICN Runtime debug API endpoints"
echo "API URL: ${API_URL}"

# List of endpoints to check
echo -e "\n${YELLOW}1. Testing root endpoint${NC}"
check_endpoint "" "Debug API index"

echo -e "\n${YELLOW}2. Testing federation endpoints${NC}"
check_endpoint "/federation/status" "Federation status"
check_endpoint "/federation/peers" "Connected peers"
check_endpoint "/federation/trust-bundle" "Current trust bundle"

# Note: These endpoints require valid CIDs that exist in the system
# We'll skip testing them unless provided with specific CIDs
echo -e "\n${YELLOW}3. Testing proposal and DAG endpoints${NC}"
echo -e "${YELLOW}Note:${NC} These endpoints require valid CIDs. Skipping automated testing."
echo "Examples:"
echo "  ${API_URL}/proposal/bafybeihwlhcxm4xebz5vdgnwhq5y5rtdgfsuhjvvkrdcwlxzcrmxbpoiq4"
echo "  ${API_URL}/dag/bafybeihwlhcxm4xebz5vdgnwhq5y5rtdgfsuhjvvkrdcwlxzcrmxbpoiq4"

echo -e "\n${GREEN}✓ All reachable endpoints verified successfully${NC}"
exit 0 