#!/bin/bash
set -e

# Resolve cargo target directory (respects CARGO_TARGET_DIR env var)
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    TARGET_DIR="$CARGO_TARGET_DIR"
else
    TARGET_DIR="icn/target"
fi

DEMO_DIR="$HOME/.icn-demo"
DEMO_NAME="Tool Library"

echo "========================================="
echo "ICN Demo Setup: $DEMO_NAME"
echo "========================================="
echo

# Clean previous demo
if [ -d "$DEMO_DIR" ]; then
    echo "Cleaning previous demo data..."
    rm -rf "$DEMO_DIR"
fi

# Create demo directories
echo "Creating demo directories..."
mkdir -p "$DEMO_DIR"/{tool-library,food-coop,credit-union}

# Build ICN if needed
if [ ! -f "$TARGET_DIR/release/icnd" ]; then
    echo "Building ICN..."
    cd icn && cargo build --release && cd ..
fi

echo
echo "✓ Demo environment ready at $DEMO_DIR"
echo
echo "Next steps:"
echo "  1. Run: ./demo/scripts/run-tool-library-demo.sh"
echo "  2. Or start manually:"
echo "     cd icn"
echo "     $TARGET_DIR/release/icnd --config ../demo/configs/tool-library.toml"
echo
