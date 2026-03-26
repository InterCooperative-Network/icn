<!-- Relocated from docs/plans/.scratch/ (tranche 3). -->

# CCL + Disputes — Repo State & Gap Analysis

## 1. Current State

### 1.1 CCL Language Model

**AST** (`icn-ccl/src/ast.rs`):
- `Contract` (line 42): name, participants (`Vec<Did>`), currency (`Option<String>`), state_vars, rules, triggers
- `Rule` (line 74): name, params, requires (preconditions), body
- `Stmt` (line 110): Assign, SetState, LedgerTransfer, SetCreditLimit, If, For, Return, Expr
- `Expr` (line 157): Literal, Var, FieldAccess, BinOp, UnOp, Call, List, Set, Map, In
- `BinOp` (line 198): Add/Sub/Mul/Div/Mod + Eq/Ne/Lt/Le/Gt/Ge + And/Or
- `Trigger` (line 97): Scheduled actions with cron expressions
- Validation: name length (128), loop depth (5), expr depth (50), max participants (100), state vars (100), rules (50), reserved keywords

**Value Types** (`icn-ccl/src/types.rs:111`):
- `Value`: Int(i64), String, Bool, Did, List, Set, Map, None

**Capabilities** (`icn-ccl/src/types.rs:14`):
- `Capability` enum: ReadLedger, WriteLedger, SendMessage, ReadState, WriteState, CreateSubContract, InvokeContract, CreateProposal, VoteProposal, ReadProposal, ExecuteProposal, ManageRoles

**Fuel Metering** (`icn-ccl/src/types.rs:272`):
- `FuelConfig`: stmt_cost, expr_cost, call_cost, loop_iteration_cost, max_loop_iterations
- Presets: `default()`, `restrictive()` (2x costs, 500 max loops), `permissive()` (reduced costs, 2000 max loops)
- `FuelEstimate` (line 489): minimum/expected/maximum with branch/loop scaling

**Execution Context** (`icn-ccl/src/types.rs:330`):
- `ExecutionContext`: caller (Did), timestamp, fuel, capabilities, participants, determinism_class
- `DeterminismClass`: Canonical (can mutate state) vs Advisory (read-only suggestions)

**Execution Result** (`icn-ccl/src/types.rs:431`):
- `ExecutionResult`: return value, fuel_consumed, state_changes, ledger_ops

### 1.2 Contract Registry

**Location**: `icn-ccl/src/registry.rs`

**ContractRegistry** (line 333): In-memory HashMap caches with optional persistent `icn_store::Store` backend.

**Storage schema**:
```
contract:<hash>          → Contract (icn_encoding::encode_versioned)
metadata:<hash>          → ContractMetadata
index:name:<name>        → Vec<(version, hash)>
index:owner:<did>        → Vec<hash>
```

**ContractMetadata** (line 175): code_hash, name, version (u32), semantic_version (`SemanticVersion`), min_interpreter_version, compatible_versions, deprecated flag + reason + successor, owner, deployed_at, description, participants, currency, rules, visibility

**Visibility** (line 163): `Private | Coop(String) | Public`

**SemanticVersion** (line 37): major.minor.patch with semver compatibility rules (pre-1.0: minor must match)

**Operations**:
- `deploy()` / `deploy_with_visibility()` — validates contract, computes blake3 hash, stores
- `get()` / `get_metadata()` — cache-first with store fallback
- `resolve(name, version)` — name+version lookup (latest if None)
- `get_by_semantic_version()` — semver-based resolution
- `mark_deprecated()` — sets deprecated flag with reason + successor
- `set_semantic_version()` — post-deployment version update
- `get_upgrade_path()` — checks semver compatibility for upgrades
- `list_all()`, `list_by_owner()`, `get_versions()`, `list_versions_metadata()`
- `load_from_store()` — hydrates caches from persistent storage

**Registry Actor** (`icn-ccl/src/registry_actor/`): Actor wrapper for ContractRegistry with command-based message passing.

**Missing**: No revoke/delete operation (only deprecate), no gossip distribution, no access control on deploy (any owner string accepted).

### 1.3 Entity Schema in CCL

Rich YAML-based schema system in `icn-ccl/src/schema/`:

**EntitySchema** (`schema/entity.rs:183`):
- `EntityType`: Cooperative, Community, Federation
- `EntitySubtype`: Worker, Consumer, Producer, MultiStakeholder (coop); Municipal, Regional, Sectoral, Purpose (community); Trade, Geographic, FederationSectoral (federation); Custom
- `MembershipConfig`: open_to (Individual/Cooperative/Community/Federation), classes with criteria (all/any conditions), rights_by_class
- `Right`: Vote, PatronageRefund, RunForBoard, WorkplaceGovernance, Propose, AccessResources, Custom
- `Jurisdiction`: Geographic, Network, Hybrid with bounds

**GovernanceSchema** (`schema/governance.rs:222`):
- `GovernanceBody`: name, composition, seats, elected_by, term (years/months/staggered), recall (petition/vote), convenes (regular/special)
- `DecisionType`: name, authority (body), threshold (SimpleMajority/Supermajority/Unanimous/Consensus/Fraction/Expression), quorum, ratification_period, constraint (must_ratify body)
- `DelegationConfig`: allowed, transitive, revocable (Instant/EndOfSession/EndOfTerm), scope (Vote/Propose/Speak/Candidacy)

**EconomicsSchema** (`schema/economics.rs:133`):
- `CapitalConfig`: member_equity (min/max/interest/refund)
- `SurplusConfig`: allocation rules with targets (Reserves, PatronageRefund, CommunityFund, WorkerBonus, EducationFund, IndivisibleReserves, Custom), fractions, conditions
- `CreditConfig`: eligibility criteria, limit expression, payment terms (Net7-Net90)

**AgreementSchema** (`schema/agreement.rs:347`):
- Parties with types (Federation/Cooperative/Community/Individual) and DID IDs
- `BoundaryProtocol`: transfers (denominations, exchange rates, settlement), credentials (mutual recognition), joint decisions
- `DisputeResolution`: ladder (Negotiation→Mediation→Arbitration→Adjudication), prohibited methods
- `ExitConfig`: notice period, surviving obligations
- `IncompatibilityHandling`: value_incompatible, governance_deadlock, credential_unknown — each with action + fallback

**Key Insight**: These schemas are structural definitions — they describe WHAT governance looks like but are not yet wired to the CCL execution engine or the runtime governance system. They exist as validated data types.

### 1.4 CCL Disputes

**Location**: `icn-ccl/src/disputes.rs`

**DisputeResolutionSystem** (line 261): Stateful system with in-memory dispute tracking, mediator pool, and callback-based integration.

**Dispute types**:
- `DisputeReason` (line 142): IncorrectResult, FuelLimitExceeded, ExecutionFailed, TimeoutNotReported
- `DisputeStatus` (line 180): Pending → Investigating → Resolved | UnderMediation → Closed
- `DisputeOutcome` (line 123): SubmitterCorrect, ExecutorCorrect, BothWrong, Inconclusive (with optional mediator)
- `DisputeEvidence` (line 161): task_hash, claimed_result, reason, additional_data, filed_at
- `Dispute` (line 209): dispute_id, task_hash, challenger, executor, evidence, status, timestamps

**Gossip integration**:
- Topics: `disputes:file`, `disputes:resolved`
- `DisputeMessage` (line 103): DisputeFiled, DisputeResolved — broadcast via callbacks
- `DisputeGossipCallback`, `MisbehaviorCallback`, `TrustCallback`, `TrustPenaltyCallback`

**Penalty system**:
- `PenaltyConfig` (line 46): per-offense trust penalties (0.02–0.10) with escalation multiplier (1.5x after threshold)
- `OffenderRecord` (line 81): tracks disputes_lost, incorrect_results, unreported_timeouts, total_trust_penalty
- Escalating penalties for repeat offenders, capped at 1.0

**Mediation**:
- `MediatorInfo` (line 253): DID + expertise tags
- Trust-weighted selection with conflict-of-interest filtering (mediator ≠ executor or challenger)
- Min trust threshold: 0.7 (Federated class)
- Auto-assignment configurable

**Investigation flow**:
1. `file_dispute()` → creates record, broadcasts via gossip
2. `investigate_dispute()` → re-executes contract with fresh interpreter
3. Compares results → determines outcome → applies penalties
4. Inconclusive → assigns mediator

**Metrics**: dispute counts, penalties applied, repeat offenders, escalated penalties (via `icn_obs::metrics::disputes`)

### 1.5 Governance Appeals

**Location**: `icn-governance/src/appeal.rs`

**Appeal** (line 419): Full appeal record with comprehensive lifecycle.

**AppealType** (line 66):
- Revocation (revocation_id, type)
- Suspension (target_id, type)
- GovernanceDecision (proposal_id, decision)
- DisputeResolution (dispute_id, resolution) — **bridges CCL disputes**
- MembershipDenial (jurisdiction_id, application_id)
- StewardAction (steward_did, action)
- Other (category, description)

**AppealGrounds** (line 269): 8 variants — ProceduralError, NewEvidence, ExceededAuthority, RightsViolation, FactualError, Bias, DisproportionatePenalty, Other

**AppealRemedy** (line 252): 6 variants — Reverse, Reinstate, Modify, Compensation, Custom, None

**AppealStatus** (line 155): Filed → UnderReview → Hearing → Resolved/Dismissed/Withdrawn

**AppealOutcome** (line 221): Upheld (with remedy), Denied, PartiallyUpheld (with remedy), Remanded (to body)

**AppealScope** (line 134): Jurisdiction, Federation, Network — determines appeal body

**Evidence** (line 350): `AppealEvidence` — evidence_id, type (Document/Transaction/Communication/WitnessStatement/ExpertOpinion/Technical), description, content_hash, URI, submitted_by, submitted_at

**Deadlines** (line 305): filing window (30d default), review (14d), hearing (7d), max duration (90d) — with expedited (7d/3d/2d/21d) and extended (60d/30d/14d/180d) presets

**Lifecycle methods**: new(), begin_review(), begin_hearing(), resolve(), dismiss(), withdraw(), add_evidence(), add_response()

**Responses** (line 389): `AppealResponse` with responder, response_type (InitialResponse/Reply/Comment/Question/Clarification), content, evidence

### 1.6 Federation Agreements

**Two separate implementations exist**:

**1. CCL Schema** (`icn-ccl/src/schema/agreement.rs`):
- `AgreementSchema`: Declarative YAML schema with parties, boundary protocol, dispute resolution ladder, exit config, incompatibility handling
- Rich type system for transfers, credentials, joint governance
- This is the "constitution" layer — what an agreement looks like

**2. icn-federation** (`icn/crates/icn-federation/`):
- Runtime agreement management with lifecycle states
- `AgreementType`: Trade, Credit, ResourceSharing, FederationMembership, LaborReciprocity, SharedServices
- `AgreementStatus`: Draft → Proposed → Signed → Active → Expired/Terminated/Disputed
- Amendment system with amendment proposals

**3. Gateway Agreements** (`icn-gateway/src/api/agreements.rs`):
- Full CRUD: list, create, get, delete
- Lifecycle: propose, sign, suspend, resume, terminate
- Amendments: list, propose, sign
- Party management: add_party, set_terms

**Gap**: The CCL AgreementSchema and the federation runtime Agreement are separate type systems. No bridge converts a CCL agreement schema into a runtime agreement or vice versa.

### 1.7 Gateway Endpoints

**Contract endpoints** (`icn-gateway/src/api/contracts.rs`):
- `deploy_contract` — deploy to registry
- `list_contracts` — list all
- `get_contract` — get by hash
- `revoke_contract` — revoke
- `get_contract_metadata` — metadata by hash
- `deprecate_contract` — mark deprecated
- `get_contract_status` — active/deprecated status
- `get_version_history` — all versions of a contract

**Governance endpoints** (`icn-gateway/src/api/governance.rs`):
- Domain management: create_domain, list_domains, get_domain, add_domain_member
- Proposals: create_proposal, list_proposals, get_proposal, open_proposal, close_proposal, get_votes, get_proposal_proof
- Federation proposals: create_join/leave/establish_clearing/terminate_clearing/vouch/revoke_vouch/update_policy proposals
- Voting: cast_vote
- Delegation: create/list/get/revoke delegation
- Discussion: start/end deliberation, add/list/edit/delete comment, add/remove reaction, get_discussion
- Action items: create/list/get/update/delete, add_note, update_status

**Constitutional** (`icn-gateway/src/api/constitutional/`):
- `appeals_ui.rs` — Appeal UI endpoints
- `voting.rs` — Constitutional voting endpoints

**Agreement endpoints** (`icn-gateway/src/api/agreements.rs`):
- Full CRUD + lifecycle + amendments (14 endpoints total)

**Steward dispute endpoints** (`icn-gateway/src/api/steward/mod.rs`):
- `POST /{steward_id}/dispute` — record dispute
- `POST /{steward_id}/dispute-won` — record dispute won

**Missing endpoints**:
- No general dispute filing endpoint (only steward-specific)
- No dispute evidence submission endpoint
- No "what governs this action?" query endpoint
- No human-readable contract/policy rendering endpoint
- No policy lookup by scope+action endpoint

## 2. "Living Law" Readiness Audit

| Requirement | Status | Evidence |
|-------------|--------|----------|
| Policies expressible in CCL | **Partial** | Contract AST supports rules, conditions, state, ledger ops. Schema types express governance/economics/agreements as YAML. But schemas aren't executable — they're structural definitions. |
| Policies deployable to registry | **Yes** | `ContractRegistry.deploy()` with content hash, versioning, visibility |
| Policies human-readable | **No** | No rendering/display layer. Contracts are JSON AST. Schemas are YAML but no display endpoint exists. |
| "What governs this?" queryable | **No** | No policy lookup by scope/action/entity. Would need to index contracts by capability scope. |
| Policy changes produce receipts | **Partial** | `GovernanceProof` exists (`icn-governance/src/proof.rs`). Contract deployment creates metadata with timestamps/owner. But no receipt for policy *changes* (deprecation, version updates). |
| Disputes fileable via API | **Partial** | Steward disputes: `POST /v1/steward/{id}/dispute`. No general CCL dispute filing via API. `DisputeResolutionSystem.file_dispute()` exists but isn't gateway-exposed. |
| Evidence chain-of-custody | **Partial** | `AppealEvidence` has content_hash + submitted_by + submitted_at. `DisputeEvidence` has task_hash + filed_at. But no cryptographic chain linking evidence items. |
| Mediation workflow exists | **Yes** | `DisputeResolutionSystem` with mediator pool, trust-weighted selection, conflict-of-interest filtering |
| Appeal path exists | **Yes** | Full `Appeal` type with Filed→UnderReview→Hearing→Resolved lifecycle, 8 grounds, 6 remedies, 3 scopes |
| Cross-org disputes escalable | **Partial** | `AppealScope::Federation` and `AppealScope::Network` exist. Agreement schema has dispute ladder (Negotiation→Mediation→Arbitration). But no runtime implementation of cross-org escalation. |
| Federation agreements in CCL | **Partial** | `AgreementSchema` in CCL schema module is rich (boundary protocols, credentials, joint governance, incompatibility handling). But not bridged to runtime `icn-federation` agreements. |

## 3. Four Dispute/Appeal Systems Analysis

### CCL Disputes (`icn-ccl/src/disputes.rs`)
- **Focus**: Contract execution correctness (compute task results)
- **Trigger**: Challenger disputes a compute result
- **Resolution**: Automatic re-execution of contract → compare results
- **Fallback**: Mediator assignment for inconclusive cases
- **Penalties**: Trust score reduction (escalating for repeat offenders)
- **Scope**: Single contract execution
- **Evidence model**: `DisputeEvidence` — task_hash, claimed_result, reason, raw bytes
- **Gossip**: `disputes:file`, `disputes:resolved` topics

### Governance Appeals (`icn-governance/src/appeal.rs`)
- **Focus**: Due process for governance decisions
- **Trigger**: Appellant challenges any governance action
- **Resolution**: Appeal body review → hearing → decision
- **Scope**: Jurisdiction/Federation/Network level
- **Evidence model**: `AppealEvidence` — typed evidence (Document/Transaction/Communication/WitnessStatement/ExpertOpinion/Technical) with content_hash + URI
- **No gossip integration** — appeal state is local

### Ledger Disputes (Third Runtime System)

**Location**: `icn-ledger/src/dispute.rs`

- **Focus**: Financial mediation — incorrect amounts, unauthorized transactions
- **Trigger**: Member files dispute about a ledger entry
- **Evidence**: `Vec<String>` (simple text-based)
- **Resolution**: Mediator judgment
- **Escalation**: `escalate_to_governance()` creates a governance proposal for community vote; `resolve_escalated_dispute()` completes the cycle
- **Status**: Normal → Contested → Resolved | Escalated

This is the **only system with explicit governance escalation** built in.

### Relationship

**They are NOT unified.** They share no evidence model, no receipt format, no common types.

**Bridge points**:
1. `AppealType::DisputeResolution` allows appealing a CCL dispute outcome via governance appeals
2. Ledger disputes can escalate to governance proposals via `escalate_to_governance()`

**Divergence**:
1. Evidence types are incompatible (`DisputeEvidence` vs `AppealEvidence` vs `Vec<String>`)
2. Resolution mechanisms differ (automatic re-execution vs mediator judgment vs appeal body review)
3. Penalty systems differ (trust reduction vs remedy ordering vs none for ledger)
4. Gossip integration: CCL disputes broadcast, appeals and ledger disputes don't
5. Storage: CCL disputes in `icn_store`, appeals have no persistent store, ledger disputes in ledger store

**What should converge**:
- Evidence model: All should use a common `Evidence` type with content_hash and chain-of-custody
- Receipts: All should produce `ArtifactReceipt` on resolution
- Gossip: Appeal and dispute events should broadcast for transparency
- Escalation: All dispute types should have a clear escalation path to governance

### Agreement Dispute Resolution (Fourth System — Schema Only)
The CCL agreement schema (`schema/agreement.rs`) defines a FOURTH dispute model:
- `DisputeResolution` with `ladder: Vec<DisputeStage>` (Negotiation→Mediation→Arbitration→Adjudication)
- `MediatorSelection`: MutualAgreement, Rotation, RandomFromPool, ThirdParty
- Binding/non-binding stages

This is **schema-only** — no runtime implementation connects the agreement dispute ladder to either the CCL dispute system or governance appeals.

## 4. Gap Analysis

### Gap 1: No Human-Readable Policy Display
- **User story blocked**: "As a cooperative member, I want to read the policies that govern my community in plain language"
- **Missing**: Rendering layer for CCL contracts and governance schemas
- **Where it belongs**: `icn-ccl` (renderer) + `icn-gateway` (endpoint)
- **Receipt expectation**: N/A (read-only)
- **Phase**: 0 (legibility foundation)

### Gap 2: No "What Governs This?" Query
- **User story blocked**: "As a member, I want to know what policies apply when I make a transfer"
- **Missing**: Policy index by scope/action/entity + query endpoint
- **Where it belongs**: `icn-ccl/src/registry.rs` (index) + `icn-gateway` (endpoint)
- **Receipt expectation**: N/A (read-only)
- **Phase**: 0 (legibility foundation)

### Gap 3: No General Dispute Filing via API
- **User story blocked**: "As a cooperative member, I want to file a dispute about an incorrect computation"
- **Missing**: Gateway endpoint for `DisputeResolutionSystem.file_dispute()`
- **Where it belongs**: `icn-gateway/src/api/disputes.rs` (new file)
- **Receipt expectation**: Dispute receipt with ID, parties, evidence hash
- **Phase**: 2

### Gap 4: No Unified Evidence Model
- **User story blocked**: "As a mediator, I want to see all evidence in one format regardless of dispute vs appeal"
- **Missing**: Common evidence type shared between CCL disputes and governance appeals
- **Where it belongs**: `icn-kernel-api/src/proofs.rs` or new `icn-kernel-api/src/evidence.rs`
- **Receipt expectation**: Evidence receipt with chain-of-custody
- **Phase**: 2

### Gap 5: CCL Schema ↔ Runtime Bridge
- **User story blocked**: "As a cooperative founder, I want to write our governance rules in CCL and have them enforced"
- **Missing**: Bridge that converts CCL entity/governance/economics schemas into runtime governance configurations and PolicyOracle constraints
- **Where it belongs**: New module in `icn-ccl` or new `apps/governance` crate
- **Receipt expectation**: Activation receipt when schema becomes enforceable
- **Phase**: 2-3

### Gap 6: Agreement Schema ↔ Federation Bridge
- **User story blocked**: "As a federation coordinator, I want to author a mutual aid compact in CCL format"
- **Missing**: Bridge between `AgreementSchema` (CCL) and `icn-federation` runtime agreements
- **Where it belongs**: `icn-federation` or new bridge module
- **Receipt expectation**: Agreement activation receipt
- **Phase**: 3

### Gap 7: Appeal System Has No Persistent Store
- **User story blocked**: "As an appellant, I want my appeal to survive node restarts"
- **Missing**: `icn-governance` appeal store (appeals are in-memory struct only)
- **Where it belongs**: `icn-governance/src/appeal_store.rs`
- **Receipt expectation**: Appeal filing receipt, resolution receipt
- **Phase**: 2

### Gap 8: No Cross-System Dispute Escalation Runtime
- **User story blocked**: "As a federation member, I want a contract dispute to escalate through the agreement's dispute ladder"
- **Missing**: Runtime implementation of agreement dispute ladder (Negotiation→Mediation→Arbitration)
- **Where it belongs**: `icn-federation` or `icn-ccl`
- **Receipt expectation**: Escalation receipt at each stage
- **Phase**: 3

### Gap 9: No Policy Change Receipts
- **User story blocked**: "As a member, I want proof that the policy was different when my transaction was evaluated"
- **Missing**: Receipts for contract deployment, deprecation, version changes
- **Where it belongs**: `icn-ccl/src/registry.rs` (emit `ArtifactReceipt`)
- **Receipt expectation**: PolicyChangeReceipt with before/after hash, authority, timestamp
- **Phase**: 2

### Gap 10: No Gossip for Appeals
- **User story blocked**: "As a federation partner, I want to be notified when an appeal is filed against a shared governance decision"
- **Missing**: Gossip integration for appeal events
- **Where it belongs**: `icn-governance/src/appeal.rs` (add gossip callbacks like disputes have)
- **Receipt expectation**: Appeal notification receipt
- **Phase**: 2

## 5. Phase 0 Tasks (2-3 tasks)

CCL/dispute contributions to legibility:

### P0-CCL-1: Policy Reference Display
- **What**: When displaying any governance action (proposal, vote, decision), include the policy ID + version + authority that authorizes it
- **Where**: `icn-gateway` governance endpoints should return `policy_ref: { contract_hash, version, authority_did }` in responses
- **How**: Add optional `policy_ref` field to governance response DTOs; populate from contract registry when available
- **Effort**: Small — DTO changes + optional registry lookup

### P0-CCL-2: "What Law Applies Here?" — Scope-Based Policy Lookup
- **What**: Given a scope (coop DID + action type), return the list of active contracts/policies that govern that scope
- **Where**: New method on `ContractRegistry` to index by capability scope; new gateway endpoint `GET /v1/policies?scope={scope}&action={action}`
- **How**: Build a secondary index mapping `(scope, capability_type)` → `Vec<ContentHash>`; query returns metadata for matching contracts
- **Effort**: Medium — index building + new endpoint

### P0-CCL-3: Contract Summary Display
- **What**: Return a human-readable summary of a deployed contract (name, version, participants, rules, capabilities, state variables)
- **Where**: New `GET /v1/contracts/{hash}/summary` endpoint returning structured but readable JSON
- **How**: Extract metadata + contract AST → produce summary with rule names, capability names, participant DIDs
- **Effort**: Small — formatting logic + new endpoint

## 6. Phase 2 Tasks (4-6 tasks)

### P2-CCL-1: Human-Readable Policy Rendering
- **What**: Render CCL contracts and governance schemas as human-readable text (Markdown or structured HTML)
- **Where**: New `icn-ccl/src/render.rs` module; gateway endpoint `GET /v1/contracts/{hash}/readable`
- **How**: Walk AST/schema → produce natural language descriptions ("If caller's equity >= 100, they may transfer up to...")
- **Effort**: Medium-Large

### P2-CCL-2: Unified Evidence Model
- **What**: Create common `Evidence` type shared between CCL disputes and governance appeals
- **Where**: `icn-kernel-api/src/evidence.rs` with types used by both `icn-ccl/disputes.rs` and `icn-governance/appeal.rs`
- **How**: Extract common fields (content_hash, type, submitted_by, timestamp, chain_id) into shared type; migrate both systems
- **Effort**: Medium (migration of two systems)

### P2-CCL-3: General Dispute Filing API
- **What**: Expose `DisputeResolutionSystem.file_dispute()` via gateway
- **Where**: New `icn-gateway/src/api/disputes.rs` with endpoints: `POST /v1/disputes`, `GET /v1/disputes/{id}`, `GET /v1/disputes`
- **How**: Wire gateway → CCL dispute system; add evidence submission endpoint
- **Effort**: Medium

### P2-CCL-4: Appeal Persistent Storage
- **What**: Persist appeals to `icn_store` so they survive restarts
- **Where**: New `icn-governance/src/appeal_store.rs`
- **How**: Sled-backed store with `appeal:{id}` → serialized Appeal; index by appellant, scope, status
- **Effort**: Medium

### P2-CCL-5: Policy Change Receipts
- **What**: Emit `ArtifactReceipt` when contracts are deployed, deprecated, or version-updated
- **Where**: `icn-ccl/src/registry.rs` — add receipt generation to deploy/deprecate/set_semantic_version
- **How**: Create `PolicyChangeReceipt` variant; sign with deployer's key; store in receipt log
- **Effort**: Small-Medium

### P2-CCL-6: CCL Schema → Runtime Governance Bridge (Design)
- **What**: Design the bridge between CCL entity/governance schemas and runtime governance system
- **Where**: Design document + interface definitions
- **How**: Map GovernanceSchema.decisions → proposal types, thresholds, quorum rules; map membership.classes → member roles
- **Effort**: Design only — Large implementation deferred to Phase 3

## 7. Phase 3 Tasks (2-3 tasks)

### P3-CCL-1: Federation Agreement Authoring in CCL
- **What**: Bridge `AgreementSchema` to `icn-federation` runtime agreements
- **Where**: New bridge module in `icn-federation/src/ccl_bridge.rs`
- **How**: Parse AgreementSchema YAML → create runtime Agreement with matching lifecycle, boundary protocol, dispute ladder
- **Effort**: Large

### P3-CCL-2: Cross-Organization Dispute Escalation Runtime
- **What**: Implement agreement dispute ladder as a runtime state machine
- **Where**: New `icn-federation/src/dispute_ladder.rs` or `icn-ccl/src/agreement_disputes.rs`
- **How**: State machine: Negotiation(timeout) → Mediation(mediator_selection) → Arbitration(panel) → Adjudication; each stage produces receipts
- **Effort**: Large

### P3-CCL-3: CCL Schema → Runtime Governance Implementation
- **What**: Implement the bridge designed in P2-CCL-6
- **Where**: `apps/governance` crate or `icn-ccl/src/schema_runtime.rs`
- **How**: Parse GovernanceSchema → register decision types as proposal templates; wire MembershipConfig → role assignments; activate EconomicsSchema → ledger constraints
- **Effort**: Very Large

## 8. Relevant Open Issues

Based on codebase analysis, the following areas have TODO/incomplete markers:

1. **Contract revocation**: `revoke_contract` gateway endpoint exists but registry only has `mark_deprecated` — no true revocation that prevents execution
2. **Schema version field**: `schema/version.rs` exists but wasn't analyzed — may contain version compatibility for schemas
3. **Charter validation**: `icn-ccl/src/charter_validator.rs` and `charter_rules.rs` exist — may provide policy validation that partially addresses "what governs this?"
4. **Governance proof**: `icn-governance/src/proof.rs` — `GovernanceProof` exists but relationship to dispute receipts unclear
5. **Constitutional module**: `icn-gateway/src/api/constitutional/` has `appeals_ui.rs` and `voting.rs` — may already expose some appeal functionality via gateway

## 9. Architecture Notes

### The Three-Layer "Living Law" Stack

```
┌─────────────────────────────────────────────┐
│  Layer 3: Federation Agreements              │
│  (Cross-org compacts, boundary protocols)    │
│  CCL: AgreementSchema                        │
│  Runtime: icn-federation Agreement           │
│  Gap: No bridge between them                 │
├─────────────────────────────────────────────┤
│  Layer 2: Governance                         │
│  (Proposals, voting, appeals)                │
│  CCL: GovernanceSchema + EntitySchema        │
│  Runtime: icn-governance proposals/appeals   │
│  Gap: Schemas aren't executable              │
├─────────────────────────────────────────────┤
│  Layer 1: Contracts                          │
│  (Executable rules, state mutations)         │
│  CCL: Contract AST + interpreter             │
│  Runtime: ContractRegistry + fuel metering   │
│  Most complete layer                         │
├─────────────────────────────────────────────┤
│  Kernel: Constraint Enforcement              │
│  (PolicyOracle → ConstraintSet → enforce)    │
│  MEANING FIREWALL                            │
└─────────────────────────────────────────────┘
```

### Key Architectural Observation

CCL is simultaneously two things:
1. **An execution engine** (Contract AST → interpreter → state changes) — this works
2. **A constitutional schema** (Entity/Governance/Economics/Agreement YAML types) — these are definitions only

The "living law" vision requires both: executable rules that humans can also read and inspect. The gap is the bridge between the constitutional schemas (which are human-readable) and the execution engine (which is machine-enforceable).
