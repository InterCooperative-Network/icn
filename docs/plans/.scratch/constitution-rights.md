# Constitution / Rights — Repo State & Gap Analysis

## 1. Current State

### 1.1 Identity Primitives

#### Layer 0: Cryptographic Root

| Type | Location | Description |
|------|----------|-------------|
| `Did` (newtype) | `icn-identity/src/lib.rs:177` | `did:icn:<base58-pubkey>` — validated on construction/deserialization. Ed25519 public key in multibase. |
| `KeyPair` | `icn-identity/src/lib.rs:308` | Ed25519 + optional ML-DSA post-quantum hybrid. Zeroizing secret bytes. |
| `Anchor` | `icn-identity/src/anchor.rs` | SDIS anchor — permanent identity root, survives key rotation. |
| `PersonhoodAnchor` | `icn-identity/src/personhood.rs:71` | Wraps `Anchor` + status lifecycle + POP attestations + key rotation history + uniqueness proofs. |
| `AnchorStatus` | `icn-identity/src/personhood.rs:98` | `Active | Suspended { reason, suspended_at, until, authority } | Revoked { reason, revoked_at, evidence, authority }` |
| `POPAttestation` | `icn-identity/src/personhood.rs:129` | Proof-of-personhood attestation: issuer DID, method, level, expiry, signature. |
| `POPMethod` | `icn-identity/src/personhood.rs:157` | 8 variants: `InPerson`, `Sponsored`, `Biometric`, `Ceremony`, `VideoCall`, `GovernmentId`, `WebOfTrust`, `Genesis` |
| `POPLevel` | `icn-identity/src/personhood.rs:232` | `Weak(1) | Strong(2) | Verified(3)` — ordinal, used for gating. |
| `UniquenessProof` | `icn-identity/src/personhood.rs:248` | Sybil resistance: VUI-based, biometric, or social graph proofs with steward attestations. |
| `UniquenessProofType` | `icn-identity/src/personhood.rs:264` | `Vui { vui_commitment } | Biometric { type, commitment } | SocialGraph { proof_hash }` |
| `KeyRotationRecord` | `icn-identity/src/personhood.rs:301` | Old key, new key, reason, authorization signature, optional recovery signatures. |
| `KeyRotationReason` | `icn-identity/src/personhood.rs:326` | `Scheduled | Compromise | DeviceChange | Recovery | Upgrade` |
| `RecoverySignature` | `icn-identity/src/personhood.rs:341` | Signer DID + signature over rotation for social recovery. |
| `Vui` | `icn-identity/src/vui.rs` | Verifiable Unique Identifier — threshold PRF-based uniqueness commitment. |
| `IdentityBundle` | `icn-identity/src/bundle.rs` | DID + binding info + verification methods. |
| `KeyBundle` | `icn-identity/src/keybundle.rs` | Extended key management with X25519 for DH. |

#### Layer 1: Commons Holder

| Type | Location | Description |
|------|----------|-------------|
| `CommonsHolderRecord` | `icn-identity/src/commons.rs:116` | Links anchor → holder. Fields: `holder_id`, `holder_did`, `anchor_id`, `display_name`, `status`, `personhood_level`, `affiliations[]`, `baseline_rights`, timestamps. |
| `HolderStatus` | `icn-identity/src/commons.rs:337` | `Active | Suspended { reason, appeal_deadline, suspended_at } | Exited { reason, exit_timestamp } | Revoked { reason, evidence, revoked_at }` |
| `Affiliation` | `icn-identity/src/commons.rs:383` | Links holder → jurisdiction: `jurisdiction_id`, `membership_status`, `capabilities[]`, `roles[]`, `joined_at`, `expires_at`. |
| `MembershipStatus` (commons) | `icn-identity/src/commons.rs:499` | `Candidate | Provisional | Member | Suspended | Exited | Banned` |
| `MembershipCapability` (commons) | `icn-identity/src/commons.rs:529` | 6 variants: `Vote | Propose | Transact | HoldOffice | AccessPrivate | Sponsor` |
| `CommonsRights` | `icn-identity/src/commons.rs:592` | Struct with 6 boolean fields — baseline rights that jurisdictions CANNOT revoke. |
| `CommonsRight` | `icn-identity/src/commons.rs:559` | 6 variants: `VoteInCommons | Petition | MinimalFuelFloor | DueProcess | ExitFreely | AuditAccess` |
| `JurisdictionId` | `icn-identity/src/commons.rs:24` | `<type>:<id>` — `coop:`, `community:`, `federation:`, `network:` prefixes. |
| `JurisdictionType` | `icn-identity/src/commons.rs:87` | `Cooperative | Community | Federation | Network` |
| `CommonsHolderStore` | `icn-identity/src/commons_store.rs` | Trait for persisting `CommonsHolderRecord`. |

#### Recovery & Revocation

| Type | Location | Description |
|------|----------|-------------|
| `RecoveryAttestation` | `icn-identity/src/recovery.rs` | Social recovery event with attestation chain. |
| `RevocationRecord` | `icn-identity/src/revocation.rs` | Records revocation: `RevocationType`, `RevocationScope`, `CommonsRevocationReason`. |
| `RevocationScope` | `icn-identity/src/revocation.rs` | Scope of revocation action. |
| `AppealStatus` | `icn-identity/src/revocation.rs` | Due process tracking for appeals. |

### 1.2 Scope / Jurisdiction Model

#### Kernel Scope Primitives

| Type | Location | Description |
|------|----------|-------------|
| `ScopeLevel` | `icn-kernel-api/src/scope.rs:45` | `Local(0) < Cell(1) < Org(2) < Federation(3) < Commons(4)` — ordinal hierarchy. Methods: `widen()`, `narrow()`, `includes()`. Defaults to `Local` (narrowest = safest). |
| `CellId` | `icn-kernel-api/src/scope.rs:162` | `[u8; 32]` — blake3 hash of `scope_id + cell_name + genesis_salt`. Deterministic, collision-resistant. |
| `MockCellService` | `icn-kernel-api/src/scope.rs:223` | Test implementation of `CellService` trait. Provides `local_cell()`, `peer_scope()`, `is_cell_peer()`, `is_org_peer()`. |

**Scope flow through the system**: `ScopeLevel` is a kernel primitive — it flows into `PolicyRequest` context, gossip topic routing, and compute task placement. The kernel never interprets what "Org" *means*; it just knows Org is wider than Cell but narrower than Federation.

#### App-Layer Jurisdiction

`JurisdictionId` (`icn-identity/src/commons.rs:24`) is the app-layer counterpart to `ScopeLevel`. It uses typed prefixes (`coop:`, `community:`, `federation:`, `network:`) to identify the specific organizational unit, while `ScopeLevel` provides the generic "how wide" dimension.

**Key separation**: The kernel uses `ScopeLevel` for routing/replication/capacity decisions. Apps use `JurisdictionId` to track membership affiliations. The meaning firewall prevents the kernel from knowing what `coop:food-portland` means.

### 1.3 Rights / Capabilities

#### Two Capability Systems

The codebase has **two distinct capability systems** serving different architectural layers:

**1. Commons-layer MembershipCapability** (`icn-identity/src/commons.rs:529`)
- 6 variants: `Vote | Propose | Transact | HoldOffice | AccessPrivate | Sponsor`
- Carried on `Affiliation` records within `CommonsHolderRecord`
- Checked by `affiliation.has_capability()`
- No kernel enforcement — purely app-layer bookkeeping

**2. Entity-layer MembershipCapability** (`icn-entity/src/membership.rs:648`)
- 9 variants: `Vote | Propose | TreasuryAccess | Invite | ManageSubEntities | Sign | Configure | ViewSensitive | Custom(String)`
- Carried on `Membership` records in the entity model
- Role-based defaults (Founder gets all; Observer gets none)
- Checked by `membership.has_capability()`
- No kernel enforcement — purely app-layer bookkeeping

**3. Kernel Capability** (`icn-kernel-api/src/authz.rs:736`)
- `Capability` struct: bearer token with `resource`, `action`, `constraints`, `holder`, `issuer`, `expiration`, `signature`
- `CapabilityEngine` trait: `issue()`, `check()`, `delegate()`, `revoke()`
- This is the *kernel enforcement layer* — cryptographically signed, delegation-aware
- **NOT yet wired into the app-layer membership capabilities**

#### How Rights Are Actually Checked Today

- **Gateway membership endpoints**: Use `CommonsHolderStore` to check `MembershipCapability::HoldOffice` before allowing admin operations (`icn-gateway/src/api/membership/mod.rs:260`)
- **Governance voting**: Checks `MembershipCapability::Vote` on `Membership` records
- **PolicyOracle**: Converts trust scores into `ConstraintSet` (rate limits, max topics) — kernel enforces blindly
- **No scope-aware capability checking**: The gateway does not ask "what capabilities does this DID have *in this scope*?"

### 1.4 Personhood Model

**Layered design is implemented:**

1. **Layer 0 — PersonhoodAnchor** (`icn-identity/src/personhood.rs:71`):
   - Permanent anchor ID (survives key rotation)
   - Multiple POP attestations with levels (Weak/Strong/Verified)
   - Uniqueness proofs (VUI, biometric, social graph)
   - Key rotation history with recovery signatures
   - Status lifecycle: Active → Suspended → Revoked (with due process)

2. **Layer 1 — CommonsHolderRecord** (`icn-identity/src/commons.rs:116`):
   - Links to Layer 0 via `anchor_id`
   - Baseline rights (non-revocable by jurisdictions)
   - Multi-affiliation tracking
   - Display name

3. **Steward Network** (`icn-steward/src/`):
   - `StewardProfile` (`profile.rs:14`): status, jurisdiction tier, stats
   - `StewardStatus` (`profile.rs:86`): `Active | Suspended | Revoked | Probationary`
   - `JurisdictionTier` (`profile.rs:123`): Steward's operating scope
   - `VuiRegistry` (`vui_registry.rs:16`): Manages VUI registrations and uniqueness checks
   - `EnrollmentService` (`enrollment.rs:170`): Full ceremony-based enrollment with config for challenge expiry, quorum requirements
   - `RecoveryService` (`recovery.rs:154`): Key recovery with evidence requirements

**Personhood vs Membership IS enforced as a distinction**: The `CommonsHolderRecord` explicitly separates baseline rights (attached to personhood, non-revocable) from `Affiliation` capabilities (attached to jurisdiction membership, revocable). The code comment at `commons.rs:9-10` states: *"A cooperative can revoke your membership. A cooperative cannot revoke your personhood."*

### 1.5 Entity Model

#### EntityId (`icn-entity/src/entity.rs`)

Format: `entity:icn:<type>:<identifier>` with three type prefixes:
- `entity:icn:individual:<pubkey>` — wraps a DID
- `entity:icn:cooperative:<name>` — organizational entity
- `entity:icn:federation:<name>` — federation of entities

`EntityId::from_did()` creates individual entities; `EntityId::to_did()` returns `Some(Did)` for individuals, `None` for organizations.

#### Cooperative Types (`icn-coop/src/types.rs:332`)

```
CoopType: Worker | Consumer | Producer | MultiStakeholder | Platform | Housing | Credit
```
(7 variants)

#### Cooperative Lifecycle (`icn-coop/src/types.rs:344`)

```
CoopStatus: Forming → Active → Suspended → Dissolving → Dissolved
                               ↑__________|
```
(5 states, with state machine at `can_transition_to()` on line 677)

Note: The issue mentions `Merged | Split` but these are NOT in the current code. Only 5 states.

#### Entity Relationships & Membership

- `Membership` (`icn-entity/src/membership.rs:23`): Recursive membership (individuals in coops, coops in federations)
- `MembershipRole` (`membership.rs:294`): 12 variants including individual roles (Founder, Worker, Consumer) and entity roles (FederatedMember, AssociateMember, ObserverMember)
- `MembershipStatus` (`membership.rs:453`): `Pending | Active | Suspended | Inactive | Resigned | Removed | Expelled`
- `UnifiedMembershipStatus` (`membership.rs:535`): Simplified state machine: `Applicant → Pending → Active → Suspended → Terminated`
- `CooperativeEntity` (`icn-entity/src/entity.rs`): Entity with profile, relationships, kind
- `EntityRelationship` / `RelationType` (`entity.rs`): How entities relate to each other
- `AccountId` (`entity.rs`): `Did(Did) | Entity(EntityId)` for backward compatibility

#### Community Types (`icn-entity/src/entity.rs`)

`CommunityProfile` exists as a profile variant on `CooperativeEntity`. Community types are defined in the `EntityKind` enum rather than a separate crate.

### 1.6 Gateway Endpoints

#### Identity Endpoints (`icn-gateway/src/api/identity.rs`)
| Method | Path | Returns |
|--------|------|---------|
| GET | `/identity/resolve/{did}` | DID resolution with optional attestations |
| GET | `/identity/health` | Identity service health check |

#### Commons Endpoints (`icn-gateway/src/api/commons/mod.rs`)
| Method | Path | Returns |
|--------|------|---------|
| GET | `/commons/status` | `CommonsStatusResponse` — holder count, network stats |
| GET | `/commons/holder/{did}` | `HolderDetailResponse` — full holder record with affiliations |
| POST | `/commons/holder/{did}/affiliate` | Join a jurisdiction |
| PUT | `/commons/holder/{did}/affiliate` | Update affiliation status |
| DELETE | `/commons/holder/{did}/affiliate/{jurisdiction}` | Leave a jurisdiction |

#### Commons Anchor Endpoints (`icn-gateway/src/api/commons/anchor.rs`)
| Method | Path | Returns |
|--------|------|---------|
| GET | `/commons/anchor/{id}` | `AnchorDetailResponse` — anchor with attestations |
| PUT | `/commons/anchor/{id}/status` | Update anchor status |
| POST | `/commons/anchor/{id}/attestation` | Add POP attestation |

#### Membership Endpoints (`icn-gateway/src/api/membership/mod.rs`)
| Method | Path | Returns |
|--------|------|---------|
| POST | `/v1/membership/enroll` | Enroll holder with capabilities |
| GET | `/v1/membership/{holder_id}/{jurisdiction_id}` | Get affiliation details |
| PUT | `/v1/membership/{holder_id}/{jurisdiction_id}/status` | Update membership status |
| POST | `/v1/membership/capability/grant` | Grant a capability to holder |
| POST | `/v1/membership/capability/revoke` | Revoke a capability |
| POST | `/v1/membership/revoke` | Revoke commons holder status |
| POST | `/v1/membership/appeal` | File an appeal |
| GET | `/v1/membership/appeal/{holder_id}` | Get appeal status |
| GET | `/v1/membership/{holder_id}/all` | List all affiliations |

#### Constitutional Endpoints (`icn-gateway/src/api/constitutional/`)
- `voting.rs` — Constitutional voting endpoints
- `appeals_ui.rs` — Appeals UI endpoints

#### Steward Endpoints (`icn-gateway/src/api/steward/mod.rs`)
- Steward profile management
- Enrollment ceremony coordination

#### SDIS Endpoints (`icn-gateway/src/api/sdis/`)
- `enrollment.rs` — SDIS enrollment flow
- `simple_enrollment.rs` — Simplified enrollment
- `verify.rs` — Verification endpoints
- `recovery.rs` — Key recovery
- `qr.rs` — QR code generation
- `ephemeral.rs` — Ephemeral sessions

#### Trust Endpoints (`icn-gateway/src/api/trust.rs`)
- Trust score queries

#### Entity Endpoints (`icn-gateway/src/api/entity.rs`)
- Entity CRUD operations

## 2. Critical Questions

### Can a user see their rights in the current system?

**Partially YES.** Evidence:
- `GET /v1/membership/{holder_id}/all` returns all affiliations with capabilities
- `CommonsHolderRecord.baseline_rights` exposes the 6 commons rights
- `Affiliation.capabilities` lists per-jurisdiction capabilities
- `GET /commons/holder/{did}` returns full holder record with affiliations and rights

**But NO for**: There is no single "what can I do right now?" endpoint. A user would need to call multiple endpoints and correlate: commons holder record + each affiliation + trust score + kernel constraint set. No "rights dashboard" endpoint exists.

### Can a user know "where am I" (scope/jurisdiction)?

**Partially YES.** Evidence:
- `CommonsHolderRecord.affiliations[]` tracks all jurisdiction memberships
- `JurisdictionId` types (`coop:`, `community:`, `federation:`, `network:`) identify location
- `ScopeLevel` hierarchy exists in the kernel
- Gateway commons endpoints expose affiliation data

**But NO for**: There is no "current scope context" concept. The API doesn't return "you are currently operating in scope X with these constraints." `ScopeLevel` is not exposed in gateway responses. A user cannot see "I'm in `coop:food-portland` at scope level `Org`."

### Is "personhood vs membership" enforced as a distinction?

**YES.** Strong evidence:
- `CommonsRights` are structurally separate from `MembershipCapability`
- `CommonsRights::restricted()` still guarantees `MinimalFuelFloor`, `DueProcess`, `ExitFreely` even when other rights are restricted
- `CommonsHolderRecord.has_right()` checks `is_active()` first but the rights struct itself is not cleared on membership revocation
- Code comment: *"A cooperative can revoke your membership. A cooperative cannot revoke your personhood."*
- `Affiliation.ban()` clears capabilities and roles but doesn't touch `CommonsHolderRecord.baseline_rights`

### What rights survive org departure?

Evidenced in the code:
1. **Always survive** (baseline rights on `CommonsHolderRecord`):
   - `VoteInCommons` — commons-level governance participation
   - `Petition` — right to petition for constitutional changes
   - `MinimalFuelFloor` — guaranteed compute fuel for civic participation
   - `DueProcess` — right to due process before suspension
   - `ExitFreely` — right to leave any jurisdiction
   - `AuditAccess` — right to audit public records
2. **Lost on departure**: All `MembershipCapability` variants attached to the specific `Affiliation` (Vote, Propose, Transact, HoldOffice, AccessPrivate, Sponsor)
3. **Retained**: `PersonhoodAnchor` and all POP attestations survive membership departure
4. **Not addressed**: Data portability (ledger history, trust edges) on departure — no export mechanism exists

### Is scope exposed in the gateway API?

**NO.** `ScopeLevel` is not returned by any gateway endpoint. The kernel scope primitive is completely invisible to API consumers. `JurisdictionId` is exposed (via affiliations), but the scope *level* (Local/Cell/Org/Federation/Commons) is not surfaced.

## 3. Gap Analysis

### Gap 1: No "rights dashboard" endpoint

- **User story blocked**: "As a member, I want to see all my rights and capabilities in one place so I can understand what I can do."
- **Missing primitive OR surface**: A composite endpoint that aggregates: commons rights + all affiliation capabilities + trust-derived constraints + active capability tokens
- **Where it belongs**: Gateway (aggregation layer)
- **Receipt expectation**: No receipts needed — this is a read-only query
- **Phase**: 0

### Gap 2: Scope not exposed in gateway API

- **User story blocked**: "As a user, I want to know what scope level I'm operating in and what law applies here."
- **Missing primitive OR surface**: `ScopeLevel` is a kernel primitive but has no gateway representation. No endpoint returns scope context.
- **Where it belongs**: Gateway (expose existing kernel data) + UI (scope context banner)
- **Receipt expectation**: N/A — metadata, not an action
- **Phase**: 0

### Gap 3: Two capability systems not unified

- **User story blocked**: "As a developer, I need one consistent way to check 'can this DID do X in scope Y?'"
- **Missing primitive OR surface**: The commons-layer `MembershipCapability` (6 variants) and entity-layer `MembershipCapability` (9 variants) are separate enums with overlapping names but different semantics. Neither is wired to the kernel `CapabilityEngine`.
- **Where it belongs**: App layer (unify) + kernel (wire to `CapabilityEngine`)
- **Receipt expectation**: Capability grants/revocations should produce audit receipts
- **Phase**: 1

### Gap 4: No scope-aware capability resolution

- **User story blocked**: "As a system, I need to answer: given DID X in scope Y, what specific capabilities apply?"
- **Missing primitive OR surface**: `PolicyOracle` evaluates per-DID but does not incorporate scope. `PolicyContext.namespace` exists but is unused for scope-aware resolution.
- **Where it belongs**: App layer (oracle implementation) — `PolicyContext.namespace` could carry scope
- **Receipt expectation**: Capability resolution should be logged for audit
- **Phase**: 1

### Gap 5: No data portability on departure

- **User story blocked**: "As a departing member, I want to export my transaction history, trust edges, and receipts before I leave."
- **Missing primitive OR surface**: No export endpoint. Ledger entries are immutable but not extractable per-member. Trust edges have no export format.
- **Where it belongs**: Gateway (export endpoints) + kernel (data extraction queries)
- **Receipt expectation**: Departure should produce an exit receipt chain: membership termination → data export acknowledgment → exit confirmation
- **Phase**: 1

### Gap 6: No "what law applies here?" surface

- **User story blocked**: "As a member, I want to know what governance rules (charter, bylaws, voting thresholds) apply in my current scope."
- **Missing primitive OR surface**: `Cooperative.charter_id` and `Cooperative.bylaws` exist but are not exposed in a user-friendly way. No endpoint returns "active governance policies for scope X."
- **Where it belongs**: Gateway (policy reference endpoint) + UI (scope context)
- **Receipt expectation**: Policy changes should produce governance receipts (already partially handled by governance proposals)
- **Phase**: 0

### Gap 7: Multi-org affiliation home screen data missing

- **User story blocked**: "As a member of multiple coops and communities, I want a home screen showing all my affiliations, roles, and standings."
- **Missing primitive OR surface**: `GET /v1/membership/{holder_id}/all` exists but returns raw affiliations. No aggregated "home view" with enriched org metadata, standings, and active proposals.
- **Where it belongs**: Gateway (aggregation endpoint) + UI (home screen)
- **Receipt expectation**: N/A — read-only
- **Phase**: 1

## 4. Phase 0 Tasks (3-5 tasks)

### Task P0-1: Add "rights summary" gateway endpoint

**Description**: Create `GET /v1/rights/{did}` that returns a composite view of all rights for a DID:
- Commons baseline rights (from `CommonsHolderRecord.baseline_rights`)
- Per-jurisdiction capabilities (from each `Affiliation.capabilities`)
- Trust-derived constraint set (from `TrustPolicyOracle` evaluation)
- Active kernel capabilities (from `CapabilityEngine` if any)

**Files to touch**:
- `icn-gateway/src/api/` — new `rights.rs` module
- `icn-gateway/src/server.rs` — register new route
- `icn-gateway/src/commons_mgr.rs` — add rights aggregation method

**Validation step**: `curl /v1/rights/{test-did}` returns JSON with `baseline_rights`, `affiliation_capabilities`, `trust_constraints` sections.

### Task P0-2: Expose scope level in gateway API responses

**Description**: Add `scope_level` field to relevant gateway responses. When a request carries a jurisdiction context, include the corresponding `ScopeLevel` in the response.

**Files to touch**:
- `icn-gateway/src/api/commons/mod.rs` — add `scope_level` to `HolderDetailResponse` and `AffiliationResponse`
- `icn-gateway/src/api/membership/mod.rs` — add scope context to membership responses
- `icn-gateway/src/models.rs` — add scope-level response types

**Validation step**: `GET /commons/holder/{did}` response includes `scope_level` for each affiliation.

### Task P0-3: Add "what law applies here?" policy reference endpoint

**Description**: Create `GET /v1/governance/policies/{jurisdiction_id}` that returns:
- Charter ID and status (from `Cooperative.charter_id`)
- Active bylaws (from `Cooperative.bylaws`)
- Governance domain (from `Cooperative.governance_domain`)
- Voting thresholds and methods (from governance config)
- Rate limits and constraint values that apply

**Files to touch**:
- `icn-gateway/src/api/governance.rs` — add policy reference endpoint
- `icn-gateway/src/governance_mgr.rs` — add policy aggregation query

**Validation step**: `curl /v1/governance/policies/coop:food-portland` returns charter, bylaws, and active constraint parameters.

### Task P0-4: Capability set exposure endpoint

**Description**: Create `GET /v1/capabilities/{did}/{jurisdiction_id}` that answers "what can this DID do in this jurisdiction?" by resolving:
- Membership status and role
- MembershipCapability list (both commons-layer and entity-layer)
- Trust score and derived constraints
- Any active delegation chains

**Files to touch**:
- `icn-gateway/src/api/membership/mod.rs` — add capability resolution endpoint
- `icn-gateway/src/commons_mgr.rs` — add scope-aware capability query

**Validation step**: `curl /v1/capabilities/{did}/coop:test` returns `{ "can_vote": true, "can_propose": true, "trust_class": "Partner", "rate_limit": 100 }`.

### Task P0-5: Document constitutional invariants (complete Issue #1011)

**Description**: Complete the "Constitutional Genesis" documentation (Wave 5, Issue #1011) that categorizes hard invariants (immutable) vs meta-invariants (governable). This provides the "what law applies" reference for the system.

**Files to touch**:
- `docs/genesis.md` (new) or `docs/ARCHITECTURE.md` (new section)
- Reference `icn-kernel-api/src/authz.rs` for enforcement mechanisms
- Reference `apps/trust/src/oracle.rs` for meta-invariant sources

**Validation step**: Document exists, reviewed, and linked from `ARCHITECTURE.md`. Contains hard/meta invariant table with enforcement locations.

## 5. Phase 1 Tasks (3-5 tasks)

### Task P1-1: Unify capability systems

**Description**: Merge the two `MembershipCapability` enums into one authoritative definition. The entity-layer enum (`icn-entity/src/membership.rs:648`) has more variants and should be the canonical source. The commons-layer enum (`icn-identity/src/commons.rs:529`) should re-export or alias it.

**Files to touch**:
- `icn-identity/src/commons.rs` — replace 6-variant enum with re-export from icn-entity
- `icn-entity/src/membership.rs` — ensure all 9+ variants cover both systems
- `icn-gateway/src/api/membership/mod.rs` — update `parse_capability()` for unified set
- All tests referencing either enum

**Validation step**: Single `MembershipCapability` definition; no two enums with the same name. All existing tests pass.

### Task P1-2: Wire MembershipCapability to kernel CapabilityEngine

**Description**: Bridge the app-layer capability grants to kernel-layer `CapabilityEngine`. When a membership grants `MembershipCapability::Vote`, it should produce a signed `Capability` token the kernel can verify.

**Files to touch**:
- `icn-kernel-api/src/authz.rs` — implement `CapabilityEngine` trait
- App layer (future `apps/membership`) — produce `Capability` tokens on grant
- `icn-gateway/src/api/membership/mod.rs` — issue capability tokens on enroll

**Validation step**: `grant_capability()` produces a cryptographically signed `Capability` token that `CapabilityEngine::check()` validates.

### Task P1-3: Multi-org affiliations home screen data model

**Description**: Create a rich "home" endpoint that aggregates across all affiliations for a DID: org name + type + role + standing + active proposals + trust score + pending notifications.

**Files to touch**:
- `icn-gateway/src/api/` — new `home.rs` or extend `commons/mod.rs`
- `icn-gateway/src/commons_mgr.rs` — aggregation query
- `icn-gateway/src/governance_mgr.rs` — pending proposals for user

**Validation step**: `GET /v1/home/{did}` returns enriched list of all org memberships with metadata.

### Task P1-4: Data portability / exit rights

**Description**: Implement member departure data export: transaction history, trust edge export, receipt chain export. This fulfills the `ExitFreely` right with practical data portability.

**Files to touch**:
- `icn-gateway/src/api/` — new `export.rs` endpoint
- `icn-ledger/src/` — per-member transaction extraction
- `icn-trust/src/` — trust edge export format
- `icn-gateway/src/receipt_store.rs` — receipt export

**Validation step**: `POST /v1/export/{did}` produces a downloadable archive with transaction history, trust edges, and receipts. Exit receipt is generated.

### Task P1-5: Scope-aware PolicyOracle evaluation

**Description**: Extend `PolicyOracle::evaluate()` to incorporate scope. Use `PolicyContext.namespace` to carry `JurisdictionId`, and have oracles return scope-specific constraint sets.

**Files to touch**:
- `apps/trust/src/oracle.rs` — check namespace/scope in evaluation
- `icn-kernel-api/src/authz.rs` — document `PolicyContext.namespace` for scope usage
- Gateway middleware — populate `PolicyContext.namespace` from request context

**Validation step**: Same DID gets different constraint sets when evaluated in `coop:A` vs `coop:B`.

## 6. Relevant Open Issues

| Issue | Title | Recommendation |
|-------|-------|----------------|
| **#1147** | [EPIC] Vertical Slice: Identity → Governance → Compute → Receipts → Audit | **Keep** — directly tracks the end-to-end rights flow this analysis supports. Phase 0 tasks feed into this epic's Track A (kernel/app separation). |
| **#1137** | [A3] Phase 5: Membership Consolidation | **Keep** — merging `icn-entity` + `icn-coop` + `icn-community` into `apps/membership` would be the natural location for the unified capability system (Task P1-1). |
| **#1139** | [B1] Treasury-Coop Integration | **Keep** — treasury access is a capability (`TreasuryAccess`) that needs to be wired into the rights system. |
| **#1011** | [Wave 5] Constitutional Genesis Documentation | **Keep and prioritize** — directly maps to Task P0-5. The hard/meta invariant categorization is foundational for the "what law applies here?" surface. |
| **#1090** | [PR8c] Fallback atomicity via CoopActor single-writer | **Keep** — treasury nonce enforcement is a capability-gated operation. |
