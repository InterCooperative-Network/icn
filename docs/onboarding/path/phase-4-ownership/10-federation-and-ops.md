# 10: Federation and Operations — Production Deployment

> **Phase**: 4 | **Tier**: Maintainer  
> **Patterns introduced**: None (integration phase)  
> **Prerequisite**: Phase 3 complete (Builder tier)

## Why This Matters

Running ICN in production requires understanding **federation** (inter-cooperative coordination) and **operations** (deployment, monitoring, incident response). This layer bridges development and production.

→ See `manual.md` § "Federation and Production" for design rationale.

## What You'll Read

### 1. Federation Agreement Lifecycle: `icn-federation/src/agreement.rs`

Cooperatives form federation agreements:
```rust
pub enum AgreementState {
    Proposed,
    Negotiating,
    Active,
    Suspended,
    Terminated,
}
```

**File**: `icn-federation/src/lifecycle.rs`

### 2. Clearing and Netting: `icn-federation/src/clearing.rs`

Inter-cooperative credit settlement:
- **Clearing**: Offsetting mutual debts
- **Netting**: Reducing to single obligation

### 3. Configuration Layering

**Files**: `icn/bins/icnd/src/config.rs`, `icn-core/src/config.rs`

```
Default → File → Environment → CLI → Runtime
(lowest priority)              (highest priority)
```

### 4. Deployment: `deploy/k8s/`

Production Kubernetes manifests:
- StatefulSet for persistent storage
- Service for peer discovery
- ConfigMap for configuration
- Secret for keystore passphrase

### 5. Monitoring: `icn-obs/src/metrics.rs`

Prometheus metrics exported at `/metrics`:
- `gossip_entries_total`
- `ledger_validation_failures_total`
- `network_connections_active`

**Grafana dashboards** in `deploy/monitoring/`

## What You'll Build

No new lab — integrate previous labs into a deployed system.

## Checkpoint

You've completed this layer when you can:

1. **Deploy ICN**: Run ICN on Kubernetes or Docker Compose
2. **Monitor health**: Query Prometheus metrics, interpret dashboards
3. **Debug production**: Trace a live issue using logs + metrics
4. **Explain federation**: Describe clearing/netting with examples

**Artifact**: Deployment proof (screenshot of running pods + metrics endpoint).

## Deep Reference

→ `reference/module-11-federation.md` — Full federation protocol  
→ `reference/module-09-ops-deploy.md` — Deployment, monitoring, backups  
→ `docs/HOMELAB_DEPLOYMENT.md` — Live deployment example  
→ `deploy/k8s/README.md` — Kubernetes deployment guide
