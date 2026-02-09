# Lab 05: Mini Ledger with Double-Entry and Quarantine

## Goal
Build a minimal double-entry ledger enforcing invariants with quarantine.

## Structure
```
lab-05-mini-ledger/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── ledger.rs       # Core ledger logic
    ├── entry.rs        # Entry and Posting types
    └── quarantine.rs   # Quarantine storage
```

## Requirements

### 1. Core Types
```rust
pub struct Entry {
    pub id: String,
    pub postings: Vec<Posting>,
    pub timestamp: u64,
}

pub struct Posting {
    pub account: String,
    pub amount: i64,  // Positive = credit, negative = debit
}
```

### 2. Double-Entry Invariant
```rust
impl Ledger {
    pub fn submit(&mut self, entry: Entry) -> Result<(), LedgerError> {
        self.validate(&entry)?;
        self.store.insert(entry.id.clone(), entry);
        Ok(())
    }
    
    fn validate(&self, entry: &Entry) -> Result<(), LedgerError> {
        let sum: i64 = entry.postings.iter().map(|p| p.amount).sum();
        if sum != 0 {
            return Err(LedgerError::UnbalancedEntry { sum });
        }
        Ok(())
    }
}
```

### 3. Quarantine Pattern
```rust
pub struct QuarantinedEntry {
    pub entry: Entry,
    pub reason: String,
    pub timestamp: u64,
}

impl Ledger {
    pub fn submit(&mut self, entry: Entry) -> Result<(), LedgerError> {
        match self.validate(&entry) {
            Ok(()) => {
                self.store.insert(entry.id.clone(), entry);
                Ok(())
            }
            Err(e) => {
                self.quarantine.insert(entry.id.clone(), QuarantinedEntry {
                    entry,
                    reason: e.to_string(),
                    timestamp: unix_timestamp(),
                });
                Err(e)
            }
        }
    }
    
    pub fn get_quarantine(&self) -> &HashMap<String, QuarantinedEntry> {
        &self.quarantine
    }
}
```

### 4. Balance Queries
```rust
impl Ledger {
    pub fn get_balance(&self, account: &str) -> i64 {
        self.store.values()
            .flat_map(|entry| &entry.postings)
            .filter(|p| p.account == account)
            .map(|p| p.amount)
            .sum()
    }
}
```

## Tests to Write

### Test 1: Balanced Entry Accepted
```rust
#[test]
fn test_balanced_entry_accepted() {
    let mut ledger = Ledger::new();
    let entry = Entry {
        id: "entry-1".into(),
        postings: vec![
            Posting { account: "alice".into(), amount: -100 },
            Posting { account: "bob".into(), amount: 100 },
        ],
        timestamp: 0,
    };
    
    assert!(ledger.submit(entry).is_ok());
    assert_eq!(ledger.get_balance("alice"), -100);
    assert_eq!(ledger.get_balance("bob"), 100);
}
```

### Test 2: Unbalanced Entry Quarantined
```rust
#[test]
fn test_unbalanced_entry_quarantined() {
    let mut ledger = Ledger::new();
    let entry = Entry {
        id: "entry-2".into(),
        postings: vec![
            Posting { account: "alice".into(), amount: -100 },
            Posting { account: "bob".into(), amount: 50 },  // Unbalanced!
        ],
        timestamp: 0,
    };
    
    let result = ledger.submit(entry.clone());
    assert!(result.is_err());
    
    // Entry should be in quarantine
    assert_eq!(ledger.get_quarantine().len(), 1);
    assert_eq!(ledger.get_quarantine().get("entry-2").unwrap().entry.id, "entry-2");
}
```

### Test 3: Multiple Entries Balance
```rust
#[test]
fn test_multiple_entries_balance() {
    let mut ledger = Ledger::new();
    
    // Entry 1: Alice pays Bob 100
    ledger.submit(Entry {
        id: "e1".into(),
        postings: vec![
            Posting { account: "alice".into(), amount: -100 },
            Posting { account: "bob".into(), amount: 100 },
        ],
        timestamp: 1,
    }).unwrap();
    
    // Entry 2: Bob pays Carol 50
    ledger.submit(Entry {
        id: "e2".into(),
        postings: vec![
            Posting { account: "bob".into(), amount: -50 },
            Posting { account: "carol".into(), amount: 50 },
        ],
        timestamp: 2,
    }).unwrap();
    
    assert_eq!(ledger.get_balance("alice"), -100);
    assert_eq!(ledger.get_balance("bob"), 50);
    assert_eq!(ledger.get_balance("carol"), 50);
}
```

## Done When
- Balanced entries accepted, stored correctly
- Unbalanced entries rejected, moved to quarantine
- Quarantined entries retrievable with reason
- Balance queries work across multiple entries
- All tests pass

## Resources
- `icn-ledger/src/ledger.rs` (validation + quarantine)
- `icn-ledger/tests/dynamic_limits_integration.rs`
- Double-entry accounting: [Wikipedia](https://en.wikipedia.org/wiki/Double-entry_bookkeeping)
