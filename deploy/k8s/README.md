# ICN Kubernetes Deployment

This directory contains all Kubernetes manifests and deployment scripts for running ICN on a K3s cluster.

## Quick Start

### Prerequisites

1. **Docker** installed on your development machine
2. **SSH access** to K3s cluster control node (default: `ubuntu@10.8.10.40`)
3. **Kubectl** access to the cluster (via SSH or local config)

### Full Deployment (Recommended)

Deploy everything with one command:

```bash
cd /home/matt/projects/icn/deploy/k8s/scripts
./full-deploy.sh
```

This will:
1. Build the Docker image from source
2. Sync the image to all K3s nodes
3. Apply Kubernetes manifests to deploy ICN

### Step-by-Step Deployment

#### 1. Build Docker Image

```bash
./scripts/build-image.sh [tag]
```

Examples:
```bash
./scripts/build-image.sh                           # Uses 'latest' tag
./scripts/build-image.sh v1.0.0                    # Uses 'v1.0.0' tag
./scripts/build-image.sh $(git rev-parse --short HEAD)  # Uses git hash
```

#### 2. Sync Image to K3s Cluster

```bash
./scripts/sync-image.sh [tag] [k3s-host]
```

This exports the Docker image and imports it to containerd on all K3s nodes.

Examples:
```bash
./scripts/sync-image.sh                            # Sync 'latest' to default host
./scripts/sync-image.sh v1.0.0                     # Sync 'v1.0.0'
./scripts/sync-image.sh latest ubuntu@10.8.10.40   # Custom host
```

#### 3. Deploy to Cluster

```bash
./scripts/deploy.sh [k3s-host] [image-tag]
```

This applies all Kubernetes manifests.

Examples:
```bash
./scripts/deploy.sh                                # Deploy to default host
./scripts/deploy.sh ubuntu@10.8.10.40 v1.0.0       # Custom host and tag
```

### Manual Deployment

If you prefer to use kubectl directly:

```bash
# Apply all manifests
kubectl apply -k .                    # Using kustomize
# OR
kubectl apply -f namespace.yaml
kubectl apply -f configmap.yaml
kubectl apply -f pvc.yaml
kubectl apply -f deployment.yaml
kubectl apply -f services.yaml
kubectl apply -f monitoring/servicemonitor.yaml
```

## Configuration

### Secrets

**IMPORTANT**: Secrets are not included in the repo. Create your own `secret.yaml`:

```bash
cp secret.yaml.example secret.yaml
# Edit secret.yaml with your passphrase
kubectl apply -f secret.yaml
```

The secret should contain:
- `icn-secrets` with key `passphrase` for the ICN keystore passphrase

### ConfigMap

Edit `configmap.yaml` to customize ICN configuration:

- Network settings (listen address, ports)
- Rate limiting
- Topology settings
- Log levels

After editing, apply with:
```bash
kubectl apply -f configmap.yaml
kubectl rollout restart deployment/icn-daemon -n icn
```

### Image Tag Updates

To update the image tag used by the deployment:

```bash
# Using kubectl
kubectl set image deployment/icn-daemon -n icn icnd=icn:v1.0.0

# Or edit deployment.yaml and reapply
kubectl apply -f deployment.yaml
```

### Storage

The deployment uses a PersistentVolumeClaim on the `atlas-nfs` storage class. 

To change storage size or class, edit `pvc.yaml`:
```yaml
spec:
  storageClassName: atlas-nfs
  resources:
    requests:
      storage: 10Gi  # Change this
```

**Note**: Existing PVCs cannot be resized. You'll need to delete and recreate (data will be lost unless backed up).

## Monitoring

The deployment includes:
- **ServiceMonitor** for Prometheus metrics scraping
- **PrometheusRule** with ICN-specific alerts

Metrics are exposed on port `9100` at `/metrics`.

### View Metrics

```bash
# Port forward to access metrics locally
kubectl -n icn port-forward svc/icn 9100:9100
# Then visit http://localhost:9100/metrics
```

### Access Grafana

If Prometheus/Grafana is deployed in the `monitoring` namespace:

```bash
kubectl -n monitoring port-forward svc/prometheus-grafana 3000:80
# Then visit http://localhost:3000
```

## Troubleshooting

### Check Pod Status

```bash
kubectl -n icn get pods
kubectl -n icn describe pod <pod-name>
```

### View Logs

```bash
# Follow logs
kubectl -n icn logs -f deployment/icn-daemon

# View recent logs
kubectl -n icn logs --tail=100 deployment/icn-daemon
```

### Check Events

```bash
kubectl -n icn get events --sort-by='.lastTimestamp'
```

### Common Issues

**Pod won't start:**
- Check if secrets are created: `kubectl -n icn get secrets`
- Check PVC status: `kubectl -n icn get pvc`
- Check image exists: `crictl images | grep icn`

**Image pull errors:**
- Verify image was synced: Run `sync-image.sh` again
- Check image name matches deployment: `kubectl -n icn get deployment icn-daemon -o yaml | grep image`

**Port conflicts:**
- Check if ports are in use: `kubectl -n icn get svc`
- Modify NodePort values in `services.yaml` if needed

### Manual Image Sync

If the sync script fails, manually sync to a single node:

```bash
# Export image
docker save icn:latest -o /tmp/icn.tar

# Copy to node
scp /tmp/icn.tar ubuntu@10.8.10.40:/tmp/

# Import on node
ssh ubuntu@10.8.10.40
sudo ctr -n k8s.io images import /tmp/icn.tar
# OR
sudo ctr images import /tmp/icn.tar
```

## Updating Deployment

### Update ICN Version

1. Build new image:
   ```bash
   ./scripts/build-image.sh v1.0.1
   ```

2. Sync to cluster:
   ```bash
   ./scripts/sync-image.sh v1.0.1
   ```

3. Update deployment:
   ```bash
   kubectl set image deployment/icn-daemon -n icn icnd=icn:v1.0.1
   ```

### Update Configuration

1. Edit `configmap.yaml`
2. Apply changes:
   ```bash
   kubectl apply -f configmap.yaml
   kubectl rollout restart deployment/icn-daemon -n icn
   ```

## File Structure

```
deploy/k8s/
├── README.md                    # This file
├── kustomization.yaml          # Kustomize configuration
├── namespace.yaml              # ICN namespace
├── configmap.yaml              # ICN configuration
├── secret.yaml.example         # Secret template (DO NOT commit secret.yaml!)
├── pvc.yaml                    # Persistent volume claim
├── deployment.yaml             # ICN daemon deployment
├── services.yaml               # ClusterIP and NodePort services
├── monitoring/
│   └── servicemonitor.yaml    # Prometheus ServiceMonitor and alerts
└── scripts/
    ├── build-image.sh         # Build Docker image
    ├── sync-image.sh          # Sync image to K3s nodes
    ├── deploy.sh              # Apply Kubernetes manifests
    └── full-deploy.sh         # Complete deployment pipeline
```

## Development Workflow

1. **Make code changes** in the ICN repository
2. **Build new image**: `./scripts/build-image.sh dev-$(date +%s)`
3. **Sync to cluster**: `./scripts/sync-image.sh dev-$(date +%s)`
4. **Update deployment**: `kubectl set image deployment/icn-daemon -n icn icnd=icn:dev-$(date +%s)`
5. **Watch rollout**: `kubectl -n icn rollout status deployment/icn-daemon`
6. **Check logs**: `kubectl -n icn logs -f deployment/icn-daemon`

## CI/CD Integration

For automated deployments, you can integrate these scripts into CI/CD:

```bash
# In your CI pipeline
export IMAGE_TAG="$(git rev-parse --short HEAD)"
./deploy/k8s/scripts/build-image.sh "$IMAGE_TAG"
./deploy/k8s/scripts/sync-image.sh "$IMAGE_TAG"
./deploy/k8s/scripts/deploy.sh "$K3S_HOST" "$IMAGE_TAG"
```

## Next Steps

- [ ] Set up local container registry for faster image sync
- [ ] Configure GitOps (Flux/ArgoCD) for automated deployments
- [ ] Add health check endpoints
- [ ] Set up automated backups of PVC data
- [ ] Configure resource limits based on workload

