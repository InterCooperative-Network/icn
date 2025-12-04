#!/bin/bash
# Validation script for ICN internal testing infrastructure
# Checks configuration files, documentation consistency, and prerequisites

# Note: We don't use 'set -e' because we want to collect all errors
# and report them at the end, rather than exiting on the first error.

echo "=== ICN Internal Testing Configuration Validator ==="
echo ""

ERRORS=0
WARNINGS=0

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

error() {
    echo -e "${RED}✗ ERROR: $1${NC}"
    ((ERRORS++))
}

warning() {
    echo -e "${YELLOW}⚠ WARNING: $1${NC}"
    ((WARNINGS++))
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

info() {
    echo "ℹ $1"
}

# Check 1: Required files exist
echo "## Checking required files..."
FILES=(
    "docker-compose.test.yml"
    "Dockerfile"
    "config/node1.toml"
    "config/node2.toml"
    "config/node3.toml"
    "config/node4.toml"
    "monitoring/prometheus.yml"
    "monitoring/alert_rules.yml"
    "docs/INTERNAL_TESTING_PLAN.md"
    "docs/TESTING_QUICKSTART.md"
    "DEPLOY_TEST_NETWORK.md"
)

for file in "${FILES[@]}"; do
    if [ -f "$file" ]; then
        success "$file exists"
    else
        error "$file is missing"
    fi
done
echo ""

# Check 2: Config files use correct ports
echo "## Checking node config files..."
for i in 1 2 3 4; do
    config="config/node$i.toml"
    if [ -f "$config" ]; then
        # Check metrics_port = 9100
        if grep -q "metrics_port = 9100" "$config"; then
            success "$config: metrics_port = 9100"
        else
            error "$config: metrics_port should be 9100"
        fi

        # Check listen_addr uses correct port
        expected_port=$((5000 + i))
        if grep -q "listen_addr = \"0.0.0.0:$expected_port\"" "$config"; then
            success "$config: listen_addr = 0.0.0.0:$expected_port"
        else
            error "$config: listen_addr should be 0.0.0.0:$expected_port"
        fi
    fi
done
echo ""

# Check 3: Docker Compose port mappings
echo "## Checking docker-compose.test.yml..."
if [ -f "docker-compose.test.yml" ]; then
    # Check node port mappings
    for i in 1 2 3 4; do
        host_port=$((9090 + i))
        if grep -q "\"$host_port:9100\"" docker-compose.test.yml; then
            success "node$i: Port mapping $host_port:9100"
        else
            error "node$i: Expected port mapping $host_port:9100"
        fi
    done

    # Check Prometheus port mapping
    if grep -q "\"9095:9090\"" docker-compose.test.yml; then
        success "prometheus: Port mapping 9095:9090"
    else
        error "prometheus: Expected port mapping 9095:9090"
    fi

    # Check all nodes use config files
    for i in 1 2 3 4; do
        if grep -q "\"--config\", \"/config/node$i.toml\"" docker-compose.test.yml; then
            success "node$i: Uses config file"
        else
            error "node$i: Should use --config /config/node$i.toml"
        fi
    done
fi
echo ""

# Check 4: Dockerfile health check
echo "## Checking Dockerfile..."
if [ -f "Dockerfile" ]; then
    if grep -q "EXPOSE.*9100" Dockerfile; then
        success "Dockerfile: Exposes port 9100"
    else
        error "Dockerfile: Should EXPOSE port 9100"
    fi

    if grep -q "localhost:9100/metrics" Dockerfile; then
        success "Dockerfile: Health check uses port 9100"
    else
        error "Dockerfile: Health check should use localhost:9100/metrics"
    fi
fi
echo ""

# Check 5: Prometheus scrape targets
echo "## Checking Prometheus configuration..."
if [ -f "monitoring/prometheus.yml" ]; then
    for i in 1 2 3 4; do
        if grep -q "node$i:9100" monitoring/prometheus.yml; then
            success "Prometheus: Scrapes node$i:9100"
        else
            error "Prometheus: Should scrape node$i:9100"
        fi
    done
fi
echo ""

# Check 6: Documentation port references
echo "## Checking documentation..."
if [ -f "DEPLOY_TEST_NETWORK.md" ]; then
    # Should reference localhost:9095 for Prometheus
    count=$(grep -c "localhost:9095" DEPLOY_TEST_NETWORK.md || true)
    if [ "$count" -gt 0 ]; then
        success "DEPLOY_TEST_NETWORK.md: References Prometheus on port 9095 ($count times)"
    else
        error "DEPLOY_TEST_NETWORK.md: Should reference localhost:9095 for Prometheus"
    fi

    # Should NOT reference localhost:9090 for Prometheus
    bad_count=$(grep "localhost:9090" DEPLOY_TEST_NETWORK.md | grep -v "9090/tcp" | grep -v "node.*:9090" | wc -l || true)
    if [ "$bad_count" -eq 0 ]; then
        success "DEPLOY_TEST_NETWORK.md: No incorrect port 9090 references"
    else
        warning "DEPLOY_TEST_NETWORK.md: Found $bad_count references to localhost:9090 (should be 9095)"
    fi
fi
echo ""

# Check 7: Prerequisites (if running on host)
echo "## Checking prerequisites (host system)..."
if command -v docker &> /dev/null; then
    docker_version=$(docker --version | grep -oP '\d+\.\d+' | head -1)
    success "Docker installed: $docker_version"

    # Check if version is 24+
    major_version=$(echo "$docker_version" | cut -d. -f1)
    if [ "$major_version" -ge 24 ]; then
        success "Docker version >= 24"
    else
        warning "Docker version < 24 (recommended: 24+)"
    fi
else
    warning "Docker not found (required for deployment)"
fi

if command -v docker compose &> /dev/null; then
    compose_version=$(docker compose version | grep -oP '\d+\.\d+' | head -1)
    success "Docker Compose installed: $compose_version"
else
    warning "Docker Compose not found (required for deployment)"
fi

if command -v curl &> /dev/null; then
    success "curl installed"
else
    warning "curl not found (needed for verification steps)"
fi

if command -v jq &> /dev/null; then
    jq_version=$(jq --version 2>/dev/null || echo "unknown")
    success "jq installed: $jq_version"
else
    warning "jq not found (needed for JSON parsing in test commands)"
fi
echo ""

# Summary
echo "=== Validation Summary ==="
echo ""
if [ $ERRORS -eq 0 ] && [ $WARNINGS -eq 0 ]; then
    echo -e "${GREEN}✓ All checks passed!${NC}"
    echo "Configuration is ready for deployment."
    exit 0
elif [ $ERRORS -eq 0 ]; then
    echo -e "${YELLOW}⚠ $WARNINGS warning(s) found${NC}"
    echo "Configuration may work but review warnings."
    exit 0
else
    echo -e "${RED}✗ $ERRORS error(s) found${NC}"
    if [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ $WARNINGS warning(s) found${NC}"
    fi
    echo "Configuration has errors that must be fixed."
    exit 1
fi
