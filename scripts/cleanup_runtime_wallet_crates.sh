#!/bin/bash
set -e

# Create backup directory
BACKUP_DIR="runtime/crates_backup_$(date +%Y%m%d%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Directories to remove
DIRS_TO_REMOVE=(
  "runtime/crates/wallet-agent"
  "runtime/crates/wallet-core"
  "runtime/crates/wallet-sync"
  "runtime/crates/wallet-ffi"
)

# Back up and remove directories
for dir in "${DIRS_TO_REMOVE[@]}"; do
  if [ -d "$dir" ]; then
    echo "Backing up and removing $dir"
    cp -r "$dir" "$BACKUP_DIR/$(basename "$dir")"
    rm -rf "$dir"
  else
    echo "Directory $dir does not exist, skipping"
  fi
done

echo "Cleanup complete. Backups stored in $BACKUP_DIR" 