#!/usr/bin/env bash
#
# Sync ICN Docker image to K3s cluster nodes
# This script exports the image from Docker and imports it to containerd on each node
#
# Usage:
#   ./sync-image.sh [tag] [k3s-control-host] [options]
#
# Options:
#   --compress    Use gzip compression (slower but less bandwidth)
#   --control-only  Only sync to control node
#
# Examples:
#   ./sync-image.sh                    # Syncs 'latest' tag to all nodes
#   ./sync-image.sh v1.0.0             # Syncs 'v1.0.0' tag
#   ./sync-image.sh latest ubuntu@10.8.10.40 --compress

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
IMAGE_NAME="${IMAGE_NAME:-icn}"
TAG="${1:-latest}"
K3S_HOST="${2:-ubuntu@10.8.10.40}"
FULL_IMAGE="${IMAGE_NAME}:${TAG}"
USE_COMPRESS=false
CONTROL_ONLY=false

# Parse optional arguments
for arg in "$@"; do
    case $arg in
        --compress)
            USE_COMPRESS=true
            ;;
        --control-only)
            CONTROL_ONLY=true
            ;;
    esac
done

TEMP_TAR="/tmp/icn-${TAG}-$(date +%s).tar"
if [ "$USE_COMPRESS" = true ]; then
    TEMP_TAR="${TEMP_TAR}.gz"
fi

echo "Syncing ICN Docker image to K3s cluster..."
echo "  Image: $FULL_IMAGE"
echo "  Target: $K3S_HOST"
echo "  Compress: $USE_COMPRESS"
echo "  All nodes: $([ "$CONTROL_ONLY" = true ] && echo "no (control only)" || echo "yes")"
echo ""

# Check if image exists locally
if ! docker image inspect "$FULL_IMAGE" &>/dev/null; then
    echo "✗ Error: Image $FULL_IMAGE not found locally"
    echo "Run './build-image.sh $TAG' first"
    exit 1
fi

# Get image size
IMAGE_SIZE=$(docker image inspect "$FULL_IMAGE" --format='{{.Size}}' | numfmt --to=iec 2>/dev/null || echo "unknown")
echo "Image size: $IMAGE_SIZE"
echo ""

# Export image to tar
echo "Exporting image..."
if [ "$USE_COMPRESS" = true ]; then
    docker save "$FULL_IMAGE" | gzip > "$TEMP_TAR"
else
    docker save "$FULL_IMAGE" -o "$TEMP_TAR"
fi
TAR_SIZE=$(ls -lh "$TEMP_TAR" | awk '{print $5}')
echo "  Archive size: $TAR_SIZE"

# Copy to K3s control node
echo ""
echo "Uploading to control node ($K3S_HOST)..."
scp "$TEMP_TAR" "${K3S_HOST}:/tmp/icn-image.tar$([ "$USE_COMPRESS" = true ] && echo ".gz" || echo "")"
echo "✓ Upload complete"

# Import to containerd on all nodes
echo ""
echo "Importing to containerd..."

REMOTE_FILE="/tmp/icn-image.tar"
if [ "$USE_COMPRESS" = true ]; then
    REMOTE_FILE="${REMOTE_FILE}.gz"
fi

ssh "$K3S_HOST" <<EOF
set -e

# Import on control node
echo "  → k3s-control..."
if [ "$USE_COMPRESS" = true ]; then
    gunzip -c $REMOTE_FILE | sudo ctr -n k8s.io images import - 2>/dev/null || \
    gunzip -c $REMOTE_FILE | sudo ctr images import -
else
    sudo ctr -n k8s.io images import $REMOTE_FILE 2>/dev/null || \
    sudo ctr images import $REMOTE_FILE
fi
echo "    ✓ Imported on control node"

# Sync to worker nodes (unless control-only)
if [ "$CONTROL_ONLY" != "true" ]; then
    for node in k3s-worker-1 k3s-worker-2; do
        echo "  → \$node..."
        if scp $REMOTE_FILE ubuntu@\$node:$REMOTE_FILE 2>/dev/null; then
            if [ "$USE_COMPRESS" = true ]; then
                ssh ubuntu@\$node "gunzip -c $REMOTE_FILE | sudo ctr -n k8s.io images import - 2>/dev/null || gunzip -c $REMOTE_FILE | sudo ctr images import -" 2>/dev/null && \
                echo "    ✓ Imported on \$node" || \
                echo "    ⚠ Failed to import on \$node (node may be offline)"
            else
                ssh ubuntu@\$node "sudo ctr -n k8s.io images import $REMOTE_FILE 2>/dev/null || sudo ctr images import $REMOTE_FILE" 2>/dev/null && \
                echo "    ✓ Imported on \$node" || \
                echo "    ⚠ Failed to import on \$node (node may be offline)"
            fi
            ssh ubuntu@\$node "rm -f $REMOTE_FILE" 2>/dev/null || true
        else
            echo "    ⚠ Could not reach \$node (node may be offline)"
        fi
    done
fi

# Cleanup control node
rm -f $REMOTE_FILE
EOF

# Cleanup local temp file
rm -f "$TEMP_TAR"

echo ""
echo "✓ Image synced successfully to K3s cluster"
echo ""
echo "To deploy, run:"
echo "  ./deploy/k8s/scripts/deploy.sh"

