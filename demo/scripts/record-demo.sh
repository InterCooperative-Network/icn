#!/usr/bin/env bash
# record-demo.sh — Record the governance demo with asciinema
#
# Usage: bash record-demo.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RECORDING_DIR="$SCRIPT_DIR/../recordings"
CAST_FILE="$RECORDING_DIR/governance-demo.cast"

# Check for asciinema
if ! command -v asciinema >/dev/null 2>&1; then
  echo ""
  echo "  asciinema is not installed."
  echo ""
  echo "  Install with:"
  echo "    sudo apt install asciinema     # Debian/Ubuntu"
  echo "    brew install asciinema          # macOS"
  echo "    pip install asciinema           # pip"
  echo ""
  exit 1
fi

mkdir -p "$RECORDING_DIR"

echo ""
echo "  ┌──────────────────────────────────────────────────────┐"
echo "  │  Recording ICN Governance Demo                       │"
echo "  └──────────────────────────────────────────────────────┘"
echo ""
echo "  Output: $CAST_FILE"
echo ""
echo "  The demo will run in presenter mode."
echo "  Press Enter at each phase pause to continue."
echo "  Press Ctrl+C when done to stop the recording."
echo ""

asciinema rec \
  --title "ICN Cooperative Governance Demo" \
  --command "bash '$SCRIPT_DIR/present-governance.sh'" \
  --overwrite \
  "$CAST_FILE"

echo ""
echo "  Recording saved to: $CAST_FILE"
echo ""
echo "  Play locally:   asciinema play $CAST_FILE"
echo "  Upload:         asciinema upload $CAST_FILE"
echo ""
