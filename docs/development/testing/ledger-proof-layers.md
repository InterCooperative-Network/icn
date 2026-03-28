---
Status: partial
Canonical: no
Last Reviewed: 2026-03-28
---

# Ledger Proof Layers

Ledger has Layers 1 and 2 complete. Layers 3–4 (restart + cross-process) remain.

---

## Layer 1 — Direct Struct Sled Write Proof ✅

**What it proves:** A `JournalEntry` written through `Ledger::new(Arc<SledStore>)` →
`append_entry()` survives an in-process sled drop-and-reopen. The in-memory
`cached_balances` is gone after the drop. The fresh `Ledger::new` reads back via sled only.

**Also proven:** The reopened store accepts a second write (not opened read-only).

**Artifact:** `crates/icn-ledger/tests/ledger_persistence.rs` →
`test_ledger_entry_survives_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-ledger --test ledger_persistence
```

**What is asserted:**
- Author DID survives round-trip
- Both `AccountDelta` sides survive with exact values (account_id, currency, amounts)
- `ProvenanceRef::SystemGenerated { reason }` survives round-trip (proves full body)
- Reopened ledger accepts a second `append_entry` without error

---

## Layer 2 — Store-backed Sled Write Proof ✅

**Architecture note:** `apps/ledger` does not have a Tokio actor with a `spawn()` method.
Instead, it provides two sled-backed stores (`SledEscrowStore`, `SledBudgetStore`) that the
daemon supervisor uses directly — the same role as `SledGovernanceStateStore` in governance.

**What it proves:** State written through `SledEscrowStore::put()` survives a drop-and-reopen
boundary. The reopened store reads back via sled and accepts further writes.

**Also noted:** `LedgerServiceImpl` in `icn-core` (the `JournalEntry` production write path)
already has `test_submit_treasury_entry_idempotency_survives_restart` which proves its writes
reach sled. That test is in `crates/icn-core/src/services/ledger_service.rs`.

**Artifact:** `apps/ledger/tests/actor_persistence_proof.rs` →
`test_escrow_record_survives_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-ledger-actor --test actor_persistence_proof
```

**What is asserted:**
- `escrow_id`, `scope_id`, `funder_did`, `beneficiary_did`, `amount`, `currency`, `status`
  all survive sled round-trip with exact values
- Released status (`EscrowStatus::Released`) and `release_decision_hash` survive a second write

---

## Run Both Layer 1 + 2

```bash
cargo test -p icn-ledger --test ledger_persistence
cargo test -p icn-ledger-actor --test actor_persistence_proof
```

---

## What Is NOT Proven

| Gap | Why it matters | Next layer target |
|-----|---------------|-------------------|
| Same-runtime close+reopen via actor handle | No actor handle or shutdown mechanism tested | Layer 3 |
| Cross-process restart | Only same-process drop proven | Layer 4 |
| Balance cache rebuild correctness | `cached_balances` repopulated from sled on `Ledger::new` — separate correctness concern | Dedicated test |
| Gossip sync across restart | Callbacks not exercised | Multi-node integration test |
| Schema migration | No test that old sled data is readable after type changes | Versioned serde roundtrip |

---

## Next Layer: Layer 3 — Same-Runtime Close+Reopen Proof

**Target:** Prove that state survives an actor/service layer drop within a single runtime.
For `LedgerServiceImpl`: drop the service + `Arc<RwLock<Ledger>>`, reopen sled, rebuild
service from fresh `Ledger::new`, read back via `count_entries()` or `get_entry()`.

This matches governance Layer 3 (deterministic shutdown + reopen). Unlike governance, ledger
has no background scheduler task — so no JoinHandle mechanism is needed. The proof is simpler:
drop the `Arc<RwLock<Ledger>>` (all clones), reopen on same path, verify.

**Artifact target:** `crates/icn-core/tests/ledger_service_persistence.rs`

---

## Comparison with Governance Proof Stack

| Layer | Governance | Ledger |
|-------|-----------|--------|
| 1 — Direct struct sled write | ✅ (implicit in persistence_proof) | ✅ `ledger_persistence.rs` |
| 2 — Store/service-backed sled write | ✅ `persistence_proof.rs` write phase | ✅ `actor_persistence_proof.rs` |
| 3 — Same-runtime close+reopen | ✅ `persistence_proof.rs` Phase 2 | ⏳ not yet |
| 4 — Cross-process restart | ✅ `governance_restart_helper` binary | ⏳ not yet |

See `governance-proof-layers.md` for the full governance pattern.
