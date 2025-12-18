#!/usr/bin/env bash
#
# Full deployment pipeline: build, sync, and deploy ICN to K3s cluster
# This script orchestrates the complete deployment process
#
# Usage:
#   ./full-deploy.sh [tag] [k3s-control-host] [options]
#
# Options:
#   --no-cache    Force fresh Docker build without cache
#   --no-verify   Skip post-deployment verification
#   --rollback    Automatically rollback on failure
#
# Examples:
#   ./full-deploy.sh                                    # Deploy latest
#   ./full-deploy.sh $(git rev-parse --short HEAD)     # Deploy with git hash
#   ./full-deploy.sh v1.0.0 ubuntu@10.8.10.40          # Custom tag and host
#   ./full-deploy.sh latest --no-cache --rollback      # Fresh build with auto-rollback

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
K8S_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TAG="${1:-latest}"
K3S_HOST="${2:-ubuntu@10.8.10.40}"
NO_CACHE=""
VERIFY=true
AUTO_ROLLBACK=false
DEPLOYMENT_START=$(date +%s)
DEPLOY_LOG="$K8S_DIR/deploy.log"
GIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")

# Parse optional arguments
for arg in "$@"; do
    case $arg in
        --no-cache)
            NO_CACHE="--no-cache"
            ;;
        --no-verify)
            VERIFY=false
            ;;
        --rollback)
            AUTO_ROLLBACK=true
            ;;
    esac
done

# Cleanup function for rollback
cleanup() {
    local exit_code=$?
    if [ $exit_code -ne 0 ]; then
        DEPLOYMENT_END=$(date +%s)
        DEPLOYMENT_DURATION=$((DEPLOYMENT_END - DEPLOYMENT_START))
        log_deployment "FAILED" "$DEPLOYMENT_DURATION"
    fi

    if [ "$AUTO_ROLLBACK" = true ] && [ "$DEPLOY_STARTED" = true ]; then
        echo ""
        echo "⚠ Deployment failed! Rolling back..."
        ssh "$K3S_HOST" "sudo kubectl -n icn rollout undo deployment/icn-daemon" 2>/dev/null || true
        log_deployment "ROLLBACK" "0"
        echo "Rollback initiated. Check status with: make status"
    fi
}

trap cleanup EXIT

log_step() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  $1"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# Log deployment to audit file
log_deployment() {
    local status=$1
    local duration=$2
    local timestamp=$(date -Iseconds)
    local user=$(whoami)

    echo "$timestamp | $status | tag=$TAG | git=$GIT_HASH | branch=$GIT_BRANCH | user=$user | duration=${duration}s" >> "$DEPLOY_LOG"
}

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       ICN Full Deployment Pipeline                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Configuration:"
echo "  Tag:          $TAG"
echo "  Target:       $K3S_HOST"
echo "  No-cache:     ${NO_CACHE:-disabled}"
echo "  Auto-verify:  $VERIFY"
echo "  Auto-rollback: $AUTO_ROLLBACK"
echo ""

# Step 1: Build image
log_step "Step 1/5: Building Docker image..."
"$SCRIPT_DIR/build-image.sh" "$TAG" $NO_CACHE

# Step 2: Validate image locally
log_step "Step 2/5: Validating image..."
echo "Checking image exists..."
if docker image inspect "icn:$TAG" > /dev/null 2>&1; then
    echo "✓ Image icn:$TAG exists"
    IMAGE_SIZE=$(docker image inspect "icn:$TAG" --format='{{.Size}}' | numfmt --to=iec 2>/dev/null || echo "unknown")
    echo "  Size: $IMAGE_SIZE"
else
    echo "✗ Image icn:$TAG not found!"
    exit 1
fi

# Step 3: Sync image to cluster
log_step "Step 3/5: Syncing image to K3s cluster..."
"$SCRIPT_DIR/sync-image.sh" "$TAG" "$K3S_HOST"

# Step 4: Deploy to cluster
log_step "Step 4/5: Deploying to K3s cluster..."
DEPLOY_STARTED=true
"$SCRIPT_DIR/deploy.sh" "$K3S_HOST" "$TAG"

# Step 5: Clean up old images to prevent disk exhaustion
log_step "Step 5/5: Cleaning up old images..."
"$SCRIPT_DIR/cleanup-images.sh" "$K3S_HOST" 2>/dev/null || echo "Cleanup skipped (non-fatal)"

# Verify deployment (optional)
if [ "$VERIFY" = true ]; then
    log_step "Verification: Waiting for healthy deployment..."

    echo "Waiting for rollout to complete..."
    if ssh "$K3S_HOST" "sudo kubectl -n icn rollout status deployment/icn-daemon --timeout=120s"; then
        echo "✓ Rollout complete"
    else
        echo "✗ Rollout failed or timed out!"
        exit 1
    fi

    echo ""
    echo "Testing health endpoint..."
    sleep 5  # Give the service a moment to be ready
    if ssh "$K3S_HOST" "curl -sf http://localhost:30080/v1/health > /dev/null"; then
        echo "✓ Health check passed"
    else
        echo "✗ Health check failed!"
        exit 1
    fi
fi

# Success - disable rollback trap
AUTO_ROLLBACK=false
DEPLOYMENT_END=$(date +%s)
DEPLOYMENT_DURATION=$((DEPLOYMENT_END - DEPLOYMENT_START))

# Log successful deployment
log_deployment "SUCCESS" "$DEPLOYMENT_DURATION"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Deployment Complete!                                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Summary:"
echo "  Image:    icn:$TAG"
echo "  Git:      $GIT_HASH ($GIT_BRANCH)"
echo "  Duration: ${DEPLOYMENT_DURATION}s"
echo "  Status:   ✓ Healthy"
echo ""
echo "Access points:"
echo "  Gateway: http://10.8.10.40:30080"
echo "  Metrics: http://10.8.10.40:30091/metrics"
echo ""
echo "Commands:"
echo "  Status:   make status"
echo "  Logs:     make logs"
echo "  Rollback: make rollback"
echo "  History:  make deploy-history"

