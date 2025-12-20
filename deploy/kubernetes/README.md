# ICN Kubernetes Deployment

Production-ready Kubernetes manifests for deploying ICN (Intercooperative Network).

## Quick Start

### Prerequisites

- Kubernetes cluster (1.20+)
- kubectl configured
- 15GB available storage
- LoadBalancer support (or NodePort)

### One-Command Deployment

```bash
cd deploy/kubernetes
./deploy.sh
```

With monitoring:

```bash
./deploy.sh --monitoring
```

With ingress:

```bash
./deploy.sh --monitoring --ingress
```

## Architecture

```
┌─────────────────────────────────────────┐
│          Kubernetes Cluster             │
├─────────────────────────────────────────┤
│  Namespace: icn                         │
│                                         │
│  ┌──────────────┐  ┌─────────────────┐ │
│  │  ICN Node    │  │  Monitoring     │ │
│  │  (Deployment)│  │  (Prometheus    │ │
│  │              │  │   + Grafana)    │ │
│  │  - icnd      │  │                 │ │
│  │  - Gateway   │  └─────────────────┘ │
│  │  - Storage   │                      │
│  └──────────────┘                      │
│                                         │
│  Services:                              │
│  - icn-gateway (HTTP/WS)               │
│  - icn-p2p (UDP)                       │
│  - icn-metrics (Prometheus)            │
└─────────────────────────────────────────┘
```

## Components

### Core Components

| Component | Type | Purpose |
|-----------|------|---------|
| `icn-node` | Deployment | Main ICN daemon |
| `icn-gateway` | Service | HTTP/WebSocket API |
| `icn-p2p` | Service | QUIC/UDP P2P network |
| `icn-data-pvc` | PVC | Persistent storage |

### Monitoring Stack (Optional)

| Component | Type | Purpose |
|-----------|------|---------|
| `prometheus` | Deployment | Metrics collection |
| `grafana` | Deployment | Visualization |
| `prometheus-data-pvc` | PVC | Prometheus storage |
| `grafana-data-pvc` | PVC | Grafana storage |

## Configuration

### ConfigMap (`configmap.yaml`)

```yaml
LOG_LEVEL: "info"
BIND_ADDRESS: "0.0.0.0:3000"
GATEWAY_PORT: "8000"
DATA_DIR: "/data/icn"
ENABLE_METRICS: "true"
```

### Secrets (`secret.yaml`)

⚠️ **Change these in production!**

```yaml
JWT_SECRET: "your-random-256-bit-key"
DB_PASSWORD: "if-using-external-db"
```

Generate secure JWT secret:

```bash
openssl rand -base64 32
```

### Resource Requests

| Resource | Request | Limit |
|----------|---------|-------|
| Memory | 512Mi | 2Gi |
| CPU | 500m | 2000m |
| Storage | 10Gi | - |

## Deployment Steps

### Manual Deployment

```bash
cd deploy/kubernetes

# 1. Create namespace
kubectl apply -f namespace.yaml

# 2. Create configuration
kubectl apply -f configmap.yaml
kubectl apply -f secret.yaml

# 3. Create storage
kubectl apply -f pvc.yaml

# 4. Deploy ICN node
kubectl apply -f deployment.yaml

# 5. Create services
kubectl apply -f service.yaml

# 6. (Optional) Deploy monitoring
kubectl apply -f prometheus.yaml
kubectl apply -f grafana.yaml

# 7. (Optional) Create ingress
kubectl apply -f ingress.yaml
```

### Verify Deployment

```bash
# Check pods
kubectl get pods -n icn

# Check services
kubectl get svc -n icn

# View logs
kubectl logs -n icn -l app=icn-node -f

# Check health
kubectl port-forward -n icn svc/icn-gateway 8000:8000
curl http://localhost:8000/health
```

## Accessing Services

### Gateway API

**Option 1: LoadBalancer**

```bash
GATEWAY_IP=$(kubectl get svc icn-gateway -n icn -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
curl http://$GATEWAY_IP:8000/health
```

**Option 2: Port Forward**

```bash
kubectl port-forward -n icn svc/icn-gateway 8000:8000
curl http://localhost:8000/health
```

**Option 3: Ingress**

```bash
# Update ingress.yaml with your domain
# Then apply
kubectl apply -f ingress.yaml

# Access via domain
curl https://api.your-coop.org/health
```

### Monitoring

**Prometheus:**

```bash
kubectl port-forward -n icn svc/prometheus 9090:9090
# Visit http://localhost:9090
```

**Grafana:**

```bash
kubectl port-forward -n icn svc/grafana 3000:3000
# Visit http://localhost:3000
# Default: admin / (check secret)
```

### P2P Network

```bash
# Get external IP
P2P_IP=$(kubectl get svc icn-p2p -n icn -o jsonpath='{.status.loadBalancer.ingress[0].ip}')

# Other nodes should connect to: $P2P_IP:3000 (UDP)
```

## Scaling

### Horizontal Scaling

Currently single-node deployment. For multi-node:

1. Update deployment replicas
2. Configure peer discovery
3. Use StatefulSet for stable network identities
4. Add headless service for peer discovery

```bash
kubectl scale deployment icn-node -n icn --replicas=3
```

### Vertical Scaling

Update resource limits in `deployment.yaml`:

```yaml
resources:
  requests:
    memory: "1Gi"
    cpu: "1000m"
  limits:
    memory: "4Gi"
    cpu: "4000m"
```

## Storage

### Persistent Volume Claims

| PVC | Size | Purpose |
|-----|------|---------|
| `icn-data-pvc` | 10Gi | Node data (ledger, keys, state) |
| `prometheus-data-pvc` | 20Gi | Metrics data |
| `grafana-data-pvc` | 5Gi | Dashboards and config |

### Backup

```bash
# Backup ICN data
kubectl exec -n icn <pod-name> -- tar czf - /data/icn > icn-backup.tar.gz

# Restore
kubectl cp icn-backup.tar.gz icn/<pod-name>:/tmp/
kubectl exec -n icn <pod-name> -- tar xzf /tmp/icn-backup.tar.gz -C /
```

## Security

### TLS/SSL Certificates

**Option 1: cert-manager (Recommended)**

```bash
# Install cert-manager
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml

# Create ClusterIssuer
kubectl apply -f - <<EOF
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: your-email@example.com
    privateKeySecretRef:
      name: letsencrypt-prod
    solvers:
    - http01:
        ingress:
          class: nginx
EOF

# Ingress will auto-provision certificates
```

**Option 2: Manual Certificates**

```bash
kubectl create secret tls icn-tls -n icn \
  --cert=path/to/cert.pem \
  --key=path/to/key.pem
```

### Network Policies

```yaml
# Restrict ingress to gateway only
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: icn-gateway-policy
  namespace: icn
spec:
  podSelector:
    matchLabels:
      app: icn-node
  policyTypes:
  - Ingress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    ports:
    - protocol: TCP
      port: 8000
```

### Secrets Management

**Option 1: Sealed Secrets**

```bash
# Install sealed-secrets controller
kubectl apply -f https://github.com/bitnami-labs/sealed-secrets/releases/download/v0.24.0/controller.yaml

# Seal your secrets
kubeseal < secret.yaml > sealed-secret.yaml
kubectl apply -f sealed-secret.yaml
```

**Option 2: External Secrets Operator**

Use AWS Secrets Manager, HashiCorp Vault, etc.

## Monitoring & Observability

### Prometheus Metrics

ICN exposes metrics at `/metrics`:

- `icn_network_peers` - Connected peers
- `icn_ledger_entries` - Total ledger entries
- `icn_gossip_messages` - Gossip protocol stats
- `icn_compute_tasks` - Compute task queue
- And more...

### Grafana Dashboards

Import pre-built dashboards (coming soon):

1. ICN Overview
2. Network Performance
3. Ledger Activity
4. Compute Utilization

### Logging

**Option 1: kubectl logs**

```bash
kubectl logs -n icn -l app=icn-node -f
```

**Option 2: Loki Stack**

```bash
# Install Loki + Promtail
helm repo add grafana https://grafana.github.io/helm-charts
helm install loki grafana/loki-stack -n icn
```

## Troubleshooting

### Pod Not Starting

```bash
# Check events
kubectl describe pod -n icn <pod-name>

# Check logs
kubectl logs -n icn <pod-name>

# Common issues:
# - Insufficient resources
# - PVC not bound
# - Image pull errors
# - Secret not found
```

### Service Not Accessible

```bash
# Check service
kubectl get svc -n icn icn-gateway

# Check endpoints
kubectl get endpoints -n icn icn-gateway

# Test from within cluster
kubectl run -it --rm debug --image=busybox -n icn -- wget -O- http://icn-gateway:8000/health
```

### Storage Issues

```bash
# Check PVC status
kubectl get pvc -n icn

# Check PV
kubectl get pv

# Resize PVC (if storage class supports it)
kubectl patch pvc icn-data-pvc -n icn -p '{"spec":{"resources":{"requests":{"storage":"20Gi"}}}}'
```

## Updating

### Rolling Update

```bash
# Update image
kubectl set image deployment/icn-node icnd=icn/icnd:v1.1.0 -n icn

# Check rollout status
kubectl rollout status deployment/icn-node -n icn

# Rollback if needed
kubectl rollout undo deployment/icn-node -n icn
```

### Configuration Changes

```bash
# Update ConfigMap
kubectl edit configmap icn-config -n icn

# Restart pods to pick up changes
kubectl rollout restart deployment/icn-node -n icn
```

## Production Checklist

- [ ] Update JWT_SECRET in secret.yaml
- [ ] Configure TLS certificates (cert-manager)
- [ ] Set up monitoring alerts
- [ ] Configure backup strategy
- [ ] Set resource limits appropriately
- [ ] Enable network policies
- [ ] Configure log aggregation
- [ ] Set up external secrets management
- [ ] Configure ingress with proper domain
- [ ] Test disaster recovery procedures
- [ ] Document runbook procedures
- [ ] Set up alerting (PagerDuty, Slack, etc.)

## Support

- Documentation: https://github.com/InterCooperative-Network/icn/tree/main/docs
- Issues: https://github.com/InterCooperative-Network/icn/issues
- Discussions: https://github.com/InterCooperative-Network/icn/discussions

## License

MIT - See [LICENSE](../../LICENSE)
