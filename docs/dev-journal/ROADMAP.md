# ICN Roadmap

**Last Updated**: 2026-01-31
**Current Focus**: Phase 3 State Generalization + Kernel Crate Cleanup
**Target**: Clean kernel with pluggable first-party and third-party apps

---

## Strategic Context

ICN is infrastructure for a **parallel political economy** — enabling cooperatives, communities, and federations to deliver better material outcomes than traditional capitalist and captured democratic systems.

**Core Vision**: ICN is not just infrastructure *for* cooperatives — it is **cooperative infrastructure that governs itself democratically**. The protocols are adjustable by the organizations using them.

### Architecture Reset (2026-01-26)

The previous roadmap (Phases 19-35) has been superseded by the **Kernel/App Separation** initiative. This architectural reset:

1. **Separates kernel from apps** - Kernel provides 8 generic primitives; apps implement domain logic
2. **Establishes the "Meaning Firewall"** - Kernel never understands domain semantics
3. **Enables third-party apps** - Same APIs as first-party apps
4. **Simplifies the codebase** - From 27 tightly-coupled crates to ~12 kernel crates + apps

**See**: [KERNEL_APP_SEPARATION.md](../KERNEL_APP_SEPARATION.md) for full architectural documentation.

---

## Current Status

### Completed Phases (1-18)

See [PHASE_HISTORY.md](../PHASE_HISTORY.md) for details.

| Phase | Name | Completed |
|-------|------|-----------|
| 1-10 | Foundation (Identity, Trust, Ledger, Network, Gossip) | 2025-Q3 |
| 11 | Multi-Device Identity & Sync | 2025-01-14 |
| 12 | Economic Safety Rails | 2025-01-14 |
| 13 | Governance Primitives | 2025-01-15 |
| 14 | Gateway API | 2025-01-17 |
| 15 | Distributed Compute | 2025-11-20 |
| 16 | Scheduler Evolution | 2025-11-24 |
| 17 | Storage Replication | 2025-11-25 |
| 18 | Pre-Pilot Hardening | 2025-11-27 |

### Kernel/App Separation (Current)

**Tracking Issue**: [#856](https://github.com/InterCooperative-Network/icn/issues/856)
**Branch**: `architecture/kernel-extraction`
**PR**: [#855](https://github.com/InterCooperative-Network/icn/pull/855)

| Phase | Status | Issue | Description |
|-------|--------|-------|-------------|
| 0: PolicyOracle Infrastructure | ✅ Complete | PR #855 | OracleRegistry, BootstrapPhase, DecisionCache |
| 1: App Runtime | ✅ Complete | PR #855 | AppRuntime, ComputeDispatcher, Manifest parsing |
| 1.5: CCL Schema Layer | ✅ Complete | PR #855 | Entity, Governance, Economics, Agreement schemas |
| 2: Trust Extraction | ✅ Complete | [#857](https://github.com/InterCooperative-Network/icn/issues/857) | Core complete (#910, #867, #869 merged). Only #877 (attestation reducer) remains as stretch. |
| 3: State Generalization | 🚧 Next | [#858](https://github.com/InterCooperative-Network/icn/issues/858) | Move domain logic out of icn-store |
| 4: Governance Extraction | ⏳ Planned | [#859](https://github.com/InterCooperative-Network/icn/issues/859) | Move governance to app, first CCL consumer |
| 5: Membership Consolidation | ⏳ Planned | [#860](https://github.com/InterCooperative-Network/icn/issues/860) | Merge entity/coop/community crates |
| 6: Crate Consolidation | ⏳ Planned | [#861](https://github.com/InterCooperative-Network/icn/issues/861) | Reduce kernel to ~12 crates |
| 7: Naming Primitive | ⏳ Planned | [#862](https://github.com/InterCooperative-Network/icn/issues/862) | App-to-app discovery and communication |

**Additional**: [#863](https://github.com/InterCooperative-Network/icn/issues/863) - Federation Agreement Support

### Completed Epics (Jan 26–30)

| Epic | Status | Key PRs | Description |
|------|--------|---------|-------------|
| Phase 2: Trust Extraction (#857) | ✅ Complete | #969, #970, #971 | TrustService migration, ledger oracle, OracleRegistry wiring |
| Kernel Crate Cleanup (#912, #916) | ✅ Complete | #972, #974 | Remove icn-trust from icn-core, strict Meaning Firewall CI |
| Cells & Scopes (#919) | ✅ Complete | #950, #962, #961 | ScopeLevel, CellId, CellService, scope-aware placement + replication |
| ExecutionReceipt & Settlement | ✅ Complete | #956, #960 | Chained Ed25519 receipts, ReceiptClearingManager |
| Commons Security (#966, #967) | ✅ Complete | #975, #976 | Sybil resistance + replay protection for commons credits |
| Service Discovery | 🚧 ~40% | #952 | Endpoint registry landed. Open: #934-#937, #953, #954 |
| Commons Resource Pool | 🚧 ~50% | #963, #975, #976 | CommonsPool + metrics + security. Open: #947-#949, #964-#965 |
| Daemon Service Wiring (#908, #909) | ✅ Complete | #968 | GovernanceService + LedgerService in icnd |

### Kernel Crate Cleanup

Prerequisites for Phase 6 (Crate Consolidation):

- ~~[#912](https://github.com/InterCooperative-Network/icn/issues/912): Remove icn-trust from icn-core~~ ✅ PR #972
- [#913](https://github.com/InterCooperative-Network/icn/issues/913): Remove icn-governance from icn-core
- [#914](https://github.com/InterCooperative-Network/icn/issues/914): Remove icn-ledger from icn-core
- [#915](https://github.com/InterCooperative-Network/icn/issues/915): Remove icn-trust dev-dep from icn-net (PR #973 open)
- ~~[#916](https://github.com/InterCooperative-Network/icn/issues/916): Enable strict Meaning Firewall CI~~ ✅ PR #974
- [#911](https://github.com/InterCooperative-Network/icn/issues/911): Remove raw_handles transition mechanism
- [#918](https://github.com/InterCooperative-Network/icn/issues/918): Move supervisor init_*.rs to app crates

---

## The Meaning Firewall

The core architectural principle driving this work:

```
CCL Document (constitution / bylaws / treaty)
         ↓
App Interpreter (Governance / Membership / Ledger / Federation)
         ↓
PolicyDecision { constraints }
         ↓
Kernel enforces constraints mechanically
```

**Example**: The kernel never sees "constitutional amendment" or "supermajority" — only:
- `min_votes = 67`
- `quorum_required = 50`
- `deadline = 1706000000`

This ensures:
1. Apps can implement any governance model
2. Kernel remains simple and auditable
3. Third-party apps have equal capabilities

---

## Target Architecture

### Kernel Crates (~12)

```
icn-kernel-api/   # Trait definitions (PolicyOracle, primitives)
icn-identity/     # DID + keystore
icn-store/        # Generic storage (KV, Log, Blob)
icn-protocol/     # Gossip + networking
icn-core/         # Runtime + supervisor + app management
icn-services/     # API surfaces (RPC, Gateway)
icn-security/     # Security primitives
icn-crypto/       # Cryptography
icn-obs/          # Observability
icn-encoding/     # Serialization
icn-time/         # Time primitives
icn-testkit/      # Test utilities
```

### First-Party Apps

```
apps/
  trust/          # Trust graph, attestations, PolicyOracle
  ledger/         # Mutual credit, escrow, budgets
  governance/     # Proposals, voting, CCL-driven rules
  membership/     # Entity management, membership classes
  echo/           # Test app (implemented)
```

### 8 Kernel Primitives

| Primitive | Purpose |
|-----------|---------|
| Identity | DID management, key operations |
| Authorization | Capability-based access control via PolicyOracle |
| State | Persistent storage (KV, Log, Blob) |
| Compute | Event/request routing (Reducer/Service split) |
| Communication | Pub/sub messaging |
| Time | Clocks, timers |
| Coordination | Consensus, CRDTs |
| Naming | Name resolution, discovery |

---

## CCL: Constitutional Layer

CCL (Cooperative Contract Language) is the declarative layer for expressing:
- Entity constitutions and bylaws
- Governance rules and voting procedures
- Economic policies (surplus allocation, credit limits)
- Federation agreements

**Schema v0 complete** (PR #855):
- `entity.rs` - Cooperatives, communities, federations, membership
- `governance.rs` - Bodies, decisions, delegation
- `economics.rs` - Capital, surplus, credit
- `agreement.rs` - Federation agreements, boundary protocols

**Key Insight**: Federation agreements use **binary boundary outcomes**:

```yaml
boundary_outcome:
  type: binary  # approved | rejected
```

Federation A uses consensus. Federation B uses majority vote. The agreement doesn't care — it only asks "Did each party approve?"

**Internal process is sovereign. Boundary outcomes are interoperable.**

---

## Timeline Estimate

| Phase | Effort | Status |
|-------|--------|--------|
| 2: Trust Extraction | ~~2 days~~ | ✅ Complete |
| 3: State Generalization | 3-4 days | 🚧 Next |
| 4: Governance Extraction | 3-4 days | ⏳ Planned |
| 5: Membership Consolidation | 4-5 days | ⏳ Planned |
| 6: Crate Consolidation | 3-4 days | ⏳ Planned |
| 7: Naming Primitive | 2-3 days (service discovery foundation exists) | ⏳ Planned |

**Total remaining: ~16-20 days of focused work** (Phase 2 complete)

---

## Post-Separation Work

After kernel/app separation is complete:

1. **Pilot Deployment** - Deploy with first cooperative partner
2. **SDK Development** - TypeScript/React Native SDKs for app developers
3. **Third-Party App Ecosystem** - Documentation and tooling for external developers
4. **Federation Testing** - Multi-federation scenarios with diverse governance models

---

## Related Documents

- [KERNEL_APP_SEPARATION.md](../KERNEL_APP_SEPARATION.md) - Full architectural documentation
- [PHASE_HISTORY.md](../PHASE_HISTORY.md) - Completed phases 1-18
- [ARCHITECTURE.md](../ARCHITECTURE.md) - System architecture overview
