# Multi-Node ICN Deployment

Deploy multiple ICN nodes for testing multiple coops/communities and multi-device scenarios.

## Overview

This directory contains templates and scripts for deploying multiple ICN nodes on the K3s cluster. Each node can represent:
- **Different coops/communities** (separate identities)
- **Multiple devices** of the same coop (same identity, different instances)

## Architecture

```
K3s Cluster
├── Namespace: icn-coop-alpha
│   ├── Deployment: icn-coop-alpha
│   ├── PVC: icn-coop-alpha-data
│   ├── ConfigMap: icn-coop-alpha-config
│   └── Secret: icn-coop-alpha-secrets
├── Namespace: icn-coop-beta
│   ├── Deployment: icn-coop-beta
│   ├── PVC: icn-coop-beta-data
│   ├── ConfigMap: icn-coop-beta-config
│   └── Secret: icn-coop-beta-secrets
└── Namespace: icn-coop-gamma
    └── ...
```

## Quick Start

### Deploy First Test Coop

```bash
cd deploy/k8s/multi-node
./deploy-coop.sh alpha "Alpha Cooperative"
```

This creates:
- Namespace `icn-coop-alpha`
- All necessary resources
- Initializes identity automatically

### Deploy Multiple Coops

```bash
./deploy-coop.sh alpha "Alpha Cooperative"
./deploy-coop.sh beta "Beta Timebank"
./deploy-coop.sh gamma "Gamma Co-op"
```

### Deploy Multiple Devices (Same Identity)

For multi-device testing with the same identity:

```bash
# First device (creates identity)
./deploy-coop.sh alpha "Alpha Cooperative"

# Additional devices (share same identity)
./deploy-device.sh alpha device-2
./deploy-device.sh alpha device-3
```

## Directory Structure

```
multi-node/
├── README.md                    # This file
├── templates/                   # Kubernetes manifest templates
│   ├── namespace.yaml.template
│   ├── configmap.yaml.template
│   ├── deployment.yaml.template
│   ├── services.yaml.template
│   └── pvc.yaml.template
├── scripts/
│   ├── deploy-coop.sh          # Deploy new coop (new identity)
│   ├── deploy-device.sh        # Deploy additional device (same identity)
│   ├── init-identity.sh        # Initialize identity for a coop
│   └── list-coops.sh           # List all deployed coops
└── configs/                    # Generated configs (not in git)
    └── coop-alpha/
        └── icn.toml
```

## Concepts

### Coop Node (Different Identity)

A **coop** represents a distinct cooperative/community with its own identity:
- Unique DID
- Separate storage
- Own configuration
- Independent from other coops

**Use Case**: Testing multiple coops on the same network

### Device Node (Same Identity)

A **device** represents an additional node sharing the same identity as an existing coop:
- Same DID
- Separate storage (but can sync state)
- Own configuration
- Part of multi-device setup

**Use Case**: Testing multi-device scenarios where one coop has multiple nodes

## Configuration

Each coop can have its own configuration. The template includes:

- Network settings (ports, mDNS)
- Region/cluster ID
- Rate limiting
- Topology settings

Edit `configs/coop-<name>/icn.toml` before deployment or modify the ConfigMap after.

## Discovery

Nodes discover each other via:
1. **mDNS** - Automatic discovery on same network
2. **Bootstrap peers** - Configure in ConfigMap if needed

For multi-coop testing, mDNS should work automatically since they're on the same cluster network.

## Management

### List All Coops

```bash
./scripts/list-coops.sh
```

### Check Coop Status

```bash
kubectl -n icn-coop-alpha get pods
kubectl -n icn-coop-alpha logs -f deployment/icn-coop-alpha
```

### Delete a Coop

```bash
kubectl delete namespace icn-coop-alpha
```

### View Coop Identity

```bash
kubectl -n icn-coop-alpha exec deployment/icn-coop-alpha -- icnctl id show
```

## Examples

### Scenario 1: Two Coops

```bash
# Deploy two separate coops
./scripts/deploy-coop.sh alpha "Alpha Cooperative"
./scripts/deploy-coop.sh beta "Beta Timebank"

# They'll discover each other via mDNS
```

### Scenario 2: Multi-Device Coop

```bash
# First device
./scripts/deploy-coop.sh alpha "Alpha Cooperative"

# Additional devices (will share identity via import)
./scripts/deploy-device.sh alpha device-2
./scripts/deploy-device.sh alpha device-3
```

## Port Allocation

Each coop gets unique ports to avoid conflicts:

| Coop | QUIC (UDP) | RPC (TCP) | Metrics (TCP) | NodePort QUIC | NodePort RPC |
|------|------------|-----------|---------------|---------------|--------------|
| alpha | 7777 | 5601 | 9100 | 30777 | 30601 |
| beta | 7778 | 5602 | 9101 | 30778 | 30602 |
| gamma | 7779 | 5603 | 9102 | 30779 | 30603 |

Ports are automatically allocated based on coop name.

## Next Steps

- [ ] Add identity export/import for multi-device
- [ ] Add network configuration for bootstrap peers
- [ ] Add monitoring dashboard per coop
- [ ] Add cleanup scripts
- [ ] Add identity backup/restore

