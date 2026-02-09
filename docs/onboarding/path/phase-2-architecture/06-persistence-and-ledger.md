# 06: Persistence and Ledger — Time-Communication + Invariants

> **Phase**: 2 | **Tier**: Fixer  
> **Patterns introduced**: [Store Abstraction](#store-abstraction), [Double-Entry Invariant](#double-entry-invariant), [Quarantine](#quarantine)  
> **Prerequisite**: 05-the-meaning-firewall.md

## Why This Matters

Storage in distributed systems isn't "a database" — it's **time-communication**: writing data is sending a message to your future self (or future nodes). The ledger demonstrates this principle: entries persist **with their invariants**, not just their data.

ICN's ledger enforces double-entry accounting, credit limits, and fork detection at the storage layer. Entries that violate invariants are **quarantined** (isolated, not dropped), making failures visible and debuggable.

→ See `manual.md` § "Storage as Time-Communication" for the design rationale.

## What You'll Read

### 1. The Ledger Validation Chain: `icn-ledger/src/ledger.rs`

**Function**: `validate_entry()` (lines ~487-520)

This function is an **invariant chain** — each check enforces one property:

```rust
pub fn validate_entry(&mut self, entry: &JournalEntry) -> Result<(), LedgerError> {
    // Invariant 1: Double-entry balance
    let sum: i64 = entry.postings.iter().map(|p| p.amount).sum();
    if sum != 0 {
        return Err(LedgerError::UnbalancedEntry { sum });
    }
    
    // Invariant 2: Signature verification
    for sig in &entry.signatures {
        if !sig.verify(&entry.id) {
            return Err(LedgerError::InvalidSignature);
        }
    }
    
    // Invariant 3: Credit limit enforcement
    for posting in &entry.postings {
        let balance = self.cached_balances.get(&posting.account).unwrap_or(&0);
        let new_balance = balance + posting.amount;
        let credit_limit = self.get_credit_limit(&posting.account)?;
        
        if new_balance < credit_limit {
            return Err(LedgerError::CreditLimitExceeded {
                account: posting.account.clone(),
                attempted: posting.amount,
                limit: credit_limit,
            });
        }
    }
    
    // Invariant 4: Fork detection
    if self.fork_detector.is_fork(&entry)? {
        return Err(LedgerError::ForkDetected {
            entry_id: entry.id.clone(),
        });
    }
    
    Ok(())
}
```

**Key insight**: Validation is **not a monolith**. Each invariant is independent, testable, and has a specific error type.

### 2. The Quarantine System

When an entry fails validation, it's **not dropped** — it's moved to quarantine:

```rust
pub fn submit_entry(&mut self, entry: JournalEntry) -> Result<(), LedgerError> {
    match self.validate_entry(&entry) {
        Ok(()) => {
            // Valid: store normally
            self.store.put(
                format!("entry:{}", entry.id).as_bytes(),
                bincode::serialize(&entry)?,
            )?;
            self.update_cached_balances(&entry);
            
            metrics::counter!("ledger_entries_total", "status" => "accepted")
                .increment(1);
        }
        Err(e) => {
            // Invalid: quarantine with reason
            self.quarantine.insert(entry.id.clone(), QuarantinedEntry {
                entry,
                reason: e.to_string(),
                timestamp: Timestamp::now(),
            });
            
            metrics::counter!("ledger_validation_failures_total",
                "reason" => error_type(&e)).increment(1);
            
            return Err(e);
        }
    }
    Ok(())
}
```

**Why quarantine matters**:
- **Visibility**: Failed entries are retrievable for debugging
- **Auditability**: You can inspect *why* an entry failed
- **Recovery**: If validation rules change, quarantined entries can be retried
- **Security**: Prevents silent data loss from malicious or buggy entries

### 3. The Ledger Struct Composition

**File**: `icn-ledger/src/ledger.rs` (struct definition, ~lines 50-80)

```rust
pub struct Ledger {
    store: Arc<dyn Store>,                           // Persistent KV storage
    cached_balances: HashMap<Did, i64>,              // In-memory balance cache
    quarantine: HashMap<EntryId, QuarantinedEntry>,  // Failed entries
    fork_detector: ForkDetector,                     // Detects conflicting entries
    fork_resolver: ForkResolver,                     // Resolves conflicts
    credit_limits: HashMap<Did, i64>,                // Per-account limits
    dynamic_limit_manager: Option<DynamicLimitManager>, // Optional: adjust limits based on behavior
}
```

**Composition principles**:
- **Store** is abstract: Works with Sled, Redis, or in-memory (for tests)
- **Cached balances** avoid re-reading entries on every validation
- **Quarantine** keeps failed entries separate from valid ones
- **Fork detector/resolver** handle network partitions and conflicting history

### 4. Test Example: Dynamic Limit Integration

**File**: `icn-ledger/tests/dynamic_limits_integration.rs`

**Test**: `test_credit_limit_enforcement_with_dynamic_manager`

This test proves the ledger enforces credit limits correctly:

```rust
#[tokio::test]
async fn test_credit_limit_enforcement_with_dynamic_manager() {
    let store = Arc::new(MemStore::new());
    let mut ledger = Ledger::new(store, IdentityBundle::new()?);
    
    // Set credit limit: -500 (can owe up to 500)
    ledger.set_credit_limit(&alice_did, -500);
    
    // Valid entry: Alice owes 300 (within limit)
    let entry1 = JournalEntry {
        postings: vec![
            Posting { account: alice_did.clone(), amount: -300 },
            Posting { account: bob_did.clone(), amount: 300 },
        ],
        ..Default::default()
    };
    assert!(ledger.submit_entry(entry1).is_ok());
    
    // Invalid entry: Alice would owe 800 (exceeds limit)
    let entry2 = JournalEntry {
        postings: vec![
            Posting { account: alice_did.clone(), amount: -500 },
            Posting { account: bob_did.clone(), amount: 500 },
        ],
        ..Default::default()
    };
    assert!(matches!(
        ledger.submit_entry(entry2),
        Err(LedgerError::CreditLimitExceeded { .. })
    ));
    
    // Entry2 should be in quarantine
    let quarantined = ledger.get_quarantine();
    assert_eq!(quarantined.len(), 1);
}
```

**What this tests**:
1. Valid entries are accepted
2. Invalid entries are rejected with specific error
3. Rejected entries end up in quarantine (not dropped)

## Patterns Introduced

### Store Abstraction
**Pattern**: Abstract storage behind traits for testability and backend flexibility.

```rust
pub trait Store: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: Vec<u8>) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
}
```

**Implementations**:
- `SledStore` — production (persistent)
- `MemStore` — tests (in-memory)
- `RedisStore` — future (distributed cache)

→ See `patterns.md` #4 for full template.

### Double-Entry Invariant
**Pattern**: Enforce balanced entries at validation time, reject unbalanced.

```rust
let sum: i64 = entry.postings.iter().map(|p| p.amount).sum();
if sum != 0 {
    return Err(LedgerError::UnbalancedEntry { sum });
}
```

**Why**: Prevents accounting errors, ensures conservation of value.

→ See `patterns.md` #14 for full template.

### Quarantine
**Pattern**: Isolate failed entries instead of dropping them.

```rust
match validate(entry) {
    Ok(()) => store_normally(entry),
    Err(e) => quarantine.insert(entry.id, QuarantinedEntry { entry, reason: e }),
}
```

**Why**: Makes failures visible, supports debugging and auditing.

→ See `patterns.md` #16 for full template.

## What You'll Build

→ **Lab**: `labs/lab-05-mini-ledger/`

Build a minimal double-entry ledger:
- Accounts, postings, transaction IDs
- Invariant checks: balanced entries required
- Quarantine for entries that fail validation (isolated, not dropped)

**Done when**: Unbalanced entries are rejected, quarantined entries are retrievable, tests prove invariants.

## Checkpoint

You've completed this layer when you can:

1. **Trace validation**: Explain the four invariants enforced by `validate_entry()`
2. **Implement quarantine**: Add quarantine to your lab ledger, write tests proving it works
3. **Reason about storage**: Explain why storage is "time-communication" (not just "persistence")
4. **Debug failures**: Given a quarantined entry, determine which invariant it violated

**Artifact**: Lab-05 complete with tests proving all three patterns (double-entry, quarantine, store abstraction).

## Deep Reference

→ `reference/module-06-ledger-contracts.md` — Full ledger design, Merkle-DAG, sync protocol  
→ `icn-ledger/src/ledger.rs` — Source code for validation and quarantine  
→ `icn-ledger/tests/dynamic_limits_integration.rs` — Example integration tests
