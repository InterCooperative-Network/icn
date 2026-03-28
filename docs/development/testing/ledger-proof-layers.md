---
Status: partial
Canonical: no
Last Reviewed: 2026-03-28
---

# Ledger Proof Layers

Ledger currently has Layer 1 (direct struct persistence). The full four-layer stack
is the target; this document records what is proven, what is not, and the next steps.

---

## Current: Layer 1 — Direct Struct Sled Write Proof

**What it proves:** A `JournalEntry` written through `Ledger::new(Arc<SledStore>)` →
`append_entry()` survives an in-process sled drop-and-reopen. The in-memory
`cached_balances` is gone after the drop. The fresh `Ledger::new` reads back via sled only.

**Also proven:** The reopened store accepts a second write — it is not opened in
read-only/WAL-replay mode.

**Artifact:** `crates/icn-ledger/tests/ledger_persistence.rs` →
`test_ledger_entry_survives_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-ledger --test ledger_persistence
```

**What is asserted:**
- Author DID survives round-trip
- Both `AccountDelta` sides survive with exact values (account_id, currency, debit/credit amounts)
- `ProvenanceRef::SystemGenerated { reason }` survives round-trip (proves full body, not just header)
- Reopened ledger accepts a second `append_entry` without error

---

## What Is NOT Proven

| Gap | Why it matters | Next layer target |
|-----|---------------|-------------------|
| Actor-backed path | `Ledger` is a direct struct; `apps/ledger` actor layer not exercised | Layer 2 |
| Same-runtime close+reopen via actor | No actor handle, no shutdown mechanism tested | Layer 2 or 3 |
| Cross-process restart | Only same-process drop proven | Layer 4 |
| Balance cache rebuild correctness | `cached_balances` repopulated from sled on `Ledger::new` — reads return correct values after reload | Separate correctness test |
| Gossip sync across restart | Gossip callbacks not exercised | Multi-node integration test |
| Schema migration | No test that old sled data is readable after type changes | Versioned serde roundtrip |

---

## Next Layer: Layer 2 — Actor-backed Sled Write Proof

**Target:** Prove that state written through the `apps/ledger` actor path (not a direct
`Ledger::new` call) persists to sled. This mirrors governance Layer 2.

**What it would need:**
- Spawn the ledger actor (equivalent of `GovernanceActor::spawn`)
- Write a transfer through the actor-backed manager
- Shut down the actor (deterministic shutdown, JoinHandle-awaited)
- Open a fresh `Ledger::new` on the same sled path
- Read back via the manager API
- Assert state persisted

**Artifact target:** `apps/ledger/tests/persistence_proof.rs`

---

## Comparison with Governance Proof Stack

| Layer | Governance | Ledger |
|-------|-----------|--------|
| 1 — Direct struct sled write | ✅ (implicit in persistence_proof Phase 1) | ✅ `ledger_persistence.rs` |
| 2 — Actor-backed sled write | ✅ `persistence_proof.rs` Phase 1 write | ⏳ not yet |
| 3 — Same-runtime close+reopen | ✅ `persistence_proof.rs` Phase 2 | ⏳ not yet |
| 4 — Cross-process restart | ✅ `governance_restart_helper` binary | ⏳ not yet |

See `governance-proof-layers.md` for the full governance proof pattern and the reusable
shutdown mechanism (`JoinHandle`-based) to copy when implementing ledger Layer 2.
