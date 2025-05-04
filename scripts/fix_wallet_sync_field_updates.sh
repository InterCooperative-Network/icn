#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

SYNC_DIR="wallet/crates/icn-wallet-sync"

echo "Fixing field changes in the wallet-sync crate..."

# Fix field name changes in trust.rs
TRUST_FILE="$SYNC_DIR/src/trust.rs"
if [ -f "$TRUST_FILE" ]; then
  echo "Fixing field name changes in trust.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Replace field names
  sed -e 's/node\.issuer/node.creator/g' \
      -e 's/node\.payload/node.content/g' \
      -e 's/\"issuer\": /\"creator\": /g' \
      -e 's/issuer: self\.issuer/creator: self.issuer/g' \
      -e 's/signature: self\.signature/signatures: vec![]/g' \
      -e 's/payload: json_bytes/content: json_bytes, content_type: "application\/json".to_string()/g' \
      "$TRUST_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$TRUST_FILE"
  echo "Fixed trust.rs"
fi

# Fix field name changes in compat.rs
COMPAT_FILE="$SYNC_DIR/src/compat.rs"
if [ -f "$COMPAT_FILE" ]; then
  echo "Fixing field name changes in compat.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Replace field names
  sed -e 's/issuer,/creator: issuer,/g' \
      -e 's/signature: Vec::new()/signatures: Vec::new()/g' \
      -e 's/payload,/content: payload, content_type: "application\/json".to_string(),/g' \
      -e 's/current\.issuer/current.creator/g' \
      -e 's/current\.payload/current.content/g' \
      -e 's/timestamp: legacy\.created_at/timestamp: SystemTime::now()/g' \
      -e 's/created_at: current\.timestamp/created_at: Utc::now()/g' \
      "$COMPAT_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$COMPAT_FILE"
  echo "Fixed compat.rs"
fi

# Fix field name changes in federation.rs
FEDERATION_FILE="$SYNC_DIR/src/federation.rs"
if [ -f "$FEDERATION_FILE" ]; then
  echo "Fixing field name changes in federation.rs..."
  # Create a temporary file for the edits
  tmp_file=$(mktemp)
  
  # Replace field names
  sed -e 's/node\.issuer/node.creator/g' \
      -e 's/node\.payload/node.content/g' \
      -e 's/\.poll_recv(/.poll_recv_with_timeout(/g' \
      "$FEDERATION_FILE" > "$tmp_file"
  
  # Replace the original file with the edited one
  mv "$tmp_file" "$FEDERATION_FILE"
  echo "Fixed federation.rs"
fi

# Fix WalletResult import in trust.rs
sed -i 's/use icn_wallet_types::WalletResult;/use crate::WalletResult;/g' "$SYNC_DIR/src/trust.rs"

echo "Wallet sync field updates complete!" 