#!/usr/bin/env bash
#
# ICN Quickstart Demo
# This script sets up a two-node ICN network and demonstrates basic operations.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ICN_DIR="${REPO_ROOT}/icn"
ICND="${ICN_DIR}/target/release/icnd"
ICNCTL="${ICN_DIR}/target/release/icnctl"
CONFIG_DIR="${REPO_ROOT}/config"

ALPHA_DATA="/tmp/icn-alpha"
BETA_DATA="/tmp/icn-beta"
ALPHA_ENDPOINT="127.0.0.1:5050"
BETA_ENDPOINT="127.0.0.1:5051"

# Passphrase for test identities
PASSPHRASE="quickstart123"

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"

    # Kill nodes
    if [ -f "/tmp/icn-alpha.pid" ]; then
        kill $(cat /tmp/icn-alpha.pid) 2>/dev/null || true
        rm /tmp/icn-alpha.pid
    fi
    if [ -f "/tmp/icn-beta.pid" ]; then
        kill $(cat /tmp/icn-beta.pid) 2>/dev/null || true
        rm /tmp/icn-beta.pid
    fi

    # Remove data directories
    rm -rf "${ALPHA_DATA}" "${BETA_DATA}"

    echo -e "${GREEN}Cleanup complete${NC}"
}

# Trap cleanup on exit
trap cleanup EXIT INT TERM

print_step() {
    echo ""
    echo -e "${BLUE}==== $1 ====${NC}"
    echo ""
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}→ $1${NC}"
}

# Check prerequisites
print_step "Checking Prerequisites"

if [ ! -f "${ICND}" ]; then
    print_error "icnd binary not found at ${ICND}"
    echo "Please build ICN first:"
    echo "  cd ${ICN_DIR} && cargo build --release"
    exit 1
fi

if [ ! -f "${ICNCTL}" ]; then
    print_error "icnctl binary not found at ${ICNCTL}"
    echo "Please build ICN first:"
    echo "  cd ${ICN_DIR} && cargo build --release"
    exit 1
fi

print_success "ICN binaries found"
print_info "icnd: ${ICND}"
print_info "icnctl: ${ICNCTL}"

# Clean any existing data
rm -rf "${ALPHA_DATA}" "${BETA_DATA}"
mkdir -p "${ALPHA_DATA}" "${BETA_DATA}"

# Start nodes
print_step "Starting ICN Nodes"

print_info "Starting Alpha node (QUIC:4433, RPC:5050, Metrics:9090)..."
echo "${PASSPHRASE}" | "${ICND}" \
    --config "${CONFIG_DIR}/icn-alpha.toml" \
    > /tmp/icn-alpha.log 2>&1 &
ALPHA_PID=$!
echo $ALPHA_PID > /tmp/icn-alpha.pid
print_success "Alpha node started (PID: ${ALPHA_PID})"

sleep 2

print_info "Starting Beta node (QUIC:4434, RPC:5051, Metrics:9091)..."
echo "${PASSPHRASE}" | "${ICND}" \
    --config "${CONFIG_DIR}/icn-beta.toml" \
    > /tmp/icn-beta.log 2>&1 &
BETA_PID=$!
echo $BETA_PID > /tmp/icn-beta.pid
print_success "Beta node started (PID: ${BETA_PID})"

# Wait for nodes to initialize
print_step "Waiting for Node Initialization"
print_info "Waiting 5 seconds for nodes to start..."
sleep 5

# Check if nodes are running
if ! kill -0 $ALPHA_PID 2>/dev/null; then
    print_error "Alpha node failed to start"
    echo "Log:"
    tail -20 /tmp/icn-alpha.log
    exit 1
fi

if ! kill -0 $BETA_PID 2>/dev/null; then
    print_error "Beta node failed to start"
    echo "Log:"
    tail -20 /tmp/icn-beta.log
    exit 1
fi

print_success "Both nodes are running"

# Get node DIDs
print_step "Retrieving Node Identities"

ALPHA_DID=$(${ICNCTL} --endpoint ${ALPHA_ENDPOINT} id show 2>/dev/null || echo "")
if [ -z "${ALPHA_DID}" ]; then
    print_error "Failed to get Alpha DID"
    exit 1
fi
print_success "Alpha DID: ${ALPHA_DID}"

BETA_DID=$(${ICNCTL} --endpoint ${BETA_ENDPOINT} id show 2>/dev/null || echo "")
if [ -z "${BETA_DID}" ]; then
    print_error "Failed to get Beta DID"
    exit 1
fi
print_success "Beta DID: ${BETA_DID}"

# Wait for peer discovery
print_step "Waiting for Peer Discovery (mDNS)"
print_info "Nodes should discover each other via mDNS..."

MAX_WAIT=30
WAIT_COUNT=0
ALPHA_DISCOVERED=false

while [ $WAIT_COUNT -lt $MAX_WAIT ]; do
    sleep 1
    WAIT_COUNT=$((WAIT_COUNT + 1))

    # Check if alpha has discovered beta
    PEERS=$(${ICNCTL} --endpoint ${ALPHA_ENDPOINT} network peers 2>/dev/null || echo "")
    if echo "${PEERS}" | grep -q "${BETA_DID}"; then
        ALPHA_DISCOVERED=true
        break
    fi

    echo -ne "\rWaiting... ${WAIT_COUNT}/${MAX_WAIT}s"
done
echo ""

if [ "${ALPHA_DISCOVERED}" = true ]; then
    print_success "Peer discovery successful!"
else
    print_error "Peer discovery failed after ${MAX_WAIT}s"
    print_info "This might be a mDNS issue. Trying manual dial..."
    ${ICNCTL} --endpoint ${ALPHA_ENDPOINT} network dial ${BETA_DID} 127.0.0.1:4434 || true
    sleep 2
fi

# Show network status
print_step "Network Status"

print_info "Alpha node status:"
${ICNCTL} --endpoint ${ALPHA_ENDPOINT} network status

echo ""
print_info "Beta node status:"
${ICNCTL} --endpoint ${BETA_ENDPOINT} network status

# Show discovered peers
print_step "Discovered Peers"

print_info "Alpha's peers:"
${ICNCTL} --endpoint ${ALPHA_ENDPOINT} network peers

echo ""
print_info "Beta's peers:"
${ICNCTL} --endpoint ${BETA_ENDPOINT} network peers

# Add trust relationships
print_step "Adding Trust Relationships"

print_info "Alpha trusts Beta with score 0.8 (Partner)..."
${ICNCTL} --endpoint ${ALPHA_ENDPOINT} trust add ${BETA_DID} --score 0.8 --label partner
print_success "Trust edge added"

print_info "Beta trusts Alpha with score 0.8 (Partner)..."
${ICNCTL} --endpoint ${BETA_ENDPOINT} trust add ${ALPHA_DID} --score 0.8 --label partner
print_success "Trust edge added"

# Query trust graph
print_step "Trust Graph Query"

print_info "Alpha's trust edges:"
${ICNCTL} --endpoint ${ALPHA_ENDPOINT} trust list

echo ""
print_info "Beta's trust edges:"
${ICNCTL} --endpoint ${BETA_ENDPOINT} trust list

# Show trust score
print_step "Computing Trust Scores"

print_info "Alpha's trust score for Beta:"
${ICNCTL} --endpoint ${ALPHA_ENDPOINT} trust show ${BETA_DID}

echo ""
print_info "Beta's trust score for Alpha:"
${ICNCTL} --endpoint ${BETA_ENDPOINT} trust show ${ALPHA_DID}

# Show metrics endpoints
print_step "Monitoring Endpoints"

print_info "Prometheus metrics are available at:"
echo "  • Alpha: http://localhost:9100/metrics"
echo "  • Beta:  http://localhost:9101/metrics"
echo ""
print_info "Try these commands to view metrics:"
echo "  curl http://localhost:9100/metrics | grep icn_network"
echo "  curl http://localhost:9101/metrics | grep icn_gossip"

# Summary
print_step "Demo Complete!"

echo "Your two-node ICN network is running:"
echo ""
echo "┌─────────────────────┐                  ┌─────────────────────┐"
echo "│   Alpha Node        │◄────mDNS────────►│   Beta Node         │"
echo "├─────────────────────┤                  ├─────────────────────┤"
echo "│ DID: ${ALPHA_DID:0:20}...│                  │ DID: ${BETA_DID:0:20}...│"
echo "│ QUIC:    4433       │◄────QUIC/TLS────►│ QUIC:    4434       │"
echo "│ RPC:     5050       │                  │ RPC:     5051       │"
echo "│ Metrics: 9090       │                  │ Metrics: 9091       │"
echo "└─────────────────────┘                  └─────────────────────┘"
echo ""
print_success "Trust relationship: Partner (0.8 score)"
echo ""
print_info "Logs are available at:"
echo "  • Alpha: /tmp/icn-alpha.log"
echo "  • Beta:  /tmp/icn-beta.log"
echo ""
print_info "Try these commands:"
echo "  ${ICNCTL} --endpoint ${ALPHA_ENDPOINT} network stats"
echo "  ${ICNCTL} --endpoint ${BETA_ENDPOINT} network stats"
echo ""
print_info "Press Ctrl+C to stop nodes and cleanup"
echo ""

# Keep running until user stops
wait
