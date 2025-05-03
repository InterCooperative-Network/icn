#!/bin/bash
set -euo pipefail

# ICN Export All Script
# This script runs all component export scripts in sequence

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXPORT_DIR="${REPO_ROOT}/export"

# Create export directory if it doesn't exist
mkdir -p "${EXPORT_DIR}"

echo "🚀 Starting ICN component exports..."

# Make all scripts executable
chmod +x "${SCRIPT_DIR}/export-runtime.sh"
chmod +x "${SCRIPT_DIR}/export-wallet.sh"
chmod +x "${SCRIPT_DIR}/export-agoranet.sh"

# Run each export script
echo "📦 Exporting Runtime component..."
"${SCRIPT_DIR}/export-runtime.sh"

echo "📦 Exporting Wallet component..."
"${SCRIPT_DIR}/export-wallet.sh"

echo "📦 Exporting AgoraNet component..."
"${SCRIPT_DIR}/export-agoranet.sh"

echo "✨ All ICN components have been exported to ${EXPORT_DIR}/"
echo "📝 See README-export.md for instructions on how to use these exported repositories."

# List the exported components
echo "📋 Exported components:"
ls -la "${EXPORT_DIR}/" 