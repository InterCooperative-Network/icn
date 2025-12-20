# ICN Helm Chart

Official Helm chart for deploying ICN on Kubernetes.

## Quick Start

```bash
helm install my-icn ./icn
```

## Prerequisites

- Kubernetes 1.20+
- Helm 3.0+
- PersistentVolume support
- (Optional) cert-manager
- (Optional) nginx-ingress

## Parameters

### ICN Node

| Parameter | Description | Default |
|-----------|-------------|---------|
| `icn.replicaCount` | Replicas | `1` |
| `icn.image.repository` | Image | `icn/icnd` |
| `icn.persistence.size` | Storage | `10Gi` |
| `icn.secrets.jwtSecret` | JWT secret | `change-me` |

### Monitoring

| Parameter | Description | Default |
|-----------|-------------|---------|
| `prometheus.enabled` | Enable Prometheus | `true` |
| `grafana.enabled` | Enable Grafana | `true` |

## Installation

```bash
# Basic
helm install icn ./icn

# Production
helm install icn ./icn \
  --set global.domain="mycoop.org" \
  --set icn.secrets.jwtSecret="$(openssl rand -base64 32)"
```

## License

MIT
