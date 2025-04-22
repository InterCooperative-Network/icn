#!/bin/bash
# rollback-constitution.sh - Roll back to a previous constitution version
# Fetches a previous .ccl file and submits a downgrade proposal

set -e

# Configuration
CONFIG_DIR="./config"
CONSTITUTION_FILE="$CONFIG_DIR/constitution.ccl"
ARCHIVE_DIR="$CONFIG_DIR/archive/constitution"
PROPOSAL_DIR="$CONFIG_DIR"

# Ensure directories exist
mkdir -p "$CONFIG_DIR" "$ARCHIVE_DIR"

# Usage information
show_usage() {
  echo "Usage: $0 [options]"
  echo ""
  echo "Options:"
  echo "  --hash, -h         Hash of the constitution to roll back to"
  echo "  --timestamp, -t    Timestamp of the constitution to roll back to"
  echo "  --reason, -r       Reason for the rollback"
  echo "  --force, -f        Skip confirmation prompts"
  echo "  --list             List available constitution versions"
  echo "  --help             Show this help message"
  echo ""
  echo "Examples:"
  echo "  $0 --hash abc123def456... --reason \"Fix governance issue\""
  echo "  $0 --timestamp 1609459200 --reason \"Revert emergency changes\""
  echo "  $0 --list"
  exit 1
}

# Check for required commands
command -v jq >/dev/null 2>&1 || { echo "Error: jq is required but not installed."; exit 1; }

# Parse arguments
HASH=""
TIMESTAMP=""
REASON=""
FORCE=false
LIST=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --hash|-h)
      HASH="$2"
      shift 2
      ;;
    --timestamp|-t)
      TIMESTAMP="$2"
      shift 2
      ;;
    --reason|-r)
      REASON="$2"
      shift 2
      ;;
    --force|-f)
      FORCE=true
      shift
      ;;
    --list)
      LIST=true
      shift
      ;;
    --help)
      show_usage
      ;;
    *)
      echo "Unknown option: $1"
      show_usage
      ;;
  esac
done

# Function to list archived constitutions
list_constitutions() {
  if [ ! -d "$ARCHIVE_DIR" ] || [ ! "$(ls -A "$ARCHIVE_DIR")" ]; then
    echo "No archived constitutions found."
    exit 0
  fi
  
  echo "Available constitution versions:"
  echo "-----------------------------------"
  
  # Get current constitution hash if it exists
  local current_hash=""
  if [ -f "$CONSTITUTION_FILE" ]; then
    current_hash=$(sha256sum "$CONSTITUTION_FILE" | cut -d ' ' -f 1)
  fi
  
  # Find all archived constitutions
  for file in "$ARCHIVE_DIR"/*.ccl; do
    if [ -f "$file" ]; then
      local timestamp=$(basename "$file" | sed 's/constitution-\(.*\)\.ccl/\1/')
      local date_human=$(date -d "@$timestamp" "+%Y-%m-%d %H:%M:%S" 2>/dev/null || echo "Unknown date")
      local hash=$(sha256sum "$file" | cut -d ' ' -f 1)
      local dag_tip_file="${file%.ccl}.dag-tip"
      local dag_tip="Unknown"
      
      if [ -f "$dag_tip_file" ]; then
        dag_tip=$(cat "$dag_tip_file")
      fi
      
      local current=""
      if [ "$hash" == "$current_hash" ]; then
        current=" (CURRENT)"
      fi
      
      echo "[$date_human]$current"
      echo "  Hash: $hash"
      echo "  Timestamp: $timestamp"
      echo "  DAG Tip: $dag_tip"
      echo "  File: $file"
      echo ""
    fi
  done
}

# If list option is specified, show archived constitutions and exit
if [ "$LIST" = true ]; then
  list_constitutions
  exit 0
fi

# Check if either hash or timestamp is provided
if [ -z "$HASH" ] && [ -z "$TIMESTAMP" ]; then
  echo "Error: Either --hash or --timestamp must be specified."
  show_usage
fi

# Function to find constitution file by hash
find_constitution_by_hash() {
  local target_hash="$1"
  local found_file=""
  
  for file in "$ARCHIVE_DIR"/*.ccl; do
    if [ -f "$file" ]; then
      local hash=$(sha256sum "$file" | cut -d ' ' -f 1)
      if [ "$hash" == "$target_hash" ]; then
        found_file="$file"
        break
      fi
    fi
  done
  
  echo "$found_file"
}

# Function to find constitution file by timestamp
find_constitution_by_timestamp() {
  local target_timestamp="$1"
  local found_file=""
  
  for file in "$ARCHIVE_DIR"/constitution-"$target_timestamp".ccl; do
    if [ -f "$file" ]; then
      found_file="$file"
      break
    fi
  done
  
  echo "$found_file"
}

# Find the target constitution file
TARGET_FILE=""
if [ -n "$HASH" ]; then
  TARGET_FILE=$(find_constitution_by_hash "$HASH")
elif [ -n "$TIMESTAMP" ]; then
  TARGET_FILE=$(find_constitution_by_timestamp "$TIMESTAMP")
fi

if [ -z "$TARGET_FILE" ] || [ ! -f "$TARGET_FILE" ]; then
  echo "Error: Could not find a constitution matching the specified criteria."
  echo "Use --list to see available constitutions."
  exit 1
fi

# Get current constitution hash
CURRENT_HASH=""
if [ -f "$CONSTITUTION_FILE" ]; then
  CURRENT_HASH=$(sha256sum "$CONSTITUTION_FILE" | cut -d ' ' -f 1)
fi

# Get target constitution hash
TARGET_HASH=$(sha256sum "$TARGET_FILE" | cut -d ' ' -f 1)

# Check if target is already the current constitution
if [ "$TARGET_HASH" == "$CURRENT_HASH" ]; then
  echo "The specified constitution is already active."
  exit 0
fi

# Display rollback information
echo "Rolling back constitution:"
echo "-----------------------------------"
echo "Current hash: $CURRENT_HASH"
echo "Target hash: $TARGET_HASH"
echo "Target file: $TARGET_FILE"
echo "Reason: ${REASON:-"Rollback to previous constitution"}"
echo "-----------------------------------"

# Confirm rollback
if [ "$FORCE" != true ]; then
  read -p "Are you sure you want to proceed with this rollback? (y/N): " confirm
  if [[ "$confirm" != [yY] ]]; then
    echo "Rollback cancelled."
    exit 0
  fi
fi

# Create proposal for constitution rollback
PROPOSAL_FILE="$PROPOSAL_DIR/constitution-rollback-$(date +%s).dsl"

cat > "$PROPOSAL_FILE" << EOF
// Title: Constitution Rollback
// Description: ${REASON:-"Rollback to previous constitution version"}
// Author: $(./icn-wallet whoami 2>/dev/null | grep "DID:" | cut -d' ' -f2 || echo "system")
// Date: $(date +"%Y-%m-%d")

constitution_rollback {
  current_hash: "${CURRENT_HASH}",
  target_hash: "${TARGET_HASH}",
  reason: "${REASON:-"Rollback to previous constitution version"}",
  
  // Constitution data (Base64 encoded)
  constitution_data: "$(base64 -w 0 "$TARGET_FILE")",
  
  // Validation check
  validate: |
    (ctx) => {
      // Check caller has proper authority
      if (!ctx.state.hasConstitutionAuthority(ctx.caller)) {
        return { valid: false, reason: "Caller lacks constitution authority" };
      }
      
      // Check target constitution exists in archives
      if (!ctx.state.constitutionHashExists(ctx.params.target_hash)) {
        return { valid: false, reason: "Target constitution hash not found in archives" };
      }
      
      return { valid: true };
    }
}

// Implementation
execute: |
  (ctx) => {
    const { constitution_data, target_hash, current_hash } = ctx.params;
    
    // Log the rollback event
    ctx.state.logConstitutionEvent({
      type: "rollback",
      from_hash: current_hash,
      to_hash: target_hash,
      initiator: ctx.caller,
      timestamp: ctx.timestamp
    });
    
    // Perform the rollback
    const result = ctx.state.updateConstitution(constitution_data, target_hash);
    
    // Notify federation nodes via AgoraNet
    ctx.agoranet.notify({
      type: "constitution_rollback",
      from_hash: current_hash,
      to_hash: target_hash,
      reason: ctx.params.reason,
      timestamp: ctx.timestamp
    });
    
    return {
      success: result.success,
      message: result.message || "Constitution rolled back successfully"
    };
  }
EOF

echo "Created constitution rollback proposal: $PROPOSAL_FILE"

# Submit the proposal
if [ "$FORCE" = true ]; then
  echo "Submitting constitution rollback proposal..."
  ./icn-wallet proposal submit --file "$PROPOSAL_FILE" --priority high || {
    echo "Failed to submit constitution rollback proposal. Manual intervention required."
    exit 1
  }
else
  echo "To execute this rollback, submit the proposal:"
  echo "  ./icn-wallet proposal submit --file \"$PROPOSAL_FILE\" --priority high"
fi

echo ""
echo "Rollback proposal created. Once approved, the constitution will be"
echo "rolled back to the version with hash: $TARGET_HASH" 