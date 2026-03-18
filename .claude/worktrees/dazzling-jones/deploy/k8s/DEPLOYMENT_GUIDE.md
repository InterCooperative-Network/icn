# ICN K3s Deployment Guide

Complete guide for deploying ICN to your K3s cluster on Hyperion.

## Overview

This deployment setup provides a complete solution for syncing ICN development with your K3s cluster. It includes:

- ✅ **Kubernetes Manifests** - All resources version controlled
- ✅ **Build Scripts** - Automated Docker image building
- ✅ **Image Sync** - Automated image distribution to all K3s nodes
- ✅ **Deployment Scripts** - One-command deployment
- ✅ **Makefile** - Convenient shortcuts for common tasks

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Development Machine (WSL/Ubuntu)                            │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  ICN Source Code (/home/matt/projects/icn)          │   │
│  │  ↓                                                    │   │
│  │  Docker Build (build-image.sh)                       │   │
│  │  ↓                                                    │   │
│  │  Image: icn:latest                                   │   │
│  └──────────────────────────────────────────────────────┘   │
└────────────────────┬────────────────────────────────────────┘
                     │ SSH + SCP
                     ↓
┌─────────────────────────────────────────────────────────────┐
│ K3s Cluster on Hyperion                                      │
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ k3s-control  │  │ k3s-worker-1 │  │ k3s-worker-2 │      │
│  │ 10.8.10.40   │  │ 10.8.10.41   │  │ 10.8.10.42   │      │
│  │              │  │              │  │              │      │
│  │ containerd   │  │ containerd   │  │ containerd   │      │
│  │  ↓           │  │  ↓           │  │  ↓           │      │
│  │ icn:latest   │  │ icn:latest   │  │ icn:latest   │      │
│  └──────┬───────┘  └──────────────┘  └──────────────┘      │
│         │                                                    │
│         ↓                                                    │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Kubernetes Resources (deploy/k8s/)                  │   │
│  │  - Namespace: icn                                     │   │
│  │  - Deployment: icn-daemon                             │   │
│  │  - Services: ClusterIP + NodePort                     │   │
│  │  - PVC: icn-data (10Gi on atlas-nfs)                 │   │
│  │  - ConfigMap: icn-config                              │   │
│  │  - Secret: icn-secrets                                │   │
│  │  - ServiceMonitor: Prometheus metrics                 │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

1. **SSH Access**: Configured SSH key access to K3s nodes
   ```bash
   ssh ubuntu@10.8.10.40  # Should work without password
   ```

2. **Docker**: Installed on development machine
   ```bash
   docker --version
   ```

3. **Git**: Repository cloned
   ```bash
   cd /home/matt/projects/icn
   ```

### First-Time Setup

1. **Create Secrets**
   ```bash
   cd deploy/k8s
   cp secret.yaml.example secret.yaml
   # Edit secret.yaml with your ICN passphrase
   # DO NOT commit secret.yaml!
   ```

2. **Deploy Secrets**
   ```bash
   ssh ubuntu@10.8.10.40 "sudo kubectl apply -f -" < secret.yaml
   ```

3. **Full Deployment**
   ```bash
   cd deploy/k8s/scripts
   ./full-deploy.sh
   ```

### Daily Development Workflow

After making code changes:

```bash
cd /home/matt/projects/icn/deploy/k8s

# Option 1: Using Makefile (recommended)
make full-deploy-dev  # Uses git hash as tag

# Option 2: Using scripts directly
./scripts/full-deploy.sh $(git rev-parse --short HEAD)

# Option 3: Step by step
make build-dev        # Build with git hash
make sync             # Sync to cluster
make deploy           # Deploy manifests
```

## Deployment Methods

### Method 1: Makefile (Recommended)

The Makefile provides convenient shortcuts:

```bash
cd deploy/k8s

# Full deployment with git hash
make full-deploy-dev

# Full deployment with custom tag
make full-deploy IMAGE_TAG=v1.0.0

# Individual steps
make build            # Build Docker image
make sync             # Sync to cluster
make deploy           # Deploy to cluster

# Management
make status           # Check deployment status
make logs             # Tail logs
make logs-recent      # Recent logs
make restart          # Restart deployment
```

### Method 2: Full Deploy Script

Single command for everything:

```bash
cd deploy/k8s/scripts
./full-deploy.sh [tag] [k3s-host]

# Examples:
./full-deploy.sh                                    # latest, default host
./full-deploy.sh v1.0.0                            # custom tag
./full-deploy.sh $(git rev-parse --short HEAD)     # git hash
./full-deploy.sh latest ubuntu@10.8.10.40          # custom host
```

### Method 3: Step-by-Step

For more control:

```bash
cd deploy/k8s/scripts

# 1. Build image
./build-image.sh v1.0.0

# 2. Sync to cluster
./sync-image.sh v1.0.0

# 3. Deploy manifests
./deploy.sh ubuntu@10.8.10.40 v1.0.0
```

### Method 4: Manual kubectl

For maximum control:

```bash
cd deploy/k8s

# Apply all manifests
kubectl apply -k .                    # Using kustomize
# OR
kubectl apply -f namespace.yaml
kubectl apply -f configmap.yaml
kubectl apply -f pvc.yaml
kubectl apply -f deployment.yaml
kubectl apply -f services.yaml
kubectl apply -f monitoring/servicemonitor.yaml

# Update image
kubectl set image deployment/icn-daemon -n icn icnd=icn:v1.0.0
```

## Image Tagging Strategy

Recommended tagging approach:

```bash
# Development builds
./build-image.sh dev-$(date +%s)              # Timestamp
./build-image.sh $(git rev-parse --short HEAD) # Git hash
./build-image.sh dev-$(whoami)-$(date +%Y%m%d) # Dev name + date

# Release builds
./build-image.sh v1.0.0                       # Semantic version
./build-image.sh v1.0.0-rc1                  # Release candidate
```

## Configuration

### Updating ICN Configuration

1. Edit `configmap.yaml`:
   ```yaml
   data:
     icn.toml: |
       # Your config changes here
   ```

2. Apply changes:
   ```bash
   kubectl apply -f configmap.yaml
   kubectl rollout restart deployment/icn-daemon -n icn
   ```

### Updating Secrets

Secrets are managed separately (not in git):

```bash
# Edit secret.yaml (local file, not committed)
vim secret.yaml

# Apply
kubectl apply -f secret.yaml

# Restart to pick up new secrets
kubectl rollout restart deployment/icn-daemon -n icn
```

### Resource Limits

Edit `deployment.yaml` to adjust resources:

```yaml
resources:
  requests:
    cpu: 100m
    memory: 512Mi
  limits:
    cpu: "1"
    memory: 2Gi
```

### Storage

Edit `pvc.yaml` to change storage:

```yaml
spec:
  storageClassName: atlas-nfs
  resources:
    requests:
      storage: 10Gi  # Change size here
```

**Note**: PVCs cannot be resized. To increase:
1. Backup data
2. Delete PVC
3. Recreate with new size
4. Restore data

## Monitoring

### View Metrics

```bash
# Port forward metrics endpoint
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn port-forward svc/icn 9100:9100"
# Visit http://localhost:9100/metrics
```

### Access Grafana

If Prometheus/Grafana is deployed:

```bash
# Port forward Grafana
ssh ubuntu@10.8.10.40 "sudo kubectl -n monitoring port-forward svc/prometheus-grafana 3000:80"
# Visit http://localhost:3000
```

### Check Pod Status

```bash
make status
# OR
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods,svc,pvc"
```

### View Logs

```bash
# Tail logs
make logs

# Recent logs
make logs-recent

# Specific pod
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs <pod-name>"
```

## Troubleshooting

### Image Build Fails

**Problem**: Docker build fails

**Solutions**:
1. Check Docker is running: `docker ps`
2. Check build context: Ensure `icn/` directory has Cargo.toml
3. Check Dockerfile path: Should be `deploy/Dockerfile.icnd`

### Image Sync Fails

**Problem**: Can't sync image to cluster

**Solutions**:
1. Check SSH access: `ssh ubuntu@10.8.10.40`
2. Check disk space on nodes
3. Try manual sync (see Manual Image Sync section)

### Pod Won't Start

**Problem**: Pod stays in CrashLoopBackOff or Pending

**Solutions**:

1. **Check pod status**:
   ```bash
   kubectl -n icn describe pod <pod-name>
   ```

2. **Check secrets exist**:
   ```bash
   kubectl -n icn get secrets
   ```

3. **Check PVC is bound**:
   ```bash
   kubectl -n icn get pvc
   ```

4. **Check image exists**:
   ```bash
   ssh ubuntu@10.8.10.40 "sudo crictl images | grep icn"
   ```

5. **Check logs**:
   ```bash
   kubectl -n icn logs <pod-name>
   ```

### Port Conflicts

**Problem**: Service ports already in use

**Solutions**:
1. Check existing services: `kubectl -n icn get svc`
2. Edit `services.yaml` to use different ports
3. Reapply: `kubectl apply -f services.yaml`

### Configuration Not Applied

**Problem**: ConfigMap changes not taking effect

**Solutions**:
1. Verify ConfigMap applied: `kubectl -n icn get configmap icn-config -o yaml`
2. Restart deployment: `kubectl rollout restart deployment/icn-daemon -n icn`
3. Check pod is using new config: `kubectl -n icn exec <pod> -- cat /etc/icn/icn.toml`

## Manual Image Sync

If automated sync fails, manually sync to a single node:

```bash
# 1. Export image from Docker
docker save icn:latest -o /tmp/icn.tar

# 2. Copy to node
scp /tmp/icn.tar ubuntu@10.8.10.40:/tmp/

# 3. Import on node
ssh ubuntu@10.8.10.40
sudo ctr -n k8s.io images import /tmp/icn.tar
# OR
sudo ctr images import /tmp/icn.tar

# 4. Cleanup
rm /tmp/icn.tar
ssh ubuntu@10.8.10.40 "rm /tmp/icn.tar"
```

## Local Registry (Optional)

For faster deployments, set up a local registry:

### Option 1: Docker Registry on K3s Node

```bash
# On k3s-control node
docker run -d -p 5000:5000 --restart=always --name registry registry:2

# Build and push
docker build -f deploy/Dockerfile.icnd -t localhost:5000/icn:latest icn/
docker push localhost:5000/icn:latest

# Update deployment to use registry
kubectl set image deployment/icn-daemon -n icn icnd=localhost:5000/icn:latest
```

### Option 2: Harbor Registry (Production)

See [Harbor documentation](https://goharbor.io/docs/) for setup.

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Deploy to K3s

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Build image
        run: |
          cd deploy/k8s/scripts
          ./build-image.sh ${{ github.sha }}
      
      - name: Sync to cluster
        run: |
          cd deploy/k8s/scripts
          ./sync-image.sh ${{ github.sha }} ${{ secrets.K3S_HOST }}
        env:
          SSH_PRIVATE_KEY: ${{ secrets.SSH_PRIVATE_KEY }}
      
      - name: Deploy
        run: |
          cd deploy/k8s/scripts
          ./deploy.sh ${{ secrets.K3S_HOST }} ${{ github.sha }}
```

## Best Practices

1. **Always use version tags** - Never deploy untagged images
2. **Test locally first** - Build and test before deploying
3. **Use git hashes for dev** - Easy to track what's deployed
4. **Backup before changes** - Especially for configuration changes
5. **Monitor after deployment** - Watch logs and metrics
6. **Keep secrets safe** - Never commit secret.yaml
7. **Document changes** - Update configmap.yaml comments

## Next Steps

- [ ] Set up local container registry
- [ ] Configure GitOps (Flux/ArgoCD)
- [ ] Add automated backups
- [ ] Set up CI/CD pipeline
- [ ] Configure resource autoscaling
- [ ] Add health check endpoints

## Support

For issues:
1. Check logs: `make logs`
2. Check status: `make status`
3. Review this guide
4. Check Kubernetes events: `kubectl -n icn get events`

