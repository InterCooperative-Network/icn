# Governance Model Validation

**Status**: Authoritative Truth Document
**Last Updated**: 2026-02-17
**Purpose**: Maps every governance operation a cooperative needs against ICN's actual implementation state.

> This is the single source of truth for what ICN's governance system **actually does** vs. what it **claims to do**. Every entry has been verified against the codebase. Generated from codebase audit against 5 cooperative governance scenarios.

## How to Read This Document

Each scenario includes a **gap matrix** table. The columns are:

| Column | Meaning |
|--------|---------|
| Operation | A discrete step required by the scenario |
| ICN API/Type | The Rust struct, function, or endpoint that handles this. "None" if absent |
| State Location | Where data lives: governance store, sled, in-memory, nowhere |
| Test Evidence | Specific test functions. "None" if untested |
| Status | `WORKS` / `TYPES_ONLY` / `MISSING` |
| Gap | Why it does not work end-to-end (if applicable) |

**Status key**:
- `WORKS` -- Exercised end-to-end with tests proving the path.
- `TYPES_ONLY` -- Rust types exist but no execution bridge, no persistence, or no API endpoint.
- `MISSING` -- No types, no code, no tests.

---

## Scenario 1: Annual Board Election

A cooperative elects 3 board members from 5 candidates using ranked-choice voting.

| Operation | ICN API/Type | State Location | Test Evidence | Status | Gap |
|-----------|-------------|----------------|---------------|--------|-----|
| Nomination submission | None | Nowhere | None | MISSING | No `ElectionProposal`, `Nomination`, or `Candidate` types. `ProposalPayload` has no `Election` variant. |
| Candidate eligibility check | None | Nowhere | None | MISSING | `MembershipConfig` has trust thresholds but no candidacy qualifications (min term, role-specific). |
| Ballot creation (ranked-choice / plurality) | None | N/A | None | MISSING | `VoteChoice` is `{For, Against, Abstain}` only (`vote.rs:9-18`). Cannot express ranked preferences. |
| Vote casting | `Vote::new()`, `GovernanceStore::store_vote()` | GovernanceStore | `test_vote_creation` (`vote.rs:77`) | TYPES_ONLY | Works for binary For/Against/Abstain. Cannot rank candidates. |
| Vote tallying | `VoteTally` (`tally.rs:13-21`), `compute_tally_with_delegations()` | Computed | `test_add_votes` (`tally.rs:211`) | TYPES_ONLY | Counts for/against/abstain only. No ranked-choice elimination rounds. |
| Winner determination | None | Nowhere | None | MISSING | `DecisionOutcome` is `{Accepted, Rejected, NoQuorum}` (`profile.rs:37-47`). No "winner" concept. |
| Term start/end | `StewardRecord::term_start/term_end` (`steward.rs:74-78`) | StewardStore | Steward tests only | TYPES_ONLY | Term tracking exists for SDIS stewards only, not general board roles. |
| Role assignment | None | Nowhere | None | MISSING | No role registry. No concept of "board member" distinct from "member". |

**Summary**: Elections are completely absent. ICN has a binary yes/no proposal system. Zero election primitives exist: no nominations, no multi-candidate ballots, no ranked-choice tallying, no winner determination, no role assignment.

---

## Scenario 2: Budget Approval Cycle

A cooperative submits an annual budget, deliberates, votes, and activates spending limits.

| Operation | ICN API/Type | State Location | Test Evidence | Status | Gap |
|-----------|-------------|----------------|---------------|--------|-----|
| Budget proposal submission | `ProposalPayload::Budget { amount, currency, recipient, purpose }` (`proposal.rs:403-413`) | GovernanceStore | None (Membership variant tested, not Budget) | TYPES_ONLY | Type exists. Submittable via `POST /gov/proposals`. No budget-specific validation. |
| Deliberation period | `Proposal::start_deliberation()`, `ProposalState::Deliberation` | GovernanceStore | `test_proposal_deliberation_lifecycle` (integration) | WORKS | Transitions Draft -> Deliberation with configurable period. |
| In-flight amendment | None | N/A | None | MISSING | Doc states: "Amendments handled by cancelling and resubmitting" (`proposal.rs:203-206`). Design decision. |
| Final budget vote | `GovernanceOps::cast_vote()` + `close_proposal()` (`handle.rs:99-107`) | GovernanceStore | `test_complete_governance_flow_accepted` | WORKS | Binary yes/no on budget proposals works. |
| Budget enforcement activation | None (executor does not handle `Budget` payload) | Nowhere | None | TYPES_ONLY | Accepted Budget proposal produces no kernel effect. `KernelGovernanceExecutor::execute_effect()` has no branch for Budget. |
| Spending limit enforcement | `TreasuryProposalOperation::ModifySpendingRule` (`proposal.rs:728-743`) | GovernanceConfig | `test_thresholds_for_treasury_withdrawal` | TYPES_ONLY | Spending rules can be proposed. No enforcement at ledger transaction time. |

**Summary**: Budget proposals flow through the proposal lifecycle (create, deliberate, vote, accept). But acceptance is a dead end -- there is no execution bridge to allocate funds or activate spending limits. The `KernelTreasuryExecutor` can execute treasury operations, but `ProposalPayload::Budget` is not mapped to a `TreasuryOperation`.

---

## Scenario 3: Bylaw Amendment

A cooperative amends its charter to change the quorum from 50% to 60%, requiring 2/3 supermajority.

| Operation | ICN API/Type | State Location | Test Evidence | Status | Gap |
|-----------|-------------|----------------|---------------|--------|-----|
| Amendment drafting | `Amendment::new()`, `AmendmentChange` (`amendment.rs:366-407`) | In-memory only | `test_amendment_creation` (`amendment.rs:714`) | TYPES_ONLY | Well-designed types. No `AmendmentStore` trait or persistence. |
| Sponsor collection | `Amendment::add_sponsor()` (`amendment.rs:446-451`) | In `Amendment` struct | `test_add_sponsor` (`amendment.rs:783`) | TYPES_ONLY | Method works (prevents duplicates, self-sponsorship). No API endpoint. |
| Supermajority threshold | `RatificationRequirements { approval_percent }` (`amendment.rs:176-246`) | In `Amendment::requirements` | `test_ratification_rejection` (`amendment.rs:924`) | WORKS | Jurisdiction=67%, Federation=67%, Network=80%, Critical=90%. Tested. |
| Voting period management | `AmendmentStatus::Voting { voting_started_at, voting_ends_at }` (`amendment.rs:126-129`) | In `Amendment::status` | `test_amendment_lifecycle` (`amendment.rs:839`) | TYPES_ONLY | State transitions work. No timer/scheduler for auto-close at period end. |
| Ratification | `Amendment::add_ratification()` + `check_ratification()` + `ratify()` (`amendment.rs:513-618`) | In `Amendment::ratifications` | `test_ratification`, `test_duplicate_ratification_prevented` | TYPES_ONLY | Multi-level ratification logic works. No persistence, no API. |
| Effective date scheduling | `AmendmentStatus::Ratified { effective_at }` (`amendment.rs:137-139`) | In `Amendment::status` | Indirect in `test_ratification` | TYPES_ONLY | Date computed (ratified_at + effective_delay_secs). Nothing enforces it. |
| Charter config update | `Charter::add_amendment(AmendmentRef)` (`charter.rs:348-350`) | Charter in CharterStore | None end-to-end | TYPES_ONLY | Appends a reference only. Does not actually update `charter.governance` config. |

**Summary**: The amendment system has excellent type design (multi-level ratification, structured changes, configurable thresholds per scope). It is entirely disconnected from execution: no persistence store, no gateway API, no timer-based period management, and no mechanism to apply ratified amendments to the charter config.

---

## Scenario 4: Emergency Action

A compromised account must be frozen immediately, with a 24-hour emergency vote.

| Operation | ICN API/Type | State Location | Test Evidence | Status | Gap |
|-----------|-------------|----------------|---------------|--------|-----|
| Emergency declaration | Implicit via payload type (`FreezeMember`, `VetoProposal`, etc.) | N/A | N/A | WORKS | Design decision: emergency = proposal with elevated thresholds, no separate state. |
| Account freeze | `ProposalPayload::FreezeMember { member, reason, duration_seconds }` (`proposal.rs:442-449`) | GovernanceStore | `test_emergency_freeze_proposal_requires_higher_quorum` | TYPES_ONLY | Proposal created and voted with 67%/75% thresholds. Executor has no handler. Ledger has no freeze API. |
| Emergency spending | `TreasuryProposalOperation::Spend`, `EmergencyThresholds::emergency_voting_period_seconds` (24h) | GovernanceConfig | `test_thresholds_for_treasury_withdrawal` | TYPES_ONLY | Shorter voting period + higher thresholds configured. Treasury executor can execute if wired. |
| Post-emergency review | None | Nowhere | None | MISSING | No automatic review trigger after emergency action completes. |
| Account unfreeze | `ProposalPayload::UnfreezeMember { member, reason }` (`proposal.rs:452-457`) | GovernanceStore | None | TYPES_ONLY | Same gap as freeze: type exists, no execution bridge. |
| Veto proposal | `ProposalPayload::VetoProposal` + `Proposal::veto()` | GovernanceStore | `test_proposal_veto_from_draft`, `test_proposal_veto_from_open` | WORKS | `KernelControlExecutor` handles veto via `ControlService`. |
| Force close | `ProposalPayload::ForceCloseProposal` + `Proposal::force_close()` | GovernanceStore | `test_proposal_force_close` | WORKS | `KernelControlExecutor` handles this via `ControlService`. |
| Audit trail | `GovernanceProofV2`, `GovernanceDecisionReceipt` (`proof.rs`) | Generated on proposal close | `test_governance_proof_roundtrip`, `test_governance_proof_tamper_detection` | WORKS | Cryptographic receipt with decision hash, signatures, tamper detection. |

**Summary**: Emergency governance is half-built. Veto, force-close, and audit trails work end-to-end. Account freeze/unfreeze and ledger rollback have types and elevated thresholds but no execution path to the ledger. Emergency voting periods (24h) are configured but not enforced by a scheduler.

---

## Scenario 5: Dispute Resolution

A member files a grievance, mediation fails, the dispute escalates to governance, and the outcome is appealed.

| Operation | ICN API/Type | State Location | Test Evidence | Status | Gap |
|-----------|-------------|----------------|---------------|--------|-----|
| Grievance filing | `ProposalPayload::DisputeResolution` (`proposal.rs:494-510`) | GovernanceStore | None | TYPES_ONLY | Type exists for **escalated** disputes only. No member-to-member grievance filing. CCL `DisputeEngine` is compute-only. |
| Mediator assignment | `DisputePolicy::ArbitratorSelection` enum (`charter.rs:625-670`) | Charter | `test_dispute_policy` (`charter.rs:900`) | TYPES_ONLY | Policy defines selection method (random, elected, party, fixed). No runtime selection code. |
| Evidence submission | `AppealEvidence { evidence_id, evidence_type, content_hash }` (`appeal.rs:350-366`) | In `Appeal::evidence` | `test_add_evidence` (`appeal.rs:907`) | TYPES_ONLY | Well-modeled for appeals. No dispute-level evidence store. |
| Mediation outcome | `DisputeResolutionOutcome` enum (in `proposal.rs`) | N/A | None | TYPES_ONLY | Type exists as proposal field. No mediation session workflow. |
| Appeal filing | `Appeal::new()` (`appeal.rs:457-491`) | In-memory only | `test_appeal_creation` (`appeal.rs:732`) | TYPES_ONLY | Rich type system. No `AppealStore` trait. No persistence. |
| Appeal review | `Appeal::begin_review()`, `begin_hearing()` (`appeal.rs:582-612`) | In `Appeal::status` | `test_appeal_lifecycle` (`appeal.rs:791`) | TYPES_ONLY | State machine works. No API to trigger transitions. |
| Binding outcome | `Appeal::resolve(AppealOutcome)` with `Upheld/Denied/PartiallyUpheld/Remanded` | In `Appeal::status` | `test_appeal_denied`, `test_partial_uphold` | TYPES_ONLY | Outcome recorded. Never executed (no effect on ledger/membership). |
| Ledger reversal | `ProposalPayload::RollbackLedger { target_hash, reason }` (`proposal.rs:481-492`) | GovernanceStore | None | TYPES_ONLY | Highest thresholds (75%/80%). No `LedgerService::rollback_to()`. No executor handler. |

**Summary**: Dispute resolution is the least complete scenario. Two disconnected dispute systems exist: (1) CCL `DisputeEngine` for compute task disputes (automated, with trust penalties), (2) governance `DisputeResolution` proposals for escalated ledger disputes. Neither provides member-to-member grievances. The appeal system has excellent type modeling but zero persistence or execution.

---

## Cross-Cutting Analysis

### What Works End-to-End (with tests)

| Capability | Key Files | Key Tests |
|------------|-----------|-----------|
| Proposal lifecycle (create/deliberate/vote/tally/close) | `proposal.rs`, `store.rs`, `handle.rs` | `test_complete_governance_flow_accepted/rejected` |
| Vote delegation (liquid democracy) | `delegation.rs`, `tally.rs` | `test_tally_with_single_delegation`, `test_tally_transitive_delegation` |
| Governance proofs (cryptographic receipts) | `proof.rs` | `test_governance_proof_roundtrip`, `test_governance_proof_tamper_detection` |
| Per-type thresholds (emergency = higher) | `config.rs` | `test_thresholds_for_freeze_member_proposal`, `test_thresholds_for_rollback_proposal` |
| Protocol parameter changes | `governance_executor.rs` (KernelProtocolExecutor) | Executor tests |
| Veto / force-close execution | `governance_executor.rs` (KernelControlExecutor) | Integration tests |
| Treasury execution (spend/allocate) | `governance_executor.rs` (KernelTreasuryExecutor) | Executor tests |

### Types Exist, Execution Absent

| Capability | Types In | Missing Piece |
|------------|----------|---------------|
| Budget activation | `ProposalPayload::Budget` | Executor branch + `LedgerService` call |
| Account freeze/unfreeze | `ProposalPayload::FreezeMember/UnfreezeMember` | `LedgerService::freeze_account()` + executor |
| Ledger rollback | `ProposalPayload::RollbackLedger` | `LedgerService::rollback_to()` + executor |
| Amendment lifecycle | `Amendment`, `AmendmentChange`, `RatificationRequirements` | `AmendmentStore` + gateway API + charter update |
| Appeal lifecycle | `Appeal`, `AppealOutcome`, `AppealRemedy` | `AppealStore` + gateway API + remedy execution |
| Dispute mediation | `DisputePolicy`, `ArbitratorSelection` | Mediator selection runtime + session workflow |
| Scheduling policy | `ProposalPayload::SchedulingPolicy` | Executor returns explicit failure in pilot v1 |

### Completely Missing

| Capability | Why It Matters |
|------------|----------------|
| Elections (multi-candidate, ranked-choice) | Cooperatives elect boards. Binary yes/no cannot elect from candidates. |
| Role registry | Board members, officers, committee chairs need term-tracked roles. |
| Member-to-member grievances | Only compute disputes and escalated ledger disputes exist. |
| Automatic period expiry | No scheduler closes deliberation/voting/amendment periods when time runs out. |
| Budget spending limit enforcement | Spending rules can be proposed but the ledger does not enforce them at transaction time. |

---

## File Reference

| Component | Path |
|-----------|------|
| Proposal types + state machine | `icn/crates/icn-governance/src/proposal.rs` |
| Vote + VoteChoice | `icn/crates/icn-governance/src/vote.rs` |
| VoteTally + delegation-aware tally | `icn/crates/icn-governance/src/tally.rs` |
| GovernanceConfig + thresholds | `icn/crates/icn-governance/src/config.rs` |
| Amendment system | `icn/crates/icn-governance/src/amendment.rs` |
| Appeal system | `icn/crates/icn-governance/src/appeal.rs` |
| Charter (founding document) | `icn/crates/icn-governance/src/charter.rs` |
| GovernanceStore trait + impls | `icn/crates/icn-governance/src/store.rs` |
| GovernanceOps trait (RPC API) | `icn/crates/icn-governance/src/handle.rs` |
| Delegation (liquid democracy) | `icn/crates/icn-governance/src/delegation.rs` |
| Governance executor (bridge) | `icn/crates/icn-core/src/supervisor/governance_executor.rs` |
| Gateway governance routes | `icn/crates/icn-gateway/src/api/governance.rs` |
| Gateway governance manager | `icn/crates/icn-gateway/src/governance_mgr.rs` |
| Integration tests | `icn/crates/icn-governance/tests/governance_integration.rs` |
| CCL compute disputes | `icn/crates/icn-ccl/src/disputes.rs` |
| Decision proofs | `icn/crates/icn-governance/src/proof.rs` |

---

## Prioritized Fix List

| Priority | Fix | Unblocks | Effort |
|----------|-----|----------|--------|
| **P0** | Election/ballot system (multi-candidate, ranked choice) | Scenario 1 | Large |
| **P0** | Background governance scheduler (auto-close, auto-expire) | Scenarios 1, 3, 4 | Medium |
| **P1** | Persist governance store to sled | All scenarios | Medium |
| **P1** | Wire remaining ProposalPayload variants to executor effects | Scenarios 2, 3, 4, 5 | Large |
| **P1** | Dispute filing + mediation API | Scenario 5 | Medium |
| **P2** | Amendment persistence + gateway API | Scenario 3 | Medium |
| **P2** | Generalize term management from Steward to Officer | Scenario 1 | Small |
| **P2** | Wire charter ConfigChange acceptance to config update | Scenario 3 | Medium |
| **P3** | Appeal persistence + gateway API + remedy execution | Scenario 5 | Medium |
| **P3** | CCL-governance integration | All (long-term) | Large |
| **P4** | Secret ballot (sealed votes) | Scenario 1 | Large |

---

## Document History

- 2026-02-17: Initial creation from comprehensive codebase audit
