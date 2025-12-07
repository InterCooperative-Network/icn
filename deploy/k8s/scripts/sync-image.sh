#!/usr/bin/env bash
#
# Sync ICN Docker image to K3s cluster nodes
# This script exports the image from Docker and imports it to containerd on each node
#
# Usage:
#   ./sync-image.sh [tag] [k3s-control-host]
#
# Examples:
#   ./sync-image.sh                    # Syncs 'latest' tag to default host
#   ./sync-image.sh v1.0.0             # Syncs 'v1.0.0' tag
#   ./sync-image.sh latest ubuntu@10.8.10.40  # Custom host

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
IMAGE_NAME="${IMAGE_NAME:-icn}"
TAG="${1:-latest}"
K3S_HOST="${2:-ubuntu@10.8.10.40}"
FULL_IMAGE="${IMAGE_NAME}:${TAG}"
TEMP_TAR="/tmp/icn-${TAG}-$(date +%s).tar"

echo "Syncing ICN Docker image to K3s cluster..."
echo "  Image: $FULL_IMAGE"
echo "  Target: $K3S_HOST"
echo ""

# Check if image exists locally
if ! docker image inspect "$FULL_IMAGE" &>/dev/null; then
    echo "Error: Image $FULL_IMAGE not found locally"
    echo "Run './build-image.sh $TAG' first"
    exit 1
fi

# Export image to tar
echo "Exporting image..."
docker save "$FULL_IMAGE" -o "$TEMP_TAR"

# Copy to K3s control node
echo "Copying to K3s node..."
scp "$TEMP_TAR" "${K3S_HOST}:/tmp/icn-image.tar"

# Import to containerd on all nodes
echo "Importing to containerd..."
ssh "$K3S_HOST" <<EOF
set -e
echo "Importing image on control node..."
sudo ctr -n k8s.io images import /tmp/icn-image.tar || \
  sudo ctr images import /tmp/icn-image.tar

# Sync to worker nodes
for node in k3s-worker-1 k3s-worker-2; do
    echo "Syncing to \$node..."
    scp /tmp/icn-image.tar ubuntu@\$node:/tmp/icn-image.tar || true
    ssh ubuntu@\$node "sudo ctr -n k8s.io images import /tmp/icn-image.tar || sudo ctr images import /tmp/icn-image.tar" || true
done

# Cleanup
rm -f /tmp/icn-image.tar
EOF

# Cleanup local temp file
rm -f "$TEMP_TAR"

echo ""
echo "✓ Image synced successfully to K3s cluster"
echo ""
echo "To deploy, run:"
echo "  ./deploy/k8s/scripts/deploy.sh"

