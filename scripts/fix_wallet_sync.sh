#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

SYNC_DIR="wallet/crates/sync"
NEW_SYNC_DIR="wallet/crates/icn-wallet-sync"

# Use the correct directory based on whether the refactoring has already happened
if [ -d "$NEW_SYNC_DIR" ]; then
  WORKING_DIR="$NEW_SYNC_DIR"
else
  WORKING_DIR="$SYNC_DIR"
fi

echo "Fixing wallet/crates/sync specific issues..."

# Fix WalletError import in error.rs
ERROR_FILE="$WORKING_DIR/src/error.rs"
if [ -f "$ERROR_FILE" ]; then
  echo "Fixing WalletError import in error.rs..."
  sed -i 's/use icn_wallet_types::WalletError;/use icn_wallet_types::error::WalletError;/g' "$ERROR_FILE"
fi

# Fix backoff::future::retry issue in lib.rs
LIB_FILE="$WORKING_DIR/src/lib.rs"
if [ -f "$LIB_FILE" ]; then
  echo "Fixing backoff::future::retry issue in lib.rs..."
  # Create a temporary file for the edits
  tmp_lib_file=$(mktemp)
  
  # Add explicit import for backoff::future::retry
  awk '
  /use backoff::{ExponentialBackoff, backoff::Backoff};/ {
    print $0;
    print "use futures::future::TryFutureExt;";
    print "use backoff::future::retry_notify;";
    next;
  }
  
  # Replace the retry call with retry_notify
  /let result = backoff::future::retry\(backoff, operation\).await\?;/ {
    print "        let result = retry_notify(backoff, operation, |err, dur| {";
    print "            warn!(\"Retrying after {:?} due to error: {}\", dur, err);";
    print "        }).await?;";
    next;
  }
  
  # Fix test function to match the new DagNode structure
  /let node = DagNode {/ {
    print "        let node = DagNode {";
    print "            cid: \"test-cid\".to_string(),";
    print "            parents: vec![\"ref1\".to_string(), \"ref2\".to_string()],";
    print "            timestamp: SystemTime::now(),";
    print "            creator: \"did:icn:test\".to_string(),";
    print "            content: serde_json::to_vec(&json!({ \"test\": \"value\" })).unwrap(),";
    print "            content_type: \"application/json\".to_string(),";
    print "            signatures: vec![],";
    print "            metadata: DagNodeMetadata {";
    print "                sequence: Some(1),";
    print "                scope: Some(\"test\".to_string()),";
    print "            },";
    print "        };";
    getline; while ($0 !~ /};/) { getline; }  # Skip until the closing brace
    next;
  }
  
  # Fix payload field reference
  /assert_eq\(node.payload, node2.payload\);/ {
    print "        assert_eq!(node.content, node2.content);";
    next;
  }
  
  # Default action: print the line
  { print; }
  ' "$LIB_FILE" > "$tmp_lib_file"
  
  # Replace the original file with the edited one
  mv "$tmp_lib_file" "$LIB_FILE"
  echo "Fixed lib.rs"
fi

echo "Wallet sync fixes complete!" 