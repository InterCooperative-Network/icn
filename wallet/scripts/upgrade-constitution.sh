#!/bin/bash
# upgrade-constitution.sh - ICN Constitution Upgrade Manager
# Handles constitution upgrades with proper DAG archiving and rollback capability

set -e

# Configuration
CONFIG_DIR="./config"
CONSTITUTION_FILE="$CONFIG_DIR/constitution.ccl"
ARCHIVE_DIR="$CONFIG_DIR/archive/constitution"
CURRENT_TIP_FILE="$CONFIG_DIR/dag-tip.json"

# Ensure directories exist
mkdir -p "$CONFIG_DIR" "$ARCHIVE_DIR"

# Usage information
show_usage() {
  echo "Usage: $0 [options]"
  echo ""
  echo "Options:"
  echo "  --file, -f         Path to new constitution file (.ccl)"
  echo "  --reason, -r       Reason for the upgrade"
  echo "  --dry-run, -d      Validate without applying changes"
  echo "  --force            Skip confirmation prompts"
  echo "  --archive-only     Only archive current constitution"
  echo "  --list             List available constitution versions"
  echo "  --help, -h         Show this help message"
  echo ""
  echo "Examples:"
  echo "  $0 --file new-constitution.ccl --reason \"Add emergency procedures\""
  echo "  $0 --dry-run --file proposed-constitution.ccl"
  echo "  $0 --list"
  exit 1
}

# Check for required commands
command -v jq >/dev/null 2>&1 || { echo "Error: jq is required but not installed."; exit 1; }

# Parse arguments
FILE=""
REASON=""
DRY_RUN=false
FORCE=false
ARCHIVE_ONLY=false
LIST=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --file|-f)
      FILE="$2"
      shift 2
      ;;
    --reason|-r)
      REASON="$2"
      shift 2
      ;;
    --dry-run|-d)
      DRY_RUN=true
      shift
      ;;
    --force)
      FORCE=true
      shift
      ;;
    --archive-only)
      ARCHIVE_ONLY=true
      shift
      ;;
    --list)
      LIST=true
      shift
      ;;
    --help|-h)
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

# Function to archive current constitution
archive_current_constitution() {
  echo "Archiving current constitution..."
  
  # Check if current constitution exists
  if [ ! -f "$CONSTITUTION_FILE" ]; then
    echo "No current constitution found. Nothing to archive."
    return
  fi
  
  # Get current DAG tip
  local dag_tip
  if ! dag_tip=$(./icn-wallet dag tip 2>/dev/null); then
    echo "Warning: Failed to get current DAG tip. Using timestamp instead."
    dag_tip="timestamp-$(date +%s)"
  fi
  
  # Generate timestamp for archive filename
  local timestamp=$(date +%s)
  local archive_file="$ARCHIVE_DIR/constitution-$timestamp.ccl"
  local dag_tip_file="$ARCHIVE_DIR/constitution-$timestamp.dag-tip"
  
  # Copy current constitution to archive
  cp "$CONSTITUTION_FILE" "$archive_file"
  echo "$dag_tip" > "$dag_tip_file"
  
  echo "Constitution archived as: $archive_file"
  echo "DAG tip saved as: $dag_tip_file"
  
  # Save DAG tip to current tip file for future reference
  echo "$dag_tip" > "$CURRENT_TIP_FILE"
}

# If archive-only is specified, archive the current constitution and exit
if [ "$ARCHIVE_ONLY" = true ]; then
  archive_current_constitution
  exit 0
fi

# Check if required file is specified
if [ -z "$FILE" ]; then
  echo "Error: Constitution file must be specified with --file option."
  show_usage
fi

# Check if file exists
if [ ! -f "$FILE" ]; then
  echo "Error: Constitution file not found: $FILE"
  exit 1
fi

# Validate constitution file format
if ! grep -q "^constitution {" "$FILE"; then
  echo "Error: Invalid constitution file format. Missing 'constitution {' declaration."
  exit 1
fi

# Compute hash of the new constitution
NEW_HASH=$(sha256sum "$FILE" | cut -d ' ' -f 1)

# Get hash of current constitution if it exists
CURRENT_HASH=""
if [ -f "$CONSTITUTION_FILE" ]; then
  CURRENT_HASH=$(sha256sum "$CONSTITUTION_FILE" | cut -d ' ' -f 1)
  
  # Check if new constitution is identical to current
  if [ "$NEW_HASH" == "$CURRENT_HASH" ]; then
    echo "Warning: New constitution is identical to current constitution."
    if [ "$FORCE" != true ]; then
      read -p "Continue anyway? (y/N): " confirm
      if [[ "$confirm" != [yY] ]]; then
        echo "Operation cancelled."
        exit 0
      fi
    fi
  fi
fi

# Function to validate the constitution
validate_constitution() {
  echo "Validating constitution file..."
  
  # Check for syntax errors
  if ! ./icn-wallet validate ccl --file "$FILE" 2>/dev/null; then
    echo "Error: Constitution validation failed. The file contains syntax errors."
    return 1
  fi
  
  echo "Constitution validation successful."
  return 0
}

# Validate the new constitution
if ! validate_constitution; then
  if [ "$FORCE" != true ]; then
    echo "Constitution validation failed. Use --force to override."
    exit 1
  else
    echo "Warning: Proceeding with invalid constitution due to --force flag."
  fi
fi

# If dry-run is specified, exit after validation
if [ "$DRY_RUN" = true ]; then
  echo "Dry run completed. No changes applied."
  exit 0
fi

# Archive current constitution
archive_current_constitution

# Apply the new constitution
echo "Applying new constitution..."

# Create proposal for constitution upgrade
PROPOSAL_FILE="$CONFIG_DIR/constitution-upgrade-$(date +%s).dsl"

cat > "$PROPOSAL_FILE" << EOF
// Title: Constitution Upgrade
// Description: ${REASON:-"Upgrade to new constitution version"}
// Author: $(./icn-wallet whoami 2>/dev/null | grep "DID:" | cut -d' ' -f2 || echo "system")
// Date: $(date +"%Y-%m-%d")

constitution_upgrade {
  prev_hash: "${CURRENT_HASH:-"none"}",
  new_hash: "${NEW_HASH}",
  reason: "${REASON:-"Upgrade to new constitution version"}",
  
  // Constitution data (Base64 encoded)
  constitution_data: "$(base64 -w 0 "$FILE")",
  
  // Validation check
  validate: |
    (ctx) => {
      // Check caller has proper authority
      return ctx.state.hasConstitutionAuthority(ctx.caller);
    }
}

// Implementation
execute: |
  (ctx) => {
    const { constitution_data, new_hash } = ctx.params;
    
    // Update the constitution
    const result = ctx.state.updateConstitution(constitution_data, new_hash);
    
    // Log the update to AgoraNet
    ctx.agoranet.notify({
      type: "constitution_upgrade",
      prev_hash: ctx.params.prev_hash,
      new_hash: ctx.params.new_hash,
      timestamp: ctx.timestamp
    });
    
    return { 
      success: result.success, 
      message: result.message || "Constitution updated successfully" 
    };
  }
EOF

echo "Created constitution upgrade proposal: $PROPOSAL_FILE"

# Submit the proposal
if [ "$FORCE" = true ]; then
  echo "Submitting constitution upgrade proposal..."
  ./icn-wallet proposal submit --file "$PROPOSAL_FILE" || {
    echo "Failed to submit constitution upgrade proposal. Manual intervention required."
    exit 1
  }
else
  echo "To apply this constitution upgrade, submit the proposal:"
  echo "  ./icn-wallet proposal submit --file \"$PROPOSAL_FILE\""
fi

# Copy new constitution to config directory
cp "$FILE" "$CONSTITUTION_FILE"
echo "New constitution installed at: $CONSTITUTION_FILE"

# Display success message
echo "Constitution upgrade process completed."
echo "New constitution hash: $NEW_HASH"
if [ -n "$CURRENT_HASH" ]; then
  echo "Previous constitution hash: $CURRENT_HASH"
fi 