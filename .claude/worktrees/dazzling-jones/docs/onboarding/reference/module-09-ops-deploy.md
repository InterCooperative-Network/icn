# Module 9: Operations and Deployment

## Learning Objectives

By the end of this module, you will:
- Understand configuration layering and validation
- Deploy ICN using native, Docker, or Kubernetes methods
- Configure production-grade secrets and security settings
- Set up monitoring with Prometheus and Grafana
- Perform backup and recovery operations
- Troubleshoot common deployment issues

## Prerequisites

- Module 8 (Web UI Integration)
- Basic Linux system administration
- Familiarity with Docker and/or Kubernetes

## Key Reading

- `deploy/README.md`
- `config/README.md`
- `docs/security/production-hardening.md`
- `docs/HOMELAB_DEPLOYMENT.md`

---

## Concepts (Textbook Style)

### Configuration Layering

ICN uses a layered configuration system where later sources override earlier ones:

```
┌─────────────────────────────────────────────────────────────────┐
│                 Configuration Precedence                        │
│                                                                 │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐ │
│  │ Built-in │───▶│  Config  │───▶│   Env    │───▶│   CLI    │ │
│  │ Defaults │    │   File   │    │   Vars   │    │  Flags   │ │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘ │
│                                                                 │
│  Lowest Priority ─────────────────────────▶ Highest Priority   │
└─────────────────────────────────────────────────────────────────┘
```

**Why layered configuration?**

1. **Safe Defaults**: Built-in defaults provide a working baseline
2. **File-based Config**: TOML files for complex, environment-specific settings
3. **Environment Variables**: 12-factor app compatibility, container orchestration
4. **CLI Flags**: Quick overrides for testing and debugging

### Configuration Sources

| Source | Example | Use Case |
|--------|---------|----------|
| Built-in | `network.listen_addr = "0.0.0.0:7777"` | Safe starting point |
| Config File | `config.toml` | Production settings |
| Environment | `ICN_GATEWAY_JWT_SECRET=...` | Secret injection |
| CLI Flag | `--data-dir /var/lib/icn` | Testing/runtime overrides |

### Configuration File Structure

```toml
# config.toml - Main configuration file

data_dir = "/var/lib/icn"
# Keystore passphrase set via ICN_KEYSTORE_PASSPHRASE (preferred) or ICN_PASSPHRASE

[network]
listen_addr = "0.0.0.0:7777"
mdns_enabled = true
bootstrap_peers = [
    "icn://did:icn:abc@192.168.1.10:7777",
]

[gateway]
enabled = true
bind_addr = "0.0.0.0:8080"
# JWT secret set via ICN_GATEWAY_JWT_SECRET env var

[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"

[rate_limits]
# Trust-based velocity limiting
requests_per_second = 100
burst_capacity = 200
```

### Configuration Validation

Always validate configuration before starting:

```bash
# Validate configuration file
icnd --config /etc/icn/config.toml --validate-config

# Output shows warnings and errors
Validating configuration...

✓ network.listen_addr: 0.0.0.0:7777
✓ gateway.enabled: true
⚠ Warning: gateway.jwt_secret not set, will use ICN_GATEWAY_JWT_SECRET env var
✓ observability.metrics_port: 9100

Configuration is valid with 1 warning(s).
```

---

## Deployment Models

### Native Installation (Systemd)

Best for dedicated servers and bare-metal deployments.

```
┌────────────────────────────────────────────────────────┐
│                 Native Deployment                      │
│                                                        │
│   systemd ──▶ icnd (daemon)                           │
│                  │                                     │
│                  ├── /var/lib/icn/ (data)             │
│                  ├── /etc/icn/config.toml (config)    │
│                  └── /etc/icn/icnd.env (secrets)      │
│                                                        │
│   nginx ──▶ pilot-ui (static files)                   │
└────────────────────────────────────────────────────────┘
```

**Installation steps**:

```bash
# 1. Build from source
cd icn
cargo build --release

# 2. Install binaries
sudo cp target/release/icnd /usr/local/bin/
sudo cp target/release/icnctl /usr/local/bin/

# 3. Create system user and directories
sudo useradd --system --home-dir /var/lib/icn icn
sudo mkdir -p /var/lib/icn /etc/icn
sudo chown icn:icn /var/lib/icn

# 4. Configure environment (secrets)
sudo cp deploy/icnd.env.example /etc/icn/icnd.env
sudo chmod 600 /etc/icn/icnd.env
# Edit to set JWT_SECRET

# 5. Install systemd service
sudo cp deploy/icnd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable icnd

# 6. Initialize identity
sudo -u icn icnctl --data-dir /var/lib/icn id init

# 7. Start service
sudo systemctl start icnd
sudo systemctl status icnd
```

**Systemd service file**:

```ini
[Unit]
Description=ICN Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=icn
Group=icn
EnvironmentFile=/etc/icn/icnd.env
ExecStart=/usr/local/bin/icnd --config /etc/icn/config.toml --data-dir /var/lib/icn
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/icn

[Install]
WantedBy=multi-user.target
```

### Docker Compose

Best for local development and small-scale pilots.

```
┌────────────────────────────────────────────────────────┐
│                Docker Compose Stack                    │
│                                                        │
│   ┌──────────┐  ┌──────────┐  ┌──────────┐           │
│   │   icnd   │  │prometheus│  │ grafana  │           │
│   │  :8080   │  │  :9091   │  │  :3001   │           │
│   └──────────┘  └──────────┘  └──────────┘           │
│        │                                              │
│        └───────────────────────────────────────┐      │
│                                                │      │
│   ┌──────────────────────────────────────────────┐   │
│   │              Docker Network                  │   │
│   └──────────────────────────────────────────────┘   │
│                                                       │
│   ┌──────────┐                                       │
│   │ pilot-ui │                                       │
│   │  :3000   │                                       │
│   └──────────┘                                       │
└────────────────────────────────────────────────────────┘
```

**Quick start**:

```bash
cd deploy

# Automated setup
./quickstart.sh "My Timebank"

# Or manual setup:
cp .env.example .env
# Edit .env to set JWT_SECRET

docker-compose build
docker-compose up -d

# Initialize identity
docker-compose exec icnd icnctl id init
docker-compose exec icnd icnctl id show

# Get auth token
docker-compose exec icnd icnctl auth token --coop my-coop
```

**Environment variables** (`.env`):

```bash
# Required secrets
JWT_SECRET=your-secure-secret-at-least-32-chars
GRAFANA_PASSWORD=admin

# Optional configuration
ICN_GATEWAY_JWT_SECRET=your-secure-secret-at-least-32-chars
ICN_KEYSTORE_PASSPHRASE=your-keystore-passphrase
RUST_LOG=icn=info
```

### Kubernetes

Best for production, multi-node deployments.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Kubernetes Cluster                           │
│                                                                 │
│  Namespace: icn                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                                                         │   │
│  │  ┌──────────────┐     ┌──────────────┐                 │   │
│  │  │ Deployment   │     │  ConfigMap   │                 │   │
│  │  │  icn-daemon  │◀────│  icn-config  │                 │   │
│  │  └──────────────┘     └──────────────┘                 │   │
│  │         │                    │                         │   │
│  │         │             ┌──────────────┐                 │   │
│  │         └────────────▶│   Secret     │                 │   │
│  │                       │ icn-secrets  │                 │   │
│  │                       └──────────────┘                 │   │
│  │                                                         │   │
│  │  ┌──────────────┐     ┌──────────────┐                 │   │
│  │  │   Service    │     │     PVC      │                 │   │
│  │  │   NodePort   │     │   icn-data   │                 │   │
│  │  │  30080/30777 │     │    10Gi      │                 │   │
│  │  └──────────────┘     └──────────────┘                 │   │
│  │                                                         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  Monitoring Namespace                                           │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Prometheus │ Grafana │ AlertManager                    │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

**Key manifests**:

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: icn-daemon
  namespace: icn
spec:
  replicas: 1
  template:
    spec:
      containers:
      - name: icnd
        image: icn:latest
        args: ["--config", "/etc/icn/config.toml", "--data-dir", "/var/lib/icn"]
        env:
        - name: ICN_PASSPHRASE
          valueFrom:
            secretKeyRef:
              name: icn-secrets
              key: passphrase
        - name: ICN_GATEWAY_JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: icn-secrets
              key: jwt-secret
        ports:
        - containerPort: 7777
          protocol: UDP
        - containerPort: 8080
          protocol: TCP
        - containerPort: 9100
          protocol: TCP
        livenessProbe:
          httpGet:
            path: /v1/health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 30
        resources:
          requests:
            cpu: 100m
            memory: 512Mi
          limits:
            cpu: "1"
            memory: 2Gi
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
```

**Deployment commands**:

```bash
cd deploy/k8s

# Create namespace and secrets
kubectl apply -f namespace.yaml
kubectl create secret generic icn-secrets \
  --from-literal=passphrase="your-passphrase" \
  --from-literal=jwt-secret="$(openssl rand -base64 32)" \
  -n icn

# Deploy
kubectl apply -k .

# Or use the Makefile
make full-deploy-dev

# Check status
kubectl -n icn get pods
kubectl -n icn logs -f deployment/icn-daemon
```

---

## Secret Management

### Required Secrets

| Secret | Purpose | Generation |
|--------|---------|------------|
| `JWT_SECRET` | Signs API tokens | `openssl rand -base64 32` |
| `passphrase` | Encrypts keystore | Strong memorable password |
| `GRAFANA_PASSWORD` | Grafana admin | `openssl rand -base64 16` |

### Security Best Practices

```bash
# Generate secure secrets
JWT_SECRET=$(openssl rand -base64 32)
GRAFANA_PASSWORD=$(openssl rand -base64 16)

# Never commit secrets to git
echo "secret.yaml" >> .gitignore
echo ".env" >> .gitignore

# Use Kubernetes secrets
kubectl create secret generic icn-secrets \
  --from-literal=jwt-secret="$JWT_SECRET" \
  --from-literal=passphrase="your-passphrase" \
  -n icn

# Verify secrets are not in git
git status --ignored
```

---

## Observability

### Metrics

ICN exposes Prometheus metrics on port 9100:

```bash
# Check metrics endpoint
curl http://localhost:9100/metrics

# Key metrics
icn_gossip_messages_received_total    # Gossip traffic
icn_network_peers_connected           # Peer count
icn_ledger_transactions_total         # Transaction volume
icn_gateway_requests_total            # API requests
icn_gateway_request_duration_seconds  # API latency
```

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'icnd'
    static_configs:
      - targets: ['icnd:9100']
    scrape_interval: 15s
    metrics_path: /metrics
```

### Grafana Dashboards

Pre-configured dashboards include:

| Dashboard | Metrics |
|-----------|---------|
| **ICN Overview** | Peer count, message rate, error rate |
| **Gateway API** | Request rate, latency, error codes |
| **Gossip Protocol** | Messages sent/received, sync status |
| **Ledger** | Transaction rate, balance distribution |

### Health Endpoints

```bash
# Gateway health (HTTP)
curl http://localhost:8080/v1/health
# Response: {"status":"healthy","network_peers":5,"gossip_entries":1234}

# Prometheus health
curl http://localhost:9100/metrics | grep icn_up
```

### Alerting

Example Prometheus alerting rules:

```yaml
# prometheusrule.yaml
groups:
- name: icn-alerts
  rules:
  - alert: ICNDaemonDown
    expr: up{job="icnd"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "ICN daemon is down"

  - alert: NoPeersConnected
    expr: icn_network_peers_connected == 0
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "No peers connected"

  - alert: HighAPILatency
    expr: histogram_quantile(0.99, icn_gateway_request_duration_seconds_bucket) > 1
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "API p99 latency > 1s"
```

---

## Backup and Recovery

### Backup Strategy

```
┌─────────────────────────────────────────────────────┐
│                 Backup Components                   │
│                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────┐  │
│  │  Keystore   │  │   Ledger    │  │  Config   │  │
│  │ (encrypted) │  │   (sled)    │  │  (toml)   │  │
│  └─────────────┘  └─────────────┘  └───────────┘  │
│        │                │                │        │
│        └────────────────┼────────────────┘        │
│                         │                         │
│                         ▼                         │
│              ┌─────────────────────┐              │
│              │  icnctl backup      │              │
│              │  (encrypted tarball)│              │
│              └─────────────────────┘              │
└─────────────────────────────────────────────────────┘
```

### Creating Backups

```bash
# Native installation
icnctl backup create -o /var/backups/icn-$(date +%Y%m%d).tar.enc

# Docker
docker-compose exec icnd icnctl backup create -o /data/backup.tar.enc
docker cp icn-daemon:/data/backup.tar.enc ./backup-$(date +%Y%m%d).tar.enc

# Kubernetes
kubectl -n icn exec deploy/icn-daemon -- icnctl backup create -o /data/backup.tar.enc
kubectl -n icn cp icn-daemon:/data/backup.tar.enc ./backup.tar.enc
```

### Restoring Backups

```bash
# Stop the daemon first
sudo systemctl stop icnd

# Restore
icnctl backup restore -i /var/backups/icn-20250115.tar.enc

# Restart
sudo systemctl start icnd
```

### Automated Backups (Kubernetes)

```yaml
# backup-cronjob.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: icn-backup
  namespace: icn
spec:
  schedule: "0 2 * * *"  # Daily at 2 AM
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: backup
            image: icn:latest
            command:
            - /bin/sh
            - -c
            - |
              icnctl backup create -o /backup/icn-$(date +%Y%m%d).tar.enc
            volumeMounts:
            - name: data
              mountPath: /data
            - name: backup
              mountPath: /backup
          volumes:
          - name: data
            persistentVolumeClaim:
              claimName: icn-data
          - name: backup
            persistentVolumeClaim:
              claimName: icn-backup
          restartPolicy: OnFailure
```

---

## Production Hardening

### Security Checklist

| Item | Status | Command |
|------|--------|---------|
| Strong JWT secret (32+ bytes) | ☐ | `openssl rand -base64 32` |
| TLS enabled for gateway | ☐ | Configure reverse proxy |
| Rate limiting enabled | ☐ | Check `icn.toml` |
| Keystore passphrase set | ☐ | Set `ICN_PASSPHRASE` |
| Resource limits configured | ☐ | Check deployment manifest |
| Network policies applied | ☐ | `kubectl apply -f network-policies.yaml` |

### Network Policies (Kubernetes)

```yaml
# network-policies.yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: icn-network-policy
  namespace: icn
spec:
  podSelector:
    matchLabels:
      app: icn
  policyTypes:
  - Ingress
  - Egress
  ingress:
  # Allow gateway traffic
  - ports:
    - port: 8080
      protocol: TCP
  # Allow P2P traffic
  - ports:
    - port: 7777
      protocol: UDP
  # Allow metrics scraping from monitoring
  - from:
    - namespaceSelector:
        matchLabels:
          name: monitoring
    ports:
    - port: 9100
      protocol: TCP
```

### TLS Termination

Example Caddy configuration for TLS:

```
# Caddyfile
api.timebank.example.com {
    reverse_proxy icnd:8080

    # Automatic HTTPS
    tls {
        protocols tls1.2 tls1.3
    }

    # Security headers
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
    }
}

app.timebank.example.com {
    reverse_proxy pilot-ui:80
    encode gzip
}
```

---

## Troubleshooting

### Common Issues

| Issue | Diagnosis | Solution |
|-------|-----------|----------|
| "JWT secret not set" | Missing env var | Set `ICN_GATEWAY_JWT_SECRET` |
| "Address already in use" | Port conflict | Change port in config |
| "Connection refused" | Service not running | Check `systemctl status icnd` |
| "Certificate expired" | TLS cert needs renewal | Update certificates |
| "No peers connected" | Network isolation | Check bootstrap peers |

### Diagnostic Commands

```bash
# Check service status
systemctl status icnd
journalctl -u icnd -f

# Docker logs
docker-compose logs -f icnd

# Kubernetes logs
kubectl -n icn logs -f deployment/icn-daemon

# Check health
curl http://localhost:8080/v1/health

# Check metrics
curl http://localhost:9100/metrics | head -50

# Check identity
icnctl id show

# Check peers
icnctl network peers
```

### Log Levels

```bash
# Set log level via environment
export RUST_LOG=icn=debug,icn_gateway=trace

# Or via CLI
icnd --config /etc/icn/config.toml --log-level debug
```

---

## Code Map

| File | Purpose |
|------|---------|
| `deploy/README.md` | Deployment overview |
| `deploy/install.sh` | Native installation script |
| `deploy/quickstart.sh` | Docker quickstart script |
| `deploy/docker-compose.yml` | Docker Compose stack |
| `deploy/k8s/` | Kubernetes manifests |
| `deploy/helm/` | Helm chart |
| `config/icn.toml.example` | Configuration reference |
| `docs/security/production-hardening.md` | Security guidance |

---

## Exercises

### Exercise 1: Deploy with Docker

Set up a local ICN instance using Docker Compose:

```bash
cd deploy
./quickstart.sh "Test Coop"
# Verify health endpoint responds
# Get an auth token and test the API
```

### Exercise 2: Configure Monitoring

Add a custom alert rule for high transaction volume:

```yaml
# Add to prometheusrule.yaml
- alert: HighTransactionVolume
  expr: rate(icn_ledger_transactions_total[5m]) > 100
  for: 10m
  annotations:
    summary: "Transaction rate exceeds 100/s"
```

### Exercise 3: Validate Configuration

Create a configuration file and validate it:

```bash
# Create custom config
cp config/icn.toml.example /tmp/test-config.toml
# Edit to change ports

# Validate
icnd --config /tmp/test-config.toml --validate-config
```

---

## Checkpoints

Before proceeding to Module 10, verify you can:

- [ ] Explain configuration layering and precedence
- [ ] Deploy ICN using Docker Compose
- [ ] Generate and securely store secrets
- [ ] Access metrics via Prometheus
- [ ] View dashboards in Grafana
- [ ] Create and restore backups
- [ ] Troubleshoot common deployment issues
- [ ] Apply security hardening measures

---

## Next Steps

In **Module 10: Contributor Workflow**, you'll learn how to contribute to ICN development,
including the git workflow, PR process, and code review guidelines.
