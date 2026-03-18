# Economic Model Validation

**Status**: Authoritative Truth Document
**Last Updated**: 2026-02-17
**Purpose**: Maps every economic operation a cooperative needs against ICN's actual implementation state.

> This document is the single source of truth for what ICN's economic system **actually does** vs. what it **claims to do**. Every entry in the gap matrices below has been verified against the codebase. If an operation is listed as "Not Supported", it means no working end-to-end path exists — regardless of whether types or stubs are present.

---

## Methodology

For each cooperative archetype, we enumerate the required economic operations and audit ICN's current state across five dimensions:

| Column | Definition |
|--------|-----------|
| **ICN API/Type** | The Rust struct, function, or REST endpoint that handles this operation. "None" = no implementation exists. |
| **State Location** | Where data lives: ledger (sled-backed), gateway (sled-backed via `icn-ledger-app`), in-memory only, or nowhere. |
| **Test Evidence** | Specific test functions that exercise this operation end-to-end. "CRUD only" = store read/write tests without business logic. "None" = zero test coverage. |
| **Failure Mode** | Why the operation doesn't work end-to-end for a real cooperative. |
| **Minimum Viable Fix** | Smallest change to make it actually functional. |

### Classification Legend

| Symbol | Meaning |
|--------|---------|
| **Works** | End-to-end path exists and is tested |
| **Partial** | Types and/or API exist but execution is incomplete |
| **Schema Only** | Types/structs exist but no functional path |
| **Not Supported** | No types, no API, no path |

---

## Archetype 1: Worker Co-op (10 members)

**Profile**: Software development cooperative. 10 member-owners, monthly payroll via surplus distribution, client invoicing, quarterly surplus allocation, member onboarding with capital contribution, departure with share buyout.

### Gap Matrix

| Required Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Status | Minimum Viable Fix |
|--------------------|-------------|----------------|---------------|--------------|--------|---------------------|
| **Monthly Payroll** | `RecurringPayment` struct, `execute_due_payments()` fn, `start_scheduler()` | Gateway sled store (`icn-ledger-app::RecurringPaymentStore`) | `test_recurring_payment_lifecycle` (integration), `test_recurring_payment_frequencies` | **Works in gateway context only.** Scheduler runs via `start_scheduler()` in gateway `server.rs:715`. Payments execute via `LedgerManager::create_payment()`. Not a daemon-level service — dies if gateway restarts without cleanup. No governance approval for payroll setup. | **Partial** | Move scheduler to `icn-core` supervisor as a background task. Add governance proposal type for payroll approval. |
| **Client Invoicing** | None | Nowhere | None | No invoice type, no accounts receivable tracking, no payment matching. | **Not Supported** | New `Invoice` type in `icn-ledger-app`, gateway CRUD endpoints, payment matching on receipt. |
| **Surplus Allocation** | `ProposalPayload::SurplusAllocation { allocation: SurplusAllocation }` | Governance proposal store (in-memory `InMemoryGovernanceStore`) | None for end-to-end execution | Types exist in `icn-governance/src/proposal.rs:587`. `SurplusAllocation` type exists in `icn-ledger`. **But**: when the governance proposal is accepted, the surplus is NOT automatically distributed to member accounts. The `KernelGovernanceExecutor` does not have a handler for `SurplusAllocation` — it would produce a `NoOp` or fail silently. | **Schema Only** | Add `SurplusAllocation` effect to `KernelEffect` enum. Wire executor to create journal entries distributing surplus to member accounts pro-rata. |
| **Member Onboarding (Capital Contribution)** | `ProposalPayload::Membership { action: Add, member }` | Governance store | `test_membership_proposal` (unit test for proposal creation only) | Proposal can be created and voted on. When accepted, `KernelEffect::Membership(MembershipEffect::Add)` fires. But there is no capital contribution collection — member is added without payment. No escrow for initial buy-in. | **Partial** | Link membership approval to escrow creation for capital contribution. Only finalize membership when contribution escrow releases. |
| **Member Departure (Share Buyout)** | `ProposalPayload::ShareRedemption { member, share_ids, payout_schedule, reason }` | Governance proposal store | None | Types exist in `icn-governance/src/proposal.rs:599`. `ShareId` and `ScheduledPayout` types exist in `icn-ledger`. **But**: no executor handles share redemption. No scheduled payout mechanism. When proposal is accepted, nothing happens in the ledger. | **Schema Only** | Add `ShareRedemption` effect. Wire executor to create scheduled payout entries in ledger. Implement payout scheduler (could extend `RecurringPayment`). |
| **Operating Expense Tracking** | `JournalEntry` with `description` field | Ledger sled store | `test_create_entries` (basic entry creation) | Journal entries can be created via `LedgerManager::create_payment()`. Basic debit/credit tracking works. **But**: no GL account categories (no distinction between payroll, rent, supplies). No expense categorization. No budget-vs-actual reporting. | **Partial** | Add `category` field to `JournalEntry`. Define standard GL categories. Build budget-vs-actual query in gateway. |
| **Bond Issuance** | `ProposalPayload::BondIssuance { bond_offering: BondOffering }` | Governance proposal store | None | Types exist in `icn-governance/src/proposal.rs:617`. `BondOffering` type exists in `icn-ledger`. No executor handles it. When proposal accepted, nothing happens. | **Schema Only** | Add bond effect to executor. Create bond lifecycle management (issuance → maturity → redemption). |

### Summary: Worker Co-op

- **Works**: 1/7 operations (payroll, partially)
- **Partial**: 3/7 (payroll scheduling, onboarding membership, expense tracking)
- **Schema Only**: 2/7 (surplus allocation, share redemption, bond issuance)
- **Not Supported**: 1/7 (client invoicing)

---

## Archetype 2: Consumer Co-op (200 members)

**Profile**: Food co-op with 200 member-consumers. Monthly dues, bulk purchasing coordination, member patronage dividends, strict budget enforcement, quarterly financial reporting.

### Gap Matrix

| Required Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Status | Minimum Viable Fix |
|--------------------|-------------|----------------|---------------|--------------|--------|---------------------|
| **Membership Dues Collection** | `RecurringPayment` | Gateway sled store | `test_recurring_payment_lifecycle` | Same as payroll — the recurring payment scheduler works but only in gateway context. Could model dues as recurring payments from each member. **But**: no batch creation for 200 members. No delinquency tracking. No automatic suspension on non-payment. | **Partial** | Batch recurring payment creation API. Add delinquency threshold → automatic `FreezeMember` proposal trigger. |
| **Bulk Purchasing Orders** | None | Nowhere | None | No purchase order type. No order aggregation. No supplier payment workflow. | **Not Supported** | New `PurchaseOrder` type. Aggregate member orders → single supplier payment. Track fulfillment. |
| **Member Patronage Dividends** | `ProposalPayload::SurplusAllocation` | Governance store | None for execution | Same gap as worker co-op surplus allocation. Types exist, executor doesn't handle it. Patronage dividends are economically equivalent to surplus allocation but pro-rata to purchases, not labor. | **Schema Only** | Extend `SurplusAllocation` to support purchase-weighted distribution. Wire executor. |
| **Budget Enforcement** | `Budget`, `BudgetStore`, gateway CRUD | Gateway sled store (`icn-ledger-app::BudgetStore`) | `test_budget_tracking`, `test_budget_period_calculations`, `test_budget_multiple_accounts` | Budget tracking exists in gateway. Budgets can be created, spending tracked against limits. **But**: enforcement is advisory only — `LedgerManager::create_payment()` checks budget but returns a warning, not a hard rejection. Budget state is in gateway, not in ledger consensus. A direct ledger API call bypasses budget checks entirely. | **Partial** | Move budget enforcement into `Ledger::append_entry()` validation. Budget violations must reject at ledger level, not just gateway. |
| **Financial Reporting** | `Ledger::balance()`, `Ledger::get_entries()` | Ledger sled store | Basic balance queries tested | Can query balances by DID and currency. Can list journal entries. **But**: no aggregation by time period, no GL categories, no income statement / balance sheet generation, no export formats. | **Partial** | Add time-range queries. GL classification. Report generation endpoints in gateway. |
| **Member Discount Tracking** | None | Nowhere | None | No discount/pricing mechanism. No purchase history per member. No patronage credit accumulation. | **Not Supported** | Member purchase ledger. Discount tiers based on patronage. |

### Summary: Consumer Co-op

- **Works**: 0/6 operations fully
- **Partial**: 3/6 (dues collection, budget enforcement, financial reporting)
- **Schema Only**: 1/6 (patronage dividends)
- **Not Supported**: 2/6 (bulk purchasing, discount tracking)

---

## Archetype 3: Housing Co-op (50 units)

**Profile**: Resident-owned housing cooperative with 50 units. Monthly rent/maintenance fees, maintenance escrow fund, capital improvement budgeting, contractor payment disbursement, reserve fund management.

### Gap Matrix

| Required Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Status | Minimum Viable Fix |
|--------------------|-------------|----------------|---------------|--------------|--------|---------------------|
| **Rent/Maintenance Fee Collection** | `RecurringPayment` | Gateway sled store | `test_recurring_payment_lifecycle` | Same recurring payment system. Works for fixed monthly amounts. **But**: no unit-specific amount tracking (different rents per unit size). No late fee calculation. No delinquency → eviction workflow. | **Partial** | Per-account recurring payment amounts (already supported). Late fee calculation. Delinquency threshold escalation. |
| **Maintenance Escrow Fund** | `Escrow`, `EscrowStore`, `EscrowCondition` | Gateway sled store (`icn-ledger-app::EscrowStore`) | `test_escrow_conditions`, `test_escrow_time_release` | Escrow CRUD works. Release executes ledger payment via `LedgerManager::create_payment()` (`escrow.rs:203`). **But**: `create_escrow` does NOT lock funds — it creates a record only. Funds are not moved to a holding account. The "refund" path (`escrow.rs:298-305`) explicitly notes this: no ledger transaction because nothing was locked. Escrow is a "promise to pay", not actual fund locking. | **Partial** | Add fund-locking on escrow creation (debit from → holding account). Release moves holding → to. Refund moves holding → from. This requires a "system holding" account pattern. |
| **Capital Improvement Budgeting** | `TreasuryBudget` in `icn-ledger/src/treasury/budgets.rs` | Ledger in-memory + sled snapshots | None for end-to-end governance→budget flow | `TreasuryBudget` has rich types: amounts, periods, spending tracking. `BudgetManager` can create/track budgets. Treasury proposals (`TreasuryProposalOperation::CreateBudget`) exist. **But**: treasury executor is a placeholder that logs but doesn't create budgets. When governance approves a budget, nothing is created in the budget manager. | **Schema Only** | Wire `TreasuryEffect::Spend` to `BudgetManager::create_budget()`. Or better: add `TreasuryEffect::CreateBudget` variant. |
| **Contractor Payment Disbursement** | `Escrow` with `EscrowCondition::RequiresApproval` | Gateway sled store | `test_escrow_conditions` | Can model as escrow: create escrow for contractor, require board approval to release. Release path works (creates ledger entry). **But**: no multi-sig approval threshold (only checks if all required DIDs approved). No milestone-based partial release. No contractor identity verification. | **Partial** | Multi-sig threshold (M-of-N approvals). Milestone-based partial release (split escrow). |
| **Reserve Fund Management** | `Treasury` types in `icn-ledger/src/treasury/` | Ledger in-memory + sled snapshots | Zero test coverage for treasury lifecycle | Treasury types are comprehensive: `TreasuryOperation`, `TreasuryAuditRecord`, `SpendingRule`, `VelocityLimit`, `TreasuryBudget`. The `TreasuryManager` exists with in-memory state and sled persistence. **But**: the manager is not exercised by any tests. Treasury operations via governance (`TreasuryProposalOperation`) produce effects that the executor handles only as logging stubs. No integration test proves treasury → ledger flow. | **Schema Only** | Write treasury integration tests. Wire executor to real `TreasuryManager`. Prove spend → journal entry → balance update. |

### Summary: Housing Co-op

- **Works**: 0/5 operations fully
- **Partial**: 3/5 (rent collection, escrow fund, contractor payment)
- **Schema Only**: 2/5 (capital budgeting, reserve fund management)
- **Not Supported**: 0/5

---

## Archetype 4: Federation of 5 Co-ops

**Profile**: Regional federation of 5 cooperatives sharing resources, settling inter-coop trades, and coordinating shared costs. Cross-coop credit, payment clearing, dispute resolution.

### Gap Matrix

| Required Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Status | Minimum Viable Fix |
|--------------------|-------------|----------------|---------------|--------------|--------|---------------------|
| **Inter-Coop Credit/Settlement** | `SettlementEngine` in `icn-ledger/src/settlement.rs`, `ClearingPosition` concept | Ledger in-memory (dedup set) | `test_settlement_dedup` (dedup only) | `SettlementEngine` exists with idempotent receipt settlement. Can create journal entries from settlement requests. **But**: no end-to-end proof of multi-coop netting. The `ClearingPosition` type from design docs doesn't exist as a concrete struct. Settlement requires a `SettlementRequest` with `executor_verified: true` — who verifies? No federation-level clearing house implementation. | **Schema Only** | Implement clearing position tracker. Create netting algorithm. Wire federation governance to settlement approval. Add multi-coop integration test. |
| **Shared Cost Allocation** | `ProposalPayload::Federation(FederationProposal::EstablishClearing { .. })` | Governance store | None | Federation proposal types exist for clearing agreements. **But**: when accepted, the executor produces a federation effect that is handled as a logging stub (`KernelFederationExecutor` logs but doesn't create actual clearing positions or journal entries). No cost-splitting algorithm. | **Schema Only** | Wire federation executor to ledger. Implement proportional cost splitting. Create clearing agreement → recurring settlement schedule. |
| **Cross-Coop Payment Clearing** | `SettlementEngine::settle_receipt()` | Ledger in-memory | Settlement dedup test only | The settlement engine can create journal entries from receipts. Has idempotency (dedup). **But**: no periodic netting cycle. No bilateral balance tracking between co-ops. No clearing deadline enforcement. | **Schema Only** | Bilateral balance tracker. Periodic netting scheduler. Net position → journal entry batch. |
| **Dispute Resolution (Financial)** | `ProposalPayload::DisputeResolution { dispute_entry_hash, filer, proposed_outcome }`, `Dispute` types in `icn-ledger/src/dispute.rs` | Governance store (proposal), Ledger (dispute types) | None | Dispute types exist in both governance and ledger. `DisputeResolutionOutcome` has variants: `Uphold`, `Reject`, `Partial`, `NullifyTransaction`. **But**: filing a dispute doesn't actually pause the disputed entry. Upholding a dispute doesn't reverse the transaction. The governance→ledger execution bridge is missing. | **Schema Only** | Dispute filing → freeze disputed entry. Governance resolution → ledger reversal/confirmation. Idempotent execution with receipt. |
| **Consolidated Financial Reporting** | None | Nowhere | None | No federation-level aggregation of co-op financials. No cross-coop balance sheet. No shared metric computation. | **Not Supported** | Federation reporting query that aggregates across co-op ledgers. Access control for federation-level views. |

### Summary: Federation

- **Works**: 0/5 operations fully
- **Partial**: 0/5
- **Schema Only**: 4/5 (settlement, cost allocation, clearing, dispute resolution)
- **Not Supported**: 1/5 (consolidated reporting)

---

## Cross-Cutting Gaps

These issues affect all archetypes:

### 1. The Execution Bridge Gap

**The central problem**: Governance decisions don't flow to economic actions.

When a proposal is accepted (`ProposalState::Accepted`), the `EffectDispatcher` (`icn-core/src/supervisor/effect_dispatcher.rs`) executes pre-translated `KernelEffect`s. But the translation layer (app → kernel effects) only handles:

| ProposalPayload Variant | KernelEffect Produced | Actual Execution |
|------------------------|----------------------|------------------|
| `Treasury { Spend }` | `KernelEffect::Treasury(TreasuryEffect::Spend)` | **Works** — logs operation, creates ledger entry if `LedgerService` is wired |
| `Treasury { CreateBudget }` | `KernelEffect::Treasury(TreasuryEffect::Allocate)` | Placeholder — logs but doesn't create budget |
| `ProtocolChange` | `KernelEffect::Protocol(SetParameter)` | **Works** — updates protocol parameter store |
| `Membership { Add/Remove }` | `KernelEffect::Membership(Add/Remove)` | **Works** if `MembershipService` is wired |
| `FreezeMember` | `KernelEffect::Membership(Freeze)` | **Works** if `MembershipService` is wired |
| `VetoProposal` | `KernelEffect::Control(Veto)` | **Works** if `ControlService` is wired |
| `Text` | `KernelEffect::Control(TextResolution)` | **Works** — records hash |
| `SurplusAllocation` | **Not translated** | Nothing happens |
| `ShareRedemption` | **Not translated** | Nothing happens |
| `BondIssuance` | **Not translated** | Nothing happens |
| `DisputeResolution` | **Not translated** | Nothing happens |
| `Federation(*)` | `KernelEffect::Federation(*)` | Placeholder — logs only |

**Impact**: 6 of 20 ProposalPayload variants have working execution. The remaining 14 are either untranslated or produce logging-only stubs.

### 2. Escrow Doesn't Lock Funds

Escrow creation (`POST /escrow`) stores a record but does not debit the from_account. This means:
- The escrow "amount" is aspirational — the funds may have been spent elsewhere
- There's no atomicity guarantee
- The "refund" path correctly does nothing because nothing was locked

**See**: `icn-gateway/src/api/escrow.rs:298-305` (explicit comment documenting this gap)

### 3. Budget Enforcement Is Advisory

`BudgetStore` tracks spending against limits, but enforcement happens in the gateway layer only. A direct ledger API call or governance execution bypasses budget checks entirely.

**Required**: Budget validation in `Ledger::append_entry()` so no path can create an entry that exceeds a budget.

### 4. Treasury Is Untested

The `icn-ledger/src/treasury/` directory contains comprehensive types (audit records, budgets, spending rules, velocity limits) with zero integration test coverage. The `TreasuryManager` has in-memory state with sled snapshot persistence, but no test proves the snapshot/restore cycle works.

### 5. No GL Account Categories

All ledger accounts are identified by `(DID, currency)` pairs. There's no distinction between asset, liability, equity, revenue, or expense accounts. This means:
- No income statement generation
- No balance sheet generation
- No budget-vs-actual comparison by category
- No financial audit trail beyond raw journal entries

---

## Validation Against Real Cooperatives

### Rochdale Principles Compliance

| Rochdale Principle | ICN Support | Gap |
|-------------------|------------|-----|
| Voluntary & Open Membership | `Membership` proposals, `ProposalPayload::Membership` | No member application workflow, no eligibility criteria enforcement |
| Democratic Member Control | 1-member-1-vote (`VoteChoice`, `weight: 1`) | No elections, no board terms, no delegation |
| Member Economic Participation | `SurplusAllocation`, `ShareRedemption` types | Neither actually executes when approved |
| Autonomy & Independence | Federation governance proposals | Federation executor is a stub |
| Education, Training, Information | Not in scope for ledger/governance | N/A |
| Cooperation Among Cooperatives | `FederationProposal`, `SettlementEngine` | Federation executor is a stub |
| Concern for Community | `ResourceAccess` proposals | Resource access executor not implemented |

### ICA Cooperative Identity Statement

ICN's `Charter` type (`icn-governance/src/charter.rs`) contains comprehensive cooperative governance structures including `MembershipPolicy`, `EconomicPolicy`, `DisputePolicy`, and `ContributionRouting`. These provide the constitutional framework. **The gap is not in governance design but in execution** — the charter describes rules that the system doesn't enforce.

---

## Prioritized Fix List

Ordered by impact (how many archetypes are unblocked):

| Priority | Fix | Unblocks | Effort |
|----------|-----|----------|--------|
| **P0** | Wire governance→ledger execution bridge for remaining ProposalPayload variants | All 4 archetypes | Large — requires new KernelEffect variants + executor handlers |
| **P1** | Make escrow actually lock funds (debit from → holding on create) | Housing, Worker | Medium — new holding account pattern |
| **P1** | Move budget enforcement into ledger consensus | Consumer, Housing | Medium — validation in `append_entry()` |
| **P1** | Write treasury integration tests | Housing, Federation | Small — test existing code paths |
| **P2** | Add GL account categories to journal entries | Consumer, Federation | Medium — schema change + migration |
| **P2** | Implement surplus allocation executor | Worker, Consumer | Medium — new effect + pro-rata distribution logic |
| **P2** | Implement share redemption executor + payout scheduler | Worker | Medium — new effect + extend recurring payments |
| **P3** | Implement federation settlement e2e | Federation | Large — clearing positions + netting + journal entries |
| **P3** | Add dispute→ledger reversal execution | Federation, Housing | Medium — dispute freeze + reversal journal entries |
| **P3** | Add financial reporting queries | Consumer, Worker | Medium — time-range aggregation + GL categories |
| **P4** | Invoice tracking | Worker | Medium — new type + matching |
| **P4** | Bulk purchasing | Consumer | Large — order aggregation + supplier payment |

---

## Document History

- 2026-02-17: Initial creation from comprehensive codebase audit
