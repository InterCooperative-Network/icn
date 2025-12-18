#!/usr/bin/env bash
#
# Set up automated image cleanup on K3s nodes
# Installs a daily cron job to clean up unused container images
#
# Usage:
#   ./setup-auto-cleanup.sh [k3s-control-host]
#
# This will:
#   - Install a cleanup script on each node
#   - Set up a daily cron job to run cleanup at 3am
#   - Configure k3s kubelet garbage collection thresholds

set -e

K3S_HOST="${1:-ubuntu@10.8.10.40}"
K3S_WORKER1="${K3S_WORKER1:-ubuntu@10.8.10.41}"
K3S_WORKER2="${K3S_WORKER2:-ubuntu@10.8.10.42}"

CLEANUP_SCRIPT='#!/bin/bash
# ICN automatic image cleanup - installed by setup-auto-cleanup.sh
# Cleans unused container images to prevent disk exhaustion

LOG_FILE="/var/log/icn-image-cleanup.log"

log() {
    echo "$(date -Iseconds) $1" >> "$LOG_FILE"
}

log "Starting image cleanup"

# Get disk usage before
BEFORE=$(df / | tail -1 | awk "{print \$5}")

# Get images currently in use by pods
USED_IMAGES=$(crictl ps -q | xargs -r crictl inspect 2>/dev/null | grep -oP "\"image\":\s*\"\K[^\"]+" | sort -u)

# Get all ICN images
ALL_ICN=$(crictl images | grep "docker.io/library/icn " | awk "{print \$3}")

# Remove images not in use
REMOVED=0
for img in $ALL_ICN; do
    if ! echo "$USED_IMAGES" | grep -q "$img"; then
        if crictl rmi $img 2>/dev/null; then
            REMOVED=$((REMOVED + 1))
        fi
    fi
done

# Prune any dangling images
crictl rmi --prune 2>/dev/null || true

# Get disk usage after
AFTER=$(df / | tail -1 | awk "{print \$5}")

log "Cleanup complete: removed $REMOVED images, disk $BEFORE -> $AFTER"
'

setup_node() {
    local host=$1
    local name=$2

    echo "Setting up auto-cleanup on $name ($host)..."

    # Install cleanup script
    echo "$CLEANUP_SCRIPT" | ssh "$host" "sudo tee /usr/local/bin/icn-cleanup-images > /dev/null && sudo chmod +x /usr/local/bin/icn-cleanup-images"

    # Install cron job (daily at 3am)
    ssh "$host" "echo '0 3 * * * root /usr/local/bin/icn-cleanup-images' | sudo tee /etc/cron.d/icn-cleanup > /dev/null"

    # Also clean images immediately when disk usage is high (via a systemd timer would be better but cron is simpler)
    # Add a check every 6 hours
    ssh "$host" "echo '0 */6 * * * root [ \$(df / | tail -1 | awk \"{print int(\\\$5)}\") -gt 80 ] && /usr/local/bin/icn-cleanup-images' | sudo tee -a /etc/cron.d/icn-cleanup > /dev/null"

    echo "  ✓ Installed cleanup script and cron jobs"
}

echo "ICN Auto-Cleanup Setup"
echo "======================"
echo ""
echo "This will install:"
echo "  - Cleanup script at /usr/local/bin/icn-cleanup-images"
echo "  - Daily cron job (3am) at /etc/cron.d/icn-cleanup"
echo "  - Emergency cleanup when disk >80% (every 6 hours)"
echo ""

for node in "$K3S_HOST" "$K3S_WORKER1" "$K3S_WORKER2"; do
    setup_node "$node" "$node"
done

echo ""
echo "Auto-cleanup configured on all nodes!"
echo ""
echo "To check logs: ssh <node> 'cat /var/log/icn-image-cleanup.log'"
echo "To run manually: ssh <node> 'sudo /usr/local/bin/icn-cleanup-images'"
