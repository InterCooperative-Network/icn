#!/bin/bash
# ICN Health Check Script
# Use with monitoring systems like Nagios, Prometheus Blackbox, or cron

set -e

# Configuration
GATEWAY_URL="${ICN_GATEWAY_URL:-http://localhost:8080}"
METRICS_URL="${ICN_METRICS_URL:-http://localhost:9100}"
TIMEOUT="${ICN_HEALTH_TIMEOUT:-5}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Exit codes for monitoring
OK=0
WARNING=1
CRITICAL=2
UNKNOWN=3

check_gateway() {
    local response
    response=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "$GATEWAY_URL/v1/health" 2>/dev/null)

    if [ "$response" = "200" ]; then
        echo -e "${GREEN}Gateway API: OK${NC}"
        return 0
    else
        echo -e "${RED}Gateway API: FAILED (HTTP $response)${NC}"
        return 1
    fi
}

check_metrics() {
    local response
    response=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "$METRICS_URL/metrics" 2>/dev/null)

    if [ "$response" = "200" ]; then
        echo -e "${GREEN}Metrics Server: OK${NC}"
        return 0
    else
        echo -e "${YELLOW}Metrics Server: UNAVAILABLE (HTTP $response)${NC}"
        return 1
    fi
}

check_process() {
    if pgrep -x "icnd" > /dev/null; then
        echo -e "${GREEN}Process: Running${NC}"
        return 0
    else
        echo -e "${RED}Process: Not running${NC}"
        return 1
    fi
}

check_disk() {
    local data_dir="${ICN_DATA_DIR:-/var/lib/icn}"
    if [ -d "$data_dir" ]; then
        local usage
        usage=$(df "$data_dir" | tail -1 | awk '{print $5}' | sed 's/%//')
        if [ "$usage" -gt 90 ]; then
            echo -e "${RED}Disk: ${usage}% used (CRITICAL)${NC}"
            return 1
        elif [ "$usage" -gt 80 ]; then
            echo -e "${YELLOW}Disk: ${usage}% used (WARNING)${NC}"
            return 0
        else
            echo -e "${GREEN}Disk: ${usage}% used${NC}"
            return 0
        fi
    else
        echo -e "${YELLOW}Disk: Data directory not found${NC}"
        return 0
    fi
}

# Main health check
main() {
    echo "ICN Health Check - $(date)"
    echo "========================="

    local exit_code=$OK
    local checks_failed=0

    # Run checks
    if ! check_process; then
        exit_code=$CRITICAL
        ((checks_failed++))
    fi

    if ! check_gateway; then
        exit_code=$CRITICAL
        ((checks_failed++))
    fi

    if ! check_metrics; then
        # Metrics failure is warning, not critical
        if [ $exit_code -eq $OK ]; then
            exit_code=$WARNING
        fi
    fi

    if ! check_disk; then
        if [ $exit_code -eq $OK ]; then
            exit_code=$WARNING
        fi
    fi

    echo "========================="

    if [ $exit_code -eq $OK ]; then
        echo -e "${GREEN}Overall: HEALTHY${NC}"
    elif [ $exit_code -eq $WARNING ]; then
        echo -e "${YELLOW}Overall: WARNING${NC}"
    else
        echo -e "${RED}Overall: CRITICAL ($checks_failed checks failed)${NC}"
    fi

    exit $exit_code
}

# JSON output for monitoring systems
json_output() {
    local gateway_ok metrics_ok process_ok

    process_ok=$(pgrep -x "icnd" > /dev/null && echo "true" || echo "false")
    gateway_ok=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "$GATEWAY_URL/v1/health" 2>/dev/null | grep -q "200" && echo "true" || echo "false")
    metrics_ok=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "$METRICS_URL/metrics" 2>/dev/null | grep -q "200" && echo "true" || echo "false")

    cat <<EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "process": $process_ok,
  "gateway": $gateway_ok,
  "metrics": $metrics_ok,
  "healthy": $([ "$process_ok" = "true" ] && [ "$gateway_ok" = "true" ] && echo "true" || echo "false")
}
EOF
}

# Parse arguments
case "${1:-}" in
    --json)
        json_output
        ;;
    --help)
        echo "Usage: $0 [--json|--help]"
        echo ""
        echo "Options:"
        echo "  --json   Output in JSON format"
        echo "  --help   Show this help"
        echo ""
        echo "Environment variables:"
        echo "  ICN_GATEWAY_URL    Gateway URL (default: http://localhost:8080)"
        echo "  ICN_METRICS_URL    Metrics URL (default: http://localhost:9100)"
        echo "  ICN_HEALTH_TIMEOUT Request timeout in seconds (default: 5)"
        echo "  ICN_DATA_DIR       Data directory for disk check"
        exit 0
        ;;
    *)
        main
        ;;
esac
