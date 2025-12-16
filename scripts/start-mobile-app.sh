#!/bin/bash
# Start CoopWallet Mobile App
# This script starts the Expo development server

set -e

cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet

echo "========================================="
echo "Starting CoopWallet Mobile App"
echo "========================================="
echo ""

# Check if node_modules exists
if [ ! -d "node_modules" ]; then
    echo "⚠️  node_modules not found. Running npm install..."
    npm install
    echo ""
fi

echo "✅ Configuration:"
echo "   Gateway: http://10.8.10.40:30080"
echo "   Coop ID: test-coop"
echo ""

echo "📱 Starting Expo development server..."
echo ""
echo "After the server starts:"
echo "  • Press 'w' to open in web browser"
echo "  • Press 'a' to open Android emulator (if installed)"
echo "  • Press 'i' to open iOS simulator (if installed)"
echo "  • Scan QR code with Expo Go app on your phone"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""
echo "========================================="
echo ""

npm start
