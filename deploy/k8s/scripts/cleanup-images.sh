#!/usr/bin/env bash
#
# Cleanup old container images on K3s cluster nodes
# This script removes unused ICN images to free disk space
#
# Usage:
#   ./cleanup-images.sh [k3s-control-host] [keep-tag]
#
# Examples:
#   ./cleanup-images.sh                           # Use default host
#   ./cleanup-images.sh ubuntu@10.8.10.40        # Specify host
#   ./cleanup-images.sh ubuntu@10.8.10.40 abc123 # Keep specific tag

set -e

K3S_HOST="${1:-ubuntu@10.8.10.40}"
KEEP_TAG="${2:-}"
K3S_WORKER1="${K3S_WORKER1:-ubuntu@10.8.10.41}"
K3S_WORKER2="${K3S_WORKER2:-ubuntu@10.8.10.42}"

# If no keep tag specified, get the current deployment image tag
if [ -z "$KEEP_TAG" ]; then
    KEEP_TAG=$(ssh "$K3S_HOST" "sudo kubectl -n icn get deployment icn-daemon -o jsonpath='{.spec.template.spec.containers[0].image}'" 2>/dev/null | sed 's/.*://' || echo "")
fi

echo "Will preserve image tag: ${KEEP_TAG:-none}"

log_step() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  $1"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

cleanup_node() {
    local host=$1
    local name=$2
    local keep_tag=$3

    echo "Cleaning up $name ($host)..."

    # Get disk usage before
    local before=$(ssh "$host" "df -h / | tail -1 | awk '{print \$3}'")

    # Remove old ICN images (keep images in use by pods AND the deployment image)
    ssh "$host" "
        KEEP_TAG='$keep_tag'

        # Get images currently in use by pods
        USED_IMAGES=\$(sudo crictl ps -q | xargs -r sudo crictl inspect 2>/dev/null | grep -oP '\"image\":\s*\"\K[^\"]+' | sort -u)

        # Get all ICN images with their tags
        while read -r line; do
            TAG=\$(echo \"\$line\" | awk '{print \$2}')
            IMG_ID=\$(echo \"\$line\" | awk '{print \$3}')

            # Skip if tag matches keep_tag
            if [ -n \"\$KEEP_TAG\" ] && [ \"\$TAG\" = \"\$KEEP_TAG\" ]; then
                continue
            fi

            # Skip if image is in use
            if echo \"\$USED_IMAGES\" | grep -q \"\$IMG_ID\"; then
                continue
            fi

            # Remove the image
            sudo crictl rmi \$IMG_ID 2>/dev/null || true
        done < <(sudo crictl images | grep 'docker.io/library/icn ')

        # Also prune any dangling images
        sudo crictl rmi --prune 2>/dev/null || true
    " 2>&1 | grep -v "DeadlineExceeded" || true

    # Get disk usage after
    local after=$(ssh "$host" "df -h / | tail -1 | awk '{print \$3}'")
    echo "  $name: $before -> $after"
}

echo "ICN Image Cleanup"
echo "================="
echo ""

log_step "Cleaning up container images on all nodes..."

# Clean each node (pass keep_tag to preserve current deployment image)
cleanup_node "$K3S_HOST" "k3s-control" "$KEEP_TAG"
cleanup_node "$K3S_WORKER1" "k3s-worker-1" "$KEEP_TAG"
cleanup_node "$K3S_WORKER2" "k3s-worker-2" "$KEEP_TAG"

log_step "Cleanup complete!"

# Show final disk status
echo ""
echo "Final disk usage:"
for host in "$K3S_HOST" "$K3S_WORKER1" "$K3S_WORKER2"; do
    usage=$(ssh "$host" "df -h / | tail -1 | awk '{print \$5, \$3\"/\"\$2}'")
    echo "  $host: $usage"
done
