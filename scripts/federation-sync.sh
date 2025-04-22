#!/bin/bash
# federation-sync.sh - Check status of all federation nodes
# Display DAG tip, peer count, proposal status, and sync status in a table

set -e

# Configuration
CONFIG_DIR="./config"
NODE_LIST_FILE="$CONFIG_DIR/federation-nodes.json"
OUTPUT_FORMAT=${1:-"table"}  # Options: table, json, csv
MAX_PARALLEL=5  # Maximum number of parallel requests

# Ensure config directory exists
mkdir -p "$CONFIG_DIR"

# Ensure node list file exists
if [ ! -f "$NODE_LIST_FILE" ]; then
    # Create a sample node list file if it doesn't exist
    cat > "$NODE_LIST_FILE" << EOF
[
  {
    "name": "node01.icn.zone",
    "url": "https://node01.icn.zone",
    "did": "did:icn:node01"
  },
  {
    "name": "node02.icn.zone",
    "url": "https://node02.icn.zone",
    "did": "did:icn:node02"
  }
]
EOF
    echo "Created sample node list at $NODE_LIST_FILE. Please update with actual federation nodes."
fi

# Function to check if a command exists
check_command() {
    if ! command -v "$1" &> /dev/null; then
        echo "Error: $1 is required but not installed." >&2
        exit 1
    fi
}

# Check for required commands
check_command "jq"
check_command "curl"

# Function to query node status
query_node_status() {
    local node_name="$1"
    local node_url="$2"
    local node_did="$3"
    
    # Get status information using API client
    local status_data
    status_data=$(curl -s -m 5 -X GET "${node_url}/api/v1/status" 2>/dev/null || echo '{"error": "Connection failed"}')
    
    # Check if we got a valid response
    if [[ "$status_data" == *"error"* ]]; then
        # Node is unreachable
        echo "{\"name\":\"$node_name\",\"status\":\"unreachable\",\"error\":\"Unable to connect to node\"}"
        return
    fi
    
    # Extract data from response
    local dag_tip=$(echo "$status_data" | jq -r '.data.dag_height // "unknown"')
    local peer_count=$(echo "$status_data" | jq -r '.data.connected_peers // "unknown"')
    
    # Query proposals data
    local proposal_data
    proposal_data=$(curl -s -m 5 -X GET "${node_url}/api/v1/proposals/summary" 2>/dev/null || echo '{"error": "Failed to get proposals"}')
    
    # Extract proposal counts
    local active_proposals="unknown"
    local pending_proposals="unknown"
    
    if [[ "$proposal_data" != *"error"* ]]; then
        active_proposals=$(echo "$proposal_data" | jq -r '.data.active // 0')
        pending_proposals=$(echo "$proposal_data" | jq -r '.data.pending // 0')
    fi
    
    # Determine sync status by comparing with other nodes
    local sync_status="unknown"
    
    # Output node status as JSON
    echo "{\"name\":\"$node_name\",\"dag_tip\":\"$dag_tip\",\"peers\":\"$peer_count\",\"active_proposals\":\"$active_proposals\",\"pending_proposals\":\"$pending_proposals\",\"status\":\"$sync_status\",\"did\":\"$node_did\"}"
}

# Function to determine sync status
determine_sync_status() {
    # Read all node statuses
    local all_nodes="$1"
    local result=""
    
    # Find the highest DAG tip
    local highest_tip=$(echo "$all_nodes" | jq -r '.[] | .dag_tip' | grep -v "unknown" | sort -n | tail -1)
    
    # If no valid DAG tip found, return the original data
    if [ -z "$highest_tip" ] || [ "$highest_tip" == "null" ]; then
        echo "$all_nodes"
        return
    fi
    
    # Update sync status for each node
    result=$(echo "$all_nodes" | jq --arg highest "$highest_tip" '
        map(
            if .dag_tip == "unknown" then
                . + {"sync_status": "❓ unknown"}
            else if .dag_tip == $highest then
                . + {"sync_status": "✅ synced"}
            else
                $diff = ($highest | tonumber) - (.dag_tip | tonumber);
                if $diff < 3 then
                    . + {"sync_status": "✅ synced"}
                else if $diff < 10 then
                    . + {"sync_status": "⚠️ lagging"}
                else
                    . + {"sync_status": "❌ behind"}
                end
            end
        )
    ')
    
    echo "$result"
}

# Function to display nodes in table format
display_table() {
    local nodes_data="$1"
    
    # Print table header
    printf "%-20s | %-7s | %-5s | %-15s | %-12s\n" "Node" "DAG Tip" "Peers" "Proposals" "Status"
    printf "%s\n" "--------------------+----------+---------+-----------------+--------------"
    
    # Print each node's data
    echo "$nodes_data" | jq -c '.[]' | while read -r node; do
        local name=$(echo "$node" | jq -r '.name')
        local dag_tip=$(echo "$node" | jq -r '.dag_tip')
        local peers=$(echo "$node" | jq -r '.peers')
        local active=$(echo "$node" | jq -r '.active_proposals')
        local pending=$(echo "$node" | jq -r '.pending_proposals')
        local status=$(echo "$node" | jq -r '.sync_status')
        
        if [ "$active" == "unknown" ]; then
            proposals="unknown"
        else
            proposals="${active} active, ${pending} pending"
        fi
        
        printf "%-20s | %-7s | %-5s | %-15s | %-12s\n" "$name" "$dag_tip" "$peers" "$proposals" "$status"
    done
}

# Function to output JSON format
output_json() {
    local nodes_data="$1"
    echo "$nodes_data" | jq '.'
}

# Function to output CSV format
output_csv() {
    local nodes_data="$1"
    
    # Print CSV header
    echo "Node,DAG Tip,Peers,Active Proposals,Pending Proposals,Status"
    
    # Print each node's data
    echo "$nodes_data" | jq -c '.[]' | while read -r node; do
        local name=$(echo "$node" | jq -r '.name')
        local dag_tip=$(echo "$node" | jq -r '.dag_tip')
        local peers=$(echo "$node" | jq -r '.peers')
        local active=$(echo "$node" | jq -r '.active_proposals')
        local pending=$(echo "$node" | jq -r '.pending_proposals')
        local status=$(echo "$node" | jq -r '.sync_status')
        
        echo "\"$name\",\"$dag_tip\",\"$peers\",\"$active\",\"$pending\",\"$status\""
    done
}

# Main function
main() {
    echo "Checking federation node status..."
    
    # Read node list
    local nodes
    nodes=$(jq -c '.' "$NODE_LIST_FILE")
    
    # Create a temporary file to store results
    local temp_file
    temp_file=$(mktemp)
    
    # Initialize output file
    echo "[]" > "$temp_file"
    
    # Process each node
    echo "$nodes" | jq -c '.[]' | while read -r node; do
        local name=$(echo "$node" | jq -r '.name')
        local url=$(echo "$node" | jq -r '.url')
        local did=$(echo "$node" | jq -r '.did')
        
        echo "Querying $name..."
        
        # Get node status
        local status
        status=$(query_node_status "$name" "$url" "$did")
        
        # Add to results file
        jq --argjson node "$status" '. += [$node]' "$temp_file" > "${temp_file}.new"
        mv "${temp_file}.new" "$temp_file"
    done
    
    # Read all results
    local all_results
    all_results=$(cat "$temp_file")
    
    # Determine sync status for all nodes
    local final_results
    final_results=$(determine_sync_status "$all_results")
    
    # Output in requested format
    case "$OUTPUT_FORMAT" in
        json)
            output_json "$final_results"
            ;;
        csv)
            output_csv "$final_results"
            ;;
        *)
            display_table "$final_results"
            ;;
    esac
    
    # Clean up temp file
    rm -f "$temp_file"
}

# Run the main function
main 