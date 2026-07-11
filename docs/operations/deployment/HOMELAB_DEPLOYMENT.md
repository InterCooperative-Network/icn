# ICN Homelab Deployment

**Status**: ICN daemon running on K3s cluster. Rebuilt 2026-02-21 with registry-first architecture on Hyperion.

## Cluster Overview

| Component | Details |
|-----------|---------|
| **K3s Version** | v1.34.4+k3s1 |
| **K3s Control** | `k3s-control` (${ICN_K3S_CONTROL}) — 4 vCPU, 8GB RAM on Hyperion, tainted `NoSchedule` |
| **Worker 1** | `k3s-worker-1` (${ICN_K3S_WORKER1}) — 4 vCPU, 16GB RAM on node-2 |
| **Worker 2** | `k3s-worker-2` (${ICN_K3S_WORKER2}) — 4 vCPU, 16GB RAM on node-3 |
| **CI Runner** | `ci-runner` (${ICN_CI_RUNNER_HOST}) — 8 vCPU, 6GB RAM on Hyperion (standalone, not in K3s) |
| **Storage** | NFS from Atlas (${ICN_NFS_HOST}) via `atlas-nfs` StorageClass (csi-driver-nfs v4.9.0) |
| **Registry** | In-cluster at `${ICN_K3S_CONTROL}:30500` (NFS-backed PVC) |
| **Ports** | 7777/UDP (QUIC), 5601/TCP (RPC), 9100/TCP (Prometheus), 8080/TCP (Gateway) |

### Image Strategy

All images use `imagePullPolicy: Always` and pull from the in-cluster registry at `${ICN_K3S_CONTROL}:30500`. SCP-based image sync is deprecated.

```bash
# Default deploy path (build + push to registry + rolling restart)
cd deploy/k8s && make fast-deploy
```

## Quick Commands

```bash
# Deploy new version (registry-first, default path)
cd deploy/k8s && make fast-deploy

# Deploy both daemon and UI
cd deploy/k8s && make fast-deploy-all

# Check status
make status
# OR: ssh ubuntu@${ICN_K3S_CONTROL} "sudo kubectl -n icn get pods"

# View logs
make logs
# OR: ssh ubuntu@${ICN_K3S_CONTROL} "sudo kubectl -n icn logs -f deployment/icn-daemon"
```

## Deployment Paths

| Target | What it does |
|--------|-------------|
| `make fast-deploy` | Build + push to registry + rolling restart (default) |
| `make fast-deploy-ui` | Build + push UI + restart |
| `make fast-deploy-all` | Both daemon and UI |
| `make full-deploy` | Legacy: build + SCP sync + deploy (deprecated) |

## Monitoring Stack (rebuilt 2026-02-21)

Deployed via `kube-prometheus-stack` Helm chart in `monitoring` namespace. The
ICN-specific overlay (ServiceMonitor, alert rules, and durable storage values)
lives in [`deploy/k8s/monitoring/`](../../../deploy/k8s/monitoring/README.md).

| Component | Access | Notes |
|-----------|--------|-------|
| **Grafana** | http://${ICN_K3S_CONTROL}:30300 | Default admin/prom-operator |
| **Prometheus** | http://${ICN_K3S_CONTROL}:30090 | Scrapes ICN metrics |
| **AlertManager** | K3s internal only | Via kube-prometheus-stack |

Storage: Prometheus and Alertmanager use PVCs on the `atlas-nfs` StorageClass
via `deploy/k8s/monitoring/values-kube-prometheus-stack.yaml`. Apply with
`helm upgrade --install kube-prometheus-stack … -f <that file>`.

## CI/CD Pipeline (rebuilt 2026-02-21)

**Self-Hosted GitHub Actions Runner** on dedicated VM (not inside K3s).

| Component | Details |
|-----------|---------|
| **Runner Name** | `ci-runner` |
| **Runner Host** | `ci-runner` VM (${ICN_CI_RUNNER_HOST}) on Hyperion |
| **Specs** | 8 vCPU, 6GB RAM, 80GB disk |
| **Labels** | `self-hosted, linux, x64, homelab, k3s` |
| **Rust** | 1.88.0 with sccache |
| **sccache** | Shared Atlas NFS at `/mnt/icn-sccache` (see [`deploy/ci-runner/`](../../../deploy/ci-runner/README.md), issue #1597) |
| **Docker** | Configured with insecure registry (${ICN_K3S_CONTROL}:30500) |
| **kubectl** | Has kubeconfig for cluster access |

**Runner Management**:
```bash
ssh ubuntu@${ICN_CI_RUNNER_HOST} "sudo systemctl status actions.runner.InterCooperative-Network-icn.ci-runner"
ssh ubuntu@${ICN_CI_RUNNER_HOST} "journalctl -u actions.runner.InterCooperative-Network-icn.ci-runner -f"
```

## Pilot Testing Status (2025-12-05)

**5-Node Pilot Network** on K3s with P2P mesh topology.

| Feature | Status | Notes |
|---------|--------|-------|
| **Identity** | ✅ Working | All 5 nodes have unique DIDs |
| **Trust Graph** | ⏳ Blocked | Needs image rebuild (#46) |
| **Governance** | ✅ Working | Domain, proposal, voting via Gateway |
| **Ledger** | ✅ Working | Mutual credit transactions via Gateway |
| **Compute** | ✅ Fixed | Gateway connected to ComputeHandle |
| **Contracts** | ✅ Fixed | icnctl runtime bug resolved |

**Test Commands** (via Gateway API):
```bash
TOKEN=$(icnctl auth token --gateway http://${ICN_K3S_CONTROL}:30080 --coop pilot-coop)
curl -H "Authorization: Bearer $TOKEN" http://${ICN_K3S_CONTROL}:30080/v1/gov/domains
```

## SDIS & Steward Dashboard (deployed 2025-12-13)

**Sovereign Digital Identity System** with steward verification network.

| Component | Access |
|-----------|--------|
| **Pilot UI** | http://${ICN_K3S_CONTROL}:30030 |
| **Steward Dashboard** | http://${ICN_K3S_CONTROL}:30030/steward-dashboard.html |
| **SDIS Enrollment** | http://${ICN_K3S_CONTROL}:30030/sdis-enrollment.html |
| **Gateway API** | http://${ICN_K3S_CONTROL}:30080/v1/sdis/* |

### SDIS API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/sdis/health` | GET | Service health check |
| `/v1/sdis/enrollment/start` | POST | Start new enrollment |
| `/v1/sdis/verify/level1` | POST | Device proof verification |
| `/v1/sdis/verify/level2` | POST | Steward vouch verification |
| `/v1/sdis/enrollment/complete` | POST | Complete enrollment |
| `/v1/sdis/pending` | GET | List pending enrollments (steward) |
| `/v1/sdis/status/{id}` | GET | Get enrollment status |
| `/v1/sdis/vouch/{id}` | POST | Submit steward vouch |
| `/v1/sdis/reject/{id}` | POST | Reject enrollment |
| `/v1/sdis/steward/stats` | GET | Steward statistics |
| `/v1/sdis/steward/history` | GET | Vouch history |

### Quick Test Commands

```bash
# Check SDIS health
curl http://${ICN_K3S_CONTROL}:30080/v1/sdis/health

# List pending enrollments
curl http://${ICN_K3S_CONTROL}:30080/v1/sdis/pending

# Get steward stats
curl http://${ICN_K3S_CONTROL}:30080/v1/sdis/steward/stats

# Start an enrollment
curl -X POST http://${ICN_K3S_CONTROL}:30080/v1/sdis/enrollment/start \
  -H "Content-Type: application/json" \
  -d '{"identity_name":"Test User","coop_id":"pilot-coop"}'
```

### Verification Levels

| Level | Description | Required |
|-------|-------------|----------|
| **0** | Enrollment started | Identity name, coop ID |
| **1** | Device verified | Device signature proof |
| **2** | Steward vouched | Steward approval + statement |

---

## Deployment History

### Infrastructure Rebuild (2026-02-21)
Full tear-down and rebuild:
- Moved control plane from node-1 (undersized 2 vCPU/8GB) to Hyperion (4 vCPU/8GB)
- Workers: 16GB RAM each (up from 8GB)
- Dedicated CI runner VM on Hyperion (8 vCPU, not inside K3s)
- Registry-first image strategy (`imagePullPolicy: Always`)
- NFS CSI driver v4.9.0 (replaces broken csi-nfs-controller)
- Prometheus + Grafana via kube-prometheus-stack Helm chart
- 4 coop instances (alpha, beta, gamma, delta)

### Initial Deployment (2025-12-03)
Manual deployment with fixes for:
1. GLIBC compatibility - Ubuntu 24.04 base image
2. STUN port conflict - Disabled STUN
3. Governance topic - Created before GovernanceActor spawn
4. Memory limit - Increased to 2Gi
5. Health probe - Port 9100 (metrics)

### Automated System (2025-12-04)
- Kubernetes manifests in `deploy/k8s/`
- Build scripts with `.dockerignore` optimization
- Image sync automation
- Comprehensive documentation

## Related Documentation

| Resource | Location |
|----------|----------|
| **Dev Environment** | [DEV_ENVIRONMENT.md](../../guides/developer/DEV_ENVIRONMENT.md) - icn-dev VM details |
| **Homelab Inventory** | `/home/matt/homelab-inventory` |
| **ICN Launchpad** | `/home/matt/homelab-inventory/projects/icn/ICN_LAUNCHPAD.md` |
| **K3s Cluster Docs** | `/home/matt/homelab-inventory/projects/icn/docs/K3S_CLUSTER.md` |
| **Deployment Plans** | `/home/matt/homelab-inventory/projects/icn/docs/DEPLOYMENT_PLANS.md` |
