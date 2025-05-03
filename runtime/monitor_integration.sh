#!/bin/bash

# ICN Runtime Integration Monitoring Script
# This script monitors logs and interactions between ICN Runtime, AgoraNet, and Wallet

# Colors for output formatting
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

LOG_FILE="./logs/runtime.log"
EVENT_LOG="./logs/events.log" # Create a symlink to where AgoraNet stores received events
FEDERATION_LOG="./logs/federation.log" # Federation interactions

echo -e "${GREEN}ICN Runtime Integration Monitor${NC}"
echo "====================================="
echo

# Check if runtime is running
PID=$(pgrep -f "icn-runtime")
if [ -z "$PID" ]; then
    echo -e "${RED}ERROR: ICN Runtime doesn't appear to be running${NC}"
    echo "Please start the runtime using ./run_integration_node.sh"
    exit 1
else
    echo -e "${GREEN}Runtime is running with PID: ${PID}${NC}"
fi

# Function to monitor runtime logs for specific patterns
monitor_logs() {
    if [ ! -f "$LOG_FILE" ]; then
        echo -e "${RED}ERROR: Log file not found at ${LOG_FILE}${NC}"
        return 1
    fi

    echo -e "${YELLOW}Monitoring runtime logs...${NC}"
    
    # Using tail -f to continuously monitor the log
    tail -f "$LOG_FILE" | grep --line-buffered -E "FederationManager|GovernanceKernel|AgoraNetIntegration|Wallet|proposal|vote|TrustBundle|VM execution" | while read -r line; do
        # Highlight different types of log lines
        if echo "$line" | grep -q "FederationManager"; then
            echo -e "${MAGENTA}[Federation] ${NC}$line"
        elif echo "$line" | grep -q "GovernanceKernel"; then
            echo -e "${BLUE}[Governance] ${NC}$line"
        elif echo "$line" | grep -q "AgoraNetIntegration"; then
            echo -e "${CYAN}[AgoraNet] ${NC}$line"
        elif echo "$line" | grep -q "VM execution"; then
            echo -e "${GREEN}[VM] ${NC}$line"
        elif echo "$line" | grep -q "TrustBundle"; then
            echo -e "${YELLOW}[TrustBundle] ${NC}$line"
        elif echo "$line" | grep -q "Wallet"; then
            echo -e "${MAGENTA}[Wallet] ${NC}$line"
        else
            echo "$line"
        fi
    done
}

# Function to show resource utilization
show_resources() {
    echo -e "${YELLOW}Resource utilization:${NC}"
    ps -p $PID -o %cpu,%mem,rss,vsz | head -1
    ps -p $PID -o %cpu,%mem,rss,vsz | grep -v CPU
    echo
}

# Function to test WebSocket event emission to AgoraNet
test_event_stream() {
    echo -e "${YELLOW}Testing event stream to AgoraNet...${NC}"
    # Extract WebSocket port from config
    WS_PORT=$(grep "events_websocket_listen" config/runtime-config-integration.toml | awk -F':' '{print $NF}' | tr -d ' "')
    
    if [ -z "$WS_PORT" ]; then
        WS_PORT=8090 # Default if not found
    fi
    
    # Simple WebSocket client (requires websocat, install with: cargo install websocat)
    if command -v websocat &> /dev/null; then
        echo "Connecting to WebSocket stream on port $WS_PORT..."
        websocat -v ws://localhost:$WS_PORT/events 2>&1 | head -n 10
    else
        echo -e "${RED}WebSocket client (websocat) not found.${NC}"
        echo "Install with: cargo install websocat"
        echo "Or use: npm install -g wscat"
    fi
}

# Function to test TrustBundle retrieval
test_trustbundle() {
    echo -e "${YELLOW}Testing TrustBundle retrieval...${NC}"
    # Extract HTTP API port from config
    HTTP_PORT=$(grep "http_listen" config/runtime-config-integration.toml | awk -F':' '{print $NF}' | tr -d ' "')
    
    if [ -z "$HTTP_PORT" ]; then
        HTTP_PORT=8080 # Default if not found
    fi
    
    # Query the API for latest TrustBundle
    echo "Requesting latest TrustBundle..."
    curl -s -X GET http://localhost:$HTTP_PORT/api/federation/trustbundle/latest | jq . 2>/dev/null || echo "Failed to retrieve TrustBundle. Make sure jq is installed."
}

# Function to display menu
display_menu() {
    echo -e "${GREEN}ICN Runtime Integration Monitor Menu${NC}"
    echo "1. Monitor runtime logs"
    echo "2. Show resource utilization"
    echo "3. Test event stream to AgoraNet"
    echo "4. Test TrustBundle retrieval"
    echo "5. Run stress test against the live system"
    echo "6. Create a test proposal and track lifecycle"
    echo "7. Exit"
    echo
    read -p "Select an option: " option
    
    case $option in
        1) clear; monitor_logs ;;
        2) clear; show_resources ;;
        3) clear; test_event_stream ;;
        4) clear; test_trustbundle ;;
        5) clear; run_stress_test ;;
        6) clear; track_proposal_lifecycle ;;
        7) exit 0 ;;
        *) echo -e "${RED}Invalid option${NC}"; sleep 1 ;;
    esac
}

# Function to run a targeted stress test
run_stress_test() {
    echo -e "${YELLOW}Running stress test against live system...${NC}"
    echo "This will generate load on the runtime and connected systems."
    read -p "Continue? (y/n): " confirm
    
    if [ "$confirm" != "y" ]; then
        return
    fi
    
    echo "Select test to run:"
    echo "1. Governance stress test (proposals & votes)"
    echo "2. Federation stress test (TrustBundle sync)"
    echo "3. Concurrent operations test"
    echo "4. Back to main menu"
    
    read -p "Select test: " test_option
    
    case $test_option in
        1) 
            echo "Running governance stress test..."
            chmod +x run_stress_tests.sh
            ./run_stress_tests.sh governance
            ;;
        2)
            echo "Running federation stress test..."
            chmod +x run_stress_tests.sh
            ./run_stress_tests.sh federation
            ;;
        3)
            echo "Running concurrent operations test..."
            chmod +x run_stress_tests.sh
            ./run_stress_tests.sh concurrent
            ;;
        4) return ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
    
    read -p "Press Enter to continue..."
}

# Function to track a proposal lifecycle
track_proposal_lifecycle() {
    echo -e "${YELLOW}Creating test proposal and tracking lifecycle...${NC}"
    
    # Extract HTTP API port from config
    HTTP_PORT=$(grep "http_listen" config/runtime-config-integration.toml | awk -F':' '{print $NF}' | tr -d ' "')
    
    if [ -z "$HTTP_PORT" ]; then
        HTTP_PORT=8080 # Default if not found
    fi
    
    # Create a test proposal
    echo "Creating test proposal..."
    PROPOSAL_DATA='{"title":"Integration Test Proposal","description":"This is a test proposal created by the monitoring script","templateText":"// Sample CCL code\nrule test_rule { always allow }","votingPeriodSeconds":3600}'
    
    PROPOSAL_RESPONSE=$(curl -s -X POST -H "Content-Type: application/json" -d "$PROPOSAL_DATA" http://localhost:$HTTP_PORT/api/governance/proposals)
    PROPOSAL_CID=$(echo $PROPOSAL_RESPONSE | grep -o '"cid":"[^"]*"' | cut -d':' -f2 | tr -d '"')
    
    if [ -z "$PROPOSAL_CID" ]; then
        echo -e "${RED}Failed to create proposal${NC}"
        return
    fi
    
    echo -e "${GREEN}Created proposal with CID: ${PROPOSAL_CID}${NC}"
    
    # Monitor logs for this proposal
    echo "Monitoring events for this proposal. Press Ctrl+C to stop monitoring."
    tail -f "$LOG_FILE" | grep --line-buffered "$PROPOSAL_CID"
}

# Main program loop
while true; do
    display_menu
done 