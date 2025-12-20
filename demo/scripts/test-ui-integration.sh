#!/bin/bash
# Test UI → Gateway API Integration
# This script verifies the pilot UI can connect to the backend

set -e

echo "========================================="
echo "ICN Demo - UI Integration Test"
echo "========================================="
echo

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Check what API endpoints the UI expects
echo "Step 1: Analyzing UI API expectations..."
echo

cd web/pilot-ui

echo "Checking for API endpoints in UI code..."
if grep -r "http.*8080" . 2>/dev/null | head -5; then
    echo -e "${GREEN}✓${NC} Found localhost:8080 references"
else
    echo -e "${YELLOW}⚠${NC} No 8080 references - checking for other patterns"
fi

echo
echo "Checking for API paths..."
grep -rh "\/api\/" app.js 2>/dev/null | grep -o "'/[^']*'" | sort -u | head -10 || echo "No /api/ paths found"

echo
echo "Checking for gateway configuration..."
grep -rh "gateway" app.js 2>/dev/null | head -5 || echo "No gateway references found"

echo
echo "========================================="
echo "Step 2: Manual Test Instructions"
echo "========================================="
echo
echo "To test the UI integration, follow these steps:"
echo
echo -e "${GREEN}Terminal 1: Start Backend${NC}"
echo "  cd /home/matt/projects/icn/icn"
echo "  ./target/release/icnd \\"
echo "    -d /home/matt/icn-demo-test/data \\"
echo "    -e 127.0.0.1:15602 \\"
echo "    --gateway-enable \\"
echo "    --gateway-bind \"127.0.0.1:8080\" \\"
echo "    --gateway-jwt-secret \"demo-secret-key-change-in-production\""
echo "  # Enter passphrase: demo123"
echo
echo -e "${GREEN}Terminal 2: Start UI${NC}"
echo "  cd /home/matt/projects/icn/web/pilot-ui"
echo "  python3 -m http.server 3000"
echo
echo -e "${GREEN}Terminal 3: Test Manually${NC}"
echo "  1. Open browser: http://localhost:3000"
echo "  2. Open browser console (F12)"
echo "  3. Try to login/connect"
echo "  4. Watch for:"
echo "     - API endpoint being called"
echo "     - Response/error messages"
echo "     - CORS errors"
echo "     - Authentication flow"
echo
echo "========================================="
echo "Common Issues & Solutions"
echo "========================================="
echo
echo "❌ CORS Error:"
echo "   Solution: Add to demo.toml:"
echo "   [gateway]"
echo "   cors_origins = [\"http://localhost:3000\"]"
echo
echo "❌ 404 on API endpoint:"
echo "   Solution: Check what endpoint UI is calling vs what gateway provides"
echo "   Compare: UI code vs gateway routes"
echo
echo "❌ Authentication fails:"
echo "   Solution: Check if UI is sending JWT token correctly"
echo "   Verify: Authorization header format"
echo
echo "❌ Can't connect to gateway:"
echo "   Solution: Verify gateway is running"
echo "   Test: curl http://localhost:8080/v1/health"
echo
echo "========================================="
echo "Next Steps"
echo "========================================="
echo
echo "After identifying issues:"
echo "  1. Document them in DEMO_INTEGRATION_ISSUES.md"
echo "  2. Fix UI code or backend as needed"
echo "  3. Test again"
echo "  4. Repeat until transaction flow works"
echo
echo "Goal: Create transaction from Alice to Bob via UI"
echo

# Quick health check if gateway is running
echo "Quick Check: Is gateway already running?"
if curl -s http://localhost:8080/v1/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC} Gateway is responding at http://localhost:8080"
    echo
    curl http://localhost:8080/v1/health | jq . 2>/dev/null || echo "Health check response received"
else
    echo -e "${YELLOW}⚠${NC} Gateway not running yet"
    echo "   Start it with instructions above"
fi

echo
echo "========================================="
echo "Ready to test!"
echo "========================================="
