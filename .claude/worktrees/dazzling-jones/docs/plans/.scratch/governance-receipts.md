# Governance / Receipts — Repo State & Gap Analysis

**Date**: 2026-02-14
**Analyst**: icn-architect agent (governance-receipts)
**Codebase**: 414K LOC, ~5,060 tests

---

## 1. Current State

### 1.1 Proposal Types

`ProposalPayload` enum — **16 variants** at `icn-governance/src/proposal.rs:396`:

| # | Variant | Purpose | File:Line |
|---|---------|---------|-----------|
| 1 | `Text` | Free-form text proposal | `proposal.rs:398` |
| 2 | `Budget` | Budget allocation (amount, currency, recipient, purpose) | `proposal.rs:404` |
| 3 | `Membership` | Add/remove member | `proposal.rs:416` |
| 4 | `ConfigChange` | Governance config change (JSON) | `proposal.rs:424` |
| 5 | `SchedulingPolicy` | Coop scheduling policy update | `proposal.rs:430` |
| 6 | `FreezeMember` | Emergency: freeze member (supermajority) | `proposal.rs:442` |
| 7 | `UnfreezeMember` | Emergency: unfreeze member | `proposal.rs:452` |
| 8 | `VetoProposal` | Emergency: veto existing proposal | `proposal.rs:460` |
| 9 | `ForceCloseProposal` | Emergency: force close proposal | `proposal.rs:470` |
| 10 | `RollbackLedger` | Emergency: rollback ledger to hash (highest threshold) | `proposal.rs:482` |
| 11 | `DisputeResolution` | Escalated dispute requiring community vote | `proposal.rs:499` |
| 12 | `Sdis` | SDIS-specific (steward mgmt, thresholds) | `proposal.rs:517` |
| 13 | `ProtocolUpgrade` | Protocol version upgrade (breaking changes) | `proposal.rs:532` |
| 14 | `ProtocolChange` | Protocol parameter change (scoped) | `proposal.rs:558` |
| 15 | `Treasury` | Treasury operations (spend, budget, withdraw) | `proposal.rs:573` |
| 16 | `SurplusAllocation` | Labor surplus distribution | `proposal.rs:587` |
| 17 | `ShareRedemption` | Departing member share redemption | `proposal.rs:599` |
| — | `Federation(FederationProposal)` | Federation operations (7 sub-types via gateway endpoints) | Used in gateway federation endpoints |

**Federation sub-proposals** (created via gateway helpers): JoinFederation, LeaveFederation, EstablishClearing, TerminateClearing, Vouch, RevokeVouch, UpdateFederationPolicy.

**TreasuryProposalOperation** sub-variants: `Spend`, `CreateBudget`, `CancelBudget`, `ModifySpendingRules`, `TransferBetweenBudgets`, `Withdraw`.

### 1.2 Voting Mechanics

**Core types** at `icn-governance/src/vote.rs`:

- `VoteChoice`: `For`, `Against`, `Abstain`
- `Vote`: `{ proposal_id, voter: Did, choice, weight: usize, timestamp, comment: Option<String> }`
- `VoteTally`: `{ for_votes, against_votes, abstain_votes }` with `from(Vec<Vote>)`, `approval_percentage()`, `total_votes()`

**Quorum & Threshold** at `icn-governance/src/domain.rs` (GovernanceParams):
- `quorum_percentage: u8` — minimum participation required
- `approval_threshold_percentage: u8` — threshold for acceptance
- Quorum = `(total_votes * 100) / total_members`
- If quorum not met → `ProposalState::NoQuorum`
- If quorum met and approval >= threshold → `ProposalState::Accepted`
- Else → `ProposalState::Rejected`

**Delegation** at `icn-governance/src/delegation.rs`:
- `Delegation`: `{ delegator, delegate, domain_id, created_at, expires_at, is_revoked }`
- Gateway endpoints: `POST /delegations`, `GET /delegations`, `GET /delegations/{id}`, `DELETE /delegations/{id}`
- **Note**: Delegation is stored but **not applied** during vote tallying in `governance_mgr.rs:close_proposal`. The `VoteTally::from(votes)` simply counts cast votes. Delegated votes do NOT automatically count.

**Weighted Voting**: Vote has `weight: usize` field. `VoteTally` sums weights. The gateway `cast_vote` handler sets weight=1 by default (`governance.rs:1455`). No trust-based weighting is wired up.

**Deliberation**: Proposals can enter a deliberation period via `POST /proposals/{id}/deliberation/start` and `POST /proposals/{id}/deliberation/end`. Comments system: `POST /proposals/{id}/comments`, `GET /proposals/{id}/comments`, reactions on comments.

### 1.3 Decision → Receipt Chain

**GovernanceProof (V1)** at `icn-governance/src/proof.rs:54`:
- Node-local proof with: `proposal_id`, `domain_id`, `outcome`, `vote_tally`, `vote_hash`, `timestamp`, `signer_did`, `proof_hash`, `signature`
- `proof_hash` = blake3 with domain tag `"icn:governance-proof:v1"`, length-prefixed fields
- `verify_binding()` recomputes and compares
- `sign()` / `verify_signature()` with Ed25519
- `vote_hash` = merkle root of sorted (voter, choice, weight) tuples

**GovernanceDecisionReceipt (V2)** at `proof.rs:224`:
- Cross-node deterministic: `proposal_id`, `domain_id`, `outcome`, `vote_tally`, `vote_hash`, `decision_hash`
- `decision_hash` = blake3 with domain tag `"icn:gov:decision:v1"`, excludes node-local fields (timestamp, signer)
- `PartialEq/Eq` anchored to `decision_hash` only
- `verify()` recomputes and compares
- `from_legacy(GovernanceProof)` conversion

**GovernanceDecisionAttestation** at `proof.rs:362`:
- Node-local signed attestation: `decision_hash`, `signer_did`, `timestamp`, `signature`
- Domain tag: `"icn:gov:attest:v1"`
- `sign()` / `verify()` with Ed25519

**GovernanceProofV2** at `proof.rs:423`:
- Bundle: `{ receipt: GovernanceDecisionReceipt, attestations: Vec<GovernanceDecisionAttestation> }`
- `PartialEq` on `receipt.decision_hash` only
- `verify_receipt()` delegates to `receipt.verify()`

**Where receipt is created**: In standalone mode (`governance_mgr.rs:638`), `close_proposal` DOES NOT create a GovernanceProof or GovernanceDecisionReceipt. It only updates `ProposalState` to Accepted/Rejected/NoQuorum. In actor-backed mode, the `GovernanceActor` (via `handle.close_proposal()`) would generate proofs, but the gateway delegates to `handle.get_proof()` which returns `None` in standalone mode (`governance_mgr.rs:790`).

**Critical Gap**: The receipt generation happens ONLY in actor-backed mode (daemon with GovernanceActor). The gateway standalone mode (which is what the pilot currently uses) does NOT generate GovernanceDecisionReceipt or GovernanceProofV2 on close.

### 1.4 Effect Path: Decision → Treasury → Settlement → Ledger

**The intended chain:**
```
GovernanceDecisionReceipt → AllocationReceipt → SettlementIntent(s) → LedgerEntry
```

**AllocationReceipt** at `icn-kernel-api/src/receipts.rs:121`:
- Fields: `receipt_id`, `decision_hash`, `intents: Vec<SettlementIntent>`, `scope`, `created_at`, `provenance`, `signature`
- Implements `CanonicalReceipt` trait
- `provenance` links back to `decision_hash` via `ProvenanceAnchors::from_parent(decision_hash)`
- Canonical hash is deterministic (sorted intent hashes, excludes receipt_id and signature)

**SettlementIntent** at `icn-kernel-api/src/economics.rs:85`:
- Fields: `intent_id`, `decision_receipt_id`, `decision_hash`, `asset: AssetType`, `from`, `to`, `amount`, `unit`, `scope`, `memo`, `provenance`, `created_at`
- Implements `CanonicalReceipt` trait
- Memo excluded from canonical hash
- 6 AssetType variants: Fungible, Consumable, Depreciating, Transformable, Service, Claim

**ReceiptStore** at `icn-gateway/src/receipt_store.rs:20`:
- Sled-backed store for AllocationReceipt and SettlementIntent
- Stores by canonical hash
- Indexes by decision_hash
- Methods: `put_allocation`, `get_allocation`, `put_intent`, `get_intent`, `list_allocations_by_decision`, `list_intents_by_decision`, `put_allocation_with_intents`, `get_chain_by_decision`
- **But**: No code path automatically creates AllocationReceipt/SettlementIntent when a governance decision occurs

**SettlementEngine** at `icn-ledger/src/settlement.rs:86`:
- Takes `SettlementRequest` (not `SettlementIntent`) — different type!
- Creates balanced double-entry `JournalEntry`
- Dedup by `sha256("icn-ledger:settlement:v1:" || receipt_hash)`
- Scope routing: Local/Cell/Org settle directly; Federation/Commons rejected (must use `ReceiptClearingManager`)

**SettlementEngine** at `icn-ledger/src/settlement.rs`:
- Works with `SettlementRequest` (from compute layer execution receipts), NOT with `SettlementIntent`
- Different type from the governance path's `SettlementIntent`

**IMPORTANT: Daemon-mode Effect Execution EXISTS** at `icn-core/src/supervisor/governance_executor.rs`:
- `KernelGovernanceExecutor` implements `EffectExecutor` trait
- Handles: Treasury (spend via `KernelTreasuryExecutor`), Protocol (param changes), Federation, Control (veto/force-close), Membership (freeze/unfreeze)
- `KernelTreasuryExecutor` has optional `ledger: Option<Arc<dyn LedgerService>>` — when present, treasury spends submit `TreasuryEntryRequest` to ledger service
- **JournalEntry already has provenance fields**: `decision_receipt_id: Option<String>` and `decision_hash: Option<String>` at `icn-ledger/src/types.rs:82-99`
- `JournalEntryBuilder::with_decision_provenance()` wires governance decision hash into ledger entries
- This path is ONLY available when running as full daemon (icnd) with GovernanceActor — NOT in gateway standalone mode

**Gateway standalone mode gap remains**:
- `propose_spend` creates a `ProposalPayload::Treasury { operation: Spend { ... } }` proposal
- In standalone mode, `close_proposal` sets outcome to Accepted but no effect execution occurs
- The `KernelGovernanceExecutor` effect path is not wired into the standalone gateway

### 1.5 Receipt Store & Provenance

**CanonicalReceipt trait** at `icn-kernel-api/src/receipts.rs:77`:
```rust
pub trait CanonicalReceipt {
    fn canonical_hash(&self) -> Hash;      // Deterministic cross-node hash
    fn receipt_id(&self) -> &ReceiptId;     // Node-local ID
    fn provenance(&self) -> &ProvenanceAnchors;  // Upstream chain
}
```

**Implementors**: `AllocationReceipt`, `SettlementIntent`

**ProvenanceAnchors** at `receipts.rs:36`:
- `upstream_hashes: Vec<Hash>` — links to parent receipts
- `root()` — genesis receipt
- `from_parent(hash)` — single parent
- `from_parents(hashes)` — multiple parents

**ArtifactReceipt** at `icn-kernel-api/src/proofs.rs`:
- Separate from CanonicalReceipt — used for compute task artifacts
- Fields: `receipt_hash`, `artifact_hash`, `artifact_size`, `submitter_did`, `executor_did`, `attester_did`, `created_at`, `binding_hash`, `signatures`
- `verify_binding()` with blake3 domain-separated hash

**ReceiptStore (icn-rpc)** at `icn-rpc/src/receipt.rs:198`:
- In-memory TTL-bounded LRU cache for RPC operation receipts
- Different from the gateway ReceiptStore — this is for RPC operation tracking, not governance receipts

**Two separate ReceiptStores exist**: Gateway (sled-backed, for AllocationReceipt/SettlementIntent) and RPC (in-memory LRU, for RPC operation receipts). Neither stores GovernanceDecisionReceipt/GovernanceProofV2.

### 1.6 Gateway Endpoints

#### Governance Endpoints (prefix: `/v1/governance/`)

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/domains` | `create_domain` | Create governance domain |
| GET | `/domains` | `list_domains` | List domains |
| GET | `/domains/{id}` | `get_domain` | Get domain details |
| POST | `/domains/{domain_id}/members` | `add_domain_member` | Add member to domain |
| POST | `/proposals` | `create_proposal` | Create proposal |
| GET | `/proposals` | `list_proposals` | List proposals (filter by domain/state) |
| GET | `/proposals/{id}` | `get_proposal` | Get proposal details |
| GET | `/proposals/{id}/votes` | `get_votes` | Get vote tally |
| GET | `/proposals/{id}/proof` | `get_proposal_proof` | Get GovernanceProofV2 (verified) |
| POST | `/proposals/{id}/open` | `open_proposal` | Open proposal for voting |
| POST | `/proposals/{id}/close` | `close_proposal` | Close proposal, determine outcome |
| POST | `/proposals/{id}/vote` | `cast_vote` | Cast vote |
| POST | `/delegations` | `create_delegation` | Create vote delegation |
| GET | `/delegations` | `list_delegations` | List delegations |
| GET | `/delegations/{id}` | `get_delegation` | Get delegation |
| DELETE | `/delegations/{id}` | `revoke_delegation` | Revoke delegation |
| POST | `/proposals/{id}/deliberation/start` | `start_deliberation` | Start deliberation period |
| POST | `/proposals/{id}/deliberation/end` | `end_deliberation` | End deliberation period |
| POST | `/proposals/{id}/comments` | `add_comment` | Add comment |
| GET | `/proposals/{id}/comments` | `list_comments` | List comments |
| GET | `/proposals/{id}/discussion` | `get_discussion` | Get discussion with reactions |
| PUT | `/comments/{id}` | `edit_comment` | Edit comment |
| DELETE | `/comments/{id}` | `delete_comment` | Delete comment |
| POST | `/comments/{id}/reactions` | `add_reaction` | Add reaction |
| DELETE | `/comments/{id}/reactions/{emoji}` | `remove_reaction` | Remove reaction |
| POST | `/domains/{domain_id}/action-items` | `create_action_item` | Create action item |
| GET | `/domains/{domain_id}/action-items` | `list_action_items` | List action items |
| GET | `/domains/{domain_id}/action-items/{item_id}` | `get_action_item` | Get action item |
| PUT | `/domains/{domain_id}/action-items/{item_id}` | `update_action_item` | Update action item |
| DELETE | `/domains/{domain_id}/action-items/{item_id}` | `delete_action_item` | Delete action item |
| POST | `/domains/{domain_id}/action-items/{item_id}/notes` | `add_action_item_note` | Add note |
| PUT | `/domains/{domain_id}/action-items/{item_id}/status` | `update_action_item_status` | Update status |

**Federation proposal endpoints** (7 specialized POST endpoints):
- `POST /proposals/federation/join`
- `POST /proposals/federation/leave`
- `POST /proposals/federation/clearing`
- `POST /proposals/federation/clearing/terminate`
- `POST /proposals/federation/vouch`
- `POST /proposals/federation/vouch/revoke`
- `POST /proposals/federation/policy`

#### Governance Dashboard

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/governance/{charter_id}/dashboard` | `get_governance_dashboard` | Aggregate stats (amendments/appeals) |

#### Receipt Endpoints (prefix: `/v1/receipts/`)

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/allocations/{hash}` | `get_allocation` | Get AllocationReceipt by hash |
| GET | `/intents/{hash}` | `get_intent` | Get SettlementIntent by hash |
| GET | `/chain?decision_hash=...` | `get_chain` | Get full economic chain for decision |
| GET | `/allocations?decision_hash=...` | `list_allocations` | List allocations for decision |
| GET | `/intents?decision_hash=...` | `list_intents` | List intents for decision |

**Total: 5 receipt endpoints**, focused on AllocationReceipt and SettlementIntent. GovernanceDecisionReceipt served via `/proposals/{id}/proof`.

#### Constitutional Endpoints (prefix: `/v1/constitutional/`)

**Amendment endpoints** (8):

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/amendments` | `create_amendment` | Create charter amendment |
| GET | `/amendments` | `list_amendments` | List amendments (paginated) |
| GET | `/amendments/{id}` | `get_amendment` | Get amendment detail |
| POST | `/amendments/{id}/submit` | `submit_amendment` | Submit for review |
| POST | `/amendments/{id}/vote` | `open_amendment_voting` | Open amendment voting |
| POST | `/amendments/{id}/ratify` | `ratify_amendment` | Ratify amendment |
| POST | `/amendments/{id}/changes` | `add_amendment_change` | Add changes to amendment |
| POST | `/amendments/{id}/withdraw` | `withdraw_amendment` | Withdraw amendment |

**Amendment voting endpoints** (4):

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/amendments/{id}/votes` | `cast_vote` | Cast vote on amendment |
| GET | `/amendments/{id}/votes` | `list_votes` | List votes |
| GET | `/amendments/{id}/results` | `get_results` | Get voting results |
| GET | `/amendments/{id}/my-vote` | `get_my_vote` | Get current user's vote |

**Appeal endpoints** (8):

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/appeals` | `file_appeal` | File new appeal |
| GET | `/appeals` | `list_appeals` | List appeals (paginated) |
| GET | `/appeals/{id}` | `get_appeal` | Get appeal detail |
| POST | `/appeals/{id}/evidence` | `add_appeal_evidence` | Add evidence |
| POST | `/appeals/{id}/respond` | `add_appeal_response` | Add response |
| POST | `/appeals/{id}/review` | `begin_appeal_review` | Start review |
| POST | `/appeals/{id}/resolve` | `resolve_appeal` | Resolve appeal |
| POST | `/appeals/{id}/withdraw` | `withdraw_appeal` | Withdraw appeal |

**Appeal UI endpoints** (4):

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/appeals/{id}/timeline` | `get_appeal_timeline` | Appeal event timeline |
| GET | `/appeals/{id}/status` | `get_appeal_status` | Status detail + next steps + deadlines |
| POST | `/appeals/{id}/assign-reviewer` | `assign_reviewer` | Assign reviewer |
| GET | `/appeals/dashboard` | `get_appeals_dashboard` | Metrics + recent appeals |

#### Ledger Provenance Endpoints (prefix: `/v1/ledger/`)

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/{coop_id}/entries/by-decision` | `get_entries_by_decision` | Trace ledger entries back to governance decision |
| GET | `/{coop_id}/history` | `get_history` | Transaction history (paginated) |

**Grand total: ~76 governance-related endpoints across governance, constitutional, receipts, ledger, and treasury modules.**

#### Treasury Endpoints (prefix: `/v1/treasury/`)

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/{coop_id}` | `get_treasury_status` | Treasury status |
| GET | `/{coop_id}/balance` | `get_treasury_balance` | Treasury balance |
| GET | `/{coop_id}/budgets` | `list_budgets` | List budgets |
| GET | `/{coop_id}/budgets/{budget_id}` | `get_budget` | Get budget |
| POST | `/{coop_id}/budgets` | `create_budget` | Create budget (governance) |
| GET | `/{coop_id}/spending-rules` | `list_spending_rules` | List spending rules |
| GET | `/{coop_id}/audit` | `get_audit_trail` | Audit trail |
| POST | `/{coop_id}/deposit` | `deposit_to_treasury` | Deposit |
| POST | `/{coop_id}/spend` | `propose_spend` | Propose spend (creates governance proposal) |

### 1.7 Appeals System

**AppealType** at `icn-governance/src/appeal.rs:66` — 7 variants:
1. `Revocation` — appeal revocation of membership/anchor/holder status
2. `Suspension` — appeal a suspension
3. `GovernanceDecision` — appeal a governance decision (with `proposal_id`, `decision`)
4. `DisputeResolution` — appeal a dispute resolution outcome
5. `MembershipDenial` — appeal membership denial
6. `StewardAction` — appeal steward action
7. `Other` — general appeal

**AppealGrounds** at `appeal.rs:269` — 8 variants:
1. `ProceduralError`
2. `NewEvidence`
3. `ExceededAuthority`
4. `RightsViolation`
5. `FactualError`
6. `Bias`
7. `DisproportionatePenalty`
8. `Other`

**AppealRemedy** at `appeal.rs:252` — 6 variants:
1. `Reverse` — reverse original decision
2. `Reinstate` — reinstate membership/status
3. `Modify` — modify decision
4. `Compensation` — monetary compensation
5. `Custom` — custom remedy
6. `None` — declaratory only

**AppealStatus** at `appeal.rs:155` — 6 states:
1. `Filed { filed_at }`
2. `UnderReview { filed_at, review_started_at }`
3. `Hearing { filed_at, hearing_started_at, hearing_ends_at }`
4. `Resolved { filed_at, resolved_at, outcome: AppealOutcome }`
5. `Dismissed { filed_at, dismissed_at, reason }`
6. `Withdrawn { filed_at, withdrawn_at, reason }`

**AppealOutcome** at `appeal.rs:221` — 4 variants:
- `Upheld { reason, remedy }`, `Denied { reason }`, `PartiallyUpheld { reason, remedy }`, `Remanded { instructions, remanded_to }`

**AppealScope** at `appeal.rs:134` — 3 levels:
- `Jurisdiction`, `Federation`, `Network`

**AppealDeadlines** at `appeal.rs:304`:
- Default: 30d filing, 14d review, 7d hearing, 90d max
- Expedited: 7d/3d/2d/21d
- Extended: 60d/30d/14d/180d

**Appeal evidence**: `AppealEvidence` struct with `evidence_type` (Document, Testimony, Record, Communication, SystemLog, Other), hash, submitted_at.

**Receipt integration**: Appeals have NO connection to the receipt system. No `decision_hash` or `ProvenanceAnchors` on Appeal. The `GovernanceDecision` appeal type stores only `proposal_id` (string) and `decision` (string description), not a `decision_hash`.

---

## 2. Chain Completeness Audit

| Link | Exists? | Connected? | Human-traceable? | Notes |
|------|---------|------------|-------------------|-------|
| Proposal → Vote | YES | YES | YES | Votes stored per proposal; tally computed on close |
| Vote → Decision | YES | PARTIAL | PARTIAL | State transitions to Accepted/Rejected/NoQuorum; but GovernanceDecisionReceipt only generated in actor-backed mode, not standalone |
| Decision → Receipt | PARTIAL | PARTIAL | NO | GovernanceProofV2 exists with canonical hashing and attestations, but only generated when GovernanceActor is running; standalone mode returns None |
| Receipt → Allocation | DESIGNED | NO | NO | AllocationReceipt has `decision_hash` field linking to receipt, but NO code path creates AllocationReceipt from a governance decision |
| Allocation → Settlement | DESIGNED | NO | NO | AllocationReceipt contains `Vec<SettlementIntent>`, but no code creates intents from governance decisions |
| Settlement → LedgerEntry | YES (daemon) / NO (standalone) | YES (daemon) | PARTIAL | `KernelGovernanceExecutor` → `KernelTreasuryExecutor` → `LedgerService.submit_treasury_entry()` in daemon mode. Gateway standalone has no path. |
| LedgerEntry → Provenance | YES (daemon) | YES (daemon) | PARTIAL | `JournalEntry.decision_receipt_id` and `decision_hash` fields set by `JournalEntryBuilder::with_decision_provenance()` in daemon mode. Not set in standalone mode. |
| Appeal → Receipt | NO | NO | NO | Appeals have no receipt chain integration — AppealType::GovernanceDecision stores proposal_id as string, not decision_hash |

**Summary**: The top half (Proposal→Vote→Decision) works in both modes. In daemon mode (icnd with GovernanceActor), the full chain works: decision→receipt→effect execution→treasury→ledger entry with provenance. In gateway standalone mode (current pilot default), the chain is **broken at Decision→Receipt** — no receipt generated, no effect execution. The gap is a **mode gap**, not a missing-types gap.

---

## 3. Issue #1012 Analysis

Issue #1012 specifies "Constraints with Receipts" pattern:
- **What**: constraint value(s)
- **Why**: policy/authority
- **Who**: authority
- **When**: timestamp

### Fields ALREADY available:

| #1012 Field | Available in Receipt Types | Where |
|-------------|--------------------------|-------|
| **What** (constraint value) | `ConstraintSet` has `rate_limit`, `max_topics`, `max_message_size`, `custom: HashMap<String, ConstraintValue>` | `icn-kernel-api/src/authz.rs:155` |
| **Why** (policy/authority) | `PolicyDecision::Deny { reason: PolicyError }` has 6 error variants. `ConstraintSet` has no "why" field. GovernanceDecisionReceipt has `outcome` + `vote_tally` but no explicit policy reference. | Partially available |
| **Who** (authority) | `GovernanceProofV2.attestations[].signer_did`. `SettlementIntent.decision_receipt_id`. `AllocationReceipt.provenance.upstream_hashes`. But: no authority DID on ConstraintSet. | Partially available |
| **When** (timestamp) | `AllocationReceipt.created_at`, `SettlementIntent.created_at`, `GovernanceProof.timestamp`, `GovernanceDecisionAttestation.timestamp` | Available |

### Fields MISSING:

1. **Policy Reference on ConstraintSet**: When the kernel enforces a constraint, there is no field saying "this constraint came from TrustPolicyOracle evaluating policy X". The `custom` hashmap COULD carry this, but no oracle currently sets a `policy_ref` key.

2. **Decision → Constraint Provenance**: When a governance decision produces a ConstraintSet change, there is no link from the new constraint to the decision that caused it. The ConstraintSet is ephemeral (computed per-request by PolicyOracle), not persisted with provenance.

3. **Constraint History**: No history of past constraint values. You can see what a constraint IS but not what it WAS or WHY it changed.

4. **Authority DID on Constraint**: ConstraintSet doesn't carry the DID of the authority that imposed it. The PolicyOracle knows, but the kernel discards that information at the meaning firewall.

---

## 4. Gap Analysis

### Gap G1: Decision Receipt Not Generated in Standalone Mode

- **User story blocked**: "View decision receipt for a closed proposal" — returns 404 in standalone gateway
- **Missing**: Runtime receipt generation outside GovernanceActor
- **Where**: `icn-gateway/src/governance_mgr.rs:close_proposal` (standalone branch)
- **Receipt expectation**: GovernanceDecisionReceipt with canonical hash, verified attestation
- **Phase**: 0

### Gap G2: Effect Execution Only Available in Daemon Mode

- **User story blocked**: "Treasury spend proposal passes → money actually moves" (in standalone gateway)
- **In daemon mode**: `KernelGovernanceExecutor` at `icn-core/src/supervisor/governance_executor.rs` handles Treasury, Protocol, Federation, Control, Membership effects. Treasury spends go through `KernelTreasuryExecutor` → `LedgerService.submit_treasury_entry()` with provenance. **This works.**
- **In standalone mode**: Gateway `close_proposal` only updates proposal state. No effect execution.
- **Where**: `icn-gateway/src/governance_mgr.rs` standalone branch vs `icn-core/src/supervisor/governance_executor.rs`
- **Receipt expectation**: AllocationReceipt → SettlementIntent(s) → JournalEntry chain with provenance
- **Phase**: 0 (ensure daemon mode used for pilot) / 1 (bring effect execution to standalone or make standalone unnecessary)

### Gap G3: Receipt Chain Not Populated at Runtime

- **User story blocked**: "View economic chain for a governance decision" — `/v1/receipts/chain?decision_hash=...` returns empty
- **Missing**: Code that creates and stores AllocationReceipt + SettlementIntent when a decision takes effect
- **Where**: `icn-gateway/src/receipt_store.rs` has storage, no producers
- **Receipt expectation**: Full chain visible via API
- **Phase**: 0 (for display of existing data) / 1 (for runtime generation)

### Gap G4: SettlementIntent ≠ SettlementRequest Type Mismatch

- **User story blocked**: "Governance treasury spend settles in ledger"
- **Missing**: Converter from `SettlementIntent` (governance path) to `SettlementRequest` (ledger engine) or unified type
- **Where**: `icn-kernel-api/src/economics.rs` vs `icn-ledger/src/settlement.rs`
- **Receipt expectation**: Unified settlement path
- **Phase**: 1

### Gap G5: Appeal Not Linked to Receipt Chain

- **User story blocked**: "File appeal against decision with decision_hash"
- **Missing**: `AppealType::GovernanceDecision` uses string `proposal_id`, not `decision_hash`
- **Where**: `icn-governance/src/appeal.rs:82`
- **Receipt expectation**: Appeal records decision_hash, appeal resolution produces own receipt
- **Phase**: 1

### Gap G6: Delegation Not Applied in Vote Tally

- **User story blocked**: "Delegate my vote to a trusted representative"
- **Missing**: Delegation application in `close_proposal` tally computation
- **Where**: `governance_mgr.rs:678` — `VoteTally::from(proposal_votes)` ignores delegations
- **Receipt expectation**: Vote tally includes delegated votes
- **Phase**: 1

### Gap G7: No Constraint Provenance (Issue #1012)

- **User story blocked**: "See WHY this rate limit was imposed on me"
- **Missing**: Policy reference / authority / timestamp on ConstraintSet or PolicyDecision
- **Where**: `icn-kernel-api/src/authz.rs:155` — ConstraintSet has no provenance
- **Receipt expectation**: "What / Why / Who / When" visible in dashboard
- **Phase**: 0 (stub with available data) / 1 (full provenance chain)

### Gap G8: No GovernanceDecisionReceipt Storage

- **User story blocked**: "Look up any past governance decision by decision_hash"
- **Missing**: Persistent storage for GovernanceDecisionReceipt (not just in GovernanceActor memory)
- **Where**: No ReceiptStore method for GovernanceDecisionReceipt
- **Receipt expectation**: Sled-persisted, queryable by proposal_id or decision_hash
- **Phase**: 0

---

## 5. Phase 0 Tasks (5 tasks)

### P0-GOV-T01: Generate GovernanceDecisionReceipt in standalone close_proposal

- **What**: When `close_proposal` runs in standalone mode, compute `GovernanceDecisionReceipt` from the votes and store it
- **Files**: `icn-gateway/src/governance_mgr.rs` (close_proposal standalone branch), add `decision_receipts: RwLock<HashMap<ProposalId, GovernanceDecisionReceipt>>` storage
- **Test**: Close a proposal → `get_proof` returns a valid GovernanceDecisionReceipt with `verify()` == true
- **Demo**: Create proposal → vote → close → GET `/proposals/{id}/proof` returns valid receipt

### P0-GOV-T02: Store GovernanceDecisionReceipt in ReceiptStore

- **What**: Extend `receipt_store.rs` to store/index GovernanceDecisionReceipt by decision_hash and proposal_id
- **Files**: `icn-gateway/src/receipt_store.rs` (add GovernanceDecisionReceipt methods), `icn-gateway/src/api/receipts.rs` (add endpoint)
- **Test**: Put receipt → get by hash → get by proposal_id → all return same receipt
- **Demo**: GET `/v1/receipts/decisions/{hash}` returns GovernanceDecisionReceipt

### P0-GOV-T03: Wire close_proposal → ReceiptStore

- **What**: After generating GovernanceDecisionReceipt in close_proposal, store it in ReceiptStore
- **Files**: `icn-gateway/src/governance_mgr.rs`, `icn-gateway/src/server.rs` (inject ReceiptStore dependency)
- **Test**: Full flow: create → vote → close → receipt stored → queryable via API
- **Demo**: Receipts Explorer can show decision receipt for any closed proposal

### P0-GOV-T04: Receipts Explorer data contract

- **What**: Define and serve the data needed for a Receipt Explorer UI — decision receipt + linked chain (even if chain is empty for now)
- **Files**: `icn-gateway/src/api/receipts.rs` (add unified chain endpoint that includes GovernanceDecisionReceipt)
- **Test**: API contract test returning `{ decision: GovernanceDecisionReceipt, allocations: [], intents: [] }`
- **Demo**: Frontend can render a receipt with "Verified" badge when `verify()` passes

### P0-GOV-T05: Constraint provenance stub (#1012)

- **What**: Add `policy_domain: Option<String>` and `evaluated_at: Option<u64>` to PolicyDecision or a new `ConstraintProvenance` struct. TrustPolicyOracle sets these when returning constraints.
- **Files**: `icn-kernel-api/src/authz.rs` (add provenance fields or struct), `apps/trust/src/` (set fields)
- **Test**: TrustPolicyOracle returns PolicyDecision with `policy_domain = "trust"`, `evaluated_at = now`
- **Demo**: Dashboard shows "Rate limit: 20 msg/sec | Source: Trust Policy | Trust Score: 0.35"

---

## 6. Phase 1 Tasks (5 tasks)

### P1-GOV-T01: Effect execution engine for treasury proposals

- **What**: When a Treasury::Spend proposal is accepted, create AllocationReceipt + SettlementIntent(s), store in ReceiptStore, and execute the spend against the ledger
- **Files**: New `icn-gateway/src/effect_executor.rs` or within `governance_mgr.rs`, `receipt_store.rs`
- **Test**: Treasury spend proposal accepted → AllocationReceipt created → SettlementIntent stored → balance changes in ledger
- **Demo**: Full treasury spend lifecycle visible in Receipts Explorer

### P1-GOV-T02: Unify SettlementIntent → SettlementRequest

- **What**: Either make SettlementEngine accept SettlementIntent directly, or create a converter. The governance path and compute path should both reach the ledger.
- **Files**: `icn-ledger/src/settlement.rs`, `icn-kernel-api/src/economics.rs`
- **Test**: SettlementIntent from governance → settled in ledger → JournalEntry created
- **Demo**: Provenance chain: decision_hash → allocation → intent → journal entry

### P1-GOV-T03: Appeal receipt chain integration

- **What**: Add `decision_hash: Option<Hash>` to `AppealType::GovernanceDecision`. When an appeal resolves, create an AppealReceipt linking to the original decision.
- **Files**: `icn-governance/src/appeal.rs`, new `AppealReceipt` type (or extend CanonicalReceipt)
- **Test**: File appeal with decision_hash → resolve → receipt links back to original decision
- **Demo**: Appeal shows "Appealing decision 0xABC123..." with link to original receipt

### P1-GOV-T04: Apply delegated votes in tally

- **What**: When computing vote tally in close_proposal, check for active delegations and include delegated votes
- **Files**: `icn-gateway/src/governance_mgr.rs` (close_proposal), delegation lookup
- **Test**: Alice delegates to Bob, Bob votes For, tally includes Alice's delegated weight
- **Demo**: Vote tally shows "3 For (including 1 delegated)" in proposal detail

### P1-GOV-T05: Full provenance navigation in receipt API

- **What**: Add upstream/downstream navigation to receipt endpoints. Given any receipt hash, return its upstream chain (provenance) and downstream dependents.
- **Files**: `icn-gateway/src/receipt_store.rs` (add reverse index), `icn-gateway/src/api/receipts.rs` (add navigation endpoints)
- **Test**: Decision receipt → follow provenance chain → allocation → intents → all linked
- **Demo**: Receipts Explorer shows clickable chain: Decision → Allocation → Settlement → Ledger Entry

---

## 7. Relevant Open Issues

- **#1012**: Legibility Dashboards UX Spec — "Constraints with Receipts" pattern (What/Why/Who/When). Directly drives P0-GOV-T05 and P1-GOV-T05.
- **#246**: Treasury operations — drives the treasury proposal types and P1-GOV-T01 (effect execution).
- **#25**: Emergency proposals — already implemented as ProposalPayload variants (FreezeMember, VetoProposal, ForceCloseProposal, RollbackLedger).
- **#52**: Dispute escalation — DisputeResolution proposal variant exists.
- **#389**: Labor share operations — SurplusAllocation and ShareRedemption proposal variants exist.

---

## Summary: Chain Status

```
Proposal → Vote → Decision    [WORKS in standalone + daemon mode]
                    ↓
    GovernanceDecisionReceipt  [WORKS in daemon mode, BROKEN in standalone]
                    ↓
    KernelGovernanceExecutor   [WORKS in daemon mode, NOT AVAILABLE in standalone]
         ↓ (effect execution)
    KernelTreasuryExecutor     [WORKS in daemon w/ LedgerService, placeholder otherwise]
         ↓
    TreasuryEntryRequest       [WORKS in daemon mode]
         ↓
    JournalEntryBuilder        [WITH decision_receipt_id + decision_hash provenance]
         ↓
    JournalEntry               [PROVENANCE FIELDS EXIST: decision_receipt_id, decision_hash]
         ↓
    Merkle-DAG                 [IMMUTABLE, PARENTS LINKED]
```

**Bottom line**: The governance decision chain is ~60% connected in daemon mode (icnd), but ~30% in standalone gateway mode (current pilot default). The full effect execution path exists: `KernelGovernanceExecutor` → `KernelTreasuryExecutor` → `LedgerService` → `JournalEntry` with provenance. The gap is a **mode gap**: standalone gateway doesn't use the daemon's execution infrastructure. Phase 0 should either (a) make standalone generate receipts, or (b) ensure the pilot uses daemon mode. Phase 1 connects the receipt store and makes everything queryable via API.
