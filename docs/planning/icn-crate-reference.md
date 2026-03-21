# ICN Crate Reference — Accurate Technical Inventory
# Generated from live repo exploration, 2026-03-12

**Source:** `~/projects/icn` on icn-dev (10.8.30.45)
**Workspace root:** `icn/` (38 members in Cargo.toml)
**Binaries:** `icnd`, `icnctl`, `icn-console`
**Last explored commit:** `72a8b7a3` (docs: FORWARD_PLAN_2026-03)

---

## Layer 0: The Rule Book

### `kernel_surface.toml` (repo root)
Tracks the Meaning Firewall health across all 29 kernel-touching crates.

| Status | Count | Meaning |
|--------|-------|---------|
| `clean` | 9 | Pure infrastructure, no domain coupling |
| `infected` | 11 | Contains domain logic being extracted to apps |
| `needs-review` | 9 | Unclear, requires analysis |

The Meaning Firewall is a ratchet: `icn-core` currently has 43 governance refs + 31 ledger refs + 32 CCL refs being extracted incrementally via the apps layer pattern.

---

## Layer 1: Kernel API (Contracts Only)

### `icn-kernel-api`
**Role:** The only crate that defines interfaces. No implementations live here.

The kernel provides **8 primitives**:
1. **Identity** — DID operations, signing, verification
2. **Authorization** — Object-capabilities, policy oracles
3. **State** — Logs, blobs, KV storage
4. **Compute** — WASM execution, scheduling
5. **Communication** — Pub/sub, request/response, streams
6. **Time** — Logical clocks, scheduling, leases
7. **Coordination** — Consensus groups, CRDTs
8. **Naming** — Name resolution, service discovery

Key exports: `ServiceRegistry`, `ProtocolParameterStore`, `GovernanceExecutor` (adapter for kernel/app boundary), `PolicyOracle`, `EscrowStore`, `BudgetStore`

---

## Layer 2: Core Infrastructure Crates

### `icn-core`
**Role:** Actor runtime and supervisor

**Status:** `infected` (medium severity, actively being cleaned)

Modules: `anti_entropy`, `apps`, `config`, `dead_letter`, `events`, `identity`, `node`, `policy`, `replication`, `resource_enforcer_actor`, `restart`, `runtime`, `services`, `storage_challenge`, `supervisor`, `trust_propagation`, `upgrade`, `upgrade_actor`

Key exports: `Runtime`, `Supervisor`, `EventBus`, `AppBuilder/AppHandle/AppRuntime`, `DeadLetterQueue`

**Remaining infections being extracted:**
- governance_handlers: 43 refs → `apps/governance` (icn-governance-actor)
- ledger refs: 31 → `apps/ledger` (icn-ledger-actor)
- CCL refs: 32 → future extraction

### `icn-store`
**Role:** Storage abstraction (Sled backend)

### `icn-encoding`
**Role:** Versioned serialization for store-level data

### `icn-obs`
**Role:** Observability — Prometheus metrics, OpenTelemetry tracing

### `icn-rpc`
**Role:** JSON-RPC over HTTP transport

### `icn-time`
**Role:** Logical clocks, scheduling, leases

### `icn-snapshot`
**Role:** State snapshot and restore

### `icn-testkit`
**Role:** Testing utilities shared across workspace

### `icn-http-kit`
**Role:** Shared HTTP scaffolding — auth middleware, error formatting, pagination, validation

---

## Layer 3: Identity & Cryptography

### `icn-identity`
**Role:** DID management, key generation, cryptographic operations

**Two DID types:**
- Traditional: `did:icn:<base58pubkey>` (Ed25519)
- SDIS Anchor: `did:icn:<anchor-id>` (privacy-preserving, backed by VUI)

**Modules:** `anchor`, `backend_factory`, `batch_verify`, `bundle`, `commons`, `commons_store`, `did_signer`, `keybundle`, `keystore`, `keystore_backend`, `multi_device`, `personhood`, `personhood_store`, `recovery`, `revocation`, `revocation_store`, `sync`, `vui`

**Optional features:**
- `hsm` — PKCS#11 hardware security module
- `tpm-experimental` — TPM 2.0 support
- `post-quantum` — PQ key bundles

**SDIS (Sovereign Digital Identity System):** Full Sybil resistance via threshold PRF + steward network. VUI = Verifiable Unique Identifier — proves uniqueness without revealing who you are.

### `icn-crypto`
**Role:** Core cryptographic primitives (Ed25519, X25519, ChaCha20-Poly1305)

### `icn-crypto-pq`
**Role:** Post-quantum cryptography (lattice-based, Kyber/Dilithium wrappers)

### `icn-zkp`
**Role:** Zero-knowledge proofs for SDIS

Three proof types:
- **Attribute proofs** — Prove age/citizenship/membership without revealing data
- **Non-revocation proofs** — Prove credential is not revoked
- **Compound proofs** — Combine attribute + non-revocation

Two accumulator backends:
- RSA accumulator (legacy, performance)
- Merkle accumulator (post-quantum safe)

### `icn-steward`
**Role:** Steward network for SDIS — the trust anchor layer

Stewards provide:
- **VUI Computation** — Threshold PRF (t-of-n distributed)
- **Enrollment Ceremonies** — Proof-of-personhood verification
- **Token Issuance** — Blind signatures for privacy-preserving enrollment
- **Key Recovery** — Social recovery through steward attestations
- **VUI Registry** — Distributed uniqueness checking

Architecture: Multiple stewards each hold a PepperShare, threshold computation ensures no single steward knows the full VUI.

---

## Layer 4: Network & Communication

### `icn-net`
**Role:** P2P transport layer

Transport: QUIC/TLS (quinn + rustls)
Discovery: mDNS local, STUN for NAT traversal, TURN for symmetric NAT fallback
Bootstrap: `icn://DID@IP:PORT` rendezvous format

### `icn-gossip`
**Role:** Topic-based message distribution (pub/sub across nodes)

Key topics used by other crates:
- `compute:submit`, `compute:claim` (icn-compute)
- `community:updates` (icn-community)
- Federation registry + attestation topics (icn-federation)
- Governance broadcast (icn-governance-actor)

### `icn-trust`
**Role:** Trust graph management with 3-dimensional scoring

**Three independent graphs** (prevent cross-domain clique capture):

| Graph | Purpose | Scoring |
|-------|---------|---------|
| **Social** | Peer endorsements, working relationships | 60% direct + 40% transitive |
| **Economic** | Payment history, credit behavior | 80% direct + 20% transitive |
| **Technical** | Node uptime, task success rates | (configurable) |

**Four trust classes (configurable in icn.toml):**
- `isolated` (<0.1) — treat as untrusted, 10 msg/s max
- `known` (0.1–0.4) — minimal trust, 50 msg/s
- `partner` (0.4–0.7) — working relationship, 100 msg/s
- `federated` (0.7+) — full federation trust, 200 msg/s

**Cache invalidation:** LRU with 5-minute TTL; edge A→B change invalidates B's score + all C where B→C exists (one-hop bounded).

### `icn-privacy`
**Role:** Metadata protection (Phase 20)

Week 1-2 (implemented): ChaCha20-Poly1305 encrypted topic names + Bloom filter discovery
Week 3-4 (planned): Onion routing (multi-hop, Tor-like)
Week 5-6 (planned): Traffic obfuscation (random delays, size padding, cover traffic)

### `icn-gateway`
**Role:** REST + WebSocket API for cooperative applications

**What it provides:**
- DID-based authentication + capability tokens
- Coop namespace isolation
- Ledger operations (balance, transfers, history)
- WebSocket event streaming

**What it is NOT:** An app runtime. External apps call this HTTP API.

---

## Layer 5: Economic Engine

### `icn-ledger`
**Role:** Double-entry mutual credit ledger with Merkle-DAG

**Design:** Append-only journal entries, each content-addressed with parent links.

**Double-entry invariant:** Σ debits == Σ credits per currency (enforced at kernel level)

**Multi-currency:** Hours, USD, kWh, and custom unit currencies all in one ledger

**Credit limits:** Per-participant, per-currency overdraft limits

**Merkle-DAG:** Tamper-evident, content-addressable, independently verifiable

### `icn-ccl`
**Role:** Cooperative Contract Language — DSL for cooperative agreements

**Properties:**
- **Deterministic** — same inputs always produce same outputs
- **Capability-based security** — explicit permissions for ledger access
- **Fuel metering** — bounded execution, prevents infinite loops
- **Not Turing-complete** — safe subset intentionally

**Example use:** TimeBank contract — `record_service(recipient, hours)` triggers debit/credit pair with validation

### `icn-protocol`
**Role:** Protocol state machine (wire format, message types)

---

## Layer 6: Governance

### `icn-governance`
**Role:** Governance substrate for decentralized decision-making

**Philosophy:** Democratic by default, configurable by communities, extensible via contracts

**Core types:**
- `GovernanceDomain` — A decision space (org, coop, community)
- `Proposal` — Title, description, payload (any action)
- `Vote` — for / against / abstain
- `GovernanceProfile` — Rules for evaluating votes (quorum, threshold, method)
- `MembershipConfig` — Who is eligible to vote

**Modules:** `amendment`, `appeal`, `charter`, `charter_store`, `config` + more

**Charter:** The governance constitution — TOML format, defines rules for a specific coop/community type

### `icn-authz`
**Role:** Capability graph model (app-layer — interprets domain semantics)

Types: `SubjectId`, `Action`, `ResourceId/Kind`, `Constraint`, `CapabilityEdge`, `CapabilityGraph`, `Decision`

The kernel sees only hashes from this crate, never the capability model itself (Meaning Firewall compliant)

---

## Layer 7: Entity Model

**Architecture principle:** Cooperatives (economic engine) and Communities (civic/political engine) are **co-equal** — neither is subordinate. Federation bridges both without owning either.

**Membership model:** A person can be a member of a cooperative, a community, or both simultaneously. Membership is non-exclusive. By extension, a person can join a federation directly (federations are composable entities). There is no forced path through either engine.

```
Person ──── member of ──→ Cooperative (economic: labor, capital, ledger, treasury)
       └─── member of ──→ Community   (civic: shared governance, political voice, solidarity)
       └─── member of ──→ Federation  (by extension: cross-coop/cross-community coordination)
```

### `icn-entity`
**Role:** Unified recursive entity abstraction

**The insight:** Membership is recursive. A cooperative's members can themselves be cooperatives. A community's members can include both individuals and organizations.

**EntityId format:** `entity:icn:<type>:<identifier>`
- `entity:icn:individual:z5TrA8...` (wraps DID)
- `entity:icn:cooperative:food-coop-2024`
- `entity:icn:federation:midwest-fed`

**AccountId:** Backward-compatibility enum wrapping `Did` (legacy) or `EntityId` (new)

### `icn-coop`
**Role:** Cooperative actor, lifecycle, and membership management

**Modules:** `actor`, `handle`, `lifecycle`, `membership`, `store`, `types`

**Lifecycle:** Formation → Active → Dissolution (with asset distribution plan)

**Types:** `Cooperative`, `CooperativeId`, `Member`, `MemberRole`, `MemberStatus`, `MembershipApplication`, `MembershipTier`, `MembershipChange`, `FormationRequest`, `DissolutionRequest`

**Treasury anchor:** `derive_treasury_anchor()` + `derive_treasury_did()` — deterministic treasury identity from coop ID (no separate key required)

**Treasury:** `TreasuryManagerHandle` — used by coop actor for budget operations

### `icn-community`
**Role:** Communities — the civic engine (complements cooperatives as economic engine)

**Purpose:** Groups cooperatives and individuals for shared governance, resources, mutual support

**Architecture:**
```
CommunityHandle → CommunityActor → CommunityStore
                        |
                 GossipActor (community:updates)
```

**Modules:** `actor`, `error`, `handle`, `lifecycle`, `membership` + types

**Community types:** Geographic, Interest, Solidarity (others configurable)

### `icn-federation`
**Role:** Inter-cooperative coordination layer

**Capabilities:**
- **Discover** — gossip-based registry announcements
- **Bridge trust** — federated attestations (foreign DIDs get trust scores)
- **Settle credits** — cross-cooperative ledger transfers
- **Route gossip** — cooperative-scoped topic visibility
- **Resolve DIDs** — across federation boundaries

**DID format with federation routing:**
```
did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
        ^^^^^^^^^
        coop prefix routes to the right node
```

**Modules:** `error`, `gossip`, `metrics`, `registry`, `types`

### `icn-naming`
**Role:** Hierarchical naming service

**Namespaces (root paths):**
- `/cell/...` — individual/household
- `/org/...` — organizations/coops
- `/fed/...` — federations
- `/commons/...` — shared resources

**Features:** Authority-signature enforcement on register/update/delete, alias-aware resolution with bounded recursion

---

## Layer 8: Advanced Capabilities

### `icn-compute`
**Role:** Trust-gated distributed task execution

**Supported execution types:** CCL contracts, WebAssembly modules

**Topics:** `compute:submit`, `compute:claim`

**Architecture:**
```
Submitter → Gossip (routing) → Executor
              Trust Graph
            (access control)
```

**icnd feature flag:** `--features wasm` enables wasmtime for WASM execution

### `icn-security`
**Role:** Security utilities and attack detection

---

## Layer 9: Apps (runtime-integrated, `icn/apps/`)

### `icn-governance-actor` (apps/governance)
**Role:** Governance actor extracted from icn-core

Handles: proposal dispatch, committee voting, execution receipts, GovernanceProof signing

Uses: actix-web for HTTP handlers, blake3 for proof hashing, Sled for action item persistence

### `icn-ledger-actor` (apps/ledger)
**Role:** Escrow and budget store implementations

Implements `EscrowStore` + `BudgetStore` traits from `icn-kernel-api`

### `icn-membership-app` (apps/membership)
**Role:** Unified entity and membership management

Handles: coop lifecycle, member applications, charter management, treasury management

---

## Binaries

### `icnd` — The Substrate Daemon

**What it starts:** Actor runtime with governance, ledger, trust, net, gossip, RPC, and optionally: Gateway API, WASM executor

**Key args:**
```
--config <path>          Configuration file
--data-dir <path>        Data directory (~/.icn default)
--gateway-enable         Enable REST/WebSocket Gateway API
--gateway-bind <addr>    Gateway bind address
--gateway-jwt-secret     JWT signing key
--insecure-gateway-no-jwt  Dev only, skip JWT
--validate-config        CI/CD config validation
--tracing               OpenTelemetry distributed tracing
```

**Features:**
- `wasm` — wasmtime WASM execution
- `hsm` — PKCS#11 hardware key storage
- `tpm-experimental` — TPM 2.0

### `icnctl` — The CLI

**Command tree:**
```
icnctl
├── status                    Daemon health
├── id                        DID management (create, show, import, export)
├── device                    Multi-device identity (add, list, remove)
├── recovery                  Social recovery (setup, initiate, complete)
├── trust                     Trust graph (add-edge, query, list)
├── ledger                    Ledger ops (balance, transfer, history, dispute)
├── dispute                   Dispute management
├── contract                  CCL contract execution
├── network                   mDNS discovery, QUIC sessions
├── federation                Cross-network (join, leave, query, sync)
├── governance                Proposals, votes, domains
│   ├── domain create/list
│   ├── proposal create/vote/list
│   └── amendment/appeal
├── charter                   Organizational governance docs
├── amendment                 Constitutional governance changes
├── appeal                    Due process for governance decisions
├── auth token               Get Gateway JWT
├── compute submit-ccl        Distributed CCL execution
├── compute submit-wasm       Distributed WASM execution
├── scheduling               Cooperative scheduling policies
├── quota                    Resource quota management
├── steward                  SDIS steward network operations
├── commons                  Commons identity operations
├── backup/restore/verify    Data backup
├── snapshot                 State snapshots
├── coop-init               Guided cooperative setup wizard
├── preflight               Pre-deployment health checks
├── audit receipt-chain      Get receipt chain for decision hash
├── audit allocation-receipt Single allocation receipt by hash
├── audit settlement-intent  Single settlement by hash
├── api-schema               OpenAPI spec management
├── openapi                  Export OpenAPI spec
└── completions              Shell completion generation
```

**Notable flags:**
- `-d <dir>` — data directory (default `~/.icn`)
- `-e <addr>` — RPC endpoint (default `[::1]:5601`)
- `--features post-quantum` — PQ identity support

**i18n support:** `rust-i18n` + `sys-locale` — CLI messages in local language

**QR code support:** `qrcode` crate — for identity export/import (max 2000 bytes for reliable scanning)

### `icn-console` — The TUI

**Framework:** ratatui + crossterm (terminal UI)

**Tabs:**
1. **Dashboard** — Node overview, health stats
2. **Members** — Cooperative member list, roles, status
3. **Ledger** — Balances, transaction history
4. **Governance** — Active proposals, voting history
5. **Trust** — Trust graph visualization

**Auth:** Gateway JWT token via `--token` arg or interactive prompt

---

## Frontend Surfaces

### `web/pilot-ui/` — Timebank PWA (vanilla JS, 288KB app.js)

**Full name:** `icn-timebank-pilot-ui`

**Core screens:**
- Main app: `index.html` + `app.js` + `style.css`
- SDIS enrollment: `sdis-enrollment.html/js/css` (identity ceremony)
- SDIS identity viewer: `sdis-identity.html/js/css`
- SDIS proofs: `sdis-proofs.html/js/css` (ZKP credential presentation)
- SDIS recovery: `sdis-recovery.html/js/css` (social key recovery)
- Steward dashboard: `steward-dashboard.html/js/css`

**Other files:** `crypto.js`, `receipts.js`, `offline-storage.js`, `sw.js` (service worker)

**I18n:** `i18n.js` + `locales/` directory

**Testing:** Playwright E2E tests in `tests/`, Jest unit tests

### `web/dashboard/` — Static operator dashboard

### `sdk/typescript/` — `@icn/client` v0.1.0

TypeScript SDK for Gateway API. OpenAPI-generated types (`src/generated/api-types.ts`). Tests for WASM deploy/list, typed error mapping.

```bash
npm run generate-types  # Regenerates from openapi.generated.yaml
npm run check-types     # Verifies generated files in sync
```

### `sdk/react-native/` — React Native SDK

Mobile cooperative management. Components: `BudgetManager.tsx`, `CooperativeManager.tsx`, `NotificationCenter.tsx`, `RecurringPaymentSetup.tsx`, `VotingScreen.tsx`

---

## Demo System (`demo/`)

### Four Cooperative Personas (live K3s nodes)

| Coop | Type | Demo question |
|------|------|---------------|
| **BrightWorks Cooperative** | Worker | Why did this member get this patronage allocation? |
| **River City Tool Library** | Community | How does a shared-resource coop track contributions? |
| **Harbor Homes Cooperative** | Housing | Did the vote that authorized this $12K spend actually happen? |
| **Finger Lakes CDN** | Intermediate/Federation | Can a federation support member coops without owning them? |

### Four Demo Flows

| Flow | Script | Duration | Core question | Known gaps |
|------|--------|----------|---------------|-----------|
| **Flow 1: Governance** | `flow-1-governance.sh` | ~5 min | Did the vote that authorized this action actually happen? | treasury:write scope missing; proof endpoint 404 (PR #1327 ExecutionReceiptGate — MERGED) |
| **Flow 2: Patronage** | `flow-2-patronage.sh` | ~5 min | Why did this member get this distribution? | treasury:write scope missing |
| **Flow 3: Federation** | `flow-3-federation.sh` | varies | How do independent co-ops coordinate without a bureaucracy? | treasury:write, federation:write scopes |
| **Flow 4: Reporting** | `flow-4-reporting.sh` | varies | Can this produce trustworthy reporting without admin overhead? | treasury:read scope |

### Presenter Mode

```bash
./demo/scripts/present.sh   # Launches tmux two-pane layout
# Each flow: ./flow-N.sh --present   or   --narrated
# Pauses at _beat() calls for presenter to speak
```

### Rehearsal Probe System

```bash
./demo/scripts/reseed-federation-demo.sh  # Seed canonical state
./demo/scripts/rehearsal-probe.sh          # Five-unknown probe
# Expected output: "✔ CONFIRMED   5 / 5"
```

---

## Ops Layer (`ops/`)

### `ops/mcp/` — `@icn/ops-mcp` v0.1.0

MCP server for Claude Code sessions on icn-dev. Built with `@modelcontextprotocol/sdk` + `better-sqlite3` + `zod`.

**8 tool sets** in `ops/mcp/src/tools/`:
- sessions — track Claude Code sessions
- tasks — task management
- repos — repository state
- health — cluster health
- decisions — ADRs and decisions
- comms — communication log
- events — event tracking
- watchers — process watchers + zombie prevention

**Process poller** in `ops/mcp/src/polling/` — prevents zombie Claude Code processes

**State** in `ops/mcp/src/state/` — SQLite-backed persistent state

---

## Simulations (`sims/`)

### `sims/mutual-credit/` — Python agent-based economic simulation

**Framework:** Mesa (agent-based modeling)

**Purpose:** Validate mutual credit parameters before production deployment

**Tests:**
- Failure modes: hoarding, free-riding, credit limit gaming, velocity collapse
- Parameter tuning: credit limits, demurrage rates, trust dynamics
- Resilience: defaults, trust breakdown, external shocks

**Files:** `model.py`, `agents.py`, `trust.py`, `economy.py`, `run_simulation.py`, scenarios JSON, analysis outputs

---

## Examples (`examples/`)

| Example | Description |
|---------|-------------|
| `01-quickstart/` | Basic ICN node setup |
| `contracts/` | CCL contract examples |
| `governance-api/` | REST governance API usage |
| `mobile-app/` | React Native app (full cooperative management) |
| `wasm-compute/` | Rust WASM module for distributed execution |

---

## Configuration (`config/`)

**`icn.toml.example`** — Full config reference (9247 bytes):
- `data_dir`, `[network]`, `[observability]`, `[rate_limiting.*]`, `[topology]`, `[gateway]`, `[governance]`, `[ledger]`

**`icn.toml.example` highlights:**
```toml
[gateway]
enabled = false
bind = "0.0.0.0:8080"
# ICN_GATEWAY_JWT_SECRET env var required

[topology]
region = "local-dev"
cluster_id = "laptop-1"
role = "edge"  # or "validator", "bootstrap"

[topology.neighbor_limits]
max_local_cluster = 10
max_regional = 10
max_backbone = 5
```

**Devnet configs:** `icn-alpha.toml` (port 7777, RPC 5601), `icn-beta.toml` (port 7778, RPC 5602)

---

## GitHub CI Workflows (`.github/workflows/`)

| Workflow | Purpose |
|----------|---------|
| `ci.yml` | Main build/test/lint |
| `api-types.yml` | Verify OpenAPI types in sync |
| `security-audit.yml` | `cargo audit` + `cargo deny` |
| `benchmark.yml` | Performance benchmarks |
| `fuzz.yml` | Fuzzing (icn-ccl has fuzz target) |
| `claude.yml` | Claude Code integration |
| `claude-code-review.yml` | Automated code review |
| `docker-build-deploy.yml` | Container build + deploy |
| `release.yml` | Release automation |
| `npm-publish.yml` | Publish @icn/client SDK |
| `issue-label-enforcer.yml` | Label enforcement |

---

## Build Commands (`justfile`)

```bash
just build          # cargo build
just build-release  # cargo build --release
just test           # nextest or cargo test
just test-safe      # 2-job parallel (memory constrained)
just test-ci        # unit parallel, integration serial
just check          # fmt + clippy
just fmt            # cargo fmt
just clippy         # cargo clippy -D warnings
just devnet-up/down # 3-node devnet
just audit          # cargo audit + cargo deny
```

---

## What Doesn't Exist Yet (Charter Templates)

The `icn-governance` crate has a `charter` module and `charter_store`, but there is no `charters/` directory at the repo root with pre-built TOML charter templates for common coop types.

**Missing:**
- `charters/worker-coop.toml`
- `charters/event-planning.toml`
- `charters/consumer-coop.toml`
- `charters/community-land.toml`
- `charters/federation.toml`

These are Phase 2 Track A deliverables. They're the "governance constitution templates" that let someone do `icnctl coop-init --charter worker-coop` and get a real working governance setup.

---

## Current Deployment State

**K3s cluster on icn-dev (10.8.30.45):**
- 4 pods: icn-alpha, icn-beta, icn-gamma (+ delta)
- Gateway API accessible at `10.8.10.40:30080` (K3s NodePort)
- SDIS API: `http://10.8.10.40:30080/v1/sdis/health`

**Known gaps in K3s deployment:**
- `treasury:read/write` scopes not in `ALLOWED_SCOPES` → treasury API unreachable
- Proof endpoint returns 404 (signing key not configured in pod)
- PR #1327 (ExecutionReceiptGate) merged → Flow 1B will work once cluster is redeployed

**ops/mcp:** Running on icn-dev, accessible via `.mcp.json` in repo root

---

*Generated 2026-03-12 from live repo exploration. Update when new crates land or deployment state changes.*
