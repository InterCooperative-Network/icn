#!/bin/bash
# ICN Demo Verification Script
# Tests that all components work before demo

set -e

FAILURES=0
WARNINGS=0

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

check() {
    local name="$1"
    local command="$2"
    local level="${3:-error}" # error or warning
    
    echo -n "Checking $name... "
    if bash -c "$command" > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
        return 0
    else
        if [ "$level" = "warning" ]; then
            echo -e "${YELLOW}⚠ WARNING${NC}"
            ((WARNINGS++))
        else
            echo -e "${RED}✗ FAILED${NC}"
            ((FAILURES++))
        fi
        return 1
    fi
}

echo "========================================="
echo "ICN Demo Verification"
echo "========================================="
echo

# Build checks
echo "Build Checks:"
check "Backend builds" "cd icn && cargo build --release --quiet"
check "icnd binary exists" "[ -x icn/target/release/icnd ]"
check "icnctl binary exists" "[ -x icn/target/release/icnctl ]"

# Demo infrastructure checks
echo
echo "Demo Infrastructure:"
check "Demo directory exists" "test -d demo"
check "Demo scripts exist" "test -f demo/scripts/setup-demo-env.sh"
check "Sample data exists" "test -f demo/data/tool-library-members.json"
check "Demo config exists" "test -f demo/configs/tool-library.toml"

# UI checks
echo
echo "Pilot UI:"
check "UI directory exists" "test -d web/pilot-ui"
check "UI files present" "test -f web/pilot-ui/index.html && test -f web/pilot-ui/app.js"
check "Node modules installed" "test -d web/pilot-ui/node_modules" "warning"

# Config checks
echo
echo "Configuration:"
check "Config directory exists" "test -d config"
check "Example configs exist" "test -f config/icn.toml.example"
check "Multi-node configs exist" "test -f config/icn-alpha.toml && test -f config/icn-beta.toml"

echo
echo "========================================="
echo "Results:"
echo "========================================="

if [ $FAILURES -eq 0 ] && [ $WARNINGS -eq 0 ]; then
    echo -e "${GREEN}✓ All checks passed!${NC}"
    echo
    echo "Demo is ready to test. Next steps:"
    echo "  1. Run: ./demo/scripts/setup-demo-env.sh"
    echo "  2. Test node startup (see DEMO_NEXT_STEPS.md)"
    exit 0
elif [ $FAILURES -eq 0 ]; then
    echo -e "${YELLOW}⚠ $WARNINGS warning(s) - demo should work but may have minor issues${NC}"
    exit 0
else
    echo -e "${RED}✗ $FAILURES check(s) failed${NC}"
    if [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ $WARNINGS warning(s)${NC}"
    fi
    echo
    echo "Fix the failed checks before proceeding with demo."
    exit 1
fi
