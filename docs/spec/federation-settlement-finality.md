# Federation Settlement Finality Spec

**Status:** Phase 1 — Formal specification (doc-only)
**Issue:** [#1365](https://github.com/InterCooperative-Network/icn/issues/1365)
**Phase 2 (protocol implementation) and Phase 3 (governance integration) are out of scope here.**

---

## 1. Background

ICN supports multi-org compute and ledger operations via federation. A _settlement_ is the process by which cross-org compute receipts are credited to the receiving cooperative's ledger. The current protocol defines the mechanics of receipt generation and commons credit flow but has no formal semantics for:

- When a settlement is _final_ (cannot be reversed)
- How disputes are raised, tracked, and resolved
- Whether net positions across multiple receipts can be collapsed before committing journal entries

This spec closes that gap for Phase 1. Implementation targets are deferred to Phase 2.

---

## 2. Definitions

| Term | Meaning |
|------|---------|
| **Receipt** | `ArtifactReceipt` — cryptographically signed proof that a task executed and produced output |
| **Settlement** | The act of writing a `JournalEntry` crediting the executor's cooperative based on a receipt |
| **Clearing receipt** | `ClearingReceipt` — wrapper used by `ReceiptClearingManager` to track batch settlement state |
| **Position** | The net amount owed between two cooperatives under a bilateral agreement after netting |
| **Finality** | The point after which a settlement cannot be reversed or disputed |
| **Dispute** | A formal objection raised by one party to a clearing receipt before finality |

---

## 3. Settlement Finality Conditions

A settlement is **final** when ALL of the following hold:

1. **Receipt chain complete.** The `ArtifactReceipt` references a task execution that has a matching `ComputeReceipt` in the issuing cooperative's local store.
2. **Signature valid.** The receipt is signed by a key bound to a DID with `compute:execute` capability, verifiable at settlement time.
3. **Dispute window elapsed.** The configurable dispute window (default: 72 hours from `ClearingReceipt` submission) has passed without a `dispute_receipt()` call from either party.
4. **Not disputed.** `ClearingReceipt.disputed == false` at the time the batch is flushed.
5. **Batch flushed.** `ReceiptClearingManager::flush_to_clearing()` has been called and the receipt was included in the flush (not skipped as disputed or expired).

A settlement that meets conditions 1–4 but whose batch has not yet been flushed is **pending** (reversible via dispute).

A settlement that meets all five conditions is **final** (irreversible absent a governance freeze — see §6).

### 3.1 Timeout-Based Finality

If a batch is not explicitly flushed within `BatchClearingConfig.max_pending_duration` (proposed default: 7 days), the implementation SHOULD:
- Log a warning at the 48-hour mark
- Force-flush at the deadline, settling all non-disputed receipts
- Mark expired-but-unflushed disputed receipts as `Escalated` (see §4)

Phase 2 will implement this timeout in `SettlementEngine`.

---

## 4. Dispute Lifecycle

Disputes follow a five-state machine:

```
Submitted ──► Disputed ──► Under Review ──► Resolved
                                 │
                                 └──► Escalated
```

### States

| State | Condition | `ClearingReceipt.disputed` |
|-------|-----------|---------------------------|
| **Submitted** | Receipt accepted, dispute window open | `false` |
| **Disputed** | `dispute_receipt()` called by submitter or executor | `true` |
| **Under Review** | Dispute acknowledged by federation arbitration | `true` |
| **Resolved** | Dispute withdrawn or found invalid; receipt re-queued for settlement | `false` (reset) |
| **Escalated** | Dispute unresolved at batch flush timeout | `true` (frozen) |

### Permitted Disputants

Only the two parties named in the clearing receipt may raise a dispute:
- **Submitter** (`ClearingReceipt.submitter_did`): disputes if work was not performed correctly
- **Executor** (`ClearingReceipt.executor_did`): disputes if the receipt is fraudulent or the claimed work was never assigned

Third parties (including federation coordinators) may not raise disputes — they may only act as arbitrators once a dispute has been raised.

### Dispute Window

The dispute window begins when `submit_receipt()` accepts the receipt and ends at `submission_timestamp + dispute_window_duration`. After the window closes without a dispute call, the receipt transitions directly to **Submitted → (batch flush) → Final**. Late dispute calls are rejected.

Phase 2 will enforce the timestamp check in `ReceiptClearingManager::dispute_receipt()`.

### Resolution Path

1. Either party calls `dispute_receipt(receipt_id, disputer_did, reason)`.
2. The receipt is flagged; `flush_to_clearing()` skips it.
3. An out-of-band arbitration process (human or governance proposal) produces one of:
   - **Withdraw dispute**: `ClearingReceipt.disputed` reset to `false`; receipt re-enters the pending pool.
   - **Confirm dispute**: receipt is marked `Escalated` and removed from the pending pool. No settlement is issued. Compensation is a separate governance action.
4. Escalated receipts are archived. They do not re-enter the settlement pipeline.

---

## 5. Netting Rules

Netting collapses multiple offsetting receipts between two cooperatives into a single net transfer before committing journal entries.

### 5.1 Bilateral Netting

For a pair of cooperatives `(A, B)` under a named bilateral agreement:

1. Collect all `final` (non-disputed, batch-eligible) clearing receipts for the agreement.
2. Separate into two buckets: `A→B` credits and `B→A` credits.
3. Compute net:
   - If `sum(A→B) > sum(B→A)`: B owes A `sum(A→B) - sum(B→A)` units.
   - If equal: no transfer is needed.
4. Propose a single `CrossCoopTransfer` for the net amount to `ClearingManager`.

This is already partially implemented via `ReceiptClearingManager::flush_to_clearing()` + `ClearingManager::calculate_position()`. Phase 2 will make netting explicit rather than implicit.

### 5.2 Multilateral Netting

For three or more cooperatives in a federation:

1. Build a directed weighted graph of bilateral net positions.
2. Apply cycle cancellation: if `A→B→C→A` forms a cycle, reduce all three edges by `min(A→B, B→C, C→A)`.
3. The resulting acyclic graph defines the minimal set of transfers.

Multilateral netting is not currently implemented. It is the primary Phase 2 implementation target in `SettlementEngine`.

### 5.3 Netting Constraints

- Netting operates only within a single `unit` (credit denomination). Cross-unit netting is not permitted.
- Disputed receipts are excluded from netting until resolved.
- Netting is applied per-flush, not continuously. The net position is a snapshot at flush time.
- If a receipt included in a net position is later disputed (before flush commits to the journal), the entire net position is recalculated without that receipt.

---

## 6. Governance Integration (Phase 3 Preview)

This section is informational only — implementation is deferred to Phase 3.

A governance proposal (via `icn-governance`) MAY trigger a **settlement freeze** on a cooperative or federation. During a freeze:
- No new clearing receipts are accepted for the frozen party.
- Pending batches are held and not flushed.
- Existing disputed receipts remain in their current state.

The freeze lifts when the governance proposal resolves. At lift time, the held batch is eligible for flush (subject to normal dispute window and timeout rules).

`AllocationReceipt` in Phase 3 will carry a `finality_attestation` field confirming that the associated settlement completed without dispute.

---

## 7. Current Implementation Gaps

The following gaps exist in the codebase as of Sprint 20. Each is a Phase 2 implementation target.

| Gap | Location | Issue |
|-----|----------|-------|
| Dispute window is not enforced at `dispute_receipt()` time | `icn-federation/src/receipt_clearing.rs:210` | #1365 |
| Batch flush timeout is not implemented | `SettlementEngine` in `icn-ledger` | #1365 |
| `DisputeResolution` message type missing from federation protocol | `icn-federation` | #1365 |
| Multilateral netting not implemented | `SettlementEngine` | #1365 |
| `finality_attestation` field absent from `AllocationReceipt` | `icn-compute/src/types.rs` | #1365 (Phase 3) |

---

## 8. Wire Format (Phase 2 Preview)

```rust
// Proposed — not yet implemented
pub enum FederationMessage {
    // ...existing variants...
    DisputeResolution(DisputeResolutionMsg),
}

pub struct DisputeResolutionMsg {
    pub receipt_id: String,
    pub resolution: DisputeOutcome,
    pub resolved_by: Did,
    pub resolved_at: u64,  // Unix seconds
}

pub enum DisputeOutcome {
    WithdrawDispute,
    ConfirmDispute { reason: String },
}
```

---

*Document authored Sprint 20 (2026-03-21). Update alongside Phase 2 implementation.*
