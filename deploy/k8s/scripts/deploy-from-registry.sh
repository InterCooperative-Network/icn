#!/usr/bin/env bash
#
# Deploy ICN from GitHub Container Registry to K3s
#
# Usage:
#   ./deploy-from-registry.sh [tag]
#
# Examples:
#   ./deploy-from-registry.sh           # Uses 'latest' tag
#   ./deploy-from-registry.sh main      # Uses 'main' tag
#   ./deploy-from-registry.sh abc1234   # Uses specific commit SHA
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"
IMAGE_TAG="${1:-latest}"
REGISTRY="ghcr.io"
IMAGE_NAME="intercooperative-network/icn"
FULL_IMAGE="${REGISTRY}/${IMAGE_NAME}:${IMAGE_TAG}"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       ICN Deployment from Registry                         ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Image: ${FULL_IMAGE}"
echo "Target: ${K3S_HOST}"
echo ""

# Check SSH connectivity
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 1: Checking connectivity..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if ! ssh -o ConnectTimeout=5 -o BatchMode=yes "$K3S_HOST" "echo 'SSH OK'" &>/dev/null; then
    echo "❌ Cannot connect to ${K3S_HOST}"
    exit 1
fi
echo "✓ SSH connection OK"

# Configure containerd to pull from ghcr.io
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 2: Pulling image on all nodes..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Get all nodes
NODES=$(ssh "$K3S_HOST" "sudo kubectl get nodes -o jsonpath='{.items[*].metadata.name}'")
echo "Nodes: $NODES"

# Pull image on control plane using crictl
ssh "$K3S_HOST" "sudo crictl pull ${FULL_IMAGE}" || {
    echo "⚠️  Pull failed on control plane. Image may need authentication."
    echo ""
    echo "To authenticate with ghcr.io, create a K8s secret:"
    echo "  kubectl create secret docker-registry ghcr-secret \\"
    echo "    --docker-server=ghcr.io \\"
    echo "    --docker-username=<github-username> \\"
    echo "    --docker-password=<github-pat-token>"
    echo ""
    echo "Then add to deployment:"
    echo "  imagePullSecrets:"
    echo "    - name: ghcr-secret"
    exit 1
}
echo "✓ Image pulled on control plane"

# Update deployments to use the new image
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 3: Updating deployments..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Update main daemon
echo "Updating icn-daemon..."
ssh "$K3S_HOST" "sudo kubectl -n icn set image deployment/icn-daemon icnd=${FULL_IMAGE}"

# Update coop nodes if they exist
for COOP in alpha beta gamma delta; do
    NS="icn-coop-${COOP}"
    if ssh "$K3S_HOST" "sudo kubectl get namespace ${NS}" &>/dev/null; then
        echo "Updating icn-${COOP}..."
        ssh "$K3S_HOST" "sudo kubectl -n ${NS} set image deployment/icn-${COOP} icnd=${FULL_IMAGE}" 2>/dev/null || true
    fi
done

echo "✓ Deployments updated"

# Restart deployments to pick up new image
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 4: Rolling restart..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ssh "$K3S_HOST" "sudo kubectl -n icn rollout restart deployment/icn-daemon"
for COOP in alpha beta gamma delta; do
    NS="icn-coop-${COOP}"
    if ssh "$K3S_HOST" "sudo kubectl get namespace ${NS}" &>/dev/null; then
        ssh "$K3S_HOST" "sudo kubectl -n ${NS} rollout restart deployment/icn-${COOP}" 2>/dev/null || true
    fi
done

echo "✓ Rollout initiated"

# Wait for rollouts
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 5: Waiting for pods to be ready..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ssh "$K3S_HOST" "sudo kubectl -n icn rollout status deployment/icn-daemon --timeout=120s" || echo "⚠️  Main daemon rollout timed out"

for COOP in alpha beta gamma delta; do
    NS="icn-coop-${COOP}"
    if ssh "$K3S_HOST" "sudo kubectl get namespace ${NS}" &>/dev/null; then
        ssh "$K3S_HOST" "sudo kubectl -n ${NS} rollout status deployment/icn-${COOP} --timeout=60s" 2>/dev/null || echo "⚠️  ${COOP} rollout timed out"
    fi
done

# Show status
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Deployment Status"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
ssh "$K3S_HOST" "sudo kubectl get pods -A | grep icn"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Deployment Complete!                                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Image: ${FULL_IMAGE}"
echo ""
echo "Check logs: ssh ${K3S_HOST} 'sudo kubectl -n icn logs -f deployment/icn-daemon'"
