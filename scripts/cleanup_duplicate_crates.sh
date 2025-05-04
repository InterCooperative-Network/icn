#!/bin/bash
# Script to clean up duplicate crate directories
# Run this after resolving dependencies in Cargo.toml

set -e

echo "ICN Monorepo Duplicate Crate Cleanup"
echo "===================================="

DUPLICATES=(
  "runtime/crates/wallet-agent"
  "runtime/crates/wallet-core"
  "runtime/crates/wallet-ffi"
  "runtime/crates/wallet-sync"
  "wallet/crates/ffi"  # Old path now renamed to wallet-ffi
)

BACKUP_DIR="duplicate_crates_backup_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

echo "Backing up duplicate crates to $BACKUP_DIR..."

for dir in "${DUPLICATES[@]}"; do
  if [ -d "$dir" ]; then
    echo "- Backing up $dir"
    mkdir -p "$BACKUP_DIR/$(dirname "$dir")"
    cp -r "$dir" "$BACKUP_DIR/$(dirname "$dir")/"
  else
    echo "- $dir already removed, skipping"
  fi
done

echo "Backup complete."

echo "Checking for new references..."
# Search for direct references to the old crate locations
grep_results=$(grep -r --include="*.rs" --include="*.toml" "runtime/crates/wallet-" . || true)
if [ -n "$grep_results" ]; then
  echo "WARNING: Found references to old crate locations:"
  echo "$grep_results"
  echo "Please update these references before removing the crates."
  echo "ABORTING removal."
  exit 1
fi

echo "Removing duplicate crates..."

for dir in "${DUPLICATES[@]}"; do
  if [ -d "$dir" ]; then
    echo "- Removing $dir"
    rm -rf "$dir"
  else
    echo "- $dir already removed, skipping"
  fi
done

echo "Checking workspace consistency..."
cargo check --workspace || echo "Workspace check failed, you may need to fix additional dependency issues."

echo "Done!"
echo "Backup available at: $BACKUP_DIR"
echo "You may want to run 'cargo clean' and rebuild to ensure everything is working correctly." 