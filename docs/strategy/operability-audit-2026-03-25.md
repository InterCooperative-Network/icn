# ICN Operability Audit — 2026-03-25

Comprehensive audit of the ICN codebase against actual institutional operability,
not abstract architectural elegance. Every finding is tied to concrete files, tests,
routes, or observed behavior.

## Core Question

> What can a cooperative, tenant union, food distro, clinic, mutual aid network,
> town, or federation actually do with ICN today?

## Audit Methodology

Six-plane analysis across Identity/Trust, Economic Execution, Governance Execution,
Gateway/API, Federation, and User Adoption Surface. Each finding rated by evidence
weight, not aspiration.

---

## A. Identity / Trust

### Proven (live, tested)

- **Ed25519 DIDs** (`icn-identity/src/lib.rs:191-244`): `did:icn:<base58-pubkey>` —
  14 unit tests for creation, parsing, validation, serialization
- **Challenge-response auth** (`icn-gateway/src/auth.rs`): DID-based nonce signing,
  constant-time verification (timing attack resistant), 1-hour JWT tokens
- **Trust graphs** (`icn-trust/src/lib.rs:23-65`): Three separate graphs
  (social/economic/technical) with weighted computation
- **TrustPolicyOracle** (`icn-gateway/src/trust_mgr.rs:119-175`): Trust→rate-limit
  constraint conversion across the meaning firewall. Gossip + network rate limiting
  wired and tested
- **Signer abstraction** (`icn-identity/src/did_signer.rs`): `DidSigner` trait with
  software and hardware variants. Hardware backends feature-gated but interface is correct

### Gaps

| Gap | Severity | Evidence |
|-----|----------|----------|
| Trust doesn't gate ledger credit limits | High | `credit_policy.rs` exists but oracle not wired |
| Trust doesn't weight governance votes | High | Tally computation is unweighted |
| Federation trust bridging | Medium | Attestation types exist, not integrated to scoring |
| Hardware signer backends (PKCS11/TPM) | Medium | Feature-gated, zero integration tests |

---

## B. Economic Execution

### Proven (live, tested)

- **Journal entry lifecycle** (`icn-ledger/src/`): Create→hash(SHA-256)→store(Sled)→
  retrieve→gossip-sync. Proven by `gossip_sync.rs` tests
- **Double-entry balance computation** (`balance.rs`): Multi-currency, checked arithmetic,
  overflow protection. Inline tests verified
- **Mutual credit policy** (`credit_policy.rs`): Trust-weighted dynamic limits with
  new-member ramping (10h initial → full over 90 days or 50h cleared volume)
- **Settlement paths**: Direct-member (`settlement.rs:110`) and commons-scope
  (`settlement.rs:300+`). Deduplication via SHA-256 keyed prefix
- **Patronage accounting** (`patronage.rs:111-180`): Per-coop internal capital accounts
- **Budget system** (`treasury/budgets.rs`): Status tracking, notification thresholds,
  spending records. Stored in Sled
- **Gateway settlement endpoint** (`api/ledger.rs:56`): `POST /{coop_id}/settle` is
  functional and real

### Gaps

| Gap | Severity | Evidence |
|-----|----------|----------|
| Credit policy ceilings not exposed in API | High | `ledger.rs:43-44` TODO comment |
| Dedup set in-memory only | High | HashSet loses state on restart |
| Federation clearing unimplemented | High | Hard rejection at `settlement.rs:120-129` |
| Commons consumer balance not locked atomically | Medium | Race condition window in `settle_commons_receipt()` |

---

## C. Governance Execution

### Proven (live, tested)

- **Proposal state machine** (`proposal.rs`): Draft→Deliberation→Open→
  Accepted/Rejected/NoQuorum + Cancel/Veto/ForceClose. Complete and tested
- **22 proposal payload types**: Text, Budget, Allocation, Membership, ConfigChange,
  SchedulingPolicy, FreezeMember, UnfreezeMember, VetoProposal, ForceCloseProposal,
  RollbackLedger, DisputeResolution, Sdis, ProtocolUpgrade, ProtocolChange, Treasury,
  SurplusAllocation, ShareRedemption, BondIssuance, Federation, ResourceAccess, Charter
- **Delegation voting** (`delegation.rs`): Recursive with 12-hop depth limit,
  scope overlap detection
- **Tally computation** (`tally.rs`): `compute_tally_with_delegations()` with
  detailed delegation resolution
- **Charter hook** (`apps/governance/src/http/handlers.rs:1018-1028`):
  When a Charter proposal is accepted, `on_charter_accepted` callback fires —
  the ONE existing governance→execution bridge

### Critical Gap

**Accepted proposals don't execute their payloads.**

This is the single most important finding. When a cooperative votes to freeze a bad
actor (`FreezeMember`), nothing happens. When they approve a treasury disbursement
(`Treasury`), nothing happens. The vote result is stored, an event is emitted,
and the system continues unchanged.

The only exception is `Charter` proposals, which have a specific hook
(`CharterAcceptedHook` in `apps/governance/src/http/configure.rs:30`) that deploys
the charter document. Every other payload type is ceremonial.

| Gap | Severity | Evidence |
|-----|----------|----------|
| **No proposal→execution dispatcher** | **CRITICAL** | Only Charter has a hook; 21 other types are no-ops |
| Effect manifests are read-only | Medium | `effect_manifest.rs` describes but doesn't execute |
| No authorization on cancel/veto/force-close | Medium | Methods have no ACL checks |
| Tally not automatic | Medium | Caller must invoke tally + close separately |

---

## D. Gateway / API / Product Surface

### Proven (live)

- **50+ HTTP endpoints** (`server.rs:1055-1407`): All route to real handler
  implementations — health, auth, sessions, WebSocket, identity, members, coops,
  SDIS, ledger, rights, invites, compute, federation, trust, devices, notifications,
  governance, constitutional
- **Auth flow**: Challenge-response with DID signing → JWT (HS256, 1hr TTL).
  Constant-time verification. Scope-based authorization
- **WebSocket** (`api/websocket.rs`): Topic-based event streaming via
  `EventBroadcaster`
- **OpenAPI** (`openapi.rs`): Auto-generated Swagger UI at `/swagger-ui/`,
  40+ models registered. CI drift detection
- **TypeScript SDK** (`sdk/typescript/`): 82KB client wrapping all endpoints,
  auto-generated types, 12+ error classes

### Gaps

| Gap | Severity | Evidence |
|-----|----------|----------|
| Naming service has `unimplemented!()` panics | High | `api/names.rs` — 8 panic points |
| No gateway integration tests | Medium | Only 2 test files in `icn-gateway/tests/` |
| Credit ceilings not in balance response | Medium | TODO at `ledger.rs:43-44` |

---

## E. Federation / Multi-node

### Proven

- **Gossip core**: Announce/Push/Pull protocol with vector clocks and Bloom filters.
  Three-node convergence tested in `devnet_integration.rs`
- **Single-node runtime**: Actor lifecycle, supervisor, Sled persistence. 58 integration
  tests in `icn-core/tests/`
- **Federation types**: Registry, attestations, agreements, clearing, routing —
  all exist as modules in `icn-federation/`

### Assessment

Single-node path is solid. Federation is correctly designed but not yet integrated
at runtime. No multi-machine tests exist. This is the right order — local truth
before distributed theater.

---

## F. User Adoption Surface

### Proven

- **Pilot UI** (`web/pilot-ui/`): Complete SPA covering auth, dashboard, hours,
  history, members, governance, treasurer dashboard, SDIS enrollment, real-time
  WebSocket updates
- **SDKs**: TypeScript (complete), React Native (available)
- **Demo scripts**: 9 real bash scripts with actual curl calls against live cluster
- **Documentation**: API docs, user guides (Quick Start, Treasurer, Admin, FAQ),
  developer guides

### Gaps

| Gap | Severity | Evidence |
|-----|----------|----------|
| No self-service identity onboarding | High | Users need external DID generation help |
| No cooperative discovery | High | Invite-only; no directory |
| Users see raw DIDs, not names | Medium | Naming service stubs |

---

## Ranked Implementation Tranches

| Rank | Tranche | User-Facing Leverage | Architectural Centrality | Boundedness |
|------|---------|---------------------|-------------------------|-------------|
| **1** | **Governance→execution: FreezeMember** | Governance becomes operational | Proves the dispatch pattern for all 22 types | High — hook + one match arm |
| 2 | Credit policy ceiling API exposure | Members see borrowing limits | Connects policy to visibility | High |
| 3 | Naming service stub fix | Removes crash risk, enables name search | Peripheral | High |
| 4 | Gateway integration test suite | Catches composition regressions | Cross-cutting | Medium |
| 5 | General ProposalAcceptedHook dispatch | Scales governance→execution to all types | Foundational | Medium |

### Tranche 1 Selected: Governance→Execution Bridge (FreezeMember)

**Why:** It's the only tranche that determines whether governance has real institutional
power. Everything else improves existing capability; this one creates new capability.

**Concrete plan:**
1. Add `ProposalAcceptedHook` to `GovernanceContext` (`apps/governance/src/http/configure.rs`)
2. Dispatch hook on acceptance in `close_proposal` handler (`apps/governance/src/http/handlers.rs`)
3. Wire hook to `LedgerManager.freeze_member_with_metadata()` (`icn-gateway/src/server.rs`)
4. Test: hook fires with correct `FreezeMember` data on proposal acceptance

**Success condition:** When `POST /gov/proposals/{id}/close` finalizes a `FreezeMember`
proposal as `Accepted`, the target member is frozen in the cooperative's ledger with
governance provenance (proposal_id attached).

**What remains after:** Wire remaining 21 payload types. Priority order:
MembershipAction, Treasury, Budget, ResourceAccess.
