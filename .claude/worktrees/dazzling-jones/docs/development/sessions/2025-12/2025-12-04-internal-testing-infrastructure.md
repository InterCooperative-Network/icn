---
date: 2025-12-04
title: "Internal Testing Infrastructure: Pre-Pilot Validation"
type: dev-journal
phase: post-18
topics: [testing, docker, monitoring, governance, validation]
status: complete
duration: ~6 hours
---

# Internal Testing Infrastructure: Pre-Pilot Validation

## Overview

Following the completion of Phase 18 (Byzantine Fault Detection), we built comprehensive internal testing infrastructure to validate multi-node behavior before pilot deployment. This ensures the system works correctly under real network conditions with governance, ledger, compute, and Byzantine scenarios.

## Motivation

Phase 18 added Byzantine fault detection at the code level, but we needed:
1. Multi-node validation in realistic deployment
2. Governance testing with actual voting across nodes
3. Performance baselines under load
4. Operational procedure validation
5. Clear Go/No-Go criteria before external pilot

## What We Built

### 1. Docker-Based Test Environment

**Files Created**:
- [Dockerfile](../../Dockerfile) - Multi-stage build (rust:slim → debian:trixie-slim)
- [docker-compose.test.yml](../../docker-compose.test.yml) - 6-service orchestration
- [config/node1-4.toml](../../config/) - Per-node TOML configuration

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                    Docker Network (icn_test)                │
├─────────────────────────────────────────────────────────────┤
│  node1:5001     node2:5002     node3:5003     node4:5004   │
│  (honest)       (honest)       (honest)       (byzantine)   │
│  metrics:9091   metrics:9092   metrics:9093   metrics:9094  │
├─────────────────────────────────────────────────────────────┤
│  prometheus:9095              grafana:3000                  │
│  (scrapes all nodes)          (visualization)               │
└─────────────────────────────────────────────────────────────┘
```

**Key Configuration Decisions**:
- **Config files over CLI args**: Each node uses `/config/nodeX.toml` for maintainability
- **Metrics on port 9100**: ICN daemon default, mapped to host ports 9091-9094
- **Prometheus on 9095**: Avoids VS Code conflict on 9090
- **Byzantine profile**: node4 only starts with `--profile byzantine`

### 2. Comprehensive Test Plan (38 Scenarios)

**File**: [docs/INTERNAL_TESTING_PLAN.md](../INTERNAL_TESTING_PLAN.md) (1,000+ lines)

**Test Categories**:

| Category | Count | Key Scenarios |
|----------|-------|---------------|
| Baseline Functionality | 10 | Network formation, gossip, ledger, governance |
| Byzantine Detection | 10 | Invalid signatures, replay, forks, governance attacks |
| Performance & Load | 6 | Throughput benchmarks, 24-hour soak test |
| Resilience | 5 | Crash recovery, partition healing |
| Operational | 4 | Backup/restore, upgrades, incidents |

**Governance Scenarios (9 total)**:
1. Domain creation & gossip sync
2. Proposal lifecycle (majority passes)
3. Proposal lifecycle (quorum fails)
4. WebSocket event delivery
5. Vote manipulation attack (non-member voting)
6. Double voting attack
7. Proposal spam resilience
8. Conflicting outcomes under partition
9. Load test (100 proposals, 300 votes)

### 3. Monitoring Stack

**Prometheus Configuration**: [monitoring/prometheus.yml](../../monitoring/prometheus.yml)
- Scrapes all 4 nodes at port 9100
- 15-second interval
- Node role labels (honest vs byzantine)

**Alert Rules**: [monitoring/alert_rules.yml](../../monitoring/alert_rules.yml) (25 rules)
- Byzantine detection (4 rules)
- Network health (4 rules)
- Ledger consistency (3 rules)
- Gossip performance (2 rules)
- Compute layer (3 rules)
- Governance (3 rules)
- System resources (3 rules)
- Monitoring system (2 rules)

**Grafana Dashboard**: [monitoring/grafana-dashboard.json](../../monitoring/grafana-dashboard.json)
- Auto-provisioned datasource
- Network, Byzantine, Governance panels
- Real-time metrics visualization

### 4. Configuration Validation Script

**File**: [scripts/validate-test-config.sh](../../scripts/validate-test-config.sh) (170 lines)

**Validates**:
- 11 required files exist
- Node config files (metrics_port, listen_addr)
- Docker Compose port mappings
- Dockerfile EXPOSE and HEALTHCHECK
- Prometheus scrape targets
- Documentation port references
- Prerequisites (Docker 24+, Docker Compose 2.20+)

**Usage**:
```bash
./scripts/validate-test-config.sh
# Output: ✓ All checks passed! or list of errors/warnings
```

### 5. Documentation Suite

| File | Lines | Purpose |
|------|-------|---------|
| [INTERNAL_TESTING_PLAN.md](../INTERNAL_TESTING_PLAN.md) | 1,000+ | Complete test scenarios with success criteria |
| [TESTING_QUICKSTART.md](../TESTING_QUICKSTART.md) | 500+ | Step-by-step manual test procedures |
| [DEPLOY_TEST_NETWORK.md](../../DEPLOY_TEST_NETWORK.md) | 400+ | Host system deployment guide |

## Configuration Issues Resolved

During development, we discovered and fixed 8 Docker configuration issues:

| Issue | Fix | Commit |
|-------|-----|--------|
| COPY paths wrong for build context | Use relative paths | `1982498` |
| Missing keystore passphrase | Add ICN_PASSPHRASE env var | `007f043` |
| Unsupported ICN_DATA_DIR etc | Use CLI arguments | `139d62e` |
| Unsupported --bind argument | Remove, use config file | `73ad13b` |
| Kubernetes files cluttering git | Add to .gitignore | `25ee80b` |
| Node4 inconsistent (no config) | Create node4.toml | `992f517` |
| Wrong metrics port (9090 vs 9100) | Fix Dockerfile, docs | `992f517` |
| set -e incompatible with error collection | Remove set -e | `5d101db` |

## Go/No-Go Criteria

Before proceeding to pilot deployment, all criteria must pass:

### Must Pass (8 criteria)
- [ ] All 38 test scenarios pass
- [ ] No crashes/panics in 24-hour soak test
- [ ] Byzantine nodes detected within 1 min SLA
- [ ] Governance voting works correctly (no vote loss)
- [ ] No false positives (honest nodes never quarantined)
- [ ] Ledger consistency maintained (no undetected forks)
- [ ] Network recovers from partitions <2 min
- [ ] Stable memory usage (<2 GB/node, no leaks)

### Performance Targets
- Gossip throughput: 1000 msg/sec
- Ledger transactions: 50 tx/sec
- Compute task queue: 500 concurrent tasks
- Governance load: 100 proposals, 300 votes <5 min

## Testing Timeline

**Week 1 (Days 1-5)**: Baseline
- Build Docker image
- Deploy 3-node network
- Run 10 baseline tests
- Establish performance baselines

**Week 2 (Days 6-12)**: Comprehensive
- Byzantine detection tests (10)
- Governance tests (9)
- Performance tests (6)
- Resilience tests (5)
- Operational tests (4)
- 24-hour soak test
- **Go/No-Go Decision**

## Security Considerations

**Test Environment**:
- Hardcoded passphrase: `test-passphrase-insecure-do-not-use-in-production`
- Prominent warnings in all documentation
- Isolated Docker network
- Non-root container users

**Production Differences**:
- Secure secrets management (Vault, K8s secrets)
- TLS termination via reverse proxy
- Authentication on Grafana
- Long-term metrics retention
- HTTPS everywhere

## Files Changed

### Created (12 files)
- Dockerfile
- docker-compose.test.yml
- config/node1.toml, node2.toml, node3.toml, node4.toml
- monitoring/prometheus.yml, alert_rules.yml, grafana-datasource.yml
- scripts/validate-test-config.sh
- docs/INTERNAL_TESTING_PLAN.md, TESTING_QUICKSTART.md
- DEPLOY_TEST_NETWORK.md

### Modified (4 files)
- CLAUDE.md (added testing infrastructure section)
- .gitignore (exclude deploy/k8s/)
- monitoring/grafana-dashboard.json (simplified)
- Various documentation (port references)

## Lessons Learned

1. **Config files > CLI args**: Complex services need declarative configuration
2. **Verify actual ports**: Don't assume - check what the binary actually uses
3. **Test environment security**: Hardcoded credentials need prominent warnings
4. **Error collection pattern**: Can't use `set -e` with error counting
5. **Documentation consistency**: Port references must match across all files

## Next Steps

1. **Deploy on host system**: Run validation script, build image, start network
2. **Execute Week 1 tests**: Baseline functionality and performance
3. **Execute Week 2 tests**: Byzantine, governance, resilience
4. **Go/No-Go decision**: All criteria must pass
5. **Proceed to Track C1**: Pilot community selection

## Impact

**Before**: Risk of deploying untested multi-node system to pilot community
**After**: Comprehensive validation with clear success criteria

The internal testing infrastructure bridges the gap between Phase 18 completion and Track C1 pilot deployment, ensuring we don't expose pilot communities to untested code paths.

---

**Status**: ✅ Complete
**Next Action**: Deploy on host system and begin 2-week testing timeline
**Blocks**: Track C1 (Pilot Community Selection) pending Go/No-Go decision
