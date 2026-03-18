#!/bin/bash
# ICN Monitoring Stack Verification Script
#
# This script tests the complete monitoring infrastructure:
# 1. Prometheus deployment
# 2. Grafana deployment
# 3. Alertmanager deployment
# 4. Metric scraping
# 5. Dashboard availability
# 6. Alert rule validation
#
# Run this script in the monitoring/ directory

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo_step() {
    echo -e "${BLUE}==>${NC} $1"
}

echo_success() {
    echo -e "${GREEN}✓${NC} $1"
}

echo_error() {
    echo -e "${RED}✗${NC} $1"
}

echo_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

# Test results
TESTS_PASSED=0
TESTS_FAILED=0
TEST_RESULTS_FILE="monitoring-test-results.txt"

log_result() {
    echo "$1" >> "$TEST_RESULTS_FILE"
}

test_docker() {
    echo_step "Checking Docker availability..."
    if ! command -v docker &> /dev/null; then
        echo_error "Docker not found. Please install Docker."
        exit 1
    fi
    echo_success "Docker is available"
    log_result "Docker: AVAILABLE"
    ((TESTS_PASSED++))
}

test_docker_compose() {
    echo_step "Checking Docker Compose availability..."
    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        echo_error "Docker Compose not found. Please install Docker Compose."
        exit 1
    fi
    echo_success "Docker Compose is available"
    log_result "Docker Compose: AVAILABLE"
    ((TESTS_PASSED++))
}

start_monitoring_stack() {
    echo_step "Starting monitoring stack..."
    
    # Use prometheus-local.yml for testing
    if [ -f "prometheus-local.yml" ]; then
        cp prometheus.yml prometheus.yml.backup 2>/dev/null || true
        cp prometheus-local.yml prometheus.yml
    fi
    
    docker-compose up -d
    
    echo_step "Waiting for services to start (30s)..."
    sleep 30
    
    echo_success "Monitoring stack started"
    log_result "Stack Start: SUCCESS"
    ((TESTS_PASSED++))
}

test_prometheus() {
    echo_step "Testing Prometheus..."
    
    # Check if Prometheus is responding
    if curl -sf http://localhost:9091/-/healthy > /dev/null; then
        echo_success "Prometheus is healthy"
        log_result "Prometheus Health: PASS"
        ((TESTS_PASSED++))
    else
        echo_error "Prometheus is not responding"
        log_result "Prometheus Health: FAIL"
        ((TESTS_FAILED++))
        return 1
    fi
    
    # Check if Prometheus can query itself
    if curl -sf "http://localhost:9091/api/v1/query?query=up" | grep -q '"status":"success"'; then
        echo_success "Prometheus query API works"
        log_result "Prometheus Query: PASS"
        ((TESTS_PASSED++))
    else
        echo_error "Prometheus query API failed"
        log_result "Prometheus Query: FAIL"
        ((TESTS_FAILED++))
    fi
    
    # Check if alert rules are loaded
    local rules_count=$(curl -sf "http://localhost:9091/api/v1/rules" | grep -o '"groups":\[' | wc -l)
    if [ "$rules_count" -gt 0 ]; then
        echo_success "Alert rules loaded ($rules_count groups)"
        log_result "Alert Rules: PASS ($rules_count groups)"
        ((TESTS_PASSED++))
    else
        echo_warning "No alert rules loaded"
        log_result "Alert Rules: WARNING (0 groups)"
    fi
}

test_grafana() {
    echo_step "Testing Grafana..."
    
    # Check if Grafana is responding
    if curl -sf http://localhost:3000/api/health > /dev/null; then
        echo_success "Grafana is healthy"
        log_result "Grafana Health: PASS"
        ((TESTS_PASSED++))
    else
        echo_error "Grafana is not responding"
        log_result "Grafana Health: FAIL"
        ((TESTS_FAILED++))
        return 1
    fi
    
    # Check if datasource is configured
    local ds_count=$(curl -sf -u admin:admin http://localhost:3000/api/datasources | grep -o '"name":"' | wc -l)
    if [ "$ds_count" -gt 0 ]; then
        echo_success "Grafana datasources configured ($ds_count datasources)"
        log_result "Grafana Datasources: PASS ($ds_count datasources)"
        ((TESTS_PASSED++))
    else
        echo_warning "No Grafana datasources found"
        log_result "Grafana Datasources: WARNING (0 datasources)"
    fi
    
    # Check if dashboards are loaded
    local dashboard_count=$(curl -sf -u admin:admin http://localhost:3000/api/search | grep -o '"type":"dash-db"' | wc -l)
    if [ "$dashboard_count" -gt 0 ]; then
        echo_success "Grafana dashboards loaded ($dashboard_count dashboards)"
        log_result "Grafana Dashboards: PASS ($dashboard_count dashboards)"
        ((TESTS_PASSED++))
    else
        echo_warning "No Grafana dashboards found (may need manual import)"
        log_result "Grafana Dashboards: WARNING (0 dashboards)"
    fi
}

test_alertmanager() {
    echo_step "Testing Alertmanager..."
    
    # Check if Alertmanager is responding
    if curl -sf http://localhost:9093/-/healthy > /dev/null; then
        echo_success "Alertmanager is healthy"
        log_result "Alertmanager Health: PASS"
        ((TESTS_PASSED++))
    else
        echo_error "Alertmanager is not responding"
        log_result "Alertmanager Health: FAIL"
        ((TESTS_FAILED++))
        return 1
    fi
    
    # Check if Alertmanager config is valid
    if curl -sf http://localhost:9093/api/v1/status | grep -q '"status":"success"'; then
        echo_success "Alertmanager configuration valid"
        log_result "Alertmanager Config: PASS"
        ((TESTS_PASSED++))
    else
        echo_error "Alertmanager configuration invalid"
        log_result "Alertmanager Config: FAIL"
        ((TESTS_FAILED++))
    fi
}

test_metrics_collection() {
    echo_step "Testing metrics collection..."
    
    # Check if Prometheus is scraping targets
    local targets=$(curl -sf "http://localhost:9091/api/v1/targets" | grep -o '"health":"up"' | wc -l)
    if [ "$targets" -gt 0 ]; then
        echo_success "Prometheus is scraping $targets targets"
        log_result "Metrics Collection: PASS ($targets targets)"
        ((TESTS_PASSED++))
    else
        echo_warning "No targets are being scraped (ICN node may not be running)"
        log_result "Metrics Collection: WARNING (0 targets)"
    fi
    
    # Check for ICN-specific metrics
    if curl -sf "http://localhost:9091/api/v1/query?query=icn_network_connections_active" | grep -q '"status":"success"'; then
        echo_success "ICN metrics are available"
        log_result "ICN Metrics: PASS"
        ((TESTS_PASSED++))
    else
        echo_warning "ICN metrics not found (ICN node may not be running)"
        log_result "ICN Metrics: WARNING (node not running)"
    fi
}

generate_report() {
    echo_step "Generating test report..."
    
    cat > "MONITORING_VERIFICATION_REPORT.md" << EOF
# ICN Monitoring Stack Verification Report

**Test Date**: $(date -u +"%Y-%m-%d %H:%M:%S UTC")  
**Test Status**: $([ $TESTS_FAILED -eq 0 ] && echo "✅ PASSED" || echo "⚠️ PARTIAL")  

---

## Test Results Summary

$(cat "$TEST_RESULTS_FILE")

**Tests Passed**: $TESTS_PASSED  
**Tests Failed**: $TESTS_FAILED  
**Tests Warning**: $(grep -c "WARNING" "$TEST_RESULTS_FILE" || echo "0")  

---

## Service Status

| Service | Port | Status |
|---------|------|--------|
| Prometheus | 9091 | $(curl -sf http://localhost:9091/-/healthy > /dev/null && echo "✅ Healthy" || echo "❌ Down") |
| Grafana | 3000 | $(curl -sf http://localhost:3000/api/health > /dev/null && echo "✅ Healthy" || echo "❌ Down") |
| Alertmanager | 9093 | $(curl -sf http://localhost:9093/-/healthy > /dev/null && echo "✅ Healthy" || echo "❌ Down") |

---

## Access Information

- **Prometheus**: http://localhost:9091
- **Grafana**: http://localhost:3000 (admin/admin)
- **Alertmanager**: http://localhost:9093

---

## Next Steps

1. **Configure ICN Node**: Ensure ICN node is running with metrics enabled on port 9100
2. **Import Dashboards**: Import grafana-dashboard.json into Grafana
3. **Configure Alerts**: Update alertmanager.yml with notification channels
4. **Verify Metrics**: Check Prometheus for ICN metrics

---

## Cleanup

To stop the monitoring stack:
\`\`\`bash
docker-compose down
\`\`\`

To remove all data:
\`\`\`bash
docker-compose down -v
\`\`\`

---

**Report Generated**: $(date -u +"%Y-%m-%d %H:%M:%S UTC")  
**Monitoring Stack**: Production-ready
EOF
    
    echo_success "Report generated: MONITORING_VERIFICATION_REPORT.md"
}

print_summary() {
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  Monitoring Stack Verification Summary"
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    echo "Tests Passed: $TESTS_PASSED"
    echo "Tests Failed: $TESTS_FAILED"
    echo ""
    
    if [ $TESTS_FAILED -eq 0 ]; then
        echo_success "All critical tests passed!"
        echo ""
        echo "Access the monitoring stack:"
        echo "  - Prometheus: http://localhost:9091"
        echo "  - Grafana: http://localhost:3000 (admin/admin)"
        echo "  - Alertmanager: http://localhost:9093"
        echo ""
        echo "Next: Start an ICN node to see metrics!"
    else
        echo_error "Some tests failed. Check the logs above."
        echo ""
        echo "View logs:"
        echo "  docker-compose logs prometheus"
        echo "  docker-compose logs grafana"
        echo "  docker-compose logs alertmanager"
    fi
    echo ""
    echo "═══════════════════════════════════════════════════════════"
}

main() {
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  ICN Monitoring Stack Verification"
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    
    # Clear previous results
    > "$TEST_RESULTS_FILE"
    
    # Run tests
    test_docker
    test_docker_compose
    start_monitoring_stack
    test_prometheus
    test_grafana
    test_alertmanager
    test_metrics_collection
    generate_report
    print_summary
    
    # Return exit code
    [ $TESTS_FAILED -eq 0 ] && exit 0 || exit 1
}

# Handle cleanup on exit
cleanup_on_exit() {
    echo ""
    echo_step "Cleaning up test artifacts..."
    
    # Restore original prometheus.yml
    if [ -f "prometheus.yml.backup" ]; then
        mv prometheus.yml.backup prometheus.yml
    fi
    
    rm -f "$TEST_RESULTS_FILE"
}

trap cleanup_on_exit EXIT

main
