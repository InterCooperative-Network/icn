# ICN Economics Subsystem — Truth Contract

**Date**: 2026-02-17
**Auditor**: Claude (automated audit)
**Scope**: All economics-related code across `icn-ledger`, `icn-gateway`, `icn-coop`, `icn-federation`, and supervisor wiring.

---

## Methodology

For each of 4 cooperative archetypes, I enumerate the economic operations they need to function, then audit ICN's current state across 5 columns:

| Column | Meaning |
|--------|---------|
| **ICN API/Type** | The Rust type or function that implements this operation |
| **State Location** | Where the data lives (sled key prefix, in-memory struct, etc.) |
| **Test Evidence** | File:line of tests proving it works |
| **Failure Mode** | What breaks or is missing |
| **Minimum Viable Fix** | Smallest change to make it work for pilot |

**Verdict key**: WORKS = tested + wired to daemon. SCHEMA = types exist, logic exists, but not wired or not called. MISSING = no code at all.

---

## 1. Worker Co-op (10 members, time-banking)

Members log hours of work, get credited. They spend hours to receive services from each other. The co-op has a treasury for collective expenses.

| Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Min Viable Fix |
|-----------|-------------|----------------|---------------|--------------|----------------|
| **Create payment** (Alice pays Bob 10 hours) | `LedgerManager::create_payment()` | sled: per-coop ledger dir `data/ledgers/{coop_id}/` | `ledger_mgr.rs:591` (test_create_payment) | **WORKS** — Gateway wired via `GatewayHandles.ledger`, `LedgerManager` creates `JournalEntry` with double-entry, appends to `Ledger`. | None |
| **Query balance** | `LedgerManager::get_balance()` | sled: per-coop ledger + cached_balances HashMap | `ledger_mgr.rs:610-620` | **WORKS** — Balance read from ledger's in-memory cache, backed by sled. | None |
| **Transaction history** | `LedgerManager::get_history()`, `get_history_paginated()` | sled: timestamp index + entry store | `ledger_mgr.rs:647-690` | **WORKS** — Cursor-based pagination for unfiltered; DID-filtered uses full scan (perf concern at scale). | Add DID index for filtered queries (P2). |
| **Register treasury** | `GatewayTreasuryManager::register_treasury()` | sled: `treasury:{did}` via `TreasuryManager` | `treasury.rs:500-514` (test_actor_backed_mode) | **WORKS** — Both standalone (in-memory, test only) and actor-backed modes. Handle wired via `GatewayHandles.treasury`. | None |
| **Treasury deposit** | `GatewayTreasuryManager::create_deposit()` | sled: ledger entry + audit trail | `treasury_mgr.rs:411-486` | **WORKS** — Creates journal entry (credit depositor, debit treasury), records audit trail atomically. | None |
| **Treasury spend (governance-gated)** | `propose_spend` gateway endpoint | Routes through governance proposal system | `icn-gateway/src/api/treasury.rs` | **SCHEMA** — Endpoint creates a governance proposal but the actual ledger transfer on proposal approval is not wired. The `SettlementEngine` exists but is never called from the governance execution path. | Wire governance proposal execution to `SettlementEngine::settle_receipt()` or `create_deposit()` reverse. |
| **Spending rules** | `TreasuryManager::validate_entry()`, `SpendingRule` | sled: `spending_rule:{id}` | `treasury.rs` (30+ unit tests) | **WORKS** — Rules with `ApprovalType::None / CouncilVote / MemberVote` correctly enforce thresholds. | None |
| **Budget limits** | `BudgetStore::check_spending()` | sled: per-account budget tracking | Gateway `budgets.rs` endpoints | **WORKS** — `LedgerManager` checks `BudgetStore` before allowing payment. | None |
| **Labor share tracking** | `TreasuryManager::create_labor_share()`, `record_labor_contribution()` | sled: `labor_share:{id}` | `treasury.rs` (test_create_labor_share, test_record_contribution) | **WORKS** as types + persistence. **SCHEMA** for gateway — no REST endpoint exposes labor share creation/query. | Add gateway endpoints for labor shares (3 routes). |
| **Surplus distribution** | `TreasuryManager::execute_surplus_allocation()` | In-memory `SurplusAllocation` + sled persistence | `treasury.rs` (test_surplus_allocation_proportional) | **WORKS** as logic — uses largest-remainder algorithm for exact integer distribution. **SCHEMA** for gateway — no REST endpoint triggers it. | Add gateway endpoint + governance trigger. |
| **Credit limits (new member ramping)** | `CreditPolicyManager::calculate_credit_limit()`, `NewMemberPolicy` | sled: `membership:since:{did}` via `MembershipStore` | `membership.rs:173-205`, `credit_policy.rs` tests | **SCHEMA** — Types and logic exist. `MembershipStore` is implemented. But `CreditPolicyManager` is never called from `Ledger::append_entry()` validation path. | Wire `CreditPolicyManager` into ledger validation. |
| **Dynamic credit limits** | `DynamicCreditLimitManager` | sled: `dynamic_limit:state:{did}:{currency}` | `dynamic_limits.rs:692-1052` (20+ tests) | **SCHEMA** — Full implementation with decay/recovery/trust-change. Persists to sled. But never wired into `Ledger::append_entry()`. | Wire `get_effective_limit()` into ledger transaction validation. |
| **Dispute resolution** | `DisputeManager::file_dispute()` etc. | sled: `dispute:{id}` | `dispute.rs` (8 tests) | **SCHEMA** — Full lifecycle (file, evidence, mediator, resolve, escalate). Persists to sled. But no gateway endpoint exposes it. | Add 4-5 gateway endpoints for dispute CRUD. |

---

## 2. Consumer Co-op (200 members, bulk purchasing)

Members pool purchasing power. They pay into the co-op, the co-op buys in bulk, members receive goods at cost. Needs payment escrow, recurring payments, and budget tracking.

| Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Min Viable Fix |
|-----------|-------------|----------------|---------------|--------------|----------------|
| **Member payment** | `LedgerManager::create_payment()` | sled: per-coop ledger | Same as Worker Co-op | **WORKS** | None |
| **Payment escrow** (member pays, released on delivery) | `create_escrow`, `release_escrow`, `refund_escrow` | sled: `icn_ledger_app::EscrowStore` | Gateway `escrow.rs` endpoints | **SCHEMA** — `create_escrow` does NOT lock funds in ledger. It creates a record but the sender's balance is not debited until `release_escrow`. This means the sender can spend the escrowed amount before release. | `create_escrow` must debit sender immediately. `refund_escrow` must credit back. |
| **Recurring payments** (monthly dues) | `RecurringPayment`, `execute_due_payments()`, `start_scheduler()` | sled: `icn_ledger_app::RecurringPaymentStore` | Gateway `recurring_payments.rs` | **SCHEMA** — Full CRUD + scheduler exists. `start_scheduler` spawns a tokio task. But the scheduler is NOT started from `init_gateway.rs` or `background_tasks.rs`. It is dead code. | Call `start_scheduler()` from supervisor or gateway init. |
| **Budget tracking** | Same as Worker Co-op | Same | Same | **WORKS** | None |
| **Progressive balance limits** (sybil resistance) | `ProgressiveLimitManager::check_balance_limits()`, `check_velocity_limits()` | sled: `progressive_velocity:{did}:{currency}` | `progressive_limits.rs:488-860` (25+ tests) | **SCHEMA** — Full implementation with POPLevel-based limits and velocity windows. Persists to sled. But never wired into `Ledger::append_entry()`. | Wire into ledger validation path. |
| **Cross-currency payment** (e.g., hours → USD) | `LedgerManager::create_cross_payment()` | sled: ledger entries (4-5 leg pattern) | `fx.rs:1050-1475` (15+ integration tests) | **WORKS** — Full pipeline: oracle rate fetch, prepare, execute with clearing account, fee collection. Gateway endpoint wired. | None |
| **FX quote** | `LedgerManager::get_cross_payment_quote()` | Stateless (reads oracle + config) | `fx.rs` integration tests | **WORKS** | None |
| **Exchange rate oracle** | `OracleManager`, `ManualRateSource`, `FederationRateSource` | sled: rate cache | `oracle.rs` tests | **WORKS** — Manual rate setting works. Federation rate source exists. | None |
| **Treasury operations** | Same as Worker Co-op | Same | Same | Same verdicts | Same fixes |
| **Member freeze** (delinquent member) | `FreezeManager::freeze_member()` | sled: freeze records | `freeze.rs` tests | **SCHEMA** — Logic exists but no gateway endpoint. | Add gateway endpoints. |

---

## 3. Housing Co-op (50 units, maintenance fees + reserves)

Members own shares in housing units. They pay monthly maintenance fees, the co-op maintains reserves. Needs cooperative bonds, capital tracking, and dissolution planning.

| Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Min Viable Fix |
|-----------|-------------|----------------|---------------|--------------|----------------|
| **Monthly payments** | `LedgerManager::create_payment()` + recurring payments | Same as above | Same | Recurring payments: **SCHEMA** (scheduler not wired). One-time payments: **WORKS**. | Wire recurring scheduler. |
| **Capital contributions** | `Cooperative.add_capital()`, `Member.capital_contribution` | sled: coop store (via `icn-coop`) | `types.rs:795-807` (test_cooperative_capital) | **WORKS** as data model. **SCHEMA** for ledger integration — capital contributions are tracked on the `Cooperative` struct but not as ledger journal entries. No double-entry accounting for capital. | Create a capital contribution journal entry type or use existing entry builder with decision provenance. |
| **Cooperative bonds** | `TreasuryManager::create_bond()`, `CooperativeBond`, `BondPayment` | sled: `bond:{id}` | `treasury.rs` (test_create_bond, test_bond_payment) | **WORKS** as types + persistence. **SCHEMA** for gateway — no REST endpoint. Bond payment scheduling not automated. | Add gateway endpoints for bond CRUD. Wire payment schedule to recurring payment system. |
| **Reserve fund management** | `TreasuryManager` (budgets + spending rules) | sled: treasury budgets | treasury.rs tests | **WORKS** for budgets/rules. Reserve tracking is just a treasury budget with restrictions. | None — model as restricted budget. |
| **Dissolution planning** | `Cooperative::set_dissolution_plan()`, `AssetDistributionPlan` | sled: coop store | `types.rs` (dissolution tests) | **SCHEMA** — Types exist (BalanceAction, DebtAction, CapitalReturnMethod). Lifecycle transitions work. But no execution logic to actually distribute assets per the plan. | Implement `execute_dissolution()` that reads the plan and creates journal entries for each member's share. |
| **Share trading** (unit transfer) | None | None | None | **MISSING** — No mechanism for transferring housing shares between members. | Model as a ledger entry with a "shares" currency and governance approval gate. |
| **Maintenance fund accounting** | `TreasuryManager` + budgets | sled: treasury | treasury.rs tests | **WORKS** — Can be modeled as a treasury budget line item. | None |
| **Member exit + capital return** | `Cooperative::remove_capital()` | In-memory coop struct | `types.rs:795-807` | **SCHEMA** — Capital pool tracking exists but no logic to calculate member's pro-rata share and create the ledger entry. | Wire capital return to ledger payment. |

---

## 4. Federation of 5 Co-ops (inter-coop clearing)

Five cooperatives trade services/goods with each other. Needs bilateral clearing agreements, multilateral netting, cross-coop transfers, and federation-level governance.

| Operation | ICN API/Type | State Location | Test Evidence | Failure Mode | Min Viable Fix |
|-----------|-------------|----------------|---------------|--------------|----------------|
| **Bilateral clearing agreement** | `BilateralClearingAgreement` | sled: via `ClearingManager` | `clearing.rs:286-337` (3 tests) | **SCHEMA** — Types fully defined (exchange rates, settlement interval, max imbalance, signatures). `ClearingManager` exists. But not wired to any gateway endpoint or gossip protocol. | Add gateway endpoints for agreement CRUD. Wire to gossip for multi-node agreement sync. |
| **Cross-coop transfer** | `CrossCoopTransfer` | sled: via clearing positions | `clearing.rs:316-337` (test_cross_coop_transfer) | **SCHEMA** — Types exist with full lifecycle (Pending→Confirmed→Settled→Disputed). But no execution path creates these transfers from a gateway API call. | Add gateway endpoint that creates CrossCoopTransfer and updates ClearingPosition. |
| **Multilateral netting** | `NettingEngine::net()` | In-memory (no persistence) | `netting.rs:299-407` (7 tests, including 4-party cycles) | **SCHEMA** — Algorithm is correct and well-tested (bilateral + cycle detection + cancellation). But: (1) in-memory only, (2) no gateway endpoint, (3) no scheduler to run netting periodically. | Add persistence, gateway endpoint for manual netting, optional scheduler. |
| **Federation settlement** | `SettlementEngine::settle_receipt()` | In-memory dedup set (no sled persistence) | `settlement.rs` tests | **SCHEMA** — Converts execution receipts to journal entries. Validates executor_verified, rejects federation scope (intentionally), prevents self-dealing. But dedup is in-memory only — restarts lose dedup state. | Persist dedup set to sled. Remove federation scope rejection or add federation-specific settlement path. |
| **Inter-coop exchange rates** | `BilateralClearingAgreement::get_rate()` | sled: agreement store | `clearing.rs:298` | **SCHEMA** — Rates stored per agreement. But not integrated with `OracleManager`. | Register `BilateralClearingAgreement` rates as an oracle source, or use separate lookup. |
| **Federation governance** (shared policies) | `icn-federation::agreement::AgreementManager` | sled: agreement store | `agreement/` module tests | **SCHEMA** — Agreement manager exists with gossip wiring via `AgreementManagerHandle` (wired in `init_gateway.rs:39`). Gateway has service discovery. But no federation-level governance proposals. | Extend governance system with federation-scoped proposals. |
| **Allocation receipts** (governance→economics bridge) | `create_budget_allocation()`, `AllocationReceipt`, `SettlementIntent` | In-memory (receipt not persisted) | `allocations.rs:60-168` (6 tests) | **WORKS** as pure logic — correctly creates receipts with provenance chain. But receipt is never persisted and `SettlementIntent` is never executed to create a journal entry. | Wire allocation receipt creation to settlement execution path. |
| **Cross-coop privacy** | Gateway `coop_id` scope checking | Per-coop ledger isolation | `ledger.rs` gateway tests (cross-coop privacy) | **WORKS** — Each coop has separate sled directory. Balance queries scoped to `coop_id`. Gateway validates `coop_id` in JWT claims. | None |

---

## Summary Matrix

| Capability | Worker | Consumer | Housing | Federation |
|-----------|--------|----------|---------|------------|
| Basic payments | WORKS | WORKS | WORKS | WORKS (within coop) |
| Balance queries | WORKS | WORKS | WORKS | WORKS |
| Transaction history | WORKS | WORKS | WORKS | WORKS |
| Treasury (register + deposit) | WORKS | WORKS | WORKS | WORKS |
| Treasury spend (governance) | SCHEMA | SCHEMA | SCHEMA | SCHEMA |
| Budget limits | WORKS | WORKS | WORKS | WORKS |
| Spending rules | WORKS | WORKS | WORKS | WORKS |
| FX / cross-currency | WORKS | WORKS | N/A | SCHEMA |
| Credit limits | SCHEMA | SCHEMA | SCHEMA | SCHEMA |
| Dynamic limits | SCHEMA | SCHEMA | SCHEMA | SCHEMA |
| Progressive limits | SCHEMA | SCHEMA | SCHEMA | SCHEMA |
| Escrow | - | SCHEMA | - | - |
| Recurring payments | SCHEMA | SCHEMA | SCHEMA | - |
| Labor shares | SCHEMA | - | - | - |
| Surplus distribution | SCHEMA | - | - | - |
| Dispute resolution | SCHEMA | SCHEMA | SCHEMA | SCHEMA |
| Member freeze | SCHEMA | SCHEMA | SCHEMA | SCHEMA |
| Capital tracking | - | - | SCHEMA | - |
| Bonds | - | - | SCHEMA | - |
| Dissolution | - | - | SCHEMA | - |
| Bilateral clearing | - | - | - | SCHEMA |
| Multilateral netting | - | - | - | SCHEMA |
| Cross-coop transfers | - | - | - | SCHEMA |
| Allocation receipts | - | - | - | SCHEMA |
| Share trading | - | - | MISSING | - |

---

## Critical Path for Pilot

The following 7 items, if wired, unlock all 4 archetypes for basic operation:

1. **Wire recurring payment scheduler** — `start_scheduler()` is dead code. One line in supervisor/gateway init.
2. **Fix escrow fund locking** — `create_escrow` must debit sender immediately. ~20 lines in escrow handler.
3. **Wire credit limits into ledger validation** — `CreditPolicyManager` exists, tested, but `Ledger::append_entry()` never calls it. ~15 lines.
4. **Wire governance proposal execution to treasury** — `propose_spend` creates a proposal but approval never creates the ledger entry. Needs settlement bridge.
5. **Add gateway endpoints for labor shares + disputes** — Types and logic exist. Need ~8 REST routes total.
6. **Persist netting engine state** — Currently in-memory only. ~30 lines to add sled persistence.
7. **Wire allocation receipt → settlement** — The governance→economics bridge types exist but the final step (creating journal entries from `SettlementIntent`) is never executed.

---

## Supervisor Wiring Audit

What economic services are actually connected to the running daemon (`init_gateway.rs`):

| Handle | Wired? | Notes |
|--------|--------|-------|
| `GatewayHandles.treasury` | YES | `TreasuryHandle = Arc<RwLock<LedgerTreasuryManager>>` |
| `GatewayHandles.ledger` | YES | `LedgerHandle = Arc<RwLock<Ledger>>` |
| `GatewayHandles.governance` | YES | For proposal creation (but not execution) |
| `GatewayHandles.agreement_manager` | YES | Federation agreements |
| `GatewayHandles.coop` | YES | Cooperative management |
| Recurring payment scheduler | **NO** | `start_scheduler()` never called |
| Dynamic limit decay scheduler | **NO** | `apply_decay()` never called periodically |
| Netting scheduler | **NO** | No background task exists |
| Settlement execution on proposal approval | **NO** | Governance proposals created but never executed |

### Background tasks that DO run (`background_tasks.rs`):

- Clock sync (NTP)
- Metrics update
- Parameter scheduler (governance parameter changes)
- Audit record pruning
- Connection candidate re-announcement
- Bootstrap peer health
- Storage maintenance

None of these are economics-related. All economics background tasks are unwired.
