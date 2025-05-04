#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "=== ICN Monorepo Refactoring ==="
echo

# Make all scripts executable
chmod +x "$SCRIPT_DIR/rename_wallet_crates.sh"
chmod +x "$SCRIPT_DIR/cleanup_runtime_wallet_crates.sh"
chmod +x "$SCRIPT_DIR/validate_crate_naming.sh"
chmod +x "$SCRIPT_DIR/update_imports.sh"
chmod +x "$SCRIPT_DIR/fix_wallet_sync.sh"

echo "Phase 1: Renaming wallet crate directories to match icn- prefix"
"$SCRIPT_DIR/rename_wallet_crates.sh"
echo

echo "Phase 2: Cleaning up orphaned wallet crates from runtime/crates"
"$SCRIPT_DIR/cleanup_runtime_wallet_crates.sh"
echo

echo "Phase 3: Updating imports in Rust files"
"$SCRIPT_DIR/update_imports.sh"
echo

echo "Phase 4: Fixing specific issues in wallet-sync"
"$SCRIPT_DIR/fix_wallet_sync.sh"
echo

echo "Phase 5: Updating Cargo.toml dependencies"
# This updates the dependencies in wallet/Cargo.toml from old paths to new paths
if [ -f "wallet/Cargo.toml" ]; then
  echo "Updating wallet/Cargo.toml dependencies..."
  sed -i 's/path = "\.\/crates\/\([^"]*\)"/path = "\.\/crates\/icn-\1"/g' wallet/Cargo.toml
  echo "Done."
fi

# Fix backoff crate features in workspace dependencies
echo "Adding features to backoff crate in workspace Cargo.toml..."
if [ -f "Cargo.toml" ]; then
  sed -i 's/backoff = "0.4.0"/backoff = { version = "0.4.0", features = ["tokio", "futures"] }/g' Cargo.toml
  echo "Done."
fi

echo "Phase 6: Validating directory/package naming consistency"
"$SCRIPT_DIR/validate_crate_naming.sh"
echo

echo
echo "Refactoring complete!"
echo "You should now:"
echo "1. Run 'cargo fix --workspace' to fix any remaining import issues"
echo "2. Run 'cargo clippy --workspace --fix --allow-dirty' to fix any clippy issues"
echo "3. Run 'cargo fmt --all' to format the code"
echo "4. Test the changes with 'cargo test --workspace'" 