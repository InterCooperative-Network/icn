# ICN Golden Development Prompt
## Master Context for AI-Assisted Development
**Version:** 1.0 — March 18, 2026
**Maintainer:** Matt Faherty
**Repo:** `InterCooperative-Network/icn` (`main` branch)

> This document provides complete context for any AI coding agent working on ICN.
> It supplements (does not replace) the repo's `CLAUDE.md`, which covers operational
> rules, branch hygiene, and CI workflow. This document covers **what**, **why**,
> **where we are**, and **where we're going**.

---

# §1 — PROJECT IDENTITY

## What ICN Is

ICN (InterCooperative Network) is a substrate daemon for the cooperative internet. It is a P2P coordination layer that enables cooperatives, communities, and federations to govern themselves, exchange value, share resources, and federate — without intermediaries that capture their value.

**It is not a blockchain.** It is not a federation server. It is a constraint enforcement engine where apps translate cooperative governance semantics into generic constraints, and the kernel enforces constraints without understanding their meaning.

**One-line version:** Anyone can record anything of value within their cell, governed by rules they democratically choose, bridged to other cells via federated trust.

## The Vision (Four Stages)

```
Stage A: Institutions     → Institution-in-a-Box (charters, treasury, governance)
Stage B: Federation       → Cross-group trust, agreements, clearing
Stage C: Commons          → Resource contribution, shared services
Stage D: Civilization     → Municipal governance, public infrastructure
```

The substrate is at Stage B depth in infrastructure but still finishing Stage A at the product layer.

## Core Principle: The Meaning Firewall

> **Apps translate meaning into constraints. The kernel enforces constraints without understanding meaning.**

This is the architectural invariant. The kernel provides mechanisms (identity, transport, storage, enforcement). Apps provide semantics (what a "vote" means, what a "credit limit" is, how "trust" is computed). The boundary is enforced by CI ratchet tests in `meaning_firewall.rs`.

**Litmus test for any change:** "Could a developer build a DIFFERENT economic model / governance system / trust algorithm using these primitives?" If no, the primitives are too specific — the change belongs in an app, not the kernel.

---

# §2 — CURRENT STATE (Honest Assessment)

## By the Numbers (as of March 2026)

| Metric | Value |
|--------|-------|
| Rust lines (crates + bins) | ~420,000 |
| Rust lines (app layer) | ~7,000 |
| Crates | 34 |
| App crates | 4 (trust, governance, ledger, echo) + membership stub |
| Binaries | 3 (icnd, icnctl, icn-console) |
| Tests | 4,287 |
| Gateway API endpoints | ~60+ |
| Workspace version | 0.1.0 |
| Rust toolchain | 1.88.0 (pinned, do not change) |
| License | MIT OR Apache-2.0 (AGPL-3.0 on repo root) |

## What Works

- **Identity:** Ed25519 DIDs, Age-encrypted keystore, multi-device, key rotation, post-quantum hybrid (ML-DSA). 14,522 lines, 221 tests.
- **Trust:** Trust graph with transitive computation, scoped per-org trust. PolicyOracle implementation is the reference pattern. 11,818 lines, 227 tests.
- **Networking:** QUIC/TLS sessions, mDNS discovery, DID-TLS binding, E2E encryption, NAT traversal stubs. 19,856 lines, 205 tests.
- **Gossip:** Topic-based with vector clocks, Bloom filters, anti-entropy, trust-gated access control. 14,949 lines, 221 tests.
- **Ledger:** Double-entry mutual credit, Merkle-DAG anchoring, commons credits, patronage, treasury, settlement, disputes, dynamic limits. 28,741 lines, 325 tests.
- **Governance:** Proposals, amendments, appeals, charter, delegation (liquid democracy), domains, effect manifests, invariant gates, protocol parameters, vote tallying. 25,427 lines, 434 tests.
- **Compute:** Actor model, WASM executor, task scheduling, commons pool, federation compute, receipts. 24,419 lines, 278 tests.
- **Federation:** Attestations, clearing, netting, receipt clearing, registry, router. 11,867 lines, 173 tests.
- **Gateway:** REST API with ~60+ endpoints, JWT auth, WebSocket, actix-web. 59,685 lines, 314 tests.
- **Demo:** 4-node K3s deployment, 4 demo flows (governance, patronage, federation, reporting), presenter mode, canonical personas.

## What's Incomplete

- **CCL Charter Engine:** The YAML schema system (Entity, Governance, Economics, Agreement) parses and validates. The expression evaluator works (`parse_expr("0.67 * members")`). But the bridge from CclDocument → ConstraintSet → kernel enforcement does not exist. Contracts are JSON ASTs interpreted with fuel metering. No text parser. No WASM compiler in this repo.
- **Kernel infection:** 11 of ~29 kernel crates still import domain types. `icn-core` has 43 governance refs, 31 ledger refs, 32 CCL refs. Being ratcheted down via CI enforcement.
- **Gateway:** 59K lines in one crate. Adapter/view-model boundary refactor is in progress.
- **Federation end-to-end:** Clearing and netting exist but aren't wired through the full AgreementSchema lifecycle.
- **Security audit:** Internal only. No external professional review.
- **Scale testing:** Works at 2-4 nodes. 10+ node testing not yet done.

## What's Deprecated — DO NOT Reference

- **Mana:** The regenerating compute capacity token system is deprecated. Use "fuel" for compute metering. Kill any remaining Mana references encountered.
- **icn-core repo:** Archived January 15, 2026. All development is in `InterCooperative-Network/icn`.
- **AgileMind Innovations Cooperative:** Never operational. Do not reference.

---

# §3 — ARCHITECTURE

## Repo Topology

```
/                           ← Monorepo root (docs, deploy, web, sdk, scripts)
├── icn/                    ← Rust workspace root (ALL cargo commands run here)
│   ├── Cargo.toml          ← Workspace definition
│   ├── crates/             ← 34 library crates
│   ├── bins/               ← 3 binary crates (icnd, icnctl, icn-console)
│   └── apps/               ← App-layer crates (trust, governance, ledger, echo)
├── apps/                   ← Same as icn/apps/ (symlink or co-located)
├── web/                    ← Web UIs (pilot-ui, dashboard)
├── sdk/                    ← SDKs (typescript, react-native)
├── deploy/                 ← Deployment configs (k8s, compose, devnet)
├── demo/                   ← Demo scripts and configs
├── contracts/              ← CCL contract examples (JSON)
├── docs/                   ← Documentation (235+ files)
├── monitoring/             ← Prometheus/Grafana configs
├── CLAUDE.md               ← Operational agent rules (branch hygiene, CI, etc.)
└── kernel_surface.toml     ← Kernel infection tracking
```

**Rule:** Rust commands (`cargo *`) run from `icn/`. Everything else from monorepo root.

## Crate Architecture

### Kernel Layer (domain-agnostic, enforces mechanisms)

| Crate | Purpose | Status |
|-------|---------|--------|
| `icn-kernel-api` | Trait definitions: PolicyOracle, ConstraintSet, State, Compute, Comms, Events, Identity, Naming (44 traits) | Clean |
| `icn-core` | Tokio actor runtime, supervisor, meaning firewall enforcement | Infected (extraction in progress) |
| `icn-net` | QUIC/TLS sessions, mDNS discovery, NAT traversal | Infected (medium) |
| `icn-gossip` | Topic-based gossip, vector clocks, Bloom filters | Infected (medium) |
| `icn-store` | Sled-backed persistent KV storage | Clean |
| `icn-time` | Rough Time Protocol clock sync | Clean |
| `icn-obs` | Prometheus metrics, tracing, logging | Clean |
| `icn-snapshot` | State snapshots for graceful restarts | Clean |
| `icn-encoding` | Serialization utilities | Clean |
| `icn-protocol` | Facade re-exporting icn-gossip + icn-net | Clean |
| `icn-services` | Facade re-exporting icn-api + icn-rpc + icn-gateway | Clean |
| `icn-crypto` | Facade re-exporting icn-crypto-pq | Clean |

### Domain Layer (semantic, interprets meaning)

| Crate | Purpose | Lines | Tests |
|-------|---------|-------|-------|
| `icn-identity` | DIDs, Ed25519, keystore, multi-device, recovery, revocation | 14,522 | 221 |
| `icn-trust` | Trust graph, transitive computation, scoped trust | 11,818 | 227 |
| `icn-ledger` | Mutual credit, Merkle-DAG, treasury, patronage, settlement | 28,741 | 325 |
| `icn-governance` | Proposals, voting, delegation, charter, protocol params | 25,427 | 434 |
| `icn-ccl` | Contract AST, interpreter, schemas, disputes, registry | 14,607 | 148 |
| `icn-compute` | WASM executor, scheduling, commons pool, receipts | 24,419 | 278 |
| `icn-federation` | Clearing, netting, attestations, registry, routing | 11,867 | 173 |
| `icn-entity` | Unified entity model, lifecycle, membership, registry | 7,138 | 113 |
| `icn-coop` | Cooperative-specific actor, lifecycle, membership | 5,506 | 69 |
| `icn-community` | Community-specific actor, lifecycle, resources | 3,719 | 134 |
| `icn-gateway` | REST API, WebSocket, JWT auth, all HTTP endpoints | 59,685 | 314 |
| `icn-naming` | Hierarchical naming service (cell/org/fed/commons) | 849 | 8 |
| `icn-authz` | Capability graph model (new, Sprint 11+) | 1,232 | 54 |
| `icn-privacy` | Onion routing, topic encryption, traffic obfuscation | 1,430 | 49 |
| `icn-security` | Byzantine detection, misbehavior | 1,782 | 26 |
| `icn-steward` | SDIS steward network, VUI registry | 6,218 | 93 |
| `icn-zkp` | Zero-knowledge proofs, accumulators | 5,843 | 74 |
| `icn-crypto-pq` | Post-quantum ML-DSA hybrid signatures | 3,707 | 106 |

### App Layer (PolicyOracle implementations)

| App | Purpose | Lines | Pattern |
|-----|---------|-------|---------|
| `apps/trust` (icn-trust-app) | TrustPolicyOracle — trust scores → kernel constraints | 3,733 | **Reference implementation** |
| `apps/governance` (icn-governance-app) | GovernancePolicyOracle — protocol params → constraints | 524 | Working |
| `apps/ledger` (icn-ledger-app) | LedgerPolicyOracle — credit policy → constraints | 1,855 | Working |
| `apps/echo` (icn-app-echo) | Test/example app | 262 | Example |

### Key Kernel Types

```rust
// The kernel asks apps: "Should this action be allowed?"
pub struct PolicyRequest {
    pub core: PolicyRequestCore,  // actor DID, action, domain
    pub context: PolicyContext,    // metadata HashMap
}

// Apps answer with constraints the kernel enforces blindly
pub struct ConstraintSet {
    pub rate_limit: Option<RateLimit>,
    pub max_topics: Option<u32>,
    pub max_message_size: Option<u64>,
    pub max_connections: Option<u32>,
    pub max_subscriptions: Option<u32>,
    pub max_outstanding_requests: Option<u32>,
    pub custom: HashMap<String, ConstraintValue>,  // ← extensibility point
}

pub enum ConstraintValue {
    Bool(bool), Int(i64), Float(OrderedFloat<f64>), String(String), List(Vec<ConstraintValue>)
}

// Apps implement this trait
pub trait PolicyOracle: Send + Sync {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;
}

pub enum PolicyDecision {
    Allow { constraints: ConstraintSet },
    Deny { reason: String },
}
```

### CCL Schema Types (the charter system)

```rust
// A complete CCL document (YAML-serializable)
pub struct CclDocument {
    pub schema_version: SchemaVersion,  // Currently V0 only
    pub entity: Option<EntitySchema>,
    pub governance: Option<GovernanceSchema>,
    pub economics: Option<EconomicsSchema>,
    pub agreement: Option<AgreementSchema>,
    pub extensions: HashMap<String, serde_yaml::Value>,
}

// Expression mini-language (string-parseable)
// parse_expr("0.67 * members") → SchemaExpr::BinaryOp { ... }
// parse_expr("min(1000, patronage * 0.5)") → SchemaExpr::FunctionCall { ... }
// EvalContext binds runtime values and evaluates deterministically
```

### The Meaning Firewall Boundary (reference: apps/trust/src/oracle.rs)

```rust
// ABOVE this line: trust semantics (scores, classes, graph)
// BELOW this line: generic kernel constraints (rate limits, multipliers)
fn score_to_constraints(&self, score: f64, _action: &ActionKind) -> ConstraintSet {
    let rate_limit = match score {
        s if s >= 0.7 => RateLimit::unlimited(),
        s if s >= 0.4 => RateLimit::standard(),
        s if s >= 0.1 => RateLimit::throttled(),
        _ => RateLimit::restricted(),
    };
    // Kernel never knows what "trust score" means
    ConstraintSet::default().with_rate_limit(rate_limit)
}
```

---

# §4 — DEVELOPMENT PHASES

## Phase 0: Close the Demo 🚧
**Status:** Active
**Target:** 2 weeks
**Proves:** "This is real. It runs. Multiple cooperatives coordinate without a central authority."

### Remaining Work

| Task | Type | Effort | Notes |
|------|------|--------|-------|
| Complete ExecutionReceiptGate (#1310) | Feature | Done | Links governance vote → execution proof → treasury action — PR #1327 merged |
| Add treasury/ledger scopes to demo auth calls | Fix | Hours | Scopes exist in ALLOWED_SCOPES but demo scripts don't request them |
| Deploy proof signing key to K3s pods | Ops | Hours | `/gov/proposals/{id}/proof` returns 404 without it |
| Verify K3s cluster operational | Ops | Hours | Runner was down Jan 2026 |
| Run all 4 flows, fix friction | Polish | Days | Use `demo/scripts/verify-demo.sh` |
| Record demo | Content | Day | `--presenter` mode exists |

### Demo Flows
1. **Governance Legitimacy** — Harbor Homes roof repair: proposal → vote → authorization → proof
2. **Patronage & Contribution** — BrightWorks Q1: allocation formula → settlement → provenance
3. **Federation Coordination** — Tool Library ↔ BrightWorks equipment agreement via Finger Lakes CDN
4. **Institutional Reporting** — Finger Lakes CDN: read-only audit across member coops

### Definition of Done
- `demo/scripts/verify-demo.sh` passes
- All 4 flows run in `--presenter` mode without errors
- Recorded demo exists as shareable asset

---

## Phase 1: The Charter Engine ⏳
**Status:** Planned
**Target:** 4–6 weeks
**Proves:** "Cooperatives define their own governance rules. The system enforces them."

### The Core Task: `charter_to_constraints()`

Build the bridge from YAML charter documents to kernel-enforced constraints.

```
CclDocument::from_yaml()          ← EXISTS
CclDocument::validate()            ← EXISTS
compute_hash(&doc)                 ← EXISTS
charter_to_constraints()           ← BUILD THIS
  ├── EvalContext binds runtime data
  ├── Schema expressions evaluate to numbers
  └── Numbers map to ConstraintSet
ConstraintSet via PolicyOracle     ← WIRE THIS (new apps/charter crate)
Kernel enforcement                 ← EXISTS
```

### Mapping: Schema → Constraints

**GovernanceSchema:**
- `VoteThreshold` expressions → `custom["min_votes"]` ConstraintValue
- `DecisionType.quorum` → `custom["min_quorum"]`
- `DelegationConfig.max_depth` → `custom["max_delegation_depth"]`

**EconomicsSchema:**
- `CreditConfig.limit` expression → `custom["credit_limit"]`
- `SurplusConfig.allocation_rules` → `custom["allocation_*"]`
- `MemberEquity.minimum/maximum` → `custom["equity_min/max"]`

**AgreementSchema:**
- `SettlementConfig.cycle` → `custom["settlement_cycle"]`
- `DisputeResolution.stages` → `custom["dispute_stages"]`

Start with governance thresholds + credit limits (the demo exercises these). Expand incrementally.

### Deliverables
- `charter_to_constraints()` in `icn-ccl`
- `CharterPolicyOracle` in new `apps/charter` crate
- 5 charter templates: worker coop, consumer coop, housing coop, community org, federation
- `icnctl charter validate/deploy/inspect` subcommands
- Demo Flow 1 uses a real charter document
- Integration test: YAML → ConstraintSet → kernel enforcement

---

## Phase 2: Pilot Launch ⏳
**Status:** Planned
**Target:** 4–6 weeks
**Proves:** "Real cooperatives use this for real decisions and real accounting."

### Deliverables
- Pilot runbook (#1222)
- One-command deployment per cooperator
- Charter customization workflow
- Deploy nodes for 3–5 pilot cooperatives
- Weekly check-ins, feedback collection
- Pilot case study for funders

### Selection Criteria for Pilot Coops
- Already governing themselves (bylaws, votes, money)
- Willing to parallel-run ICN alongside existing tools
- Can articulate current coordination pain
- In Matt's network reach

---

## Phase 3: Federation Depth ⏳
**Status:** Planned
**Target:** 2–3 months
**Proves:** "Autonomous cooperatives coordinate at scale without new bureaucracies."

### Deliverables
- Federation Agreement lifecycle (AgreementSchema → live agreement)
- Cross-org credential recognition
- Federation clearing end-to-end (credit settlement between coops)
- Dispute resolution flow
- NAT traversal for WAN federation
- 10+ node scale test
- Federation dashboard in pilot UI

---

## Phase 4: Institution-in-a-Box ⏳
**Status:** Planned
**Target:** 2–3 months
**Proves:** "Any group can form a cooperative and be operational in hours."

### Deliverables
- `icnctl init-coop` interactive wizard
- Web-based charter builder
- One-click node deployment (Docker)
- Member invitation flow (QR/link)
- Mobile app (React Native from existing SDK)
- Offline-first capability

---

## Phase 5: The Commons Layer ⏳
**Status:** Planned
**Target:** 3–6 months
**Proves:** "Cooperatives pool resources. The network sustains itself."

### Deliverables
- Commons resource contribution accounting (#925)
- Resource metering (CPU, storage, bandwidth)
- Commons credit formula via CCL (#1308)
- Shared service registry and marketplace
- WASM app deployment
- Resource allocation governance

---

## Phase 6: Civilization Tools ⏳
**Status:** Horizon
**Proves:** "This infrastructure replaces the coordination functions of state and corporation."

Emerges from Phases 1–5 working. Municipal governance, cooperative health networks, climate coordination, education cooperatives, mutual aid at scale.

---

# §5 — PROGRESS TRACKING

## Phase Tracking Format

Every phase follows this structure in `docs/PHASE_PROGRESS.md`:

```markdown
### Phase N: <Name>
**Status:** ✅ Complete | 🚧 In Progress | ⏳ Planned
**Started:** YYYY-MM-DD
**Completed:** YYYY-MM-DD (if done)
**Sprint(s):** S16, S17, etc.

**Objective:** One sentence.

**Deliverables:**
- [ ] Deliverable 1 — PR #NNNN
- [x] Deliverable 2 — PR #NNNN (merged YYYY-MM-DD)
- [ ] Deliverable 3 — unstarted

**Blockers:**
- Description of blocker (if any)

**Decisions Made:**
- Decision and rationale (dated)

**Metrics:**
- Tests added: N
- Lines changed: +N / -N
- Kernel infection delta: -N refs
```

## Kernel Infection Ratchet

Tracked in `kernel_surface.toml` and enforced by `meaning_firewall.rs` CI tests.

| Metric | Current | Target |
|--------|---------|--------|
| `icn-core` governance refs | 43 | 0 |
| `icn-core` ledger refs | 31 | 0 |
| `icn-core` CCL refs | 32 | 0 |
| Infected crates | 11 | 0 |
| Clean crates | 9 | all |

**Rule:** Every sprint, infection count must not increase. Decrease when touching infected code.

## Charter Engine Progress

Track in `docs/charter-engine/PROGRESS.md`:

```markdown
### Schema → Constraint Mappings

| Schema Type | Field | Constraint Key | Status |
|-------------|-------|----------------|--------|
| GovernanceSchema | VoteThreshold | custom["min_votes"] | ⏳ |
| GovernanceSchema | quorum | custom["min_quorum"] | ⏳ |
| EconomicsSchema | CreditConfig.limit | custom["credit_limit"] | ⏳ |
| ... | ... | ... | ... |
```

## Sprint Close Checklist

Every sprint close updates:
1. `docs/PHASE_PROGRESS.md` — deliverable checkboxes
2. `kernel_surface.toml` — infection counts (if changed)
3. `docs/STATE.md` — current status snapshot (dated)
4. `docs/TODO.md` — reorder/update active items
5. Run `doc_reality_scan.sh` — fix broken links

---

# §6 — SESSION PROTOCOL

## Starting a Session

```bash
# 1. Preflight
cd /path/to/icn/icn
git pull --rebase
cargo check 2>&1 | tail -5  # Fast type-check
git status --short            # Must be clean

# 2. Orient
cat ../docs/PHASE_PROGRESS.md | head -40   # Where are we?
cat ../docs/TODO.md | head -20             # What's next?
git log --oneline -10                       # Recent commits

# 3. Identify scope
# What phase are we in? What deliverable is next?
# State scope explicitly before writing any code.
```

## During a Session

- **One deliverable at a time.** Complete it, test it, commit it.
- **Test before commit.** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test -p <crate>`
- **Scope is law.** The user's request defines scope. Note adjacent concerns but do not act on them.
- **No parallel changes** unless explicitly instructed.
- **Meaning Firewall check:** If adding code to a kernel crate, verify it doesn't import domain types.

## Ending a Session

```bash
# 1. Verify
cargo test -p <touched-crates>
cargo clippy --all-targets -- -D warnings

# 2. Update tracking
# Update docs/PHASE_PROGRESS.md deliverable checkboxes
# Update docs/STATE.md if meaningful state changed

# 3. Summary
# State: what was done, what's next, any blockers
```

---

# §7 — TECHNICAL SPECIFICATIONS

## Charter Bridge Spec (Phase 1 — Primary Engineering Task)

### Function Signature

```rust
// In icn-ccl/src/schema/bridge.rs (new file)

use icn_kernel_api::authz::{ConstraintSet, ConstraintValue};

/// Runtime context for evaluating charter expressions.
/// Binds live data (member count, balances, trust scores) to field names
/// used in schema expressions.
pub struct CharterContext {
    eval_ctx: EvalContext,  // from schema/expr.rs
}

impl CharterContext {
    pub fn new() -> Self { ... }
    pub fn with_member_count(self, count: u64) -> Self { ... }
    pub fn with_trust_score(self, score: f64) -> Self { ... }
    // etc.
}

/// Convert a validated CclDocument into kernel-enforceable constraints.
///
/// This is THE Meaning Firewall boundary for charters.
/// Above: cooperative governance semantics (quorum, delegation, credit policy)
/// Below: generic kernel constraints (numbers, limits, flags)
pub fn charter_to_constraints(
    doc: &CclDocument,
    context: &CharterContext,
) -> Result<ConstraintSet, SchemaError> { ... }
```

### Expression Evaluation Flow

```
YAML: threshold: "0.67 * members"
         │
         v
parse_expr("0.67 * members")                    ← EXISTS in schema/expr.rs
         │
         v
SchemaExpr::BinaryOp { Literal(0.67), Mul, Field("members") }
         │
         v
EvalContext { fields: { "members": Int(100) } }  ← Bind from CharterContext
         │
         v
evaluate() → SchemaValue::Float(67.0)           ← EXISTS in schema/expr.rs
         │
         v
ConstraintSet { custom: { "min_votes": Int(67) } }  ← Map to constraint
```

### CharterPolicyOracle

```rust
// In apps/charter/src/oracle.rs (new crate)

pub struct CharterPolicyOracle {
    // Stores active charters per entity, keyed by content hash
    charters: Arc<RwLock<HashMap<ContentHash, (CclDocument, ConstraintSet)>>>,
}

impl PolicyOracle for CharterPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Look up charter for the requesting entity's scope
        // Return the pre-computed ConstraintSet
        // Kernel enforces without knowing it came from a charter
    }
}
```

---

# §8 — ANTI-PATTERNS

**Do not:**
- Build a CCL text parser. The YAML schema system with expression strings is the v1 interface.
- Chase WASM compilation. The interpreter with fuel metering is sufficient.
- Expand the kernel trait surface. 44 traits is enough. Implement existing ones.
- Build new web frontends. Polish `web/pilot-ui`.
- Rewrite the gateway. Decompose incrementally.
- Fix unrelated lints/warnings when working on a specific task.
- Upgrade the Rust toolchain (pinned at 1.88.0).
- Use "Mana" anywhere. The term is deprecated. Use "fuel" for compute metering.
- Invent new tracking systems. Use the phase format defined in §5.
- Make parallel changes without explicit instruction.
- Add domain-specific logic to kernel crates.

**Do:**
- Follow the trust app (`apps/trust/src/oracle.rs`) as the reference PolicyOracle pattern.
- Run the meaning firewall check when touching kernel crates.
- Update `docs/PHASE_PROGRESS.md` when completing deliverables.
- Start sessions with the preflight in §6.
- Keep commits scoped to one deliverable.
- Be honest in documentation about what works and what doesn't.

---

# §9 — KEY FILE LOCATIONS

| What | Where |
|------|-------|
| Operational agent rules | `CLAUDE.md` (repo root) |
| This development context | `docs/GOLDEN_PROMPT.md` |
| Phase progress tracking | `docs/PHASE_PROGRESS.md` |
| Current state snapshot | `docs/STATE.md` |
| Active TODO | `docs/TODO.md` |
| Kernel infection tracking | `kernel_surface.toml` |
| Meaning Firewall enforcement | `icn/crates/icn-core/src/meaning_firewall.rs` |
| Vision document | `VISION.md` |
| CCL schemas | `icn/crates/icn-ccl/src/schema/` |
| CCL expression parser | `icn/crates/icn-ccl/src/schema/expr.rs` |
| CCL contract AST | `icn/crates/icn-ccl/src/ast.rs` |
| Trust PolicyOracle (reference) | `apps/trust/src/oracle.rs` |
| Governance PolicyOracle | `apps/governance/src/oracle.rs` |
| Ledger PolicyOracle | `apps/ledger/src/oracle.rs` |
| Kernel API traits | `icn/crates/icn-kernel-api/src/` |
| Demo scripts | `demo/scripts/` |
| Demo design doc | `docs/plans/2026-03-07-demo-system-design.md` |
| Sprint planning | `docs/plans/2026-03-05-sprint-14-closeout-sprint-15-scope-design.md` |
| Active sprint (Mar 17-30) | `docs/strategy/ICN-Sprint-March17.md` |
| Gap analysis (Mar 2026) | `docs/strategy/ICN-Gap-Analysis-March-2026.md` |
| 90-day roadmap | `docs/strategy/ICN-Roadmap-Live.md` |
| Crate reference (38 crates) | `docs/planning/icn-crate-reference.md` |
| Ecosystem map | `docs/planning/icn-ecosystem-map.md` |
| Vertical slice assessment | `docs/planning/icn-vertical-slice-assessment.md` |
| Mobile UX spec v1 | `docs/mobile/icn-mobile-ux-spec-v1.md` |
| Charter templates (to be created) | `contracts/templates/` |
| Pilot UI | `web/pilot-ui/` |
| Gateway scopes | `icn/crates/icn-gateway/src/validation.rs` (ALLOWED_SCOPES) |
| K3s deployment | `deploy/k8s/` |
| Monitoring | `monitoring/` |

---

# §10 — CONTEXT FOR THE HUMAN

Matt Faherty is the sole developer. He's a systems-oriented thinker and cooperative infrastructure builder based in Rochester/Palmyra, NY. He works from SSI disability income. His partner Katie is also his caregiver post-shoulder surgery. He organizes with the NY Cooperative Network.

His development style: mechanism-first, sharp, no-bullshit. He prefers concise output over verbose narration. He builds because he must, not because he's confident it will work. The fatalism and the building run parallel.

Two voice modes: **polemical** (sharp, cutting, directed at systems/hypocrisy) and **teacher** (structured, practical, growth-oriented). When writing docs or communications for him, match the context.

Key failure modes to watch for: overcompression of opposition, isolation through clarity, certainty as armor, burnout from permanent urgency. When you see scope creeping toward "rebuild civilization this session," gently redirect to the next concrete deliverable.

**The question that drives everything:** "How should the world actually work?" — running continuously since 2009.

**The answer being built:** Programmable, federated, democratic infrastructure — owned by the people who use it.

---

*This document should be reviewed and updated at every phase transition.*
