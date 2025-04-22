#!/bin/bash
# federationd.sh - Federation Daemon for ICN
# Watches drafts/ directory for .dsl proposals, submits them to DAG, 
# triggers votes from guardian consensus, and aggregates outcomes

set -e

# Configuration
DRAFTS_DIR="./drafts"
CONFIG_DIR="./config"
LOG_FILE="./logs/federationd.log"
SLEEP_INTERVAL=60  # Check interval in seconds
IDENTITY_FILE="./config/federation-identity.json"

# Ensure directories exist
mkdir -p "$DRAFTS_DIR" "$CONFIG_DIR" "./logs"

# Initialize logging
log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Check for required tools and files
check_requirements() {
  if [ ! -f "$IDENTITY_FILE" ]; then
    log "ERROR: Federation identity file not found at $IDENTITY_FILE"
    log "Please run: ./icn-wallet init --scope federation --username node"
    exit 1
  fi

  # Check if we have the required CLI tools
  command -v jq >/dev/null 2>&1 || { log "ERROR: jq is required but not installed"; exit 1; }
  command -v curl >/dev/null 2>&1 || { log "ERROR: curl is required but not installed"; exit 1; }
}

# Function to process a draft proposal
process_proposal() {
  local proposal_file="$1"
  local proposal_name=$(basename "$proposal_file")
  
  log "Processing proposal: $proposal_name"
  
  # Validate proposal format
  if ! grep -q "// Title:" "$proposal_file"; then
    log "ERROR: Invalid proposal format (missing Title) in $proposal_name - skipping"
    return 1
  fi
  
  # Get proposal hash
  local proposal_hash=$(./icn-wallet proposal hash --file "$proposal_file")
  
  # Check if proposal already exists in DAG
  local proposal_exists=$(./icn-wallet proposal query --hash "$proposal_hash" 2>/dev/null || echo "")
  if [ -n "$proposal_exists" ]; then
    log "Proposal $proposal_hash already exists in DAG - skipping"
    # Move to processed directory
    mkdir -p "$DRAFTS_DIR/processed"
    mv "$proposal_file" "$DRAFTS_DIR/processed/"
    return 0
  fi
  
  # Submit the proposal to the DAG
  log "Submitting proposal $proposal_hash to DAG..."
  if ./icn-wallet proposal submit --file "$proposal_file"; then
    log "Successfully submitted proposal $proposal_hash"
    
    # Trigger guardian voting
    log "Notifying guardians for consensus voting..."
    ./icn-wallet guardians notify --proposal-hash "$proposal_hash" --message "New proposal requires your vote"
    
    # Move to submitted directory
    mkdir -p "$DRAFTS_DIR/submitted"
    mv "$proposal_file" "$DRAFTS_DIR/submitted/$proposal_name"
  else
    log "ERROR: Failed to submit proposal $proposal_hash"
    # Move to failed directory
    mkdir -p "$DRAFTS_DIR/failed"
    mv "$proposal_file" "$DRAFTS_DIR/failed/$proposal_name"
    return 1
  fi
}

# Function to process guardian votes
process_votes() {
  # Get all active proposals
  local active_proposals=$(./icn-wallet proposal list --status voting)
  
  if [ -z "$active_proposals" ]; then
    return 0
  fi
  
  log "Processing votes for active proposals"
  
  # For each active proposal
  echo "$active_proposals" | jq -c '.[]' | while read -r proposal; do
    local proposal_hash=$(echo "$proposal" | jq -r '.hash')
    log "Checking votes for proposal $proposal_hash"
    
    # Get vote counts
    local vote_stats=$(./icn-wallet proposal vote-stats --hash "$proposal_hash")
    local yes_votes=$(echo "$vote_stats" | jq -r '.yes')
    local no_votes=$(echo "$vote_stats" | jq -r '.no')
    local abstain_votes=$(echo "$vote_stats" | jq -r '.abstain')
    local threshold=$(echo "$vote_stats" | jq -r '.threshold')
    local total_votes=$((yes_votes + no_votes + abstain_votes))
    
    log "Proposal $proposal_hash: $yes_votes yes, $no_votes no, $abstain_votes abstain (threshold: $threshold)"
    
    # Check if threshold reached
    if [ "$total_votes" -ge "$threshold" ]; then
      if [ "$yes_votes" -gt "$no_votes" ]; then
        log "Proposal $proposal_hash PASSED with majority YES"
        ./icn-wallet proposal execute --hash "$proposal_hash" || log "ERROR: Failed to execute proposal $proposal_hash"
      else
        log "Proposal $proposal_hash REJECTED with majority NO"
        ./icn-wallet proposal reject --hash "$proposal_hash" || log "ERROR: Failed to reject proposal $proposal_hash"
      fi
      
      # Sync with AgoraNet
      ./icn-wallet agoranet sync --proposal-hash "$proposal_hash" || log "WARNING: Failed to sync proposal $proposal_hash with AgoraNet"
    fi
  done
}

# Function to sync DAG events and replay
sync_dag() {
  log "Syncing DAG events..."
  ./icn-wallet dag sync || log "WARNING: DAG sync failed"
  
  # Replay events if needed
  ./icn-wallet dag replay-pending || log "WARNING: DAG replay failed"
}

# Main daemon loop
run_daemon() {
  log "Starting Federation Daemon"
  log "Watching for proposals in $DRAFTS_DIR"
  
  while true; do
    # Process any new proposal drafts
    log "Checking for new proposals..."
    find "$DRAFTS_DIR" -maxdepth 1 -name "*.dsl" -type f | while read -r proposal_file; do
      process_proposal "$proposal_file"
    done
    
    # Process any guardian votes on active proposals
    process_votes
    
    # Sync with DAG to get latest events
    sync_dag
    
    # Sleep before next cycle
    log "Sleeping for $SLEEP_INTERVAL seconds..."
    sleep "$SLEEP_INTERVAL"
  done
}

# Handle signals
trap 'log "Stopping Federation Daemon"; exit 0' SIGINT SIGTERM

# Start daemon
check_requirements
run_daemon 