# ICN Gap Analysis

**Date**: 2025-11-18 (Updated from 2025-01-15)
**Purpose**: Comprehensive analysis of implementation status vs vision
**Status**: Historical analysis snapshot (2025-11-18)

> Historical note: This document reflects a point-in-time assessment from 2025-11-18.
> Validate current readiness against latest status reports and live CI results.

---

## Executive Summary

ICN's protocol substrate was assessed as production-ready in this 2025-11-18 snapshot (Phase 14 complete). The primary identified gap was the **livability layer** - the things that make it usable for humans and legible as an ecosystem.

**Core thesis**: Engine/transmission/frame present; dashboard/seatbelts/driving school missing.

### What Exists

- **Identity**: DIDs, Age-encrypted keystore, multi-device support, social recovery logic
- **Trust**: Trust graph, transitive scoring, trust classes, rate limiting
- **Network**: QUIC/TLS, mDNS discovery, signed messages, E2E encryption, replay protection
- **Gossip**: Topic-based pub/sub, vector clocks, Bloom filters, anti-entropy
- **Ledger**: Double-entry mutual credit, Merkle-DAG, economic safety (credit limits, disputes)
- **Contracts**: CCL interpreter with fuel metering
- **Governance**: Domains, proposals, voting, membership resolution
- **Gateway**: REST API + WebSocket, JWT auth, rate limiting, scope-based authorization
- **Operations**: Graceful restart, metrics, health checks, backup/restore

### What's Missing

1. **Protocol-level**: Identity actor, programmable trust, contract wiring, federation semantics
2. **Product/UX**: Co-op Console, installer, first-run wizard
3. **Ecosystem**: Reference apps, legal templates, federation playbooks
4. **Operations**: Monitoring dashboards, incident automation, key management
5. **Developer experience**: SDKs, local dev harness, example workflows
6. **Compute substrate**: Capacity registry, job scheduling, metering, execution harness
7. **Fundamental**: Conflict resolution, offline support, mobile-first, i18n

---

## 1. Protocol-Level Gaps

### 1.1 Identity as First-Class Actor

**Current state**: Identity is library code in `icn-identity`, not a running actor.

| Feature | Status | Notes |
|---------|--------|-------|
| DID generation | ✅ | Ed25519-based DIDs |
| Encrypted keystore | ✅ | Age-encrypted, passphrase-protected |
| Multi-device identity | ✅ | Phase 11: DID Document v2 |
| Key rotation | ✅ | RotationEvent chain with audit |
| Device capabilities | ✅ | Sign, AddDevice, RevokeDevice, etc. |
| Social recovery | 🚧 | Logic exists, not wired as actor |
| IdentityActor | ❌ | Not in supervisor |
| Hardware key support | ❌ | No YubiKey/FIDO2 |
| Capability tokens | ❌ | No scoped credentials per DID |

**Missing**:
- **IdentityActor as running thing**: Device enrollment/revocation as gossip-aware events, identity sync via `identity:updates` topic
- **Hardware key support as default**: YubiKey/FIDO2 so default story is "secure device + recovery"
- **Capability tokens**: Scoped credentials tied to DIDs instead of "DID can do everything"

### 1.2 Trust Graph as Programmable Substrate

**Current state**: Trust graph provides scoring and rate limiting by trust class.

| Feature | Status | Notes |
|---------|--------|-------|
| Weighted adjacency | ✅ | Basic graph storage |
| Trust computation | ✅ | Transitive scoring |
| Trust classes | ✅ | Isolated, Known, Partner, Federated |
| Trust-gated features | ✅ | Rate limiting, topic access |
| Trust decay | ❌ | No time-based decay |
| Domain-specific trust | ❌ | Single scalar only |
| Programmable consequences | ❌ | Trust doesn't auto-adjust limits |

**Missing**:
- **Trust decay and recency**: Scores should fade without ongoing participation
- **Domain-specific trust**: "Trust financially but not for governance" vs single scalar
- **Programmable consequences**: Trust auto-adjusts credit limits, federation visibility, governance weight

### 1.3 Contracts/CCL as Nervous System

**Current state**: CCL interpreter exists with fuel metering and basic integration.

| Feature | Status | Notes |
|---------|--------|-------|
| AST-based interpreter | ✅ | Core execution engine |
| Fuel metering | ✅ | Bounded execution |
| Capability system | ✅ | ReadLedger, WriteLedger, ReadTrust |
| Deterministic execution | ✅ | No randomness, no syscalls |
| Standard contract libraries | ❌ | No templates |
| Event wiring | ❌ | Loose governance→contracts→ledger |
| DSL syntax | ❌ | AST only, no parser |

**Missing**:
- **Standard contract libraries**: Worker share distribution, timebank exchange, CSA subscription, mutual aid reimbursement
- **Tight event wiring**: Governance decisions → contracts, ledger entries → contracts (escrow), trust attestations → capabilities
- **Policy layer**: "Contracts can't do X without Y consent/trust/governance"

### 1.4 Topology & Federation Semantics

**Current state**: Topology modes exist (standalone/regional/clustered), fanout works.

| Feature | Status | Notes |
|---------|--------|-------|
| Topology modes | ✅ | Standalone, regional, clustered |
| mDNS discovery | ✅ | Local network |
| Bootstrap peers | ✅ | Config-driven |
| Federation namespaces | ❌ | No human-usable names |
| Discovery directories | ❌ | Beyond mDNS |
| Inter-federation policy | ❌ | No trust policies |

**Missing**:
- **Federation-level namespaces**: Human-usable "this coop is part of NYCN federation"
- **Discovery beyond mDNS**: Cooperative directories/rendezvous services
- **Inter-federation policy**: "Federation A trusts Federation B at score X"

---

## 2. Product/UX Gaps

### 2.1 Cooperative Admin Dashboard (CRITICAL - Blocks Pilot)

**Current state**: Gateway API ready (14 endpoints, WebSocket, auth). No UI.

**Missing - "Co-op Console"**:
- Member list with roles
- Balance view per member
- Active proposals with vote buttons
- Simple payment form
- Invite/onboarding flow
- Credit policy configuration
- Trust relationship visualization

**Minimum viable console** (single-page React/Vue app):
- Auth flow (challenge → sign → JWT)
- Member management
- Balance display
- Create payment
- View/vote on proposals

**Impact**: #1 blocker. Pilot coop cannot see balances, vote, or manage members.

### 2.2 Human-Friendly Ledger & Governance UX

**Current state**: Primitives exist, CLI access via icnctl.

**Missing**:
- **Wallet-style views**: "My balance with this coop", "My obligations", "My trust lines"
- **Governance timeline**: "What your org is deciding this month", "How you voted"
- **Notifications**: New proposals, pending payments, limit increases

### 2.3 Installer & Operations UX

**Current state**: Manual installation, config, service setup.

**Missing**:
- **One-line installer**: `curl | bash` that detects OS, installs, writes config, sets up systemd
- **First-run wizard**: `icnctl init-coop` that creates identity, domain, member, starts daemon
- **Upgrade/migration tooling**: Safe upgrades, clear restore steps for non-nerds

---

## 3. Ecosystem & Social Gaps

### 3.1 Reference Applications

**Current state**: No end-to-end applications.

**Missing 2-3 "solves real thing today" apps**:
1. **Timebank / skill exchange**: Members log hours, exchange services
2. **Shared expense / mutual aid pot**: Pool funds, reimburse needs
3. **Worker co-op revenue distribution**: Revenue → member shares

Each should be opinionated, deployable, documented as "recipes."

### 3.2 Legal & Governance Alignment

**Current state**: Governance primitives work.

**Missing**:
- **Templates**: Mapping governance domains → bylaws, resolutions, agreements
- **Playbooks**: "US worker co-op → ICN governance mapping"

### 3.3 Federation Playbook

**Current state**: Federation technically possible.

**Missing**:
- **"How federations form on ICN"**: Example with member coops, federated domain, shared ledger
- **Onboarding steps**: Single node, regional federation, multi-region network

---

## 4. Operations, Safety & Reliability Gaps

### 4.1 Default Monitoring Stack

**Current state**: Prometheus metrics on :9090, health endpoint, monitoring dashboard.

| Feature | Status | Notes |
|---------|--------|-------|
| Prometheus exporter | ✅ | /metrics endpoint |
| Key metrics | ✅ | Gossip, network, ledger |
| Health endpoint | ✅ | JSON on :8080/health |
| Web dashboard | ✅ | Real-time UI on :8080/ |
| Incident runbooks | ✅ | 7 incident types documented |
| Operations guide | ✅ | 1035 lines |
| Grafana dashboards | ❌ | Not shipped |
| Alert rules | 🚧 | Examples documented, not shipped as YAML |

**Missing**:
- **Grafana dashboards**: Pre-built for gossip health, ledger activity, gateway errors
- **Prometheus alerting config**: Ship as .yaml, not just documentation

### 4.2 Incident Automation Hooks

**Current state**: Manual incident response.

**Missing**:
- **Webhooks/scripts** for: Replay attack spikes, trust anomalies, quarantine fills
- **"Red button" workflows**: Quarantine peer across network/trust/governance

### 4.3 Key Management Helpers

**Current state**: Keystore works, rotation via icnctl.

**Missing**:
- **Guided rotation flows**: Reminders, step-by-step help
- **Social recovery as first-class path**: Not power-user only

---

## 5. Developer Experience Gaps

### 5.1 SDKs / Client Libraries

**Current state**: Gateway REST API exists. No client libraries.

**Missing**:
- TypeScript (highest priority)
- Python
- Go/Rust for services

### 5.2 Mock / Local Dev Harness

**Current state**: Manual multi-node setup.

**Missing**:
- **`icn-dev up`**: Spin up 3-5 nodes + gateway + dashboard, hot-reload

### 5.3 Example Contracts & Workflows

**Current state**: CCL docs exist, no end-to-end examples.

**Missing**:
- "Contract sets up membership dues + ledger + votes"
- "Trust edge triggers increased credit limit via CCL"

---

## 6. Additional Protocol Gaps

### 6.1 Conflict Resolution Semantics

**Current state**: Ledger has quarantine. Governance has no equivalent.

**Missing**:
- Handling competing governance proposals (same ID)
- Handling proposal close during voting
- General conflict resolution for non-ledger data

### 6.2 Offline/Intermittent Connectivity

**Current state**: Assumes always-on nodes.

**Missing**:
- **Queue-and-sync**: Buffer operations while offline
- **Conflict resolution on reconnect**: After extended offline
- **Catch-up protocol**: Beyond anti-entropy for long disconnections

### 6.3 Mobile-First Considerations

**Current state**: REST/WebSocket is mobile-friendly transport.

**Missing**:
- Push notification integration
- Mobile wallet reference app
- JWT token refresh for long-lived sessions

### 6.4 Data Portability

**Current state**: Backup creates encrypted tarballs.

**Missing**:
- Human-readable export (ledger entries, votes, attestations)
- Migration path to different node/federation
- Tax-year / legal archive format

### 6.5 Audit Trails for Compliance

**Current state**: Merkle-DAG ledger is good foundation.

**Missing**:
- Immutable audit logs
- Signatures on every governance decision
- Tax-year report generation

### 6.6 Internationalization

**Current state**: All strings hardcoded English.

**Missing**:
- i18n infrastructure
- Locale-aware formatting
- RTL support

---

## 7. Compute Substrate Gaps (MAJOR)

### 7.1 Vision

Compute in ICN should be:
- **Shared cooperative resource**: Homelabs, co-op servers, community data centers
- **Economic input**: Contributing compute regenerates credit
- **Workload substrate**: Schedule data processing, AI, media encoding, batch jobs

ICN should be both **accounting + governance brain** and **coordination spine** for distributed compute.

### 7.2 Current State

- CCL fuel metering exists (local sandboxing)
- No network-wide compute accounting
- No job scheduling, capacity registry, or metering
- No integration with K8s/Nomad/Slurm

ICN can **represent** a compute economy but cannot:
- Discover compute nodes
- Send them jobs
- Verify execution
- Credit them based on usage

### 7.3 Missing Sub-systems

#### A. Node Capacity & Capability Registry

Nodes advertise capabilities:

```rust
struct ComputeProfile {
    cpu_cores: u32,
    ram_gb: u32,
    disk_gb: u32,
    gpu: Option<GpuProfile>,
    capabilities: Vec<ComputeCapability>, // wasm, containers, sgx
    network_mbps: u32,
    region: String,
    legal_profile: String,
}
```

Delivered via:
- Gossip topic `compute:nodes`
- Stored for scheduler queries
- Governable (org policy controls pool membership)

#### B. Compute Metering & Telemetry → Ledger

**Telemetry model** (per-job):
- CPU-seconds, RAM peak/avg, disk/network IO, duration, exit code

**ComputeUsageReport** (gossip topic):
- Origin node attests usage

**Ledger integration**:
- CCL contract: "For each CPU-second, credit X"
- Transparent, auditable conversion

#### C. Job Model & Scheduling

**Job spec**:

```rust
struct ComputeJob {
    id: JobId,
    owner: Did,
    image: ImageRef,
    resources: ResourceRequest,
    constraints: PlacementConstraints,
    input_refs: Vec<DataRef>,
    settlement_contract: Option<ContractRef>,
}
```

**Placement logic** using trust, capacity, topology.

**Scheduling patterns**: Best-fit, fair rotation based on contributions/governance.

#### D. Execution Harness

**Adapters for**: Kubernetes, Nomad, Proxmox, WASM

**ComputeAgent** (sidecar on each node):
- Listens for jobs, provisions workloads, collects telemetry, reports back

#### E. Verification & Anti-Cheating

- **Random redundancy**: Run same job on multiple nodes
- **Spot checks + trust integration**: Divergence → trust drops
- **Determinism**: Prefer deterministic workloads

#### F. Governance of Compute Pools

**Decisions**:
- Capacity reserved for local vs federation
- Pricing policies
- Data sensitivity tiers

**Implementation**: Domains like `coop.compute.policy`, proposals for pricing/pools

#### G. UX for Compute

**Console - Compute tab**:
- Capacity charts, utilization, jobs in flight
- Settings: share %, pricing, pools
- Node list with status

**Job submission**: Form for image, constraints, who pays; history/logs/cost view

### 7.4 Phased Implementation

| Phase | Scope | Deliverables |
|-------|-------|--------------|
| **C0** | Representation | ComputeProfile, `compute:nodes` topic, gateway endpoints, icnctl commands |
| **C1** | Off-chain integration | External scheduler prototype, flat-rate ledger entries |
| **C2** | Metering & contracts | UsageReport, CCL billing contracts, basic verification |
| **C3** | Full compute pools | Governance domains, federation placement, Console UX |

---

## 8. Critical Path to First Pilot

### Phase 1: Pilot Readiness (Immediate Priority)

| Item | What | Effort |
|------|------|--------|
| 1 | Installer script (`install.sh`) | Low |
| 2 | `icnctl init-coop` wizard | Low |
| 3 | Minimal Co-op Console | Medium |
| 4 | Pilot playbook (10-page doc) | Low |

### Phase 2: Pilot Support

| Item | What | Effort |
|------|------|--------|
| 5 | Monitoring dashboard (Grafana) | Low |
| 6 | Incident runbook for pilot | Low |

### Phase 3: Post-Pilot

| Item | What | Effort |
|------|------|--------|
| 7 | Reference app #1 (timebank) | Medium |
| 8 | TypeScript SDK | Medium |
| 9 | IdentityActor as real actor | Medium |
| 10 | Compute C0 | Medium |

---

## 9. Priority Matrix

| Gap | Blocks Pilot? | Effort | Impact |
|-----|---------------|--------|--------|
| Co-op Console | **YES** | Medium | Critical |
| `icnctl init-coop` | **YES** | Low | High |
| Installer script | **YES** | Low | High |
| Pilot playbook | **YES** | Low | High |
| Monitoring dashboard | No | Low | Medium |
| TypeScript SDK | No | Medium | High |
| IdentityActor | No | Medium | Medium |
| Compute C0 | No | Medium | High |
| Reference app | No | Medium | High |
| Federation playbook | No | Low | Medium |
| Offline support | No | High | Medium |
| Mobile app | No | High | High |
| i18n | No | High | Medium |
| Conflict resolution | No | Medium | Medium |
| Data portability | No | Medium | Medium |

---

## 10. Narrative & Onboarding Gaps

### Different Tiers of Explanation Needed

1. **Organizer level**: "We're sick of Google Sheets and SurveyMonkey; ICN fixes that."
2. **Tech ally level**: "Not blockchain - mutual credit with gossip and trust graphs."
3. **Movement level**: "This is the substrate for a parallel, cooperative economy."

### "Path to First Pilot" Guide

Step-by-step from:
- "We're a small coop with messy spreadsheets"
- to "We're using ICN for membership, money flows, and decisions"

---

## 11. Success Criteria

### 30-Day Validation
- Real cooperative deployed and operational
- 100+ transactions without manual intervention
- Survives 1 node failure without data loss
- Members can join/leave without technical support

### 6-Month Trial
- Pilot operates continuously without catastrophic failure
- Community provides actionable feedback

---

## 12. Summary

ICN has the **protocol substrate** complete. What's missing:

1. **Dashboard**: Co-op Console for humans to use it
2. **Seatbelts**: Monitoring, incident automation, key management
3. **Driving school**: Pilot playbook, reference apps, federation guides
4. **Compute layer**: Infrastructure-as-cooperative-resource

Once there's a clean Co-op Console, a few reference apps, and a pilot playbook, ICN stops being "incredible substrate" and starts being "obvious next step" for any serious cooperative project.

---

*Document created: 2025-01-15*
*Last updated: 2025-11-18*
*Status: Phase 14 (Gateway + Production Hardening) complete*
