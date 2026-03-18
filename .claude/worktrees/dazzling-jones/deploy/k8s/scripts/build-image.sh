#!/usr/bin/env bash
#
# Build ICN Docker image for K3s deployment
# This script builds the image and tags it appropriately
#
# Usage:
#   ./build-image.sh [tag] [--no-cache]
#
# Examples:
#   ./build-image.sh                    # Builds with 'latest' tag
#   ./build-image.sh v1.0.0             # Builds with 'v1.0.0' tag
#   ./build-image.sh $(git rev-parse --short HEAD)  # Builds with git hash tag
#   ./build-image.sh latest --no-cache  # Force fresh build without cache

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
IMAGE_NAME="${IMAGE_NAME:-icn}"
TAG="${1:-latest}"
NO_CACHE=""

# Parse arguments
for arg in "$@"; do
    case $arg in
        --no-cache)
            NO_CACHE="--no-cache"
            echo "⚠ Building without cache (fresh build)"
            ;;
    esac
done

FULL_IMAGE="${IMAGE_NAME}:${TAG}"

echo "Building ICN Docker image..."
echo "  Image: $FULL_IMAGE"
echo "  Context: $REPO_ROOT"
echo "  No-cache: ${NO_CACHE:-disabled}"
echo ""

cd "$REPO_ROOT"

# Build-time provenance
GIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo "  Git SHA: $GIT_SHA"
echo "  Build time: $BUILD_TIME"

# Build the image
# Use Dockerfile.icnd with repo root as context (apps/ lives outside icn/ workspace)
docker build \
  $NO_CACHE \
  --build-arg GIT_SHA="$GIT_SHA" \
  --build-arg BUILD_TIME="$BUILD_TIME" \
  -f deploy/Dockerfile.icnd \
  -t "$FULL_IMAGE" \
  -t "${IMAGE_NAME}:latest" \
  "$REPO_ROOT"

echo ""
echo "✓ Image built successfully: $FULL_IMAGE"
echo ""
echo "To load image to K3s cluster, run:"
echo "  ./deploy/k8s/scripts/sync-image.sh $TAG"

