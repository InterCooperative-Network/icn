⚠️ **SUPERSEDED** - This document has been superseded.

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

See current deployment guide: [HOMELAB_DEPLOYMENT.md](../../operations/deployment/HOMELAB_DEPLOYMENT.md)

---

# Kubernetes Deployment Guide

Complete guide for deploying ICN to Kubernetes clusters.

## Prerequisites

- Kubernetes 1.24+ cluster
- `kubectl` configured
- Helm 3.x (optional, for Helm charts)
- Storage class for persistent volumes
- Optional: Ingress controller (nginx, traefik)
- Optional: cert-manager for TLS

## Deployment Options

### 1. Quick Start (kubectl)
Basic deployment using raw manifests.

### 2. Helm Charts (Recommended)
Production deployment with configurable Helm charts.

### 3. Production Setup
High-availability deployment with monitoring.

---

## Quick Start with kubectl

### 1. Create Namespace

```bash
kubectl create namespace icn
```

### 2. Create Secrets

```bash
# Generate JWT secret
JWT_SECRET=$(openssl rand -hex 32)

# Create secret
kubectl create secret generic icn-secrets \
  --namespace=icn \
  --from-literal=jwt-secret=$JWT_SECRET
```

### 3. Apply Manifests

```bash
cd deploy/kubernetes

# Apply in order
kubectl apply -f namespace.yaml
kubectl apply -f configmap.yaml
kubectl apply -f secret.yaml
kubectl apply -f pvc.yaml
kubectl apply -f deployment.yaml
kubectl apply -f service.yaml
kubectl apply -f ingress.yaml
```

### 4. Verify Deployment

```bash
# Check pods
kubectl get pods -n icn

# Check services
kubectl get svc -n icn

# View logs
kubectl logs -n icn -l app=icnd -f
```

### 5. Initialize Identity

```bash
# Get pod name
POD=$(kubectl get pods -n icn -l app=icnd -o jsonpath='{.items[0].metadata.name}')

# Initialize identity
kubectl exec -n icn $POD -- icnctl id init

# Show DID
kubectl exec -n icn $POD -- icnctl id show
```

---

## Helm Deployment

### 1. Install from Local Charts

```bash
cd deploy/helm

helm install icn ./icn \
  --namespace icn \
  --create-namespace \
  --set jwtSecret=$(openssl rand -hex 32) \
  --set ingress.enabled=true \
  --set ingress.host=icn.example.com
```

### 2. Configuration Options

```bash
# Custom values file
cat > values.yaml <<EOF
replicaCount: 3

resources:
  limits:
    cpu: 2
    memory: 4Gi
  requests:
    cpu: 500m
    memory: 1Gi

persistence:
  enabled: true
  size: 20Gi
  storageClass: fast-ssd

ingress:
  enabled: true
  host: icn.example.com
  tls:
    enabled: true
    secretName: icn-tls

monitoring:
  enabled: true
  prometheus:
    enabled: true
  grafana:
    enabled: true

autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
EOF

# Install with custom values
helm install icn ./icn -f values.yaml --namespace icn
```

### 3. Upgrade Deployment

```bash
# Upgrade to new version
helm upgrade icn ./icn \
  --namespace icn \
  -f values.yaml

# Rollback if needed
helm rollback icn --namespace icn
```

### 4. Uninstall

```bash
helm uninstall icn --namespace icn
```

---

## Production Setup

### Architecture

```
┌─────────────────────────────────────────────────┐
│                  Ingress Controller              │
│              (TLS Termination)                   │
└──────────────────┬──────────────────────────────┘
                   │
       ┌───────────┴───────────┐
       │                       │
┌──────▼──────┐        ┌──────▼──────┐
│  Gateway     │        │   WebSocket  │
│  Service     │        │   Service    │
│  (ClusterIP) │        │  (ClusterIP) │
└──────┬──────┘        └──────┬──────┘
       │                       │
       └───────────┬───────────┘
                   │
         ┌─────────▼─────────┐
         │  ICNd StatefulSet  │
         │  (3+ replicas)     │
         └─────────┬─────────┘
                   │
         ┌─────────▼─────────┐
         │  Persistent Volume │
         │  (Per Pod)         │
         └───────────────────┘
```

### 1. High Availability Configuration

```yaml
# icn-ha-values.yaml
replicaCount: 3

# Pod anti-affinity for distribution
affinity:
  podAntiAffinity:
    requiredDuringSchedulingIgnoredDuringExecution:
    - labelSelector:
        matchExpressions:
        - key: app
          operator: In
          values:
          - icnd
      topologyKey: kubernetes.io/hostname

# Pod disruption budget
podDisruptionBudget:
  enabled: true
  minAvailable: 2

# Resource limits
resources:
  limits:
    cpu: "2"
    memory: "4Gi"
  requests:
    cpu: "500m"
    memory: "1Gi"

# Health checks
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 30
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 5
  timeoutSeconds: 3
  failureThreshold: 3
```

### 2. Storage Configuration

```yaml
# Persistent storage
persistence:
  enabled: true
  storageClass: "fast-ssd"  # Use your cluster's fast storage class
  size: 50Gi
  accessMode: ReadWriteOnce

# Backup configuration
backup:
  enabled: true
  schedule: "0 2 * * *"  # Daily at 2 AM
  retention: 7  # Keep 7 days of backups
  storageClass: "standard"  # Use cheaper storage for backups
```

### 3. Monitoring Stack

```bash
# Install Prometheus Operator
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
helm install prometheus prometheus-community/kube-prometheus-stack \
  --namespace monitoring \
  --create-namespace

# Deploy ICN with monitoring
helm install icn ./icn \
  --namespace icn \
  --set monitoring.enabled=true \
  --set monitoring.serviceMonitor.enabled=true \
  -f icn-ha-values.yaml
```

### 4. Ingress with TLS

```yaml
# ingress.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: icn-ingress
  namespace: icn
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/websocket-services: icn-websocket
spec:
  ingressClassName: nginx
  tls:
  - hosts:
    - icn.example.com
    - api.icn.example.com
    secretName: icn-tls
  rules:
  - host: icn.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: icn-web
            port:
              number: 80
  - host: api.icn.example.com
    http:
      paths:
      - path: /ws
        pathType: Prefix
        backend:
          service:
            name: icn-websocket
            port:
              number: 8080
      - path: /
        pathType: Prefix
        backend:
          service:
            name: icn-gateway
            port:
              number: 8080
```

### 5. Network Policies

```yaml
# network-policy.yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: icn-network-policy
  namespace: icn
spec:
  podSelector:
    matchLabels:
      app: icnd
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: icn
    - podSelector:
        matchLabels:
          app: nginx-ingress
    ports:
    - protocol: TCP
      port: 8080
    - protocol: UDP
      port: 7777  # QUIC
  egress:
  - to:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 53  # DNS
    - protocol: UDP
      port: 53
  - to:
    - podSelector:
        matchLabels:
          app: icnd
    ports:
    - protocol: TCP
      port: 7777
    - protocol: UDP
      port: 7777
```

---

## Operations

### Scaling

```bash
# Manual scaling
kubectl scale deployment icnd --replicas=5 -n icn

# Horizontal Pod Autoscaler
kubectl autoscale deployment icnd \
  --namespace=icn \
  --min=3 \
  --max=10 \
  --cpu-percent=70
```

### Rolling Updates

```bash
# Update image
kubectl set image deployment/icnd \
  icnd=icn/icnd:v0.2.0 \
  --namespace=icn \
  --record

# Check rollout status
kubectl rollout status deployment/icnd -n icn

# Rollback if needed
kubectl rollout undo deployment/icnd -n icn
```

### Backup and Restore

```bash
# Manual backup
kubectl exec -n icn icnd-0 -- icnctl backup create -o /data/backup.tar.enc

# Copy backup out
kubectl cp icn/icnd-0:/data/backup.tar.enc ./backup-$(date +%Y%m%d).tar.enc

# Restore backup
kubectl cp backup.tar.enc icn/icnd-0:/data/backup.tar.enc
kubectl exec -n icn icnd-0 -- icnctl backup restore -i /data/backup.tar.enc
```

### Log Aggregation

```bash
# View logs
kubectl logs -n icn -l app=icnd -f

# With stern (multi-pod tailing)
stern icnd -n icn

# Export logs
kubectl logs -n icn -l app=icnd --since=24h > icn-logs.txt
```

### Debugging

```bash
# Get shell in pod
kubectl exec -n icn -it icnd-0 -- /bin/bash

# Port forward for local access
kubectl port-forward -n icn svc/icn-gateway 8080:8080

# Check events
kubectl get events -n icn --sort-by='.lastTimestamp'
```

---

## Monitoring and Alerts

### Grafana Dashboards

Access Grafana:
```bash
kubectl port-forward -n monitoring svc/prometheus-grafana 3000:80
```

Default credentials: `admin` / `prom-operator`

Import ICN dashboard from `deploy/grafana/dashboards/icn-overview.json`

### Prometheus Alerts

Example alert rules:

```yaml
# icn-alerts.yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: icn-alerts
  namespace: icn
spec:
  groups:
  - name: icn
    interval: 30s
    rules:
    - alert: ICNHighMemoryUsage
      expr: container_memory_usage_bytes{pod=~"icnd-.*"} / container_spec_memory_limit_bytes > 0.8
      for: 5m
      labels:
        severity: warning
      annotations:
        summary: "ICN pod {{ $labels.pod }} high memory usage"
        
    - alert: ICNPodDown
      expr: up{job="icnd"} == 0
      for: 2m
      labels:
        severity: critical
      annotations:
        summary: "ICN pod is down"
        
    - alert: ICNHighLatency
      expr: histogram_quantile(0.95, rate(icn_request_duration_seconds_bucket[5m])) > 1
      for: 5m
      labels:
        severity: warning
      annotations:
        summary: "ICN API latency is high"
```

---

## Security Hardening

### 1. Pod Security Standards

```yaml
# Apply restricted security context
apiVersion: v1
kind: Pod
metadata:
  name: icnd
spec:
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    fsGroup: 1000
    seccompProfile:
      type: RuntimeDefault
  containers:
  - name: icnd
    securityContext:
      allowPrivilegeEscalation: false
      capabilities:
        drop:
        - ALL
      readOnlyRootFilesystem: true
```

### 2. Secrets Management

```bash
# Use external secrets (recommended for production)
kubectl apply -f https://raw.githubusercontent.com/external-secrets/external-secrets/main/deploy/crds/bundle.yaml

# Configure with your secrets backend (Vault, AWS Secrets Manager, etc.)
```

### 3. RBAC Configuration

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: icnd
  namespace: icn
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: icnd-role
  namespace: icn
rules:
- apiGroups: [""]
  resources: ["configmaps", "secrets"]
  verbs: ["get", "list"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: icnd-rolebinding
  namespace: icn
subjects:
- kind: ServiceAccount
  name: icnd
  namespace: icn
roleRef:
  kind: Role
  name: icnd-role
  apiGroup: rbac.authorization.k8s.io
```

---

## Cost Optimization

### 1. Resource Requests/Limits

Start conservative and tune based on metrics:

```yaml
resources:
  requests:
    cpu: "250m"
    memory: "512Mi"
  limits:
    cpu: "1"
    memory: "2Gi"
```

### 2. Storage Optimization

Use tiered storage:
- Fast SSD for active data
- Standard HDD for backups
- Object storage for long-term archives

### 3. Autoscaling

Use HPA to scale based on load:

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: icnd-hpa
  namespace: icn
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: icnd
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

---

## Troubleshooting

### Pods Not Starting

```bash
# Describe pod
kubectl describe pod -n icn icnd-0

# Check events
kubectl get events -n icn | grep icnd

# Common issues:
# - Image pull errors: Check image name/tag
# - CrashLoopBackOff: Check logs and liveness probe
# - Pending: Check PVC binding and node resources
```

### Network Issues

```bash
# Test internal connectivity
kubectl run -n icn test --rm -it --image=busybox -- sh
wget -O- http://icn-gateway:8080/health

# Check DNS
kubectl exec -n icn icnd-0 -- nslookup icn-gateway

# Verify network policies
kubectl get networkpolicy -n icn
```

### Performance Issues

```bash
# Check resource usage
kubectl top pods -n icn

# Check for CPU throttling
kubectl describe pod -n icn icnd-0 | grep -i throttl

# Review metrics in Grafana
kubectl port-forward -n monitoring svc/prometheus-grafana 3000:80
```

---

## Migration Guide

### From Docker Compose to Kubernetes

1. **Export data**:
   ```bash
   docker-compose exec icnd icnctl backup create -o /root/.icn/backup.tar.enc
   docker cp icn-daemon:/root/.icn/backup.tar.enc ./backup.tar.enc
   ```

2. **Deploy to Kubernetes**:
   ```bash
   helm install icn ./icn --namespace icn -f values.yaml
   ```

3. **Import data**:
   ```bash
   kubectl cp backup.tar.enc icn/icnd-0:/data/backup.tar.enc
   kubectl exec -n icn icnd-0 -- icnctl backup restore -i /data/backup.tar.enc
   ```

4. **Verify**:
   ```bash
   kubectl logs -n icn icnd-0
   kubectl port-forward -n icn svc/icn-gateway 8080:8080
   curl http://localhost:8080/health
   ```

---

## Best Practices

1. **Always use persistent volumes** for production
2. **Run at least 3 replicas** for high availability
3. **Set resource requests/limits** based on load testing
4. **Use pod disruption budgets** to ensure availability during updates
5. **Enable monitoring** and set up alerts
6. **Regular backups** with automated retention
7. **Use network policies** to restrict traffic
8. **Keep secrets encrypted** at rest and in transit
9. **Test disaster recovery** procedures regularly
10. **Document your configuration** and runbooks

---

## Additional Resources

- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [Helm Documentation](https://helm.sh/docs/)
- [ICN Architecture](../../docs/ARCHITECTURE.md)
- [ICN API Reference](../../docs/API_REFERENCE.md)
- [Production Hardening](../../docs/production-hardening.md)

## Support

- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Discussions: https://github.com/InterCooperative-Network/icn/discussions
