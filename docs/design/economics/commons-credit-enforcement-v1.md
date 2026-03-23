# Commons Credit Enforcement V1 — Implementation Note

**Issue:** #1397
**PR:** #1398
**Branch:** `feat/1397-commons-credit-enforcement`
**Commit:** `5b3eed46`
**Status:** PR open / awaiting CI+review (2026-03-23)

---

## Summary

This document records the exact boundary of the V1 scheduling-time commons credit enforcement
slice. It exists to keep the accounting/enforcement architecture legible while PR #1398 is in
review and after it merges.

The accounting substrate (#925, closed) built the ledger primitives. This slice wires them
into the scheduler. V1 is deliberately minimal: it proves the wire exists and rejects
zero-balance submitters. It is not a pricing model.

---

## What Changed

**Single seam:** `handle_submit()` in `icn-compute/src/actor/lifecycle.rs`

Inserted after the existing `CommonsPoolPolicy` governance checks (standing + ceiling),
before the policy manager compliance check:

```rust
if task.scope == ScopeLevel::Commons {
    if let Some(ref balance_cb) = self.balance_callback {
        const COMMONS_CREDIT_FLOOR: i64 = 1;
        if balance < COMMONS_CREDIT_FLOOR {
            return Err(ComputeError::InsufficientCommonsCredits {
                balance,
                required: COMMONS_CREDIT_FLOOR,
            });
        }
    }
}
```

**Integration test un-ignored:**
`test_scheduling_enforces_credits` in `tests/commons_integration.rs` was previously
`#[ignore = "enforcement not yet implemented"]`. It now passes without `#[ignore]`:

- Negative path: `balance_callback` returning `0` → `InsufficientCommonsCredits { balance: 0, required: 1 }`
- Positive path: `balance_callback` returning `1` → passes the credit gate (fails later for an
  unrelated reason — no CCL executor configured — which is the honest positive-path proof)

---

## What Did Not Change

- `CommonsPoolPolicy::validate_submitter_credit()` — the ceiling check is untouched
- `error.rs` — `InsufficientCommonsCredits` variant was already correct
- `Local` and `Cell` scoped tasks — unaffected; the guard is `scope == Commons` only
- `balance_callback = None` behaviour — gate is skipped silently (opt-in)
- Any existing commons pool admission logic (pool membership ≠ credit enforcement)

---

## Repo-Shape Constraint: Why the Check Is Inlined

`icn-ledger` is declared as a `[dev-dependencies]` entry in `icn-compute/Cargo.toml`:

```toml
[dev-dependencies]
icn-ledger = { path = "../icn-ledger" }
```

This means `icn_ledger::` is available to test binaries (integration tests, benchmarks)
but **not** to library code (`src/`). `handle_submit()` is library code.

Options considered:

| Option | Decision |
|--------|----------|
| Move `icn-ledger` to `[dependencies]` | Deferred — wider change, own PR scope |
| Import `check_sufficient_balance` in lifecycle.rs | Blocked by dev-dep constraint |
| Inline `balance < 1` in `handle_submit()` | **Chosen** — identical semantics, no dep change |

The inline check (`balance < COMMONS_CREDIT_FLOOR`) is semantically identical to
`icn_ledger::commons_credits::check_sufficient_balance(balance, 1)`. The constant name
and comment in the code name the future upgrade path explicitly.

When `icn-ledger` moves to `[dependencies]` (a future PR), the inline can be replaced
with the library call and the constant folded in.

---

## Validation Completed (pre-PR)

```
cargo test -p icn-compute --lib
# 356 passed; 0 failed; 0 ignored

cargo test -p icn-compute --test commons_integration
# 9 passed; 0 failed; 0 ignored   (was: 8 passed; 1 ignored)

cargo clippy -p icn-compute --all-targets -- -D warnings
# clean

cargo fmt --all --check
# clean
```

---

## Future Enforcement Boundary

`COMMONS_CREDIT_FLOOR = 1` is a V1 sentinel, not a pricing model. The remaining work
to complete cost-based enforcement:

1. **Task cost estimation** — derive required credits from `task.resource_profile`,
   `task.fuel_limit`, or an explicit `task.estimated_cost` field. Replace
   `COMMONS_CREDIT_FLOOR` with `compute_credits_required(&task)`.

2. **Library import** — move `icn-ledger` from `[dev-dependencies]` to `[dependencies]`
   in `icn-compute/Cargo.toml` and call `check_sufficient_balance(balance, required)`
   directly instead of the inline comparison.

3. **Pre-execution reservation** — reserve the estimated credit amount at submit time,
   settle actual cost from `ExecutionReceipt` at completion. This prevents races
   between concurrent submitters with the same balance.

These are distinct slices. None are required for V1 to be useful — a submitter with
zero credits is blocked, and a submitter with any positive balance is not.

---

## Draft Closure Text (DO NOT USE until PR #1398 merges)

### #1397 closing comment

```
Closed via PR #1398 (merged to main).

V1 scheduling-time credit enforcement is live:
- handle_submit() in icn-compute/src/actor/lifecycle.rs rejects Commons-scoped
  task submissions when balance < 1 and balance_callback is configured
- test_scheduling_enforces_credits passes without #[ignore]
- Local and Cell scoped tasks are unaffected
- enforcement is opt-in (skipped silently when balance_callback is not configured)

Remaining work (cost-based enforcement, task cost estimation, pre-execution
reservation) is deferred and should be filed as a follow-on issue when prioritised.
```

### Sprint board note

```
s24-t6 (or follow-on): #1397 V1 enforcement landed via PR #1398.
Remaining: cost-based enforcement (task cost estimation + icn-ledger dep promotion).
```
