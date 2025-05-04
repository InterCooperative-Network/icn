#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

SYNC_DIR="wallet/crates/icn-wallet-sync"

echo "Fixing import issues in the wallet-sync crate..."

# Fix duplicate imports and other issues in lib.rs
LIB_FILE="$SYNC_DIR/src/lib.rs"
if [ -f "$LIB_FILE" ]; then
  echo "Fixing imports in lib.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Remove duplicate SystemTime import and replace with correct imports
  grep -v "use std::time::SystemTime;" "$LIB_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$LIB_FILE"
  
  # Insert the SystemTime import at the correct position
  sed -i '1s/^/use std::time::SystemTime;\n/' "$LIB_FILE"
  echo "Fixed lib.rs"
fi

# Fix duplicate imports in compat.rs
COMPAT_FILE="$SYNC_DIR/src/compat.rs"
if [ -f "$COMPAT_FILE" ]; then
  echo "Fixing imports in compat.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Remove all Utc imports
  grep -v "use chrono::Utc;" "$COMPAT_FILE" | grep -v "use chrono::{DateTime, Utc};" > "$tmp_file"
  
  # Insert correct imports at the top
  sed -i '1s/^/use chrono::{DateTime, Utc};\n/' "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$COMPAT_FILE"
  echo "Fixed compat.rs"
fi

# Fix WalletError import in error.rs
ERROR_FILE="$SYNC_DIR/src/error.rs"
if [ -f "$ERROR_FILE" ]; then
  echo "Fixing WalletError import in error.rs again..."
  
  # Check if icn-wallet-types/src/error.rs exists
  if [ -f "wallet/crates/icn-wallet-types/src/error.rs" ]; then
    echo "Using import from error module"
    sed -i '3s/use icn_wallet_types::WalletError;/use icn_wallet_types::error::WalletError;/' "$ERROR_FILE"
  else
    echo "Using WalletError from main module"
    # Likely the WalletError is in the main module
    sed -i '3s/use icn_wallet_types::WalletError;/use icn_wallet_types::WalletError;/' "$ERROR_FILE"
  fi
  echo "Fixed error.rs"
fi

# Fix poll_recv issue in federation.rs
FEDERATION_FILE="$SYNC_DIR/src/federation.rs"
if [ -f "$FEDERATION_FILE" ]; then
  echo "Fixing poll_recv issue in federation.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Replace the poll_recv implementation with a basic implementation using non-blocking receive
  awk '
  /match Pin::new\(&mut self\.receiver\)\.poll_recv\(cx\) {/ {
    print "        // Manual implementation for poll_recv";
    print "        match self.receiver.try_recv() {";
    print "            Ok(bundle) => Poll::Ready(Some(bundle)),";
    print "            Err(TryRecvError::Empty) => Poll::Pending,";
    print "            Err(_) => Poll::Ready(None),";
    print "        }";
    # Skip the next few lines
    getline;
    while ($0 !~ /},/ && $0 !~ /}$/) { getline; }
    next;
  }
  { print; }
  ' "$FEDERATION_FILE" > "$tmp_file"
  
  # Add tokio::sync::broadcast try_recv import
  sed -i '16i use tokio::sync::broadcast::error::TryRecvError;' "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$FEDERATION_FILE"
  echo "Fixed federation.rs"
fi

echo "Wallet sync import fixes complete!" 