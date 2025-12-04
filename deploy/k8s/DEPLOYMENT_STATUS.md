# ICN K3s Deployment Status

## Current Deployment

**Last Deployed**: 2025-12-04 20:20 UTC  
**Image Tag**: `2122145` (git commit hash)  
**Cluster**: K3s on Hyperion (10.8.10.40)  
**Namespace**: `icn`

## Deployment Summary

The ICN daemon is fully deployed and operational on the K3s cluster. Deployment is fully automated with version-controlled manifests and one-command deployment.

### Architecture

```
Development Machine → Build Image → Sync to Cluster → Deploy to K3s
```

### Quick Deployment

```bash
cd deploy/k8s
make full-deploy-dev
```

## Current Status

| Component | Status | Details |
|-----------|--------|---------|
| Pod | ✅ Running | `icn-daemon-9b9b66445-btjtq` on k3s-control |
| Image | ✅ Deployed | `icn:latest` (146MB, sha256:b5a101b56e522) |
| Services | ✅ Active | ClusterIP (10.43.127.217) + NodePort (30777/30601) |
| Storage | ✅ Bound | 10Gi PVC on atlas-nfs storage class |
| ConfigMap | ✅ Applied | ICN configuration loaded |
| Secrets | ✅ Configured | Passphrase from secret.yaml |
| Monitoring | ✅ Active | ServiceMonitor + PrometheusRules |

## Deployment Components

### Kubernetes Resources

- **Namespace**: `icn`
- **Deployment**: `icn-daemon` (1 replica, Recreate strategy)
- **Services**: 
  - `icn` (ClusterIP): Internal cluster access
  - `icn-nodeport` (NodePort): External access via ports 30777 (UDP) and 30601 (TCP)
- **PVC**: `icn-data` (10Gi, ReadWriteOnce, atlas-nfs)
- **ConfigMap**: `icn-config` (icn.toml configuration)
- **Secrets**: `icn-secrets` (passphrase for keystore)

### Ports

| Port | Protocol | Purpose | Service Type |
|------|----------|---------|--------------|
| 7777 | UDP | QUIC/P2P | ClusterIP + NodePort (30777) |
| 5601 | TCP | RPC/gRPC | ClusterIP + NodePort (30601) |
| 9100 | TCP | Prometheus metrics | ClusterIP only |
| 8080 | TCP | Health/Gateway API | ClusterIP only |

### Resource Limits

- **CPU**: 100m request, 1 core limit
- **Memory**: 512Mi request, 2Gi limit
- **Storage**: 10Gi persistent volume

## Deployment History

| Date | Image Tag | Status | Notes |
|------|-----------|--------|-------|
| 2025-12-04 | 2122145 | ✅ Success | Initial automated deployment setup |

## Access Points

### Metrics

```bash
# Port forward to access metrics
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn port-forward svc/icn 9100:9100"
# Then visit: http://localhost:9100/metrics
```

### Logs

```bash
# Tail logs
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -f deployment/icn-daemon"

# Or use Makefile
cd deploy/k8s
make logs
```

### Status

```bash
# Check pod status
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods -o wide"

# Or use Makefile
cd deploy/k8s
make status
```

## Monitoring

Prometheus is configured to scrape ICN metrics via ServiceMonitor:
- **Job**: `icn-daemon`
- **Namespace**: `icn`
- **Endpoint**: `:9100/metrics`
- **Interval**: 15s

PrometheusRules include alerts for:
- Byzantine node detection
- Network health
- Ledger consistency
- Compute layer issues
- System resources

## Configuration

ICN configuration is managed via ConfigMap (`icn-config`). Current settings:

- **Network**: Listen on 0.0.0.0:7777, mDNS enabled
- **Region**: faherty-homelab
- **Cluster ID**: k3s-icn
- **Role**: edge
- **Rate Limiting**: Enabled with tiered limits
- **Topology**: Configured neighbor limits and fanout

To update configuration:
1. Edit `deploy/k8s/configmap.yaml`
2. Apply: `kubectl apply -f deploy/k8s/configmap.yaml`
3. Restart: `kubectl rollout restart deployment/icn-daemon -n icn`

## Secrets

Secrets are stored in Kubernetes Secret (`icn-secrets`). 

**⚠️ Important**: The secret file (`secret.yaml`) is NOT committed to git. It must be created from `secret.yaml.example` and managed separately.

## Deployment Automation

The deployment system provides:

- **Build**: Automated Docker image building from source
- **Sync**: Automatic image distribution to all cluster nodes
- **Deploy**: One-command Kubernetes manifest application
- **Versioning**: Git hash tagging for tracking deployments

See [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md) for complete documentation.

## Known Issues

1. **Worker Node Image Sync**: DNS resolution for worker nodes fails during sync (low impact - control node works)
2. **First Build**: Initial build takes ~90 seconds (subsequent builds are cached)

## Next Steps

- [ ] Fix worker node image sync (DNS resolution)
- [ ] Set up local container registry for faster sync
- [ ] Configure automated backups of PVC data
- [ ] Monitor resource usage and adjust limits
- [ ] Set up CI/CD integration

## Related Documentation

- [README.md](README.md) - Complete deployment reference
- [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md) - Detailed deployment guide
- [QUICKSTART.md](QUICKSTART.md) - Quick start guide
- [WORKFLOW.md](WORKFLOW.md) - Development workflow

---

**Last Updated**: 2025-12-04  
**Maintainer**: Infrastructure Team  
**Status**: ✅ Operational

