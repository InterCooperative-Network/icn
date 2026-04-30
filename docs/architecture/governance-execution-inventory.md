---
title: "Governance Execution Inventory"
status: "current"
date: "2026-04-07"
context: "Phase 1 execution closure / ADR-0015 architectural pass"
---

# Governance Execution Inventory

> **When to use this doc:** Before writing a new proposal type, check here first.
> After wiring a new proposal, update the table.

This document maps every `ProposalPayload` variant (and sub-variants) to its
kernel execution path. The goal is to make hidden gaps visible and track the
difference between "intentionally deferred" and "not yet implemented."

**Classification key:**

| Symbol | Meaning |
|--------|---------|
| ✅ FULLY WIRED | Proposal accepted → real durable state change → audit receipt |
| ⚠️ PARTIALLY WIRED | Produces a kernel effect but with known gaps (empty hashes, missing fields) |
| 📋 RECORD-ONLY | Intentionally produces no state change — the governance vote IS the institutional act |
| 🔜 DEFERRED | Recognized + translated to a kernel effect; executor returns `not_executed=true` pending service implementation |
| ❌ FAIL-FAST | Returns `TranslationError::unsupported` — gap is named and visible, not silently swallowed |

---

## Top-Level ProposalPayload Variants

| Proposal Type | Classification | Kernel Effect | Execution Notes |
|---------------|---------------|---------------|----------------|
| `Text` | ✅ FULLY WIRED | `Control::TextResolution` | The vote hash is the resolution |
| `Budget` | ✅ FULLY WIRED | `Treasury::CreateBudget` | decision_receipt_id + decision_hash carried |
| `Allocation` | ✅ FULLY WIRED | `Treasury::CreateBudget` + `Treasury::Allocate` | One CreateBudget + N Allocate effects |
| `Membership(Add)` | ✅ FULLY WIRED | `Membership::AddMember` | |
| `Membership(Remove)` | ✅ FULLY WIRED | `Membership::RemoveMember` | |
| `FreezeMember` | ✅ FULLY WIRED | `Membership::FreezeMember` | |
| `UnfreezeMember` | ✅ FULLY WIRED | `Membership::UnfreezeMember` | |
| `ConfigChange` | ✅ FULLY WIRED | `Protocol::SetGovernanceConfig` | |
| `SchedulingPolicy` | ✅ FULLY WIRED | `Protocol::SetSchedulingPolicy` | |
| `ProtocolChange` | ✅ FULLY WIRED | `Protocol::SetParameter` | `old_value_hash` is empty (not carried by payload) |
| `ProtocolUpgrade` | ⚠️ PARTIALLY WIRED | `Protocol::Upgrade` | `upgrade_hash` empty, `activation_height` = 0 |
| `VetoProposal` | ✅ FULLY WIRED | `Control::VetoProposal` | |
| `ForceCloseProposal` | ✅ FULLY WIRED | `Control::ForceCloseProposal` | |
| `RollbackLedger` | ✅ FULLY WIRED | `Dispute::RollbackLedger` | |
| `DisputeResolution` | ⚠️ PARTIALLY WIRED | `Dispute::ResolveDispute` | Compensation calculation only covers `Partial` outcome; `Uphold`/`Reject`/`VoidTransaction` produce empty compensations |
| `SurplusAllocation` | ✅ FULLY WIRED | `Treasury::DistributeSurplus` | Uses `member_payments` (resolved DIDs), not `allocations` (ShareId keys) |
| `ShareRedemption` | ✅ FULLY WIRED | `Treasury::RedeemShares` | Installment tracking out of scope (total payout only) |
| `BondIssuance` | ✅ FULLY WIRED | `Treasury::IssueBond` | governance-approved capital raise; not a validator bond |
| `Charter` | ✅ FULLY WIRED | `Protocol::SetGovernanceConfig` | charter_yaml stored as config_json; CharterPolicyOracle reads it |
| `ResourceAccess(Grant)` | ✅ FULLY WIRED | `Resource::GrantAccess` | |
| `ResourceAccess(Revoke)` | ✅ FULLY WIRED | `Resource::RevokeAccess` | |
| `Federation(...)` | see below | | |
| `Sdis(...)` | see below | | |
| `Treasury(...)` | see below | | |

---

## Treasury ProposalOperation Sub-variants

| Operation | Classification | Kernel Effect | Notes |
|-----------|---------------|---------------|-------|
| `Withdraw` | ✅ FULLY WIRED | `Treasury::Spend` | nonce-checked |
| `Spend` | ✅ FULLY WIRED | `Treasury::Spend` | no budget constraint |
| `CreateBudget` | ✅ FULLY WIRED | `Treasury::CreateBudget` | |
| `TransferBetweenBudgets` | ✅ FULLY WIRED | `Treasury::Transfer` | from/to encoded as `{treasury_did}:budget:{budget_id}` |
| `CancelBudget` | 📋 RECORD-ONLY | `NoOp` | No `TreasuryEffect::CancelBudget` yet. The governance decision is the cancellation record. |
| `ReclaimBudget` | 📋 RECORD-ONLY | `NoOp` | No `TreasuryEffect::ReclaimBudget` yet. |
| `ModifySpendingRule` | 📋 RECORD-ONLY | `NoOp` | Spending rules are CCL/policy-layer governance documents. |

---

## SDIS (SdisProposal) Sub-variants

| Proposal | Classification | Kernel Effect | Notes |
|----------|---------------|---------------|-------|
| `AppointSteward` | ✅ FULLY WIRED | `Sdis::ApproveSteward` | No financial collateral. Governance vote is the legitimating act. |
| `RemoveSteward` | ✅ FULLY WIRED | `Sdis::RevokeSteward` | |
| `ReconfirmSteward` | ✅ FULLY WIRED | `Sdis::ReconfirmSteward` | Extends term_end |
| `SuspendSteward` | ✅ FULLY WIRED | `Sdis::SuspendSteward` | Duration advisory; timed auto-reinstatement not enforced |
| `ReinstateSteward` | ✅ FULLY WIRED | `Sdis::ReinstateSteward` | Idempotent |
| `UpdateJurisdictionTier` | 🔜 DEFERRED | `Sdis::UpdateJurisdictionTier` | Recognized; executor returns `not_executed=true`. CommonsHandle needs `update_jurisdiction_tier`. |
| `SanctionSteward(Warning)` | 📋 RECORD-ONLY | `NoOp` | ADR-0014: warning is institutional record, not state change |
| `SanctionSteward(Censure)` | 📋 RECORD-ONLY | `NoOp` | ADR-0014: censure is institutional record, not state change |
| `SanctionSteward(Probation)` | 📋 RECORD-ONLY | `NoOp` | Probation tracking at CCL/policy layer, not kernel |
| `SanctionSteward(Suspension)` | ✅ FULLY WIRED | `Sdis::SuspendSteward` | |
| `SanctionSteward(Removal)` | ✅ FULLY WIRED | `Sdis::RevokeSteward` | |
| `SanctionSteward(TierDemotion)` | 🔜 DEFERRED | `Sdis::UpdateJurisdictionTier` | Same deferred path as UpdateJurisdictionTier |
| `ModifyThreshold` | ❌ FAIL-FAST | `Err` | No kernel threshold-registry. Thresholds are policy-layer constants. |
| `ApproveAuthority` | ❌ FAIL-FAST | `Err` | No kernel authority-registry. Requires dedicated authority registry. |
| `RevokeAuthority` | ❌ FAIL-FAST | `Err` | No kernel authority-registry. |
| `RevocationAppeal` | ❌ FAIL-FAST | `Err` | Multi-step adjudication flow; no single kernel effect. |
| `ForceKeyRotation` | ❌ FAIL-FAST | `Err` | Requires DID rotation across identity/network/steward layers. |

---

## FederationProposal Sub-variants

| Proposal | Classification | Kernel Effect | Notes |
|----------|---------------|---------------|-------|
| `JoinFederation` | ✅ FULLY WIRED | `Federation::JoinFederation` | Creates durable federation record |
| `LeaveFederation` | ✅ FULLY WIRED | `Federation::LeaveFederation` | Removes federation record |
| `EstablishClearing` | ✅ FULLY WIRED | `Federation::EstablishClearing` | ADR-0013: settlement_interval + max_imbalance terms propagated |
| `VouchForCooperative` | ✅ FULLY WIRED | `Federation::VouchForCoop` | |
| `TerminateClearing` | 🔜 DEFERRED | `Federation::TerminateClearing` | Recognized; executor returns `Deferred`. FederationService needs `terminate_clearing`. |
| `RevokeVouch` | 🔜 DEFERRED | `Federation::RevokeVouch` | Recognized; executor returns `Deferred`. FederationService needs `revoke_vouch`. |
| `UpdateFederationPolicy` | 📋 RECORD-ONLY | `NoOp` | Federation policy is a CCL governance document. The accepted proposal IS the policy update. |

---

## Summary Counts (as of 2026-04-07)

| Classification | Count |
|----------------|-------|
| ✅ FULLY WIRED | 29 |
| ⚠️ PARTIALLY WIRED | 2 (`ProtocolUpgrade`, `DisputeResolution`) |
| 📋 RECORD-ONLY (intentional) | 9 |
| 🔜 DEFERRED (wired, executor pending) | 4 |
| ❌ FAIL-FAST (explicit named gaps) | 5 |
| **Total tracked** | **49** |

**Before this pass (2026-04-07):**
- 12+ proposals fell through a generic catch-all `Err` with no documentation
- `Charter` was untranslated despite being one of the most important governance actions
- `TransferBetweenBudgets` was untranslated despite `TreasuryEffect::Transfer` existing
- `TierDemotion` and `UpdateJurisdictionTier` had no kernel effect type at all

---

## Implementation Roadmap for Deferred Items

### High Priority (unblocked, just needs CommonsHandle work)
- **`UpdateJurisdictionTier` / `TierDemotion`**: Add `update_jurisdiction_tier` to `CommonsHandle`, implement in `SdisServiceImpl`, wire executor. 1-2 days.

### Medium Priority (needs FederationService additions)
- **`TerminateClearing`**: Add `terminate_clearing` to `FederationService` trait, implement in supervisor, remove `Deferred` from executor. 2-3 days.
- **`RevokeVouch`**: Add `revoke_vouch` to `FederationService` trait, implement in supervisor. 1-2 days.

### Architectural Gaps (require design)
- **`ModifyThreshold`**: Define threshold registry model. Where do thresholds live? How are they enforced? Needs ADR.
- **`ApproveAuthority` / `RevokeAuthority`**: Design institutional authority registry. Needs schema + CCL bindings.
- **`RevocationAppeal`**: Multi-step adjudication model. Needs its own ADR.
- **`ForceKeyRotation`**: Coordinate with DID rotation spec. Touches identity, network, and steward layers.

### Minor gaps (polish)
- **`ProtocolUpgrade`**: Populate `upgrade_hash` and `activation_height` from proposal payload.
- **`DisputeResolution`**: Handle `Uphold`, `Reject`, `VoidTransaction` outcomes explicitly.
