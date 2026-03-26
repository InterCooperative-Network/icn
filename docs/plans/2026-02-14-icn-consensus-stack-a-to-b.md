# ICN Point A → Point B: Civilizational Consensus Stack

**Date**: 2026-02-14 (revised)
**Repo**: ~/projects/icn (main @ `86677ac6`, clean)
**Workspace**: `icn/` — 31 library crates + 3 binaries + 2 app crates. 414K LOC Rust. ~5,060 tests.
**Meaning Firewall**: Enforced (kernel domain-agnostic)

---

## The Societal Coin Principle

> **Cooperatives are the economic engine. Communities are the civic engine. Federations bridge them. The universal commons holds them all.**

ICN has **one set of kernel primitives** — treasury, budgets, allocation, settlement, receipts, disputes, rights, scope, governance. Co-ops and communities are **CCL-governed policy profiles** over that shared substrate, optimized for different purposes.

| | Cooperative | Community |
|---|---|---|
| **Orientation** | Economics-led, governance-constrained | Governance-led, economics-funded |
| **Primary function** | Production, labor coordination, enterprise | Public services, civic provisioning, stewardship |
| **Treasury logic** | Revenue → costs → surplus → distribution | Dues/levies → service budgets → entitlements |
| **Budget categories** | Operations, capital, R&D, reserves, distributions | Housing, care, education, infrastructure, emergency, procurement |
| **Membership model** | Workers, consumers, producers, multi-stakeholder | Residents, citizens, participants |
| **Surplus handling** | Patronage refunds, reinvestment, capital accounts | Service expansion, reserve building, grant allocation |
| **Key receipt chains** | Revenue → Cost → Surplus → Distribution → Member accounts | Budget → Entitlement → Service access → Audit |
| **Characteristic disputes** | Labor/compensation, surplus allocation, capital returns | Service denial, entitlement eligibility, stewardship conduct |

**Same machinery. Different constitutions. The kernel doesn't know which it's running.**

Federations exist in four compositions:
1. **Economic federation** (co-op ↔ co-op): supply chains, trade networks, labor reciprocity
2. **Civic federation** (community ↔ community): regional public services, shared infrastructure
3. **Mixed federation** (co-op ↔ community): procurement, service delivery contracts, civic-economic coordination
4. **Universal commons** (all → `ScopeLevel::Commons`): global compute, shared public goods, commons-level governance

**Individuals** interact via mobile-first interfaces with hosted services, participate in governance of every entity they belong to, and engage in economics across all their affiliations. All nodes together form a **distributed computing cloud network**.

---

## 0) Executive Summary

**Point A**: The substrate is real — identity with durable rights, scoped trust, 16+ governance proposal types, double-entry mutual credit, receipt chain types with canonical hashing, QUIC/TLS with DID binding, topic-based gossip, 200+ gateway endpoints. The **type system is ~90% complete**. But:

- **Execution wiring is ~40-60%** — types exist throughout but runtime paths are broken or bypassed. The governance effect chain works in daemon mode but not in standalone gateway mode (which the pilot uses).
- **The civic half is structurally underweight** — `icn-coop` has treasury, budgets, spending rules, audit trails, capital tracking. `icn-community` has only `ResourcePool` (capacity units). Communities need the same economic machinery as co-ops, governed by civic constitutions.
- **Human surfaces are ~20% legible** — pilot-ui has 12 tabs but no scope awareness, no receipt visibility (except one hidden tab), requires CLI token paste for auth.
- **CoopWallet does not exist** — no mobile app in the repo. QR login is dead-ended.
- **The system is LAN-only** — mDNS discovery works, STUN/TURN modules exist, but NAT traversal is not wired into session establishment.

**Point B**: People participate across plural jurisdictions — economic cooperatives and civic communities — through scope-aware interfaces where they always know: where they are (scope), what law applies (constitution), what they can do (standing), what happened and why (receipts), and how conflict resolves (disputes + appeals).

**Primary Gap**: The economic half (co-ops) has real machinery. The civic half (communities) is structurally absent — same primitives need to be available under different policy profiles. And neither half is legible to a regular human.

**Strategy**: Ship legibility on top of existing substrate. Extend shared primitives to communities via CCL policy profiles. Don't fork code paths — surface, wire, and constitute.

---

## 1) Repo State Summary (Point A)

### 1.1 Kernel & Meaning Firewall

The meaning firewall is structurally enforced. Key kernel primitives:

| Primitive | Location | Purpose |
|-----------|----------|---------|
| `ScopeLevel` | `icn-kernel-api/src/scope.rs:45` | `Local(0) < Cell(1) < Org(2) < Federation(3) < Commons(4)` — ordinal hierarchy |
| `CellId` | `icn-kernel-api/src/scope.rs:162` | `[u8; 32]` blake3 deterministic cell addressing |
| `PolicyOracle` | `icn-kernel-api/src/authz.rs` | Apps translate domain semantics → `ConstraintSet` |
| `ConstraintSet` | `icn-kernel-api/src/authz.rs:155` | rate_limit, max_topics, max_message_size, custom HashMap |
| `CapabilityEngine` | `icn-kernel-api/src/authz.rs:736` | Bearer tokens: issue, check, delegate, revoke (NOT yet wired to app layer) |
| `CanonicalReceipt` | `icn-kernel-api/src/receipts.rs:77` | `canonical_hash()`, `receipt_id()`, `provenance()` — implemented by AllocationReceipt, SettlementIntent |
| `ProvenanceAnchors` | `icn-kernel-api/src/receipts.rs:36` | Receipt chain linking via `upstream_hashes` |

**Firewall status**: No domain imports in kernel crates. `TrustPolicyOracle` converts trust scores → `ConstraintSet` at the boundary. Kernel never sees "trust score" or "trust class."

**Key observation for the Societal Coin**: The kernel is already organization-type-agnostic. `ScopeLevel::Org(2)` doesn't distinguish co-op from community. `JurisdictionType` in the app layer has `Cooperative | Community | Federation | Network` — the duality is *permitted* but not *leveraged*. The machinery (treasury, budgets, settlement) that currently lives only in `icn-coop` + `icn-ledger` needs to be accessible to communities through the same kernel primitives, governed by different CCL policy profiles.

### 1.2 Identity & Rights

**Layered model is implemented and the personhood/membership distinction IS enforced:**

- **Layer 0 — PersonhoodAnchor** (`icn-identity/src/personhood.rs:71`): Permanent anchor, POP attestations (Weak/Strong/Verified via 8 methods), uniqueness proofs (VUI/biometric/social), key rotation chain with recovery signatures.
- **Layer 1 — CommonsHolderRecord** (`icn-identity/src/commons.rs:116`): Links anchor → holder. 6 `CommonsRight` variants (VoteInCommons, Petition, MinimalFuelFloor, DueProcess, ExitFreely, AuditAccess). `CommonsRights::restricted()` preserves MinimalFuelFloor + DueProcess + ExitFreely even when other rights are revoked. Code comment: *"A cooperative can revoke your membership. A cooperative cannot revoke your personhood."*
- **Affiliations** (`icn-identity/src/commons.rs:383`): Multi-jurisdiction membership with per-jurisdiction capabilities. `JurisdictionType` already distinguishes `Cooperative | Community | Federation | Network`.
- **Two MembershipCapability enums**: Commons-layer (6 variants, `icn-identity/src/commons.rs:529`) and entity-layer (9 variants, `icn-entity/src/membership.rs:648`). Neither wired to kernel `CapabilityEngine`.
- **Entity model**: `EntityId` with type prefixes (`entity:icn:{individual|cooperative|federation}:`). 7 `CoopType` variants, 5-state lifecycle (Forming→Active→Suspended→Dissolving→Dissolved).
- **Steward network**: VuiRegistry with Bloom filters, StewardProfile with bonds/jurisdiction, enrollment ceremony scaffolding.

**Key gap**: Scope level (`ScopeLevel`) is invisible to the gateway API. No endpoint returns scope context. A user cannot see "I'm operating at Org scope" or "what law applies here." No distinction between co-op standing and community standing in any UI surface.

### 1.3 Governance + Receipts

**16+ ProposalPayload variants** (`icn-governance/src/proposal.rs:396`): Text, Budget, Membership, ConfigChange, SchedulingPolicy, 5 emergency types (Freeze/Unfreeze/Veto/ForceClose/Rollback), DisputeResolution, SDIS, ProtocolUpgrade, ProtocolChange, Treasury (6 sub-ops), SurplusAllocation, ShareRedemption, Federation (7 sub-types).

**Voting**: VoteChoice (For/Against/Abstain), weighted (weight field, default=1), quorum + approval threshold. Delegation stored but NOT applied in tally.

**Receipt chain status** (revised after deep analysis):

```
                              DAEMON MODE          STANDALONE MODE
Proposal → Vote → Decision    ✅ Works              ✅ Works
Decision → Receipt             ✅ GovernanceProofV2   ❌ Returns None
Receipt → AllocationReceipt    ❌ SKIPPED             ❌ N/A
Allocation → SettlementIntent  ❌ SKIPPED             ❌ N/A
Settlement → LedgerEntry       ⚠️ Type mismatch       ❌ N/A
LedgerEntry → Provenance       ✅ Fields exist         ❌ N/A
Appeal → Receipt               ❌ No connection        ❌ No connection
```

**Key finding**: The governance effect execution chain (`KernelGovernanceExecutor` → `KernelTreasuryExecutor` → `LedgerService`) exists in daemon mode and preserves decision provenance on `JournalEntry`. But it **bypasses `AllocationReceipt`** — going straight from decision to ledger entry. In standalone gateway mode (current pilot), no receipts or effects are generated at all.

**Appeals**: Comprehensive (7 types, 8 grounds, 6 remedies, Filed→UnderReview→Hearing→Resolved) but disconnected from receipt chain — uses string `proposal_id`, not `decision_hash`.

**Societal Coin observation**: Governance proposal types are implicitly co-op-flavored (Treasury, SurplusAllocation, ShareRedemption). Community-specific proposal types are missing: service creation, entitlement rule changes, steward appointment, civic budget allocation, participatory budgeting. These should be expressible as CCL policy profiles over the existing proposal machinery, not hardcoded variants.

### 1.4 Economics + Ledger

**Double-entry mutual credit**: `JournalEntry` with `AccountDelta[]`, content-addressed via SHA-256, Merkle-DAG parents, witness policies (BFT), fork detection + quarantine. Decision provenance fields: `decision_receipt_id` + `decision_hash`.

**6 AssetTypes** (`icn-kernel-api/src/economics.rs:17`): Fungible, Consumable, Depreciating, Transformable, Service, Claim — with deterministic mapping to `TreasuryOperationType`.

**Treasury**: Full `TreasuryManager` with budgets, spending rules, velocity limits, audit trails, sled persistence. 7 `TreasuryProposalOperation` variants with nonce-based replay protection. Coop treasury DID derived deterministically via `blake3("icn:treasury:{coop_id}")`.

**Settlement**: Two paths — local via `SettlementEngine` (dedup in-memory only), federation via `ReceiptClearingManager` + `ClearingManager` with netting engine. Commons clearing is **STUBBED**.

**Societal Coin gap**: Treasury machinery lives in `icn-coop` and `icn-ledger`. Communities (`icn-community`) have only `ResourcePool` — capacity units, not financial. The treasury system needs to be **entity-type-agnostic**: any governed entity (co-op OR community) can have a treasury. The *policy* governing that treasury (what inflows are valid, what budget categories exist, what spending rules apply) comes from CCL. Currently treasury DID derivation uses `blake3("icn:treasury:{coop_id}")` — this should generalize to `blake3("icn:treasury:{entity_id}")`.

### 1.5 CCL + Contracts + Disputes

**CCL is two systems in one crate:**
1. **Execution engine** (works): Contract AST (Contract/Rule/Stmt/Expr/Value), 12 capabilities, fuel metering with presets, deterministic execution.
2. **Constitutional schema** (definitions only): Rich YAML types for Entity/Governance/Economics/Agreement — but not wired to runtime.

**CCL Schema already models the duality**: `EntityType` in `icn-ccl/src/schema/entity.rs` has `Cooperative | Community | Federation`. `EntitySubtype` has co-op variants (Worker, Consumer, Producer, MultiStakeholder) and community variants (Municipal, Regional, Sectoral, Purpose). `MembershipConfig`, `GovernanceSchema`, `EconomicsSchema` are all entity-type-aware. The infrastructure for expressing different policy profiles per entity type **exists in CCL schema** — it just isn't bridged to the runtime.

**Contract Registry** (`icn-ccl/src/registry.rs`): Deploy, resolve (name/version/hash), semver, deprecate, visibility (Private/Coop/Public), sled persistence. Missing: gossip distribution, true revocation.

**Three dispute/appeal systems (not unified):**
1. **CCL Disputes** (`icn-ccl/src/disputes.rs`): Automatic contract re-execution, trust penalties, mediator pool, gossip integration.
2. **Governance Appeals** (`icn-governance/src/appeal.rs`): Human due process, Filed→Hearing→Resolved, 8 grounds, 6 remedies, 3 scopes. No persistent store.
3. **Ledger Disputes** (`icn-ledger/src/dispute.rs`): Financial mediation, mediator judgment, `escalate_to_governance()` bridge. The only system with explicit governance escalation.
4. **Agreement Dispute Ladder** (`icn-ccl/src/schema/agreement.rs`): Schema-only (Negotiation→Mediation→Arbitration→Adjudication). No runtime.

Evidence models are incompatible across all four systems. Bridge: `AppealType::DisputeResolution` allows appealing a CCL dispute outcome.

### 1.6 Federation + Discovery + Naming

**Federation agreements**: 6 types (Trade, Credit, ResourceSharing, FederationMembership, LaborReciprocity, SharedServices), full lifecycle (Draft→Signed→Active→Expired/Terminated), amendment system.

**Societal Coin gap**: Federation currently assumes members are cooperatives. No explicit support for community-to-community federations (regional public service scaling), mixed co-op + community federations (civic-economic coordination), or the universal commons as a top-level federation. CCL `AgreementSchema` is rich (boundary protocols, credential recognition, dispute ladders) but NOT bridged to runtime.

**Discovery**: mDNS only (LAN scope). `NamingService`/`ScopedDiscovery` traits exist but not implemented. No DNS-SD, no bootstrap nodes, no DHT.

**NAT traversal**: STUN client, NAT type detection, TURN relay client all implemented (`icn-net/src/{stun,nat,turn}.rs`) but **NOT integrated** into QUIC session establishment. System is effectively LAN-only for P2P.

### 1.7 Human Surfaces

**pilot-ui** (`web/pilot-ui/`): 12 tabs/views in main SPA + 5 standalone SDIS pages. Login via manual JWT paste from CLI. QR login backend exists but no mobile client to scan. Hidden Receipt Chain Viewer tab can display chains if you know the hash. No scope awareness. No receipt links from actions.

**CoopWallet**: **Does not exist.** React Native SDK scaffolded (21 screens, Ed25519 + hybrid PQ crypto, offline queue) but no app ships.

**icnctl**: 32 command groups, 100+ subcommands, 10K LOC. Comprehensive power-user tool. Requires Rust toolchain to build.

**TypeScript SDK** (`sdk/typescript/`): 90+ methods across 19 domains. Full gateway coverage. Requires external `SignatureProvider` — no built-in key management.

**Gateway**: 200+ endpoints across 34+ domain groups. ~75% live with real logic, ~25% stub/partial.

**Societal Coin gap**: No UI surface distinguishes co-op vs community context. No "economic standing" vs "civic standing" display. No budget category labeling that reflects organizational type. The interface needs modes that reflect what you're operating: as a person, in a co-op, in a community, in a federation.

### 1.8 Security + Operations

**Strengths**: Three-layer security (TLS + SignedEnvelope + E2E), constant-time auth, Byzantine fault detection with monitoring alerts, post-quantum signatures (ML-DSA hybrid), social recovery, supply chain auditing.

**Threat model** (7 domains assessed + 3 institutional capture domains):

| Threat | Residual Risk | Priority |
|--------|---------------|----------|
| T1: State Capture | Governance primitives incomplete | Medium |
| T2: Capital Capture | Trust graph manipulation possible | Medium |
| T3: Sybil | Enrollment ceremonies not production-wired | **High** |
| T4: Coercion | No duress key/threshold signing | Medium |
| T5: Censorship | LAN-only, DNS dependency | **High** |
| T6: Dependency | 4 unmaintained sled deps | Medium |
| T7: Insider | Snapshots unencrypted, JWT secret not rotatable | Medium |
| T8: Civic Capture | Community governance manipulation — service denial as political weapon | **High** (no civic constraints exist yet) |
| T9: Economic Capture | Co-op supply chain coercion — federation clearing manipulation | Medium |
| T10: Federation Capture | Regional planning capture — treaty abuse | Medium |

**Phase 0 security risks**: QR session URL spoofing via `X-Forwarded-*`, snapshot plaintext at rest, gateway binding to `0.0.0.0`.

---

## 2) Vision Requirements (Point B as "what users can DO")

### 2.1 Person

- Create identity in browser or wallet, prove personhood, hold durable commons rights (VoteInCommons, Petition, MinimalFuelFloor, DueProcess, ExitFreely, AuditAccess)
- Join/leave cooperatives AND communities without losing rights; data portable on departure
- Inspect: all rights, affiliations (economic + civic), capabilities, receipt chains — in one view, distinguished by jurisdiction type
- Mobile-first: interact with hosted services, participate in governance, engage in economics

### 2.2 Cooperative (Economic Engine)

- Form entity, author economic constitution in CCL (surplus rules, capital accounts, procurement constraints, labor accounting)
- Operate treasury with revenue tracking, budgets (operations, capital, R&D, reserves, distributions), spending rules, audit trails
- Manage membership with standing/capability display, role-based access (Worker, Consumer, Producer, Multi-stakeholder)
- Run productive services, coordinate labor, manage supply chain relationships
- All under own enforceable economic rules; visible to members
- Key receipt chains: Revenue → Cost → Surplus → Distribution → Member accounts
- Characteristic disputes: labor/compensation, surplus allocation, capital returns, procurement

### 2.3 Community (Civic Engine)

- Form entity, author civic charter in CCL (service entitlements, stewardship rules, civic budgeting constraints, anti-capture protections)
- Operate treasury with civic budgets (housing, care, education, infrastructure, emergency, procurement from co-ops), participatory budgeting processes
- Manage entitlements: who gets what service, under what conditions, with what audit trail
- Appoint and audit stewards (service administrators with strict accountability)
- Provide public services, manage shared spaces, maintain civic infrastructure
- All under own enforceable civic rules; visible to participants
- Key receipt chains: Budget → Entitlement → Service access → Audit; Denial → Dispute → Appeal → Remedy
- Characteristic disputes: service denial, entitlement eligibility, stewardship conduct, civic budget allocation fairness

### 2.4 Federation (Scale Bridge)

Four composition types:

1. **Economic federation** (co-op ↔ co-op): supply chain coordination, trade agreements, labor reciprocity, bilateral clearing with netting
2. **Civic federation** (community ↔ community): regional public service scaling, shared infrastructure, joint civic governance
3. **Mixed federation** (co-op ↔ community): procurement (community buys from co-op), service delivery contracts, civic-economic planning
4. **Universal commons** (all → commons scope): global compute, shared public goods, commons-level governance, universal service guarantees

For all federation types:
- Form via CCL treaties with boundary protocols, credential recognition, dispute ladders
- Manage cross-org agreements with lifecycle (Draft→Signed→Active→Expired/Terminated)
- Clear credit across members (monetary for co-ops, capacity for communities, mixed for cross-type)
- Resolve cross-org disputes via escalation ladder (Negotiation→Mediation→Arbitration→Adjudication)
- Govern shared resources via member-org voting
- Exits are procedural, surviving obligations enforced
- Inspect: obligations, clearing, disputes, receipts, exit paths

### 2.5 Navigation & Legibility (Five Invariant Questions)

Every screen, every interaction, every receipt must answer:

1. **"Where am I?"** — scope context banner: jurisdiction type (co-op/community/federation), entity name, scope level
2. **"What law applies?"** — governing constitution/charter queryable, policy references at every action
3. **"What can I do?"** — standing/capability resolution with explanation ("You can vote because you're a Worker member" / "You receive housing service because you're an Active resident")
4. **"What happened and why?"** — receipt chain from any action back to policy authority, always navigable
5. **"How do I challenge this?"** — dispute/appeal path visible from any denial or adverse action

### 2.6 Distributed Compute

All nodes contribute to a distributed computing cloud:
- Trust-gated task placement (WASM registry, deterministic execution, verification receipts)
- Compute allocation via governance (market vs entitlement vs policy)
- Commons compute pool for public goods
- Co-ops can sell compute services; communities can provision compute as civic infrastructure
- Settlement receipts for all compute — economic (billing) and civic (entitlement accounting)

---

## 3) Gap Map (State → Vision)

### 3.1 Structural Gap: The Missing Civic Half

The single most important gap. The kernel primitives are organization-type-agnostic, but the *app layer* has built economic machinery (treasury, budgets, surplus, capital) only for cooperatives. Communities need the same primitives governed by different CCL policy profiles.

| What Exists (Co-op) | What's Missing (Community) | Kernel Primitive |
|---------------------|---------------------------|-----------------|
| `TreasuryManager` with budgets, spending rules, audit | No community treasury | `Treasury` (should be entity-agnostic) |
| `TreasuryProposalOperation` (7 variants) | No civic budget proposals | `ProposalPayload` (CCL-extensible) |
| `SurplusAllocation`, `ShareRedemption` | No entitlement allocation | CCL policy profile |
| `CoopType` (7 variants) | `CommunityType` (4 variants, stubs only) | `EntityType` |
| Capital tracking, member shares | No civic participation accounting | Ledger (same) |
| `ResourcePool` exists in `icn-community` | Not wired to treasury system | Bridge needed |

**Resolution**: Don't fork. Generalize `TreasuryManager` to work with any `EntityId`. Community-specific budget categories, entitlement rules, stewardship constraints, and anti-capture protections are expressed as CCL policy profiles.

### 3.2 Constitution Layer Gaps

| Gap | User Story Blocked | Missing | Phase |
|-----|-------------------|---------|-------|
| No rights dashboard endpoint | "Show me all my rights" | Composite endpoint aggregating commons rights + affiliation capabilities + trust constraints | 0 |
| Scope invisible in API | "Where am I?" | `ScopeLevel` not returned by any gateway endpoint | 0 |
| "What law applies?" absent | "What governs this space?" | Policy reference endpoint for a jurisdiction | 0 |
| No co-op vs community standing distinction | "Am I a worker or a resident here?" | Jurisdiction-type-aware capability display | 0 |
| Dual capability enums | "Consistent capability checking" | Two `MembershipCapability` enums, neither wired to kernel `CapabilityEngine` | 1 |
| No data portability on departure | "Export my data before leaving" | Member data export (ledger, trust edges, receipts) | 1 |
| No scope-aware capability resolution | "What can I do in this scope?" | `PolicyOracle` doesn't incorporate scope in evaluation | 1 |

### 3.3 Governance Layer Gaps

| Gap | User Story Blocked | Missing | Phase |
|-----|-------------------|---------|-------|
| No receipt in standalone mode | "View decision receipt" | `close_proposal` standalone doesn't generate `GovernanceDecisionReceipt` | 0 |
| No GovernanceDecisionReceipt storage | "Look up past decisions by hash" | ReceiptStore doesn't store governance receipts | 0 |
| No constraint provenance (#1012) | "Why is my rate limit 20/s?" | ConstraintSet has no policy_ref/authority/timestamp | 0 |
| No effect execution on approval | "Treasury spend passes → money moves" | Effect engine not triggered in standalone | 1 |
| SettlementIntent ≠ SettlementRequest | "Governance spend settles in ledger" | Type mismatch between governance and ledger paths | 1 |
| Delegation not applied in tally | "Delegate my vote" | Delegations stored but ignored in `close_proposal` | 1 |
| Appeals not linked to receipts | "Appeal decision 0xABC" | Uses string `proposal_id`, not `decision_hash` | 1 |
| No civic proposal types in CCL | "Propose a new community service" | Service creation, entitlement rules, steward appointment as CCL-governed proposals | 2 |

### 3.4 Economics Layer Gaps

| Gap | User Story Blocked | Missing | Phase |
|-----|-------------------|---------|-------|
| Transaction history lacks provenance | "What authorized this charge?" | `decision_receipt_id` not returned in transaction list | 0 |
| No "why did this fail?" explainer | "Transfer failed, why?" | Generic error strings, no structured explanation | 0 |
| Balance missing credit limits | "How much can I spend?" | Credit limit not returned with balance | 0 |
| Treasury not entity-agnostic | "Community needs a treasury" | Treasury DID derivation hardcoded to coop_id | 1 |
| Treasury not auto-created with entity | "Create coop/community with treasury" | Treasury creation not in entity lifecycle (#1139) | 1 |
| AllocationReceipt skipped in spend path | "Audit this spend" | Governance executor bypasses AllocationReceipt | 1 |
| Settlement dedup not persisted | "Restart without duplicates" | In-memory HashSet lost on restart | 1 |
| No entitlement receipt chain | "Why was my service access denied?" | `EligibilityReceipt → EntitlementGrant → ServiceAccessEffect → AuditReceipt` | 2 |
| Commons clearing stubbed | "Bill to commons pool" | Commons scope not processed | 3 |

### 3.5 CCL / Disputes Layer Gaps

| Gap | User Story Blocked | Missing | Phase |
|-----|-------------------|---------|-------|
| No human-readable policy display | "Read policies that govern me" | No rendering layer for CCL contracts/schemas | 0 |
| No "what governs this?" query | "What policy applies to transfers?" | No policy index by scope/action | 0 |
| CCL Schema → Runtime bridge absent | "Write governance in CCL, enforce it" | Constitutional schemas are definitions only | 2 |
| No civic CCL templates | "Bootstrap a community constitution" | Default policy profiles for community entity types | 2 |
| No general dispute filing API | "File a contract dispute" | `DisputeResolutionSystem.file_dispute()` not gateway-exposed | 2 |
| No unified evidence model | "See all evidence in one format" | 4 dispute systems use incompatible evidence types | 2 |
| Appeals have no persistent store | "My appeal survives restart" | In-memory only | 2 |
| Agreement schema ↔ federation bridge | "Author treaty in CCL" | Rich schema not connected to runtime agreements | 3 |

### 3.6 Interface Layer Gaps

| Gap | User Story Blocked | Missing | Phase |
|-----|-------------------|---------|-------|
| Auth requires CLI token paste | "Log in without terminal" | No in-browser key generation or challenge-response | **0** |
| No scope context banner | "Where am I?" | No jurisdiction/charter/role display on any screen | **0** |
| No co-op vs community distinction in UI | "Is this my workplace or my town?" | Entity type not displayed in navigation | **0** |
| Receipts not linked from actions | "What happened?" | Receipt chain hidden, not linked from governance/transactions | **0** |
| No multi-org navigation | "I'm in 3 cooperatives and 2 communities" | Login locked to single coop_id, no scope switcher | 1 |
| No standing/capability display | "What can I do here?" | No trust class, role, or capability panel | 1 |
| No delegation UI | "Delegate my vote" | CLI only | 1 |
| No mobile app | "Do this from my phone" | CoopWallet does not exist | 4 |

### 3.7 Ops Layer Gaps

| Gap | Threat | Missing | Phase |
|-----|--------|---------|-------|
| QR session URL spoofing | T7 | `X-Forwarded-*` trusted without validation | 0 |
| Snapshots unencrypted | T7 | TLS keys in plaintext JSON | 0 |
| No civic capture protections | T8 | Community governance anti-capture constraints | 2 |
| NAT traversal not wired | T5 | STUN/TURN exist but not in session setup | 5 |
| LAN-only discovery | T5 | No bootstrap nodes, no DHT, no DNS-SD | 5 |
| Sybil resistance incomplete | T3 | Enrollment ceremonies not production-wired | 5 |
| No system packaging | T5 | Must compile from source or use Docker | 5 |

---

# PHASED ROADMAP (Sequential)

## Phase 0: Legibility + Receipts Surface + Non-Dev Onboarding

**Goal**: A normal person on the LAN can log in without CLI tokens, see scope context (including whether they're in a co-op or community), view "constraints with receipts," and inspect receipt provenance for key actions.

**Strategic decision**: Run the pilot in **daemon mode** (icnd with GovernanceActor), not standalone gateway. This unlocks the existing effect execution chain (~60% connected) instead of rebuilding it.

### Phase 0 Definition of Done (LAN demo)

From a workstation on the LAN:
1. Access pilot-ui via gateway (not localhost-bound)
2. Log in via in-browser challenge-response (no CLI, no manual JWT paste)
3. See a **Scope Context Banner** on every screen: entity type (co-op/community) + entity name + charter + role + trust class
4. Open a **Receipts Explorer**: decision receipts + linked chain + "What/Why/Who/When"
5. Create proposal → vote → close → receipt visible with verification badge
6. See "What law applies here?" stub (charter + policy references for current scope)

### Phase 0 Execution Plan (22 Tasks)

#### Auth & Onboarding (3 tasks)

**P0-T01**: In-browser identity creation
- Add "Create Identity" button to login screen. Generate Ed25519 keypair via WebCrypto, derive DID, store encrypted in IndexedDB.
- Files: `web/pilot-ui/index.html`, new `web/pilot-ui/crypto.js`, `web/pilot-ui/app.js`
- Test: User creates identity and sees DID — all in browser, zero CLI
- Demo: Click "Get Started" → identity created → DID displayed

**P0-T02**: In-browser challenge-response auth
- Replace JWT paste with automatic flow: enter DID → fetch challenge → sign with stored key → receive JWT.
- Files: `web/pilot-ui/app.js` (login flow), `web/pilot-ui/crypto.js` (signing)
- API deps: `POST /v1/auth/challenge`, `POST /v1/auth/verify` (both exist)
- Test: User enters DID + passphrase → JWT acquired automatically
- Demo: Login without CLI

**P0-T03**: Non-developer onboarding wizard
- 3-step wizard: Create Identity → Join via Invite Code → Auto-login → Dashboard.
- Files: `web/pilot-ui/index.html` (wizard overlay), `web/pilot-ui/app.js` (state machine)
- API deps: `POST /v1/invites/join` (exists), P0-T01, P0-T02
- Test: Non-developer opens URL → completes wizard → sees dashboard
- Demo: End-to-end onboarding in <60 seconds

#### Scope & Legibility (4 tasks)

**P0-T04**: Scope context banner
- Persistent header showing: **entity type** (Cooperative / Community), entity name, jurisdiction type, active charter, user's role, trust class.
- Files: `web/pilot-ui/index.html` (banner HTML), `web/pilot-ui/app.js` (fetch on login), `web/pilot-ui/style.css`
- API deps: `GET /v1/coops/{coop}`, `GET /v1/charter?coop_id=`, `GET /v1/trust/score/{did}` (all exist)
- Test: After login, banner shows "Timebank Coop [Cooperative] | Cooperative Charter v1 | Worker | Trust: Known (0.35)"
- Societal Coin: Banner must show entity TYPE prominently so users always know if they're in an economic or civic context

**P0-T05**: "What law applies here?" stub
- Extend scope banner with clickable charter link → charter detail view. Show policy ID + version + authority + last updated.
- Files: `web/pilot-ui/index.html`, `web/pilot-ui/app.js`
- API deps: `GET /v1/charter/{id}` (exists)
- Test: Banner shows "Governed by: Cooperative Charter v1 (Active)" → click → detail

**P0-T06**: Add scope_level to gateway API responses
- Add `scope_level` and `jurisdiction_type` fields to relevant gateway responses (commons, membership, governance).
- Files: `icn-gateway/src/api/commons/mod.rs`, `icn-gateway/src/api/membership/mod.rs`
- Test: `GET /commons/holder/{did}` includes `scope_level` and `jurisdiction_type` per affiliation

**P0-T07**: Rights summary endpoint
- `GET /v1/rights/{did}` returning: baseline_rights, per-jurisdiction capabilities **grouped by jurisdiction type** (cooperatives vs communities), trust-derived constraints.
- Files: New `icn-gateway/src/api/rights.rs`, `icn-gateway/src/server.rs`
- Test: Single call returns complete rights picture for a DID, separated by economic vs civic affiliations

#### Receipts Infrastructure (5 tasks)

**P0-T08**: Ensure pilot runs in daemon mode
- Verify icnd + GovernanceActor produces `GovernanceDecisionReceipt` on `close_proposal`. Document daemon-mode deployment for pilot.
- Files: `deploy/` configs, `docs/` deployment guide
- Test: Close proposal via daemon → `GET /proposals/{id}/proof` returns valid receipt

**P0-T09**: Store GovernanceDecisionReceipt in ReceiptStore
- Extend `receipt_store.rs` to store/index GovernanceDecisionReceipt by `decision_hash` and `proposal_id`.
- Files: `icn-gateway/src/receipt_store.rs`, `icn-gateway/src/api/receipts.rs`
- Test: Put receipt → get by hash → get by proposal_id → all return same receipt

**P0-T10**: Wire close_proposal → ReceiptStore
- After GovernanceDecisionReceipt generation, store in ReceiptStore.
- Files: `icn-gateway/src/governance_mgr.rs`, `icn-gateway/src/server.rs` (inject dependency)
- Test: Create → vote → close → receipt stored → queryable via API

**P0-T11**: Receipt Explorer — unhide and link from governance
- Unhide receipts tab. Add "View Receipt Chain" button on closed proposals pre-populated with decision hash.
- Files: `web/pilot-ui/app.js` (proposal render), `web/pilot-ui/receipts.js` (public API), `web/pilot-ui/index.html`
- API deps: `GET /v1/receipts/chain/{hash}`, `GET /v1/gov/proposals/{id}/proof` (both exist)
- Test: Close proposal → "View Receipt Chain" button → full chain displayed

**P0-T12**: Receipts Explorer data contract
- Unified chain endpoint returning `{ decision: GovernanceDecisionReceipt, allocations: [], intents: [], ledger_entries: [] }` — even if chain is empty, structure is consistent.
- Files: `icn-gateway/src/api/receipts.rs` (extend chain endpoint)
- Test: API contract test with verification badge based on `verify()` result

#### Constraints with Receipts (#1012) (3 tasks)

**P0-T13**: Constraint provenance stub
- Add `policy_domain: Option<String>` and `evaluated_at: Option<u64>` to `PolicyDecision`. TrustPolicyOracle populates these.
- Files: `icn-kernel-api/src/authz.rs`, `apps/trust/src/oracle.rs`
- Test: PolicyDecision includes `policy_domain = "trust"`, `evaluated_at = timestamp`

**P0-T14**: "My Constraints" dashboard card
- Show active constraints (rate limit, trust class, credit limit) with source attribution.
- Files: `web/pilot-ui/index.html` (dashboard card), `web/pilot-ui/app.js`
- API deps: `GET /v1/trust/score/{did}` (exists), P0-T13 for provenance
- Test: Dashboard shows "Rate Limit: 20 msg/sec | Source: Trust Policy | Score: 0.35"

**P0-T15**: Policy reference in governance responses
- Add `policy_ref: { contract_hash, version, authority_did }` to governance response DTOs.
- Files: `icn-gateway/src/api/governance.rs`
- Test: Proposal response includes policy reference when available

#### Economics Legibility (3 tasks)

**P0-T16**: Transaction history with provenance
- Extend `GET /v1/ledger/entries` to include `decision_receipt_id` and `decision_hash`.
- Files: `icn-gateway/src/api/ledger.rs`
- Test: Transaction list includes governance origin when applicable

**P0-T17**: Balance + credit limit display
- Extend `GET /v1/ledger/balances/{did}` to include credit limit information.
- Files: `icn-gateway/src/api/ledger.rs`
- Test: Balance response includes `credit_limit` field

**P0-T18**: Transfer failure explainer
- Return structured error responses with machine-readable codes and human explanations.
- Files: `icn-gateway/src/error.rs`, `icn-gateway/src/api/ledger.rs`
- Test: Transfer exceeding credit limit returns `{ "code": "credit_limit_exceeded", "explanation": "..." }`

#### Security & Ops (3 tasks)

**P0-T19**: QR session hardening
- Pin `GATEWAY_BASE_URL` env var for demo. Validate `X-Forwarded-*` headers come from trusted proxy.
- Files: `icn-gateway/src/api/sessions.rs`
- Test: QR data contains pinned URL, not header-derived URL

**P0-T20**: LAN accessibility
- Auto-detect gateway URL from `window.location.origin` (pilot-ui served from gateway). Remove localhost assumptions.
- Files: `web/pilot-ui/app.js` (URL detection), `web/pilot-ui/index.html`
- Test: Access from `http://192.168.1.X:8080` works without manual URL change

**P0-T21**: Operational monitoring for demo
- Verify ServiceMonitor alerts active for: failed signatures, Byzantine quarantine, auth failures. Document monitoring access.
- Files: `deploy/` configs
- Test: Simulated violation triggers alert

#### Contract Legibility (1 task)

**P0-T22**: Contract summary endpoint
- `GET /v1/contracts/{hash}/summary` returning structured human-readable JSON (name, version, participants, rule names, capabilities).
- Files: New handler in `icn-gateway/src/api/contracts.rs`
- Test: Deploy contract → get summary → readable output

### Phase 0 Demo Script

```bash
# Prerequisites: icnd running in daemon mode on LAN

# 1. Open pilot-ui from another machine
open http://<gateway-ip>:8080

# 2. Click "Get Started" → Create Identity → download keystore
# 3. Enter invite code (pre-generated by admin)
# 4. Auto-login → Dashboard with scope banner

# 5. Verify scope context
# Banner shows: "Timebank Coop [Cooperative] | Charter v1 | Worker | Trust: Known"
# Entity type badge clearly shows this is an economic entity

# 6. View "My Constraints" card
# Shows: Rate Limit, Trust Class, Credit Limit with sources

# 7. Click "What law applies?" in banner
# Shows charter detail with version, authority, governance domain

# 8. Create a proposal (Governance tab)
# Submit text proposal → open for voting → vote For → close

# 9. View receipt chain
# Click "View Receipt Chain" on closed proposal
# Receipt Explorer shows: GovernanceDecisionReceipt with verify badge

# 10. View transaction history
# Ledger tab shows transactions with provenance links where applicable

# 11. Transfer test
# Make a transfer → success with receipt
# Attempt over-limit transfer → structured error with explanation
```

### Phase 0 → Phase 1 Exit Checklist

- [ ] In-browser identity creation works (no CLI needed)
- [ ] Challenge-response auth works from LAN workstation
- [ ] Scope context banner renders on every pilot-ui screen with entity type badge
- [ ] At least one "Constraints with Receipts" card exists (#1012 pattern)
- [ ] Receipt Explorer shows GovernanceDecisionReceipt → chain
- [ ] "What law applies here?" shows charter + policy references
- [ ] Gateway binds to LAN-accessible address
- [ ] Demo script runs end-to-end without manual intervention
- [ ] icnd runs in daemon mode with GovernanceActor for pilot
- [ ] No new `unwrap`/`expect` in protocol paths
- [ ] Meaning Firewall: no domain imports in kernel crates

---

## Phase 1: Scope-Aware Governance + Treasury + Membership End-to-End

**Goal**: Complete one real "government with receipts" loop: identity → standing → proposal → vote → decision → treasury spend → settlement → audit — scoped, legible, and multi-org ready. Begin generalizing treasury for the civic half.

### Phase 1 Definition of Done

1. User selects scope (Org vs Federation) via scope switcher — sees both co-ops and communities listed
2. User sees standing/capabilities in that scope with explanation ("You can vote because..." / "You receive housing service because...")
3. User creates treasury spend proposal (in a co-op) or civic budget proposal (in a community, if community treasury exists)
4. Proposal passes governance → treasury spend executes → AllocationReceipt + SettlementIntent + LedgerEntry
5. Complete receipt chain visible in UI
6. Auditor view shows "why" (policy reference at each step)

### Phase 1 Deliverables

#### 1.1 Standing / Capability Resolution (3 tasks)
- Capability set endpoint: `GET /v1/capabilities/{did}/{jurisdiction_id}` returning roles, capabilities, trust class, rate limits
- **Jurisdiction-type-aware display**: co-op standing shows economic roles (Worker, Producer); community standing shows civic roles (Resident, Steward)
- If denied: "What would grant standing" (membership, trust threshold)

#### 1.2 Treasury Generalization + Spend Execution (5 tasks)
- **Entity-agnostic treasury**: Generalize `TreasuryManager` to work with any `EntityId` (not just coop_id). Derive treasury DID from `blake3("icn:treasury:{entity_id}")`
- Treasury-entity auto-creation: derive treasury in entity lifecycle (co-ops AND communities)
- AllocationReceipt in spend path (#1141): governance executor creates AllocationReceipt + SettlementIntent
- Concrete `submit_treasury_entry()`: implement on Ledger actor
- Surplus distribution execution: N journal entries per member

#### 1.3 Multi-org Affiliation (2 tasks)
- Affiliations home screen: show all orgs **grouped by type** — "My Cooperatives" (economic standing) + "My Communities" (civic standing)
- Scope switcher: switch entity context without full logout

#### 1.4 Governance Appeals First Surface (2 tasks)
- "File Appeal" button on closed proposals with grounds selection
- Appeal receipt: resolution events produce receipts linked to original decision

#### 1.5 Receipt Chain Wiring (2 tasks)
- Delegation applied in vote tally
- Full provenance navigation: upstream/downstream traversal in receipt API

### Phase 1 → Phase 2 Exit Checklist

- [ ] Full governance loop: proposal → vote → decision → treasury spend → receipt chain → audit
- [ ] Treasury works for any entity type (co-op or community)
- [ ] Standing/capability resolution visible, jurisdiction-type-aware ("why you can/can't act")
- [ ] Multi-org scope switching works with co-op/community distinction
- [ ] Appeal entry point exists with receipt recording
- [ ] Receipt chain navigable end-to-end in UI
- [ ] AllocationReceipt in treasury spend path
- [ ] All Phase 0 items remain satisfied

---

## Phase 2: CCL as Living Law + Civic Primitives

**Goal**: Policy registry browsing, human-readable rendering, "what governs this?" surfaces. Community-specific CCL policy templates. Dispute filing MVP integrated with receipts. Entitlement receipt chain.

### Phase 2 Deliverables

#### 2.1 CCL Living Law (4 tasks)
- Human-readable policy rendering (`icn-ccl/src/render.rs`): AST/schema → natural language
- "What governs this action?" query: policy index by scope/action/entity
- Policy change receipts: deploy/deprecate/version changes produce `ArtifactReceipt`
- CCL Schema → Runtime bridge: map GovernanceSchema → proposal types + thresholds + quorum rules

#### 2.2 Civic CCL Templates (3 tasks)
- **Community constitution template**: default CCL policy profile for community entity types — civic budgeting rules, entitlement eligibility, stewardship appointment, anti-capture constraints
- **Co-op constitution template**: default CCL policy profile for cooperative entity types — surplus rules, capital accounts, labor accounting, procurement
- **Service entitlement model**: `EligibilityReceipt → EntitlementGrant → ServiceAccessEffect → AuditReceipt` chain, plus denial path: `DenialReceipt → DisputeFiling → AppealReceipt → RemedyEffect`

#### 2.3 Disputes & Appeals (4 tasks)
- General dispute filing API: expose `DisputeResolutionSystem` via gateway
- Unified evidence model: common `Evidence` type shared by all 4 dispute systems
- Appeal persistent storage: sled-backed
- SDIS pages integrated into main SPA

### Phase 2 → Phase 3 Exit Checklist
- [ ] Policies browsable and human-readable in UI
- [ ] "What governs this?" resolves for any scoped action
- [ ] Community constitution template compiles and enforces
- [ ] Entitlement receipt chain works (grant → access → audit)
- [ ] Dispute filing → mediation → resolution → receipt chain works
- [ ] Evidence chain-of-custody inspectable

---

## Phase 3: Federation as Contract

**Goal**: Federation generalization for all four composition types. Agreement authoring in CCL. Cross-org clearing for both monetary credit and civic resource pools. Naming/discovery primitive.

### Phase 3 Deliverables

#### 3.1 Federation Generalization (3 tasks)
- **Federation member abstraction**: federation membership accepts any `EntityId` (individual, cooperative, community)
- **Four federation composition types**: economic (co-op↔co-op), civic (community↔community), mixed (co-op↔community), universal commons
- Federation governance: member-entity voting on federation proposals, weighted by entity type or equal

#### 3.2 CCL Treaties (2 tasks)
- Federation agreement authoring in CCL: bridge `AgreementSchema` → runtime agreements
- Treaty runtime bridge: CCL boundary protocols → clearing terms + credential recognition + dispute ladders

#### 3.3 Cross-Org Clearing (3 tasks)
- Commons clearing engine: replace stub — handle both monetary credit and resource pool clearing
- Auto-flush background task for clearing batches
- **Mixed clearing semantics**: co-op→community (procurement), community→co-op (service contracts), resource pool↔mutual credit conversion rules

#### 3.4 Cross-Org Disputes + Discovery (2 tasks)
- Cross-org dispute escalation: agreement dispute ladder runtime (Negotiation→Mediation→Arbitration→Adjudication)
- Naming/discovery primitive: configurable bootstrap nodes, gossip-based discovery, human-readable entity naming

### Phase 3 → Phase 4 Exit Checklist
- [ ] Federation formed via CCL treaty with mixed membership (co-ops + communities)
- [ ] Cross-org credit clearing with receipts (monetary and resource pool)
- [ ] Federation governance works across entity types
- [ ] Naming/discovery works without DNS
- [ ] Dispute escalation across org boundaries

---

## Phase 4: Wallet Evolution

**Goal**: Signer → lightweight node → optional commons participation. Mobile-first identity + offline queue + affiliations. The individual's unified window into economic and civic life.

### Phase 4 Deliverables
- CoopWallet MVP (React Native): create identity, scan QR, sign challenges, view balance, vote
- **Dual-context mobile**: "My Cooperatives" (economic) + "My Communities" (civic) — same app, different contexts
- Mobile-first identity: signer → lightweight node evolution
- Offline queue + sync: write queue with conflict detection
- Mobile affiliations: multi-org view with push notifications per org
- Steward mobile enrollment: camera access for spatial proofs

### Phase 4 → Phase 5 Exit Checklist
- [ ] Wallet handles identity + signing + affiliations across co-ops and communities
- [ ] Offline queue works and syncs
- [ ] Personhood proof flow works mobile-first
- [ ] Commons rights accessible from wallet

---

## Phase 5: Resilience Hardening

**Goal**: NAT traversal, naming without DNS dependence, anti-censorship, institutional capture resistance, packaging/distribution.

### Phase 5 Deliverables
- NAT traversal end-to-end: wire STUN/TURN into QUIC session establishment
- DNS-independent naming: bootstrap node list, DHT, Tor hidden service support
- Anti-censorship relay: TURN with DID auth, onion routing (scaffolding exists in `icn-privacy`)
- **Anti-capture constraints**: civic governance manipulation resistance, economic coercion detection, federation capture prevention — encoded as CCL invariants
- Key recovery end-to-end test: multi-node social recovery under partition
- System packaging: .deb/.rpm, systemd unit, install script
- Supply chain security: reproducible builds, SBOM, sled migration
- Sybil resistance production wiring: enrollment ceremonies enforceable
- Threat model documentation: STRIDE analysis covering T1-T10, attack trees, incident response playbook

### Phase 5 Exit Checklist
- [ ] NAT traversal works (symmetric NAT tested)
- [ ] Naming works without DNS single point of failure
- [ ] Anti-censorship measures in place
- [ ] Anti-capture constraints protect both co-op and community governance
- [ ] Packaging: .deb/.rpm for nodes
- [ ] Key recovery works end-to-end
- [ ] Threat model documented and mitigations verified
- [ ] Sled migration plan or replacement

---

## Research Tracks (Cross-Phase)

> **Full research agenda with formalization targets, prototype targets, failure modes, dependencies, and pseudocode**: [`docs/plans/2026-02-14-icn-research-agenda.md`](2026-02-14-icn-research-agenda.md)

These are the "unknowns that block correctness" — 13 research threads that may unlock or reshape multiple phases. Each track includes core research questions, formal targets, prototype targets, and adversarial failure modes (see companion doc for full detail).

| # | Track | Key Question | Blocks | Priority |
|---|-------|-------------|--------|----------|
| 1 | **Kernel Constraint Model & Meaning Firewall** | What is the minimal kernel action interface (`ActionIntent` → `ConstraintSet` → `Effect` → `Receipt`) that enforces blindly while remaining sufficient? How do you prevent semantic capture? | P0 (kernel freeze), all phases | **Critical** |
| 2 | **Identity & Personhood Under Hostile Conditions** | What constitutes personhood? How do multi-device bindings + social recovery resist collusion, coercion, and Sybils? What's the minimal disclosure model for entitlement proofs? | P0 (onboarding), P1 (recovery), P5 (sybil) | **Critical** |
| 3 | **Scope & Jurisdiction Formal Model** | Is scope a tree, DAG, or lattice? How do cross-scope references resolve deterministically under partitions? How do scopes compose without mixing jurisdiction? | P0 (scope banner), P1 (multi-org), P3 (federation) | **Critical** |
| 4 | **Authorization, Delegations & Effective Rights** | How do rights compose across personhood + membership + roles + delegation + scope? How do revocations propagate? Can the UI always explain "why can/can't I do this?" with a proof trail? | P0 (rights summary), P1 (standing), P2 (entitlements) | **Critical** |
| 5 | **Governance Mechanisms & Receipts Lifecycle** | What canonical state machine do all governance models map onto? Where is the Meaning Firewall boundary in governance output? How are governance decisions themselves appealable? | P0 (receipts), P1 (effect execution), P2 (CCL governance) | **Critical** |
| 6 | **Economics: Mutual Credit, Treasuries, Settlement** | What kernel-level constraints maintain ledger integrity under partition? How do safety rails (dynamic limits, throttling) interact with governance changes? What is the minimal inter-scope settlement primitive? | P1 (treasury generalization), P2 (civic economics), P3 (clearing) | High |
| 7 | **CCL: Contract Semantics, Compilation, Determinism** | What is the minimal semantic core? How are time, randomness, external data controlled for reproducibility? What is the formal compile target? How do you make CCL explainable to non-technical users? | P2 (living law), P3 (treaty authoring) | High |
| 8 | **Disputes, Evidence & Appeals Across Scopes** | What unified dispute object model covers ledger, compute, and governance disputes? How does cross-scope escalation work? How do you prevent mediator cartel capture? | P2 (dispute MVP), P3 (cross-org escalation) | High |
| 9 | **Naming, Discovery & Navigable Provenance** | What replaces DNS? How do service endpoint ads get authenticated and trust-gated? What's the human navigation model for receipt provenance across scopes? | P3 (naming), P5 (DNS independence) | Medium |
| 10 | **Networking, NAT Traversal & Censorship Resistance** | What connectivity assumptions for real co-ops/communities (home NAT, CGNAT, mobile)? How is relay selection trust-governed? What's the censorship resistance posture? | P1 (real-world pilot), P5 (resilience) | High |
| 11 | **Receipts Datastore: Provenance Graph, Sharding, Query** | How do receipts link (DAG, per-scope log, or both)? What's the retention policy? How is the query surface exposed securely to mobile without cross-scope leakage? | P0 (explorer MVP), P1 (full chain), P2+ (scale) | High |
| 12 | **Compute Fabric: Placement, Verification, Disputes** | What is the minimal compute model preserving local sovereignty? What verification modes (single, multi-executor, optimistic)? How do compute receipts tie to economic settlement? | P3 (commons clearing), P5 (resilience) | Medium |
| 13 | **Federation Theory: Treaties, Cross-Scope Settlement, Escalation** | What is the minimal treaty object? How does a federation enforce treaties while preserving local sovereignty? What is the composition rule for federations of federations? | P3 (all deliverables), P5 (scale) | Medium |
| — | **Threat Model & Security Posture** (cross-cutting) | What is the explicit attacker taxonomy? How is TOFU justified and where must it be replaced? How do you test semantic capture as a security threat? | All phases | **Critical** |
| — | **Migration, Adoption & Mobile Explainability** (cross-cutting) | What is the minimal safe onboarding flow? What must UI always surface? What's the incremental adoption path for existing co-ops/communities? | P0 (demo), P1 (pilot), P4 (wallet) | High |

### Minimal Formal Core (from research agenda)

The companion document defines a **Minimal Formal Core** — kernel-level primitives that must be defined correctly early so everything else layers cleanly:

1. **Scope identity and resolution** — `ScopeID` + `ScopeLevel` + parent linkage + deterministic resolution rules
2. **Actor identity and key state** — DID format + signed operation envelope rules (rotation, recovery)
3. **Authorization evaluation contract** — deterministic `EffectiveRights(actor, scope, time)` → capabilities + explainable proof path
4. **Governance receipts lifecycle hooks** — receiptable state transitions + canonical receipt format for "decision produced" / "effect applied"
5. **Receipt canonicalization and anchoring** — canonical serialization, hashing, signature, and provenance edge rules
6. **Inter-scope agreement representation** — treaty envelope (kernel validates identity/scope/signatures/receipts; semantics evaluated by oracles)
7. **Naming and discovery primitives** — signed service endpoint ads with TTL, scope binding, trust gating

Pseudocode for scope resolution, effective rights computation, governance lifecycle receipts, receipt structure, and naming/discovery is provided in the companion document.

---

## Appendix A: Issue Triage Recommendations

### Keep (Core to Roadmap)

| Issue | Title | Phase | Rationale |
|-------|-------|-------|-----------|
| #1147 | EPIC: Vertical Slice | 0-1 | Tracks the end-to-end chain this plan builds |
| #1012 | Legibility Dashboards UX Spec | 0 | "Constraints with Receipts" — directly drives P0-T13/T14 |
| #1139 | B1: Treasury-Coop Integration | 1 | Generalizes to Treasury-Entity Integration |
| #1141 | B3: Allocation Receipt Chain | 1 | Receipt chain completeness |
| #1140 | B2: Asset Type Foundation | 1 | Asset metadata on JournalEntry |
| #1137 | A3: Membership Consolidation | 1 | Unify dual MembershipCapability enums |
| #1011 | Constitutional Genesis Documentation | 0 | Hard/meta invariant categorization for "what law applies" |
| #1145 | D1: Vertical Slice Integration Test | 1 | Integration verification |
| #1090 | PR8c: Fallback atomicity via CoopActor | 1 | Treasury nonce enforcement |
| #925 | Commons credit earning/spending | 3 | Commons clearing engine |
| #948 | Commons credits | 3 | Commons credit flow |
| #862 | Naming primitive | 3 | Human-navigable naming |
| #863 | Federation agreements | 3 | Agreement lifecycle |
| #1009 | Attestation model | 2 | Evidence/attestation shared model |

### Close (Superseded or Done)

| Issue | Title | Rationale |
|-------|-------|-----------|
| #25 | Emergency proposals | Implemented — 5 emergency ProposalPayload variants exist |
| #52 | Dispute escalation | Partial — DisputeResolution proposal variant exists; remainder tracked by Phase 2 tasks |
| #389 | Labor share operations | Implemented — SurplusAllocation + ShareRedemption variants exist |

### Repurpose

| Issue | Title | New Purpose |
|-------|-------|------------|
| #992 | Settlement receipt_hash in errors | Expand to "structured error explainer" (P0-T18) |
| #957 | Receipt settlement metrics | Roll into Phase 1 observability for receipt chain |
| #965 | Configurable credit formula via CCL | Defer to Phase 2 (CCL as living law — Track 7) |
| #1092-1094 | PR9a/b/c treasury endpoints | Roll into Phase 1 treasury generalization tasks |
| #1134 | E7 Commons compute governance | Defer to Phase 3 (federation economics — Track 12) |

---

## Appendix B: Non-Negotiables Enforcement Checklist

| Non-Negotiable | Verification | Current Status |
|---------------|-------------|----------------|
| **Meaning Firewall**: no domain imports in kernel | `/firewall-check` skill, `firewall-guard.sh` hook | ✅ Enforced |
| **Same primitives, different constitutions**: co-ops and communities share kernel machinery | Treasury/Budget/Receipt types are entity-type-agnostic | ⚠️ Treasury hardcoded to coop_id — needs generalization |
| **Federations are contracts** (CCL) | `AgreementSchema` + `icn-federation` agreement types | ⚠️ Types exist, not bridged; only co-op federation assumed |
| **Four federation compositions** | Economic, civic, mixed, universal commons | ❌ Only co-op federation exists |
| **Rights durable beyond org membership** | `CommonsRights::restricted()` preserves 3 core rights | ✅ Structurally enforced |
| **Receipts for everything** | `CanonicalReceipt` chain types | ⚠️ Types exist, runtime wiring ~40-60% |
| **CCL is living law** (human-readable) | `ContractRegistry` + rendering | ❌ Registry exists, no rendering |
| **Wallet-first UX** | CoopWallet | ❌ Does not exist |
| **Node modes explicit** | N/A | ❌ Not implemented |
| **Five invariant questions** always answerable | Scope, law, standing, receipts, disputes | ❌ None visible in current UI |

---

## Appendix C: Cross-Cutting Architecture Observations

### The "Last Mile" Pattern

Every subsystem exhibits the same pattern:
- **Types**: ~90% complete, well-designed, canonical-hashed
- **Storage**: Sled-backed stores exist for most types
- **API endpoints**: Gateway has routes, they work
- **Runtime wiring**: 40-60% — types exist but execution paths are broken, bypassed, or only work in daemon mode
- **Human surface**: 10-20% — data exists but isn't visible to users

This is good news: the work is **wiring and surfacing**, not **designing and building**.

### The Mode Gap

The most important implementation finding: the governance effect chain works in **daemon mode** (icnd with GovernanceActor + KernelGovernanceExecutor) but not in **standalone gateway mode** (current pilot default). Phase 0's cheapest path is to ensure the pilot runs in daemon mode.

### The Societal Coin in Code

The kernel is already organization-type-agnostic. The meaning firewall prevents it from knowing if it's running a co-op or community. The gap is at the **app layer**: treasury/budget/surplus machinery exists only in `icn-coop`. The fix is not to fork — it's to:
1. Make `TreasuryManager` entity-agnostic (Phase 1)
2. Express co-op vs community differences as CCL policy profiles (Phase 2)
3. Ensure federation supports all four composition types (Phase 3)

**One substrate. Two constitutional orientations. Four federation geometries. Infinite specific policies.**

### The Three-Layer Living Law Stack

```
┌─────────────────────────────────────────────┐
│  Layer 3: Federation Treaties                │
│  4 types: economic, civic, mixed, commons    │
│  CCL: AgreementSchema ←→ Runtime (not bridged)          │
├─────────────────────────────────────────────┤
│  Layer 2: Organizational Constitutions       │
│  Co-op: economic constitution (surplus, capital, labor)  │
│  Community: civic charter (services, entitlements, stewardship) │
│  CCL: GovernanceSchema + EconomicsSchema ←→ Runtime (not bridged) │
├─────────────────────────────────────────────┤
│  Layer 1: Executable Contracts               │
│  CCL: AST + interpreter ←→ ContractRegistry  │
│  (most complete layer)                       │
├─────────────────────────────────────────────┤
│  Kernel: ConstraintSet enforcement           │
│  ═══════ MEANING FIREWALL ═══════            │
│  Treasury, Budgets, Receipts, Settlement     │
│  (organization-type-agnostic)                │
└─────────────────────────────────────────────┘
```

---

## Appendix D: Agent Research Files

Detailed per-subsystem analysis (177KB total):

| Agent | File | Size | Key Finding |
|-------|------|------|-------------|
| Constitution / Rights | `docs/archive/2026/plans-scratch/constitution-rights.md` | 29KB | Personhood/membership enforced; scope invisible; dual capability enums |
| Governance / Receipts | `docs/archive/2026/plans-scratch/governance-receipts.md` | 35KB | Chain 60% in daemon, 30% in standalone; AllocationReceipt bypassed |
| Economics / Treasury | `docs/archive/2026/plans-scratch/economics-treasury.md` | 27KB | Types 90%, execution 60%; treasury-coop gap; commons clearing stubbed |
| CCL + Disputes | `docs/archive/2026/plans-scratch/ccl-disputes.md` | 30KB | CCL = engine + schema (unlinked); 4 dispute systems, no unification |
| Human Interface | `docs/archive/2026/plans-scratch/human-interface.md` | 31KB | Auth requires CLI; no scope; CoopWallet doesn't exist; 12 tabs, 0 scope-aware |
| Security / Ops | `docs/archive/2026/plans-scratch/security-ops.md` | 25KB | LAN-only; NAT code exists unwired; QR spoofable; 3-layer security sound |

---

## Appendix E: The Societal Coin — One-Liner Spec

> ICN has one treasury system, one receipt chain, one governance engine, one dispute path, one identity layer. Cooperatives and communities are CCL-governed policy profiles over that shared substrate — economics-led for co-ops, governance-led for communities. Federations bridge any combination. The universal commons holds them all. The kernel enforces constraints without knowing which side of the coin it's serving.
