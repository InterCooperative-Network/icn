---
Status: layers 1-3 complete
Canonical: no
Last Reviewed: 2026-03-28
---

# Ledger Proof Layers

Ledger has Layers 1, 2, and 3 complete. Layer 4 (cross-process) remains.

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

**Architecture note:** `apps/ledger` provides two sled-backed stores (`SledEscrowStore`,
`SledBudgetStore`) used directly by the daemon — the same role as `SledGovernanceStateStore`
in governance. There is no Tokio actor in `apps/ledger`.

**What it proves:** State written through `SledEscrowStore::put()` survives a drop-and-reopen
boundary with exact field values. The reopened store accepts further writes.

**Also noted:** `LedgerServiceImpl` (in icn-core) has a separate existing proof:
`test_submit_treasury_entry_idempotency_survives_restart` in `ledger_service.rs`.

**Artifact:** `apps/ledger/tests/actor_persistence_proof.rs` →
`test_escrow_record_survives_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-ledger-actor --test actor_persistence_proof
```

**What is asserted:**
- `escrow_id`, `scope_id`, `funder_did`, `beneficiary_did`, `amount`, `currency`, `status`
  survive sled round-trip with exact values
- `EscrowStatus::Released` + `release_decision_hash` survive a state-transition write

---

## Layer 3 — Service-layer Same-Runtime Reopen Proof ✅

**What it proves:** A `JournalEntry` written through `LedgerServiceImpl::submit_treasury_entry()`
(the canonical production service path that wraps `Arc<RwLock<Ledger>>`) survives dropping all
runtime state and reopening sled in the same runtime. Unlike governance Layer 3, no JoinHandle
or background-task shutdown is needed — there is no scheduler task.

**Artifact:** `crates/icn-core/tests/ledger_service_persistence.rs` →
`test_ledger_service_entry_survives_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-core --test ledger_service_persistence
```

**What is asserted:**
- `count_entries() == 1` on fresh `Ledger::new` — entry came from sled, not in-memory state
- `get_entry(hash)` returns entry with exact `author` DID and 2 `AccountDelta`s
- `ProvenanceRef::Governance { receipt_id }` survives round-trip (proves the service-layer
  provenance path, not just system provenance)
- Reopened ledger accepts a second direct `append_entry` (writable, not read-only)

---

## Run Full Layer 1–3 Suite

```bash
cargo test -p icn-ledger --test ledger_persistence
cargo test -p icn-ledger-actor --test actor_persistence_proof
cargo test -p icn-core --test ledger_service_persistence
```

---

## What Is NOT Proven

| Gap | Why it matters | Next layer target |
|-----|---------------|-------------------|
| Cross-process restart | Only same-runtime drop proven | Layer 4 |
| Receipt-index idempotency across restart | Receipt index is a separate sled store — proven separately in `test_submit_treasury_entry_idempotency_survives_restart` | Already covered |
| Balance cache rebuild correctness | Separate correctness concern, not persistence | Dedicated test |
| Gossip sync across restart | Callbacks not exercised | Multi-node integration test |

---

## Next Layer: Layer 4 — Cross-Process Restart Proof

**Target:** Prove that ledger state written in one OS process is readable in a fresh OS process.

**Pattern:** Follow governance Layer 4 exactly:
- Add `src/bin/ledger_restart_helper.rs` to `crates/icn-core` (or `apps/ledger`)
- Write mode: open sled, construct `LedgerServiceImpl`, call `submit_treasury_entry`, print entry hash, exit
- Read mode: open same sled path, construct fresh `Ledger::new`, call `get_entry(hash)`, exit 0 if found
- Integration test: `cargo test -p icn-core --test ledger_service_persistence` (or a new file) spawns write subprocess, reads stdout hash, spawns read subprocess

**Key difference from governance:** No `GovernanceHandle::shutdown()` needed — just `drop(rt)` before `process::exit` (as in `governance_restart_helper.rs`). The `Ledger`'s sled store flushes when its `Arc` drops.

---

## Comparison with Governance Proof Stack

| Layer | Governance | Ledger |
|-------|-----------|--------|
| 1 — Direct struct sled write | ✅ (implicit in persistence_proof) | ✅ `ledger_persistence.rs` |
| 2 — Store/service-backed sled write | ✅ `persistence_proof.rs` write phase | ✅ `actor_persistence_proof.rs` |
| 3 — Same-runtime close+reopen | ✅ `persistence_proof.rs` Phase 2 | ✅ `ledger_service_persistence.rs` |
| 4 — Cross-process restart | ✅ `governance_restart_helper` binary | ⏳ not yet |

See `governance-proof-layers.md` for the full governance pattern and Layer 4 implementation guide.
