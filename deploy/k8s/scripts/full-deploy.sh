#!/usr/bin/env bash
#
# Full deployment pipeline: build, sync, and deploy ICN to K3s cluster
# This script orchestrates the complete deployment process
#
# Usage:
#   ./full-deploy.sh [tag] [k3s-control-host]
#
# Examples:
#   ./full-deploy.sh                                    # Deploy latest
#   ./full-deploy.sh $(git rev-parse --short HEAD)     # Deploy with git hash
#   ./full-deploy.sh v1.0.0 ubuntu@10.8.10.40          # Custom tag and host

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAG="${1:-latest}"
K3S_HOST="${2:-ubuntu@10.8.10.40}"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       ICN Full Deployment Pipeline                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Tag: $TAG"
echo "Target: $K3S_HOST"
echo ""

# Step 1: Build image
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 1: Building Docker image..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
"$SCRIPT_DIR/build-image.sh" "$TAG"
echo ""

# Step 2: Sync image to cluster
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 2: Syncing image to K3s cluster..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
"$SCRIPT_DIR/sync-image.sh" "$TAG" "$K3S_HOST"
echo ""

# Step 3: Deploy to cluster
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 3: Deploying to K3s cluster..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
"$SCRIPT_DIR/deploy.sh" "$K3S_HOST" "$TAG"
echo ""

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Deployment Complete!                                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Next steps:"
echo "  1. Check pod status:"
echo "     ssh $K3S_HOST 'sudo kubectl -n icn get pods -w'"
echo ""
echo "  2. View logs:"
echo "     ssh $K3S_HOST 'sudo kubectl -n icn logs -f deployment/icn-daemon'"
echo ""
echo "  3. Check metrics:"
echo "     ssh $K3S_HOST 'sudo kubectl -n icn port-forward svc/icn 9100:9100'"
echo "     # Then visit http://localhost:9100/metrics"

