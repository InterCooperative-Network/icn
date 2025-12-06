# ICN Homelab Deployment

**Status**: ICN daemon running on K3s cluster (deployed 2025-12-03, automated 2025-12-04).

## Cluster Overview

| Component | Details |
|-----------|---------|
| **Node Identity** | `did:icn:z3TE1ei6B4L5j6Jp29RmJKt1FYonGaQAXQoYHJL3GULR3` |
| **K3s Control** | `k3s-control` (10.8.10.40) |
| **Workers** | `k3s-worker-1` (10.8.10.41), `k3s-worker-2` (10.8.10.42) |
| **Storage** | NFS from Atlas (10.8.10.25) via `atlas-nfs` StorageClass |
| **Ports** | 7777/UDP (QUIC), 5601/TCP (RPC), 9100/TCP (Prometheus) |

## Quick Commands

```bash
# Deploy new version
cd /home/matt/projects/icn/deploy/k8s && make full-deploy-dev

# Check status
make status
# OR: ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods"

# View logs
make logs
# OR: ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -f deployment/icn-daemon"

# Show identity
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn exec deploy/icn-daemon -- /usr/local/bin/icnctl id show"
```

## Automated Deployment

Full deployment automation available via `deploy/k8s/`:

```bash
cd deploy/k8s
make full-deploy-dev  # Build, sync, and deploy in one command
```

See [deploy/k8s/README.md](../deploy/k8s/README.md) for complete documentation.

**Features**:
- Version-controlled Kubernetes manifests
- Automated Docker image building
- Image sync to all cluster nodes
- One-command deployment
- Git hash tagging for tracking

## Monitoring Stack (deployed 2025-12-04)

| Component | Access | Notes |
|-----------|--------|-------|
| **Grafana** | http://10.8.10.40:30300 | ICN Node Dashboard |
| **Prometheus** | K3s internal only | Scrapes ICN metrics every 15s |
| **AlertManager** | K3s internal only | 15 ICN-specific alerts configured |

## CI/CD Pipeline (deployed 2025-12-05)

**Self-Hosted GitHub Actions Runner** on K3s control plane.

| Component | Details |
|-----------|---------|
| **Runner Name** | `homelab-runner` |
| **Runner Host** | `k3s-control` (10.8.10.40) |
| **Labels** | `self-hosted, linux, x64, homelab, k3s` |
| **Workflow** | `.github/workflows/docker-build-deploy.yml` |

**How It Works**:
1. Push to `main` triggers workflow
2. Tests run on GitHub-hosted runners
3. Build & deploy runs on self-hosted runner

**Manual Trigger**:
```bash
gh workflow run docker-build-deploy.yml
```

**Runner Management**:
```bash
ssh ubuntu@10.8.10.40 "cd ~/actions-runner && sudo ./svc.sh status"
ssh ubuntu@10.8.10.40 "journalctl -u actions.runner.* -f"
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
TOKEN=$(icnctl auth token --gateway http://10.8.10.40:30080 --coop pilot-coop)
curl -H "Authorization: Bearer $TOKEN" http://10.8.10.40:30080/v1/gov/domains
```

## Deployment History

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
| **Homelab Inventory** | `/home/matt/homelab-inventory` |
| **ICN Launchpad** | `/home/matt/homelab-inventory/projects/icn/ICN_LAUNCHPAD.md` |
| **K3s Cluster Docs** | `/home/matt/homelab-inventory/projects/icn/docs/K3S_CLUSTER.md` |
| **Deployment Plans** | `/home/matt/homelab-inventory/projects/icn/docs/DEPLOYMENT_PLANS.md` |
