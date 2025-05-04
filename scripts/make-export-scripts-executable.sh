#!/bin/bash
set -euo pipefail

# Make ICN export scripts executable
chmod +x "$(dirname "$0")/export-runtime.sh"
chmod +x "$(dirname "$0")/export-wallet.sh"
chmod +x "$(dirname "$0")/export-agoranet.sh"

echo "✅ All ICN export scripts are now executable."
echo "Run them individually or use the export-all.sh script to export all components." 