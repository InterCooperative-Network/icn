#!/bin/bash
# ICN Gateway - Production Startup Script
# Run from the icn/ workspace directory (e.g., cd icn)

set -e

echo "========================================="
echo "  ICN Gateway Server"
echo "========================================="
echo ""

# Check we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: Must run from icn workspace directory"
    echo "   cd icn  # (from repo root)"
    exit 1
fi

# Set JWT secret (change in production!)
if [ -z "$ICN_GATEWAY_JWT_SECRET" ]; then
    export ICN_GATEWAY_JWT_SECRET="demo-secret-CHANGE-IN-PRODUCTION"
    echo "⚠ Using demo JWT secret (set ICN_GATEWAY_JWT_SECRET for production)"
fi

# Check for identity
IDENTITY_PATH="$HOME/.icn/identity.age"
IDENTITY_PATH2="$HOME/.local/share/icn/identity.age"

if [ -f "$IDENTITY_PATH" ] || [ -f "$IDENTITY_PATH2" ]; then
    echo "✓ Identity found - will prompt for passphrase"
    echo ""
    echo "  Tip: Set ICN_PASSPHRASE environment variable to skip prompt"
    echo ""
else
    echo "ℹ No identity found - running in UI-only mode"
    echo "  (Network features will be disabled)"
    echo ""
    echo "  To create identity: ./target/debug/icnctl id init"
    echo ""
fi

echo "========================================="
echo "  Starting Gateway on http://localhost:9090"
echo "========================================="
echo ""
echo "  Press Ctrl+C to stop"
echo ""

# Run the daemon
./target/debug/icnd \
    --gateway-enable \
    --gateway-bind 127.0.0.1:9090 \
    --gateway-jwt-secret "$ICN_GATEWAY_JWT_SECRET"
