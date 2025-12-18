#!/usr/bin/env bash
#
# Cleanup old container images on K3s cluster nodes
# This script removes unused ICN images to free disk space
#
# Usage:
#   ./cleanup-images.sh [k3s-control-host]
#
# Examples:
#   ./cleanup-images.sh                        # Use default host
#   ./cleanup-images.sh ubuntu@10.8.10.40     # Specify host

set -e

K3S_HOST="${1:-ubuntu@10.8.10.40}"
K3S_WORKER1="${K3S_WORKER1:-ubuntu@10.8.10.41}"
K3S_WORKER2="${K3S_WORKER2:-ubuntu@10.8.10.42}"

log_step() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  $1"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

cleanup_node() {
    local host=$1
    local name=$2

    echo "Cleaning up $name ($host)..."

    # Get disk usage before
    local before=$(ssh "$host" "df -h / | tail -1 | awk '{print \$3}'")

    # Remove old ICN images (keep only images currently in use by pods)
    # This gets images used by running containers and keeps only those
    ssh "$host" "
        # Get images currently in use by pods
        USED_IMAGES=\$(sudo crictl ps -q | xargs -r sudo crictl inspect 2>/dev/null | grep -oP '\"image\":\s*\"\K[^\"]+' | sort -u)

        # Get all ICN images
        ALL_ICN=\$(sudo crictl images | grep 'docker.io/library/icn ' | awk '{print \$3}')

        # Remove images not in use
        for img in \$ALL_ICN; do
            if ! echo \"\$USED_IMAGES\" | grep -q \"\$img\"; then
                sudo crictl rmi \$img 2>/dev/null || true
            fi
        done

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

# Clean each node
cleanup_node "$K3S_HOST" "k3s-control"
cleanup_node "$K3S_WORKER1" "k3s-worker-1"
cleanup_node "$K3S_WORKER2" "k3s-worker-2"

log_step "Cleanup complete!"

# Show final disk status
echo ""
echo "Final disk usage:"
for host in "$K3S_HOST" "$K3S_WORKER1" "$K3S_WORKER2"; do
    usage=$(ssh "$host" "df -h / | tail -1 | awk '{print \$5, \$3\"/\"\$2}'")
    echo "  $host: $usage"
done
