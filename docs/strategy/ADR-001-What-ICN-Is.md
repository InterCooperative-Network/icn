# ADR-001: What ICN Is

**Status:** Proposed
**Date:** 2026-03-17
**Deciders:** Matt Faherty

---

## Context

ICN has accumulated two years of spec weight. The August 2025 protocol suite described nine protocols (Identity, Governance, Economic, DAG Storage, Federation Sync, Mesh Jobs, CCL, Security, Consensus). Various planning documents added a regenerative economic model (Mana), a compute marketplace, a tiered storage economy, a shared data transfer layer, and seven voting models. The codebase grew to 38 crates and 414K lines.

The problem is not that these things are bad ideas. The problem is that they obscure what ICN actually is. When you describe ICN by its applications — "it does governance, it does mutual credit, it does federation, it does compute" — it sounds like a product. Products get regulated. Products compete with other products. Products have feature lists and roadmaps and market positioning.

ICN is not a product. It is infrastructure. And infrastructure is defined by what it provides mechanically, not by what people use it for.

This ADR establishes what ICN is at the kernel level and formally descopes everything that belongs in the application layer.

---

## Decision

**ICN is a P2P constraint engine.** It provides eight mechanical primitives behind a hard architectural boundary (the Meaning Firewall) that prevents the kernel from acquiring semantic meaning. Applications translate human meaning into constraints. The kernel enforces constraints without understanding them.

### The Eight Primitives

These are the only things ICN does. Everything else is an application.

| Primitive | What it does | What it doesn't do |
|---|---|---|
| **Identity** | Create, sign, verify, resolve DIDs. Prove who you are with cryptography. | Know what a "member" is. Know what a "cooperative" is. |
| **Authorization** | Issue and check unforgeable capability tokens. | Know what the capabilities mean. Know why access was granted. |
| **State** | Append to logs. Store blobs. Read/write KV. Content-addressed, tamper-evident. | Know what the log entries represent. Know why data was stored. |
| **Compute** | Execute deterministic sandboxed programs with fuel metering. | Know what the programs are computing. Know what the results mean. |
| **Communication** | Publish/subscribe to topics. Request/response. Stream. Move bytes over QUIC/TLS. | Know what the messages mean. Know why they were sent. |
| **Time** | Logical clocks for causal ordering. Scheduling. Leases. | Know what events the clocks are ordering. Know what the schedules are for. |
| **Coordination** | Form consensus groups. Merge concurrent state with CRDTs. Resolve conflicts. | Know what the groups are deciding. Know what the conflicts are about. |
| **Naming** | Resolve human-readable names to machine addresses. Hierarchical namespace. | Know what the names represent. Know why something has that name. |

### The Meaning Firewall

The hard boundary between kernel and apps. Enforced structurally:

**Mechanism:** Apps implement `PolicyOracle`. The kernel calls `PolicyOracle.evaluate(request)` and receives a `ConstraintSet` — numeric rate limits, capability requirements, duration bounds, quotas. The kernel enforces the `ConstraintSet`. It never looks inside the `PolicyOracle` to understand why those constraints were chosen.

**Enforcement:** The dependency graph is one-way. Kernel crates never import app crates. CI checks this. `kernel_surface.toml` audits every kernel-touching crate for domain contamination.

**Consequence:** The kernel is infrastructure. Like TCP/IP. Like the Linux kernel. Like electricity. It has no opinions. Applications built on top of it carry their own regulatory and semantic obligations.

### What Is Formally Descoped From the Kernel

The following are **applications, not kernel features**. They may ship with `icnd` as bundled apps, or they may be separate projects. But they are not ICN:

| Thing | Status | Where it lives |
|---|---|---|
| Governance engine | Bundled app (`apps/governance`) | Uses State, Communication, Coordination, Authorization, Time |
| Mutual credit ledger | Bundled app (`apps/ledger`) | Uses State, Compute, Authorization |
| Membership/entity management | Bundled app (`apps/membership`) | Uses Identity, State, Authorization, Naming |
| Trust scoring | App crate (`icn-trust`) | Uses State, Compute → feeds PolicyOracle |
| Federation coordination | App crate (`icn-federation`) | Uses Communication, Naming, Authorization, Coordination |
| Mana economic model | **Killed.** Not in current iteration. | Was never implemented. |
| Compute marketplace | **Descoped.** Future app if needed. | Would use Compute, State, Authorization, Communication |
| Tiered storage economy | **Descoped.** Not kernel business. | Would use State, Compute |
| Shared data transfer protocol | **Descoped.** Communication primitive is sufficient. | Communication handles byte movement. |
| CCL pattern library | Future app work | CCL runtime is kernel (Compute). CCL contracts are apps. |
| Charter templates | Future app work | TOML configs that feed governance app |
| Patronage distribution | App logic in ledger | Uses Compute, State |
| Dispute resolution | Future app | Would use Governance, State, Coordination |

### The Kernel Crates

These are ICN:

| Crate | Primitive(s) |
|---|---|
| `icn-kernel-api` | All 8 (trait definitions only) |
| `icn-core` | Runtime, supervisor, actor lifecycle |
| `icn-net` | Communication (QUIC/TLS transport) |
| `icn-gossip` | Communication (pub/sub, anti-entropy) |
| `icn-store` | State (Sled KV, persistence) |
| `icn-rpc` | Communication (RPC layer) |
| `icn-gateway` | Communication (HTTP/WebSocket API) |
| `icn-crypto` | Identity (Ed25519, X25519, ChaCha20) |
| `icn-crypto-pq` | Identity (post-quantum) |
| `icn-encoding` | State (canonical serialization) |
| `icn-time` | Time (logical clocks, leases) |
| `icn-snapshot` | State (checkpoint/restore) |
| `icn-authz` | Authorization (capability graph) |
| `icn-obs` | (observability — Prometheus, tracing) |
| `icn-http-kit` | (HTTP scaffolding — auth, errors, pagination) |
| `icn-protocol` | Communication (wire format) |
| `icn-privacy` | Communication (encrypted topics, metadata protection) |
| `icn-security` | (security utilities) |
| `icn-testkit` | (testing harness) |

These are apps that run on ICN (they live above the Meaning Firewall):

| Crate | Uses primitives |
|---|---|
| `icn-identity` | Identity, State |
| `icn-trust` | State, Compute → PolicyOracle |
| `icn-ledger` | State, Compute, Authorization |
| `icn-ccl` | Compute (DSL runtime + interpreter) |
| `icn-governance` | State, Communication, Coordination, Authorization, Time |
| `icn-entity` | Identity, State, Naming |
| `icn-coop` | Identity, State, Authorization, Naming |
| `icn-community` | Identity, State, Authorization |
| `icn-federation` | Communication, Naming, Authorization, Coordination |
| `icn-naming` | Naming, State |
| `icn-compute` | Compute, Authorization, Communication |
| `icn-steward` | Identity, Coordination |
| `icn-zkp` | Identity, Compute |
| `apps/governance` | HTTP handlers for governance app |
| `apps/ledger` | HTTP handlers for ledger app |
| `apps/membership` | HTTP handlers for entity management |

### The CCL Boundary Question

CCL (Cooperative Contract Language) straddles the Meaning Firewall. The **runtime** — the interpreter, fuel metering, sandbox, capability enforcement — is a kernel component (it's the Compute primitive). The **contracts** — what the programs actually do, what they compute, what they enforce — are applications.

This is exactly analogous to: the Linux kernel provides `execve()` (Compute primitive). User programs are applications. The kernel doesn't know what `/usr/bin/python` computes. It just runs the process in a sandbox with resource limits.

CCL contracts should be thought of as app-layer programs running on the kernel's Compute primitive. The CCL interpreter is kernel. The contract code is apps.

---

## Consequences

### What becomes easier

**Regulatory positioning.** If someone asks "what does ICN do?" the answer is "it provides eight mechanical primitives." It doesn't do governance. It doesn't do payments. It doesn't do federation. Applications do those things using ICN's primitives. The regulatory obligation falls on the applications, not the infrastructure.

**Architectural clarity.** The 11 infected kernel crates have a clear remediation target: extract any domain logic into app crates. The test is simple — does this code need to know what a "vote" is, what a "credit" is, what a "cooperative" is? If yes, it's an app. Move it above the Meaning Firewall.

**Scope control.** No more feature accumulation in the kernel. Compute marketplace? That's an app. Storage economy? App. Data transfer protocol? The Communication primitive already moves bytes. Build what you need on top.

**Composability.** Other developers can build applications on ICN that we haven't imagined. A scheduling app. A resource-sharing app. A reputation system. A cooperative marketplace. An insurance pool. If the kernel is clean, anything that can be expressed as constraints on eight primitives can be built.

### What becomes harder

**Pitching to cooperatives.** Cooperatives don't care about eight primitives. They care about governance, accounting, and coordination. The pitch needs two layers: "ICN is infrastructure" (for regulators, funders, technical reviewers) and "these apps give you governance, accounting, and coordination" (for cooperative organizers).

**Bundled app maintenance.** The four bundled apps (governance, ledger, membership, trust) need to be maintained as first-class software even though they're "just apps." They are the reason anyone runs `icnd` today.

### What we'll need to revisit

**The `icn-core` infection.** 43 governance refs, 31 ledger refs, 32 CCL refs still in the core crate. This is the #1 technical priority. Each one is a Meaning Firewall violation.

**`icn-identity` and `icn-naming` classification.** These feel like kernel crates (identity and naming are primitives) but they implement domain-specific logic (DID formats, SDIS steward protocols, naming conventions). The traits are kernel. The implementations may need to stay as apps. Need to audit whether the current implementations contain semantic assumptions.

**The gateway.** `icn-gateway` routes HTTP requests to app handlers. It lives in the kernel layer but it knows about governance endpoints, ledger endpoints, etc. It needs to become app-agnostic — a generic HTTP gateway that apps register routes with, rather than a hardcoded set of domain endpoints.

---

## Action Items

1. [ ] Clean the 11 infected kernel crates — extract all domain logic to apps layer
2. [ ] Extract commons credit formula from Rust to CCL (#1308)
3. [ ] Audit `icn-identity` and `icn-naming` for semantic contamination
4. [ ] Make `icn-gateway` app-agnostic (apps register their own routes)
5. [ ] Update VISION.md and ARCHITECTURE.md to reflect this framing
6. [ ] Create two-layer pitch deck: "infrastructure" (for technical/regulatory audiences) + "apps" (for cooperative audiences)
7. [ ] Update `kernel_surface.toml` categories to match this ADR's kernel/app boundary
