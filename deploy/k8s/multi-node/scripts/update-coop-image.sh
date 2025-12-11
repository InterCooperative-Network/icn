#!/usr/bin/env bash
#
# Build and deploy new ICN image to all coop namespaces
#
# Usage:
#   ./update-coop-image.sh [tag]
#
# Examples:
#   ./update-coop-image.sh                    # Use git short hash
#   ./update-coop-image.sh v1.0.0             # Use specific tag
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
K8S_SCRIPTS="$(cd "$SCRIPT_DIR/../../scripts" && pwd)"
K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"
TAG="${1:-$(git rev-parse --short HEAD)}"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Update ICN Coop Deployments with New Image               ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Tag: $TAG"
echo "Target: $K3S_HOST"
echo ""

# Step 1: Build image
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 1: Building Docker image..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
"$K8S_SCRIPTS/build-image.sh" "$TAG"
echo ""

# Step 2: Sync image to cluster
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 2: Syncing image to K3s cluster..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
"$K8S_SCRIPTS/sync-image.sh" "$TAG" "$K3S_HOST"
echo ""

# Step 3: Update all coop deployments
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 3: Updating coop deployments..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Scale down all coops first to release ports
echo "Scaling down deployments..."
for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
    COOP_NAME=${ns#icn-coop-}
    ssh "$K3S_HOST" "sudo kubectl -n $ns scale deployment icn-${COOP_NAME} --replicas=0" 2>/dev/null || true
done

echo "Waiting for pods to terminate..."
sleep 10

# Update image and scale back up
for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
    COOP_NAME=${ns#icn-coop-}
    echo "Updating $ns..."

    # Update image
    ssh "$K3S_HOST" "sudo kubectl -n $ns set image deployment/icn-${COOP_NAME} icnd=icn:${TAG}"

    # Scale back up
    ssh "$K3S_HOST" "sudo kubectl -n $ns scale deployment icn-${COOP_NAME} --replicas=1"
done

echo ""
echo "Waiting for rollouts..."
sleep 5

for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
    COOP_NAME=${ns#icn-coop-}
    echo "Waiting for $ns/icn-${COOP_NAME}..."
    ssh "$K3S_HOST" "sudo kubectl -n $ns rollout status deployment icn-${COOP_NAME} --timeout=120s" || true
done

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  ✅ All coop deployments updated to icn:$TAG              ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Check connectivity:"
echo "  ssh $K3S_HOST 'sudo kubectl -n icn-coop-alpha logs deployment/icn-alpha | tail -30'"
