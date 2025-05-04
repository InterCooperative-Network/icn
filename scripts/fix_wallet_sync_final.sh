#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

SYNC_DIR="wallet/crates/icn-wallet-sync"

echo "Fixing remaining issues in the wallet-sync crate..."

# Fix imports and add helper methods in lib.rs
LIB_FILE="$SYNC_DIR/src/lib.rs"
if [ -f "$LIB_FILE" ]; then
  echo "Fixing imports and adding helper methods in lib.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Update the imports to include SystemTime
  sed -i '5i use std::time::SystemTime;' "$LIB_FILE"
  
  # Add the payload_as_json method to DagNode
  awk '
  /impl DagNode {/ {
    print $0;
    print "    // Helper method for compatibility";
    print "    pub fn content_as_json(&self) -> Result<Value, serde_json::Error> {";
    print "        serde_json::from_slice(&self.content)";
    print "    }";
    next;
  }
  { print; }
  ' "$LIB_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$LIB_FILE"
  echo "Fixed lib.rs"
fi

# Fix error.rs to use the correct WalletError import
ERROR_FILE="$SYNC_DIR/src/error.rs"
if [ -f "$ERROR_FILE" ]; then
  echo "Fixing WalletError import in error.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Replace the import
  sed '3s/use icn_wallet_types::error::WalletError;/use icn_wallet_types::WalletError;/' "$ERROR_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$ERROR_FILE"
  echo "Fixed error.rs"
fi

# Fix compat.rs to use the correct imports and types
COMPAT_FILE="$SYNC_DIR/src/compat.rs"
if [ -f "$COMPAT_FILE" ]; then
  echo "Fixing imports and types in compat.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Add SystemTime import
  sed -i '8i use std::time::SystemTime;' "$COMPAT_FILE"
  sed -i '9i use chrono::Utc;' "$COMPAT_FILE"
  
  # Replace the DagNodeMetadata reference
  sed 's/icn_wallet_types::DagNodeMetadata::default()/crate::DagNodeMetadata::default()/g' "$COMPAT_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$COMPAT_FILE"
  echo "Fixed compat.rs"
fi

# Fix trust.rs to fix issues with serde_json and federation.rs to fix poll_recv
TRUST_FILE="$SYNC_DIR/src/trust.rs"
if [ -f "$TRUST_FILE" ]; then
  echo "Fixing serde_json and other issues in trust.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Fix the serde_json issue
  sed -e 's/let value = serde_json::to_value(self)/let value = serde_json::to_value(\&*self)/g' \
      -e 's/signatures: vec!\[\].clone().unwrap_or_default().into_bytes()/signatures: vec!\[\]/g' \
      "$TRUST_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$TRUST_FILE"
  echo "Fixed trust.rs"
fi

FEDERATION_FILE="$SYNC_DIR/src/federation.rs"
if [ -f "$FEDERATION_FILE" ]; then
  echo "Fixing poll_recv_with_timeout issue in federation.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Fix the poll_recv_with_timeout issue
  sed 's/poll_recv_with_timeout/poll_recv/g' "$FEDERATION_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$FEDERATION_FILE"
  echo "Fixed federation.rs"
fi

echo "Wallet sync final fixes complete!" 