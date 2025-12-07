#!/usr/bin/env bash
#
# Deploy ICN to K3s cluster
# This script applies all Kubernetes manifests to the cluster
#
# Usage:
#   ./deploy.sh [k3s-control-host] [image-tag]
#
# Examples:
#   ./deploy.sh                                    # Deploy to default host with latest
#   ./deploy.sh ubuntu@10.8.10.40                 # Custom host
#   ./deploy.sh ubuntu@10.8.10.40 v1.0.0          # Custom host and tag

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
K8S_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
K3S_HOST="${1:-ubuntu@10.8.10.40}"
IMAGE_TAG="${2:-latest}"
IMAGE_NAME="${IMAGE_NAME:-icn}"

echo "Deploying ICN to K3s cluster..."
echo "  Host: $K3S_HOST"
echo "  Image: ${IMAGE_NAME}:${IMAGE_TAG}"
echo ""

# Check if kustomize is available, otherwise use kubectl apply
if command -v kustomize &>/dev/null; then
    echo "Using kustomize to build manifests..."
    # Update image tag in kustomization if provided
    if [ "$IMAGE_TAG" != "latest" ]; then
        TEMP_DIR=$(mktemp -d)
        cp -r "$K8S_DIR"/* "$TEMP_DIR/"
        # Update kustomization.yaml image tag
        sed -i "s/newTag:.*/newTag: ${IMAGE_TAG}/" "$TEMP_DIR/kustomization.yaml"
        K8S_DIR="$TEMP_DIR"
    fi
    
    kustomize build "$K8S_DIR" | ssh "$K3S_HOST" "sudo kubectl apply -f -"
else
    echo "Using kubectl apply directly..."
    # Apply manifests in order
    ssh "$K3S_HOST" <<EOF
set -e
cd /tmp
sudo kubectl apply -f - <<'MANIFESTS'
$(cat "$K8S_DIR/namespace.yaml")
MANIFESTS

sudo kubectl apply -f - <<'MANIFESTS'
$(cat "$K8S_DIR/configmap.yaml")
MANIFESTS

sudo kubectl apply -f - <<'MANIFESTS'
$(cat "$K8S_DIR/pvc.yaml")
MANIFESTS

sudo kubectl apply -f - <<'MANIFESTS'
$(cat "$K8S_DIR/services.yaml")
MANIFESTS

# Update deployment with image tag if needed
if [ "$IMAGE_TAG" != "latest" ]; then
    sudo kubectl set image deployment/icn-daemon -n icn icnd=${IMAGE_NAME}:${IMAGE_TAG}
fi

sudo kubectl apply -f - <<'MANIFESTS'
$(cat "$K8S_DIR/deployment.yaml")
MANIFESTS

sudo kubectl apply -f - <<'MANIFESTS'
$(cat "$K8S_DIR/monitoring/servicemonitor.yaml")
MANIFESTS
EOF
fi

echo ""
echo "✓ Deployment complete!"
echo ""
echo "To check status, run:"
echo "  ssh $K3S_HOST 'sudo kubectl -n icn get pods'"
echo ""
echo "To view logs, run:"
echo "  ssh $K3S_HOST 'sudo kubectl -n icn logs -f deployment/icn-daemon'"

