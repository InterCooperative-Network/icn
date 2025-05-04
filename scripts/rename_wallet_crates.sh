#!/bin/bash
set -e

# Create backup directory
BACKUP_DIR="wallet/crates_backup_$(date +%Y%m%d%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Directory mappings
declare -A dir_mappings=(
  ["wallet/crates/wallet-types"]="wallet/crates/icn-wallet-types"
  ["wallet/crates/storage"]="wallet/crates/icn-wallet-storage"
  ["wallet/crates/identity"]="wallet/crates/icn-wallet-identity"
  ["wallet/crates/api"]="wallet/crates/icn-wallet-api"
  ["wallet/crates/actions"]="wallet/crates/icn-wallet-actions"
  ["wallet/crates/sync"]="wallet/crates/icn-wallet-sync"
  ["wallet/crates/wallet-agent"]="wallet/crates/icn-wallet-agent"
  ["wallet/crates/wallet-core"]="wallet/crates/icn-wallet-core"
  ["wallet/crates/wallet-ffi"]="wallet/crates/icn-wallet-ffi"
  ["wallet/crates/ffi"]="wallet/crates/icn-wallet-ffi"
)

# Perform the renaming
for old_dir in "${!dir_mappings[@]}"; do
  new_dir="${dir_mappings[$old_dir]}"
  
  # Skip if the old directory doesn't exist
  if [ ! -d "$old_dir" ]; then
    echo "Directory $old_dir does not exist, skipping"
    continue
  fi
  
  # Skip if the new directory already exists
  if [ -d "$new_dir" ]; then
    echo "Directory $new_dir already exists, skipping"
    continue
  fi
  
  echo "Renaming $old_dir to $new_dir"
  
  # Backup the original directory
  cp -r "$old_dir" "$BACKUP_DIR/$(basename "$old_dir")"
  
  # Create new directory and copy contents
  mkdir -p "$new_dir"
  cp -r "$old_dir"/* "$new_dir"
  
  # Create a note in the old directory to help with transition
  mkdir -p "$old_dir"
  echo "This directory has been moved to $new_dir as part of the wallet crate standardization." > "$old_dir/README.md"
done

echo "Directory renaming complete. Backups stored in $BACKUP_DIR"
echo "Please update Cargo.toml dependencies and paths as needed." 