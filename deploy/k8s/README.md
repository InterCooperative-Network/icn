# ICN Kubernetes Deployment

This directory contains all Kubernetes manifests and deployment scripts for running ICN on a K3s cluster.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         K3s Cluster (Hyperion)                          │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                        ICN Namespace                             │   │
│  │                                                                  │   │
│  │  ┌──────────────────┐      ┌──────────────────┐                 │   │
│  │  │   ICN Daemon     │      │    Pilot UI      │                 │   │
│  │  │   (Pod)          │◄────►│    (Pod)         │                 │   │
│  │  │                  │      │                  │                 │   │
│  │  │  ┌────────────┐  │      │  nginx serving   │                 │   │
│  │  │  │ init:      │  │      │  React app       │                 │   │
│  │  │  │ fix-perms  │  │      └────────┬─────────┘                 │   │
│  │  │  └────────────┘  │               │                           │   │
│  │  │  ┌────────────┐  │               │ :3000                     │   │
│  │  │  │ icnd       │  │               ▼                           │   │
│  │  │  │ (Rust)     │  │      ┌──────────────────┐                 │   │
│  │  │  └────────────┘  │      │ pilot-ui-nodeport│ :30030          │   │
│  │  │        │         │      └──────────────────┘                 │   │
│  │  │   :7777 UDP (P2P)│                                           │   │
│  │  │   :5601 TCP (RPC)│                                           │   │
│  │  │   :8080 TCP (API)│                                           │   │
│  │  │   :9100 TCP (metrics)                                        │   │
│  │  │        │         │                                           │   │
│  │  │        ▼         │                                           │   │
│  │  │  ┌────────────┐  │      ┌──────────────────┐                 │   │
│  │  │  │ Services   │  │      │  Network         │                 │   │
│  │  │  │ ClusterIP  │  │      │  Policies        │                 │   │
│  │  │  │ NodePort   │  │      │  (firewall)      │                 │   │
│  │  │  └────────────┘  │      └──────────────────┘                 │   │
│  │  │        │         │                                           │   │
│  │  └────────┼─────────┘                                           │   │
│  │           │                                                      │   │
│  │           ▼                                                      │   │
│  │  ┌─────────────────────────────────────────┐                    │   │
│  │  │              PVCs (NFS)                  │                    │   │
│  │  │  ┌─────────────┐    ┌─────────────┐     │                    │   │
│  │  │  │ icn-data    │    │ icn-backups │     │                    │   │
│  │  │  │ 10Gi        │    │ 20Gi        │     │                    │   │
│  │  │  │ identity    │    │ daily .tar  │     │                    │   │
│  │  │  │ ledgers     │    │ files       │     │                    │   │
│  │  │  │ store       │    └─────────────┘     │                    │   │
│  │  │  └─────────────┘                        │                    │   │
│  │  └─────────────────────────────────────────┘                    │   │
│  │                         │                                        │   │
│  └─────────────────────────┼────────────────────────────────────────┘   │
│                            │ NFS                                        │
└────────────────────────────┼────────────────────────────────────────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │  Atlas (TrueNAS)│
                    │  /mnt/ssd_pool/ │
                    │  icn-vols       │
                    └─────────────────┘
```

### Data Flow

```
User Browser
     │
     ▼ :30030 (NodePort)
┌─────────────┐
│  Pilot UI   │ React frontend
└──────┬──────┘
       │ HTTP API calls
       ▼ :30080 or :8080
┌─────────────┐
│ ICN Gateway │ REST API (Actix-web)
└──────┬──────┘
       │ Internal calls
       ▼
┌─────────────┐
│ ICN Core    │ Trust graph, ledger, gossip
└──────┬──────┘
       │ P2P
       ▼ :7777 UDP (QUIC)
┌─────────────┐
│ Other Nodes │ Federation (future)
└─────────────┘
```

### Health Probes

| Probe | Endpoint | Interval | Purpose |
|-------|----------|----------|---------|
| Startup | `/v1/health` | 5s (max 155s) | Allows slow starts during key generation |
| Readiness | `/v1/health` | 10s | Traffic routing control |
| Liveness | `/v1/health` | 30s | Restart if stuck (3 failures) |

## Quick Start

### Prerequisites

1. **Docker** installed on your development machine
2. **SSH access** to K3s cluster control node (default: `ubuntu@10.8.10.40`)
3. **Kubectl** access to the cluster (via SSH or local config)

### Full Deployment (Recommended)

Deploy everything with one command:

```bash
make full-deploy
# Or with options:
make full-deploy IMAGE_TAG=$(git rev-parse --short HEAD)
```

This will:
1. Build the Docker image from source
2. Sync the image to all K3s nodes
3. Apply Kubernetes manifests to deploy ICN
4. Verify deployment health

### Make Targets

```bash
# Core deployment
make build              # Build Docker image
make sync               # Sync image to K3s cluster
make deploy             # Deploy ICN to K3s cluster
make full-deploy        # Full pipeline: build, sync, deploy

# Status & Logs
make status             # Check deployment status
make logs               # Tail ICN daemon logs
make logs-recent        # Show recent logs

# Deployment management
make restart            # Restart ICN deployment
make rollback           # Rollback to previous deployment
make rollback-history   # Show deployment rollout history
make verify             # Verify deployment is healthy

# Backup
make backup             # Backup ICN data from cluster
make safe-deploy        # Deploy with backup and verification
make deploy-history     # Show deployment audit log

# Pilot UI
make build-ui-image     # Build Pilot UI Docker image
make sync-ui-image      # Sync Pilot UI to all nodes
make deploy-ui          # Deploy Pilot UI
make ui-status          # Check Pilot UI status
make ui-logs            # Tail Pilot UI logs

# Fresh builds (no cache)
make build-fresh        # Build without Docker cache
make full-deploy-fresh  # Full deploy with fresh build
```

### Quick Commands

```bash
# See what's running
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get all"

# Check ICN health
curl http://10.8.10.40:30080/v1/health

# View ICN logs
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -l app=icn,component=daemon -f"

# Trigger backup manually
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn create job --from=cronjob/icn-backup backup-now"

# Access Grafana
open http://10.8.10.40:30300  # ICN dashboard under dashboards
```

## File Structure

```
deploy/k8s/
├── README.md                  # This file
├── kustomization.yaml         # Kustomize configuration
├── Makefile                   # Deployment automation
│
├── # Core Manifests
├── namespace.yaml             # ICN namespace
├── configmap.yaml             # ICN daemon configuration (icn.toml)
├── pvc.yaml                   # 10Gi storage for identity, ledgers, store
├── backup-pvc.yaml            # 20Gi storage for backups
├── deployment.yaml            # ICN daemon pod spec, probes, security
├── services.yaml              # ClusterIP + NodePort for external access
├── pdb.yaml                   # Pod Disruption Budget
├── network-policies.yaml      # Firewall rules for pod traffic
│
├── # Pilot UI
├── pilot-ui-deployment.yaml   # Pilot UI deployment
│
├── # Operations
├── backup-cronjob.yaml        # Daily backup automation (2am)
│
├── # Monitoring
├── prometheusrule.yaml        # Alert definitions (9 rules)
├── grafana-dashboard.yaml     # Metrics visualization
├── monitoring/
│   └── servicemonitor.yaml    # Prometheus ServiceMonitor
│
└── scripts/
    ├── build-image.sh         # Build Docker image
    ├── sync-image.sh          # Sync image to K3s nodes
    ├── deploy.sh              # Apply Kubernetes manifests
    └── full-deploy.sh         # Complete deployment pipeline
```

## Configuration

### Secrets

**IMPORTANT**: Secrets are not included in the repo. Create your own `secret.yaml`:

```bash
cp secret.yaml.example secret.yaml
# Edit secret.yaml with your passphrase and JWT secret
kubectl apply -f secret.yaml
```

Required secrets:
- `icn-secrets.passphrase` - ICN keystore passphrase
- `icn-secrets.jwt-secret` - Gateway JWT signing secret

### ConfigMap

Edit `configmap.yaml` to customize ICN configuration:

- Network settings (listen address, ports)
- Rate limiting
- Topology settings
- Log levels

After editing:
```bash
kubectl apply -f configmap.yaml
kubectl rollout restart deployment/icn-daemon -n icn
```

### Storage

| PVC | Size | Purpose |
|-----|------|---------|
| `icn-data` | 10Gi | Identity, ledgers, store |
| `icn-backups` | 20Gi | Daily backup archives |

Both use the `nfs-client` storage class backed by Atlas (TrueNAS).

## Monitoring

### Prometheus Alerts

The `prometheusrule.yaml` defines these alerts:

| Alert | Severity | Description |
|-------|----------|-------------|
| ICNDaemonDown | critical | Daemon unavailable > 2min |
| ICNDaemonNotReady | warning | Pod not ready > 5min |
| ICNHighMemory | warning | Memory > 85% limit |
| ICNHighCPU | warning | CPU > 80% for 10min |
| ICNFrequentRestarts | warning | > 3 restarts/hour |
| ICNCrashLooping | critical | CrashLoopBackOff state |
| ICNStorageAlmostFull | warning | Storage > 80% |
| ICNStorageFull | critical | Storage > 95% |
| ICNBackupFailed | warning | Backup job failed |

### Grafana Dashboard

The `grafana-dashboard.yaml` provides visualization for:
- Daemon status and restarts
- Memory and CPU usage over time
- Storage usage gauges
- Network I/O

Access at: `http://10.8.10.40:30300`

### View Metrics Directly

```bash
# Port forward to access metrics locally
kubectl -n icn port-forward svc/icn 9100:9100
# Then visit http://localhost:9100/metrics
```

## Backup System

### Automated Backups

The `backup-cronjob.yaml` runs daily at 2am:
1. Creates compressed tarball of `/data`
2. Verifies archive integrity
3. Deletes backups older than 7 days

### Manual Backup

```bash
# Using Makefile
make backup

# Or trigger cronjob manually
kubectl -n icn create job --from=cronjob/icn-backup backup-$(date +%s)
```

### Restore

```bash
# Get backup file
kubectl -n icn exec deployment/icn-daemon -- ls /backups

# Restore (stop daemon first!)
kubectl -n icn scale deployment/icn-daemon --replicas=0
kubectl -n icn exec <backup-pod> -- tar -xzf /backups/icn-backup-YYYYMMDD.tar.gz -C /data
kubectl -n icn scale deployment/icn-daemon --replicas=1
```

## Network Policies

The `network-policies.yaml` implements:
- Default deny all ingress
- Allow `:7777 UDP` from anywhere (P2P)
- Allow `:8080` from pilot-ui and cluster nodes (Gateway)
- Allow `:9100` from monitoring namespace only (Metrics)

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
- Verify image was synced: Run `make sync` again
- Check image name matches deployment

**Health check failing:**
- Check logs for startup errors
- Verify config is valid: `kubectl -n icn get configmap icn-config -o yaml`

### Manual Image Sync

If the sync script fails:

```bash
# Export image
docker save icn:latest -o /tmp/icn.tar

# Copy to node
scp /tmp/icn.tar ubuntu@10.8.10.40:/tmp/

# Import on node
ssh ubuntu@10.8.10.40 "sudo ctr -n k8s.io images import /tmp/icn.tar"
```

## Access Points

| Service | URL | Description |
|---------|-----|-------------|
| Gateway API | http://10.8.10.40:30080 | REST API |
| Pilot UI | http://10.8.10.40:30030 | Web interface |
| Metrics | http://10.8.10.40:30091/metrics | Prometheus metrics |
| Grafana | http://10.8.10.40:30300 | Dashboards |

## CI/CD Integration

The GitHub Actions workflow `.github/workflows/k3s-deploy.yml` automatically:
1. Builds the Docker image on push to main
2. Syncs to K3s cluster
3. Deploys and verifies health

For manual CI:
```bash
export IMAGE_TAG="$(git rev-parse --short HEAD)"
make full-deploy IMAGE_TAG="$IMAGE_TAG"
```
