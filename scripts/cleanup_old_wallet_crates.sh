#!/bin/bash
set -e

# Create backup directory (just in case we need the old directories)
BACKUP_DIR="wallet/old_crates_backup_$(date +%Y%m%d%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Directories to remove
OLD_DIRS=(
  "wallet/crates/sync"
  "wallet/crates/storage" 
  "wallet/crates/identity"
  "wallet/crates/actions"
  "wallet/crates/api"
  "wallet/crates/wallet-types"
  "wallet/crates/wallet-agent"
  "wallet/crates/wallet-core"
  "wallet/crates/wallet-ffi"
  "wallet/crates/ffi"
)

# Back up and remove directories
for dir in "${OLD_DIRS[@]}"; do
  if [ -d "$dir" ]; then
    echo "Backing up and removing $dir"
    cp -r "$dir" "$BACKUP_DIR/$(basename "$dir")"
    rm -rf "$dir"
  else
    echo "Directory $dir does not exist, skipping"
  fi
done

echo "Old wallet crates cleanup complete. Backups stored in $BACKUP_DIR" 