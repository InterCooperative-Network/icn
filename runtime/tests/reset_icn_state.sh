#!/bin/bash
# reset_icn_state.sh
# 
# This script resets the ICN Runtime state for clean test runs.
# It can be called by automated test frameworks to ensure test isolation.

set -e

# Default directories - override with environment variables if needed
DATA_DIR=${ICN_DATA_DIR:-"./data"}
LOG_DIR=${ICN_LOG_DIR:-"./logs"}
BACKUP_DIR="./data_backups"

# Create backup directory if it doesn't exist
mkdir -p "$BACKUP_DIR"

# Function to display usage
function show_usage {
  echo "Usage: $0 [options]"
  echo "Reset ICN Runtime state for clean test runs"
  echo ""
  echo "Options:"
  echo "  -h, --help                Display this help message"
  echo "  -k, --keep-backup         Create a timestamped backup of current data"
  echo "  -m, --mode <MODE>         Reset mode: 'full' (all data) or 'partial' (preserve keys)"
  echo "  -d, --data-dir <DIR>      Data directory (default: ./data)"
  echo "  -l, --log-dir <DIR>       Log directory (default: ./logs)"
  echo ""
}

# Default options
KEEP_BACKUP=false
RESET_MODE="full"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      show_usage
      exit 0
      ;;
    -k|--keep-backup)
      KEEP_BACKUP=true
      shift
      ;;
    -m|--mode)
      RESET_MODE="$2"
      shift 2
      ;;
    -d|--data-dir)
      DATA_DIR="$2"
      shift 2
      ;;
    -l|--log-dir)
      LOG_DIR="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      show_usage
      exit 1
      ;;
  esac
done

# Verify reset mode is valid
if [[ "$RESET_MODE" != "full" && "$RESET_MODE" != "partial" ]]; then
  echo "Error: Invalid reset mode '$RESET_MODE'. Must be 'full' or 'partial'."
  exit 1
fi

# Create backup if requested
if $KEEP_BACKUP; then
  TIMESTAMP=$(date +%Y%m%d_%H%M%S)
  BACKUP_PATH="$BACKUP_DIR/icn_data_$TIMESTAMP"
  echo "Creating backup at $BACKUP_PATH"
  mkdir -p "$BACKUP_PATH"
  
  if [ -d "$DATA_DIR" ]; then
    cp -r "$DATA_DIR" "$BACKUP_PATH/"
  fi
  
  if [ -d "$LOG_DIR" ]; then
    cp -r "$LOG_DIR" "$BACKUP_PATH/"
  fi
  
  echo "Backup complete"
fi

# Function to stop the ICN Runtime container if it's running
function stop_runtime {
  if docker ps | grep -q icn-runtime; then
    echo "Stopping ICN Runtime container..."
    docker stop icn-runtime || true
    sleep 2
  fi
}

# Stop the runtime if it's running in Docker
stop_runtime

echo "Resetting ICN Runtime state (mode: $RESET_MODE)..."

# Clear data directories
if [ -d "$DATA_DIR" ]; then
  if [ "$RESET_MODE" = "full" ]; then
    # Full reset - remove everything except the keys directory
    find "$DATA_DIR" -mindepth 1 -not -path "$DATA_DIR/keys*" -exec rm -rf {} \; 2>/dev/null || true
    echo "Cleared all data except keys"
  else
    # Partial reset - keep keys and configurations
    if [ -d "$DATA_DIR/storage" ]; then
      rm -rf "$DATA_DIR/storage"/*
      echo "Cleared storage data"
    fi
    
    if [ -d "$DATA_DIR/db" ]; then
      rm -rf "$DATA_DIR/db"/*
      echo "Cleared database data"
    fi
    
    if [ -d "$DATA_DIR/dag" ]; then
      rm -rf "$DATA_DIR/dag"/*
      echo "Cleared DAG data"
    fi
  fi
  
  # Create necessary subdirectories
  mkdir -p "$DATA_DIR/storage"
  mkdir -p "$DATA_DIR/db"
  mkdir -p "$DATA_DIR/dag"
  mkdir -p "$DATA_DIR/keys"
  
  # Set proper permissions
  chmod -R 755 "$DATA_DIR"
else
  echo "Data directory does not exist, creating it..."
  mkdir -p "$DATA_DIR"
  mkdir -p "$DATA_DIR/storage"
  mkdir -p "$DATA_DIR/db"
  mkdir -p "$DATA_DIR/dag"
  mkdir -p "$DATA_DIR/keys"
  chmod -R 755 "$DATA_DIR"
fi

# Clear log files
if [ -d "$LOG_DIR" ]; then
  rm -f "$LOG_DIR"/*
  echo "Cleared logs"
else
  mkdir -p "$LOG_DIR"
fi

echo "ICN Runtime state reset complete"
echo "You can now restart the runtime with a clean state"

exit 0 