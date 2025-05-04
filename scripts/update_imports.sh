#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "Updating import paths in Rust files..."

# Mapping of old import paths to new ones
declare -A import_mappings=(
  ["use wallet_types"]="use icn_wallet_types"
  ["use wallet-types"]="use icn-wallet-types"
  ["use wallet_storage"]="use icn_wallet_storage"
  ["use wallet-storage"]="use icn-wallet-storage"
  ["use wallet_identity"]="use icn_wallet_identity"
  ["use wallet-identity"]="use icn-wallet-identity"
  ["use wallet_actions"]="use icn_wallet_actions"
  ["use wallet-actions"]="use icn-wallet-actions"
  ["use wallet_api"]="use icn_wallet_api"
  ["use wallet-api"]="use icn-wallet-api"
  ["use wallet_sync"]="use icn_wallet_sync"
  ["use wallet-sync"]="use icn-wallet-sync"
)

# Find all Rust files
find wallet -name "*.rs" | while read file; do
  echo "Processing $file"
  
  # Create a temporary file
  tmp_file=$(mktemp)
  
  # Copy the original file to the temporary file
  cp "$file" "$tmp_file"
  
  # Process each mapping
  for old_import in "${!import_mappings[@]}"; do
    new_import="${import_mappings[$old_import]}"
    sed -i "s/$old_import/$new_import/g" "$tmp_file"
  done
  
  # Check if the file has changed
  if ! cmp -s "$file" "$tmp_file"; then
    # File has changed, replace it
    cp "$tmp_file" "$file"
    echo "  Updated imports in $file"
  fi
  
  # Remove the temporary file
  rm "$tmp_file"
done

echo "Import updates complete!" 