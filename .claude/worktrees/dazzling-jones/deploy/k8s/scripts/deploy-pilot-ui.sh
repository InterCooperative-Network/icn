#!/bin/bash
# Deploy Pilot UI to K3s cluster
# Usage: ./deploy-pilot-ui.sh [k3s-host]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
K3S_HOST="${1:-ubuntu@10.8.10.40}"
IMAGE_NAME="icn-pilot-ui"
IMAGE_TAG="${2:-latest}"

echo "=== ICN Pilot UI Deployment ==="
echo "Project root: $PROJECT_ROOT"
echo "K3s host: $K3S_HOST"
echo "Image: $IMAGE_NAME:$IMAGE_TAG"
echo ""

# Step 1: Build Docker image
echo "=== Building Docker image ==="
cd "$PROJECT_ROOT/web/pilot-ui"

# Update default Gateway URL for staging
echo "Updating default Gateway URL for staging..."
STAGING_GATEWAY="http://10.8.10.40:30080"

# Backup originals
cp index.html index.html.bak
cp app.js app.js.bak

# Update default value in index.html
sed -i "s|value=\"http://localhost:8080\"|value=\"$STAGING_GATEWAY\"|g" index.html

# Update help modal default in app.js
sed -i "s|'http://localhost:8080'|'$STAGING_GATEWAY'|g" app.js

docker build -t "$IMAGE_NAME:$IMAGE_TAG" .

# Restore originals
mv index.html.bak index.html
mv app.js.bak app.js

echo "Image built: $IMAGE_NAME:$IMAGE_TAG"

# Step 2: Export image
echo ""
echo "=== Exporting Docker image ==="
TEMP_TAR="/tmp/${IMAGE_NAME}-${IMAGE_TAG}.tar"
docker save "$IMAGE_NAME:$IMAGE_TAG" -o "$TEMP_TAR"
echo "Exported to: $TEMP_TAR ($(du -h "$TEMP_TAR" | cut -f1))"

# Step 3: Copy to K3s host
echo ""
echo "=== Copying image to K3s host ==="
scp "$TEMP_TAR" "$K3S_HOST:/tmp/"

# Step 4: Import into containerd on all nodes
echo ""
echo "=== Importing image to containerd ==="

# Get list of worker nodes
NODES=$(ssh "$K3S_HOST" "sudo kubectl get nodes -o jsonpath='{.items[*].status.addresses[?(@.type==\"InternalIP\")].address}'")

for NODE in $NODES; do
    echo "Importing to node: $NODE"
    if [ "$NODE" == "10.8.10.40" ]; then
        # Control plane - already has the file
        ssh "$K3S_HOST" "sudo k3s ctr images import /tmp/${IMAGE_NAME}-${IMAGE_TAG}.tar"
    else
        # Worker node - need to copy first
        ssh "$K3S_HOST" "scp /tmp/${IMAGE_NAME}-${IMAGE_TAG}.tar ubuntu@$NODE:/tmp/"
        ssh "$K3S_HOST" "ssh ubuntu@$NODE 'sudo k3s ctr images import /tmp/${IMAGE_NAME}-${IMAGE_TAG}.tar'"
        ssh "$K3S_HOST" "ssh ubuntu@$NODE 'rm /tmp/${IMAGE_NAME}-${IMAGE_TAG}.tar'"
    fi
done

# Cleanup temp file on control plane
ssh "$K3S_HOST" "rm /tmp/${IMAGE_NAME}-${IMAGE_TAG}.tar"
rm "$TEMP_TAR"

# Step 5: Apply K8s manifests
echo ""
echo "=== Applying Kubernetes manifests ==="
cd "$SCRIPT_DIR/.."
scp pilot-ui-deployment.yaml "$K3S_HOST:/tmp/"
ssh "$K3S_HOST" "sudo kubectl apply -f /tmp/pilot-ui-deployment.yaml"
ssh "$K3S_HOST" "rm /tmp/pilot-ui-deployment.yaml"

# Step 6: Restart deployment to pick up new image
echo ""
echo "=== Restarting deployment ==="
ssh "$K3S_HOST" "sudo kubectl -n icn rollout restart deployment/pilot-ui"

# Step 7: Wait for rollout
echo ""
echo "=== Waiting for rollout ==="
ssh "$K3S_HOST" "sudo kubectl -n icn rollout status deployment/pilot-ui --timeout=120s"

# Step 8: Show status
echo ""
echo "=== Deployment Status ==="
ssh "$K3S_HOST" "sudo kubectl -n icn get pods -l component=pilot-ui"
ssh "$K3S_HOST" "sudo kubectl -n icn get svc pilot-ui-nodeport"

echo ""
echo "=== Pilot UI deployed successfully! ==="
echo ""
echo "Access URLs:"
echo "  - http://10.8.10.40:30030 (Control Plane)"
echo "  - http://10.8.10.41:30030 (Worker 1)"
echo "  - http://10.8.10.42:30030 (Worker 2)"
echo ""
echo "Gateway API: http://10.8.10.40:30080"
