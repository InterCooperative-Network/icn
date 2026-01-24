# Workshop 6: Ledger and Contract Flow

## Goal
Trace a ledger payment flow from API request to gossip propagation, and understand
how CCL contracts execute with capabilities.

## Prerequisites
- Completed Module 6 reading
- ICN binaries built (`cargo build`)
- Understanding of double-entry accounting basics

## Estimated time
3-4 hours

## Part 1: Ledger Entry Structure

### Steps
1. Open `icn/crates/icn-ledger/src/types.rs` (or search for LedgerEntry)
2. Find the `LedgerEntry` struct definition
3. Identify all fields and their purposes

### Expected structure
```rust
pub struct LedgerEntry {
    pub id: EntryId,           // Unique hash-based identifier
    pub debit: Did,            // Account debited (money leaves)
    pub credit: Did,           // Account credited (money arrives)
    pub amount: i64,           // Units transferred
    pub memo: Option<String>,  // Human-readable description
    pub timestamp: u64,        // When created
    pub parents: Vec<EntryId>, // Merkle-DAG references
    pub signature: Signature,  // Creator's signature
}
```

### Questions to answer
1. Why are there two accounts (debit and credit) per entry?
2. What does the `parents` field enable?
3. How is the entry ID computed?

### Checkpoint
- [ ] You understand double-entry structure
- [ ] You can explain the Merkle-DAG relationship

## Part 2: Merkle-DAG Structure

### Steps
1. Search for parent/child handling:
   ```bash
   grep -r "parents\|parent" icn/crates/icn-ledger/src/ --include="*.rs" | head -15
   ```
2. Find how entry IDs are computed
3. Understand how entries reference each other

### Diagram: Merkle-DAG
```
    Entry A (genesis)
         │
         ▼
    Entry B ◄─────────┐
         │            │
         ▼            │
    Entry C      Entry D
         │            │
         └────────────┤
                      ▼
                  Entry E
                (references B, C, D)
```

### Questions to answer
1. What is a genesis entry?
2. Can an entry have multiple parents?
3. How does the DAG enable parallel transactions?

### Code to find
```rust
impl LedgerEntry {
    pub fn compute_id(&self) -> EntryId {
        // Hash of entry content (excluding signature)
        let mut hasher = Blake2b::new();
        hasher.update(&self.debit.to_bytes());
        hasher.update(&self.credit.to_bytes());
        hasher.update(&self.amount.to_le_bytes());
        // ... hash parents too
        EntryId::from(hasher.finalize())
    }
}
```

### Checkpoint
- [ ] You understand Merkle-DAG structure
- [ ] You can explain how integrity is verified

## Part 3: Balance Computation

### Steps
1. Find balance calculation code in `icn-ledger`
2. Understand how balances are derived from entries
3. Identify caching or optimization strategies

### Balance computation
```
Balance(Alice) = Σ credits_to_Alice - Σ debits_from_Alice
```

Example:
```
Entry 1: Bob → Alice: 100   (Alice +100)
Entry 2: Alice → Carol: 30  (Alice -30)
Entry 3: Dan → Alice: 50    (Alice +50)
---
Balance(Alice) = 100 - 30 + 50 = 120
```

### Questions to answer
1. How does the ledger handle concurrent balance queries?
2. Is balance computed on-demand or cached?
3. What happens if an entry is missing from the DAG?

### Exercise
Search for balance methods:
```bash
grep -r "balance\|Balance" icn/crates/icn-ledger/src/ --include="*.rs" | head -20
```

### Checkpoint
- [ ] You understand how balances are derived
- [ ] You can trace the balance computation path

## Part 4: Transaction Validation

### Steps
1. Find the validation code for new entries
2. List all validation checks performed
3. Understand error handling

### Validation checks
1. **Signature valid**: Entry is signed by debit account holder
2. **Amount positive**: Cannot transfer negative amounts
3. **Sufficient balance**: Debit account has enough (may allow credit limits)
4. **Unique ID**: Entry hasn't been processed before
5. **Valid parents**: Referenced entries exist

### Questions to answer
1. Who must sign a ledger entry?
2. What happens if validation fails?
3. How are credit limits enforced?

### Code pattern
```rust
fn validate_entry(&self, entry: &LedgerEntry) -> Result<(), LedgerError> {
    // Verify signature
    entry.debit.verify(&entry.signature, &entry.content_hash())?;

    // Check amount
    if entry.amount <= 0 {
        return Err(LedgerError::InvalidAmount);
    }

    // Check balance
    let balance = self.get_balance(&entry.debit)?;
    if balance < entry.amount {
        return Err(LedgerError::InsufficientBalance);
    }

    Ok(())
}
```

### Checkpoint
- [ ] You can list all validation checks
- [ ] You understand error handling for invalid entries

## Part 5: Gossip Integration

### Steps
1. Find where ledger entries are announced to gossip
2. Trace how incoming gossip entries are processed
3. Understand conflict resolution

### Flow: Creating a Transaction
```
Gateway API
    │
    ▼
Ledger.create_entry()
    │
    ├─► Validate
    │
    ├─► Store locally
    │
    └─► GossipActor.announce("ledger:entries", entry)
            │
            └─► NetworkActor.broadcast()
```

### Flow: Receiving a Transaction
```
NetworkActor
    │
    ▼
GossipActor.handle_message()
    │
    ▼
Ledger.apply_entry()
    │
    ├─► Validate
    │
    └─► Store locally
```

### Questions to answer
1. How is the "ledger:entries" topic used?
2. What happens if an entry arrives out of order?
3. How are duplicate entries handled?

### Checkpoint
- [ ] You understand ledger-gossip integration
- [ ] You can trace an entry from creation to propagation

## Part 6: CCL Contract Structure

### Steps
1. Open `icn/crates/icn-ccl/src/ast.rs`
2. Find the `Contract` struct
3. Explore `Rule`, `Condition`, and `Effect` types

### Contract structure
```rust
pub struct Contract {
    pub name: String,
    pub version: Version,       // Semantic version
    pub parties: Vec<Did>,      // Who can participate
    pub rules: Vec<Rule>,       // Conditional logic
    pub state: ContractState,   // Mutable state
}

pub struct Rule {
    pub name: String,
    pub conditions: Vec<Expr>,  // When to trigger
    pub effects: Vec<Stmt>,     // What happens
}
```

### Questions to answer
1. What makes CCL "not Turing-complete"?
2. How are contract parties specified?
3. Where is contract state stored?

### Checkpoint
- [ ] You understand contract structure
- [ ] You can identify rules and their components

## Part 7: Capability System

### Steps
1. Find the `Capability` enum in `icn-ccl`
2. List all capability types
3. Understand how capabilities are checked

### Capabilities
```rust
pub enum Capability {
    ReadLedger,                    // Query balances
    WriteLedger { max_amount: i64 }, // Transfer up to max
    ReadTrust,                     // Query trust scores
    SendMessage { topic: Topic },  // Publish to topic
}
```

### Questions to answer
1. How does a contract declare required capabilities?
2. Who grants capabilities to a contract?
3. What happens if a contract exceeds its capabilities?

### Example
```
Contract "profit-share" requires:
  - ReadLedger (to check current balances)
  - WriteLedger { max_amount: 1000 } (to distribute profits)
```

### Checkpoint
- [ ] You understand capability-based security
- [ ] You can trace capability checking

## Part 8: Fuel Metering

### Steps
1. Search for fuel/gas metering:
   ```bash
   grep -r "fuel\|Fuel" icn/crates/icn-ccl/src/ --include="*.rs" | head -15
   ```
2. Find how fuel is consumed
3. Understand fuel limits

### Fuel costs (conceptual)
| Operation | Fuel Cost |
|-----------|-----------|
| Variable read | 1 |
| Arithmetic op | 1 |
| Ledger read | 10 |
| Ledger write | 100 |
| Loop iteration | 5 |

### Questions to answer
1. Why is fuel metering necessary?
2. How is fuel limit set for a contract execution?
3. What happens when fuel is exhausted?

### Code pattern
```rust
struct Interpreter {
    fuel: u64,
    fuel_limit: u64,
}

impl Interpreter {
    fn consume_fuel(&mut self, amount: u64) -> Result<()> {
        if self.fuel + amount > self.fuel_limit {
            return Err(ExecutionError::OutOfFuel);
        }
        self.fuel += amount;
        Ok(())
    }
}
```

### Checkpoint
- [ ] You understand fuel metering purpose
- [ ] You can trace fuel consumption

## Part 9: End-to-End Transaction Exercise

### Steps
1. Set up a local node (if not already running)
2. Use the SDK or CLI to create a transaction
3. Trace through logs to see the flow

### Using CLI
```bash
export ICN_DATA=$(mktemp -d)
export ICN_PASSPHRASE="workshop"

# Initialize identity
./target/debug/icnctl --data-dir "$ICN_DATA" id init

# Start daemon with debug logging
RUST_LOG=icn_ledger=debug ./target/debug/icnd --data-dir "$ICN_DATA" &

# Create a test transaction (syntax may vary)
./target/debug/icnctl --data-dir "$ICN_DATA" ledger transfer \
  --to did:icn:somerecipient \
  --amount 10 \
  --memo "test transfer"
```

### Expected log output
- "Creating ledger entry"
- "Validating entry..."
- "Entry stored: <id>"
- "Announcing to gossip"

### Checkpoint
- [ ] You created a transaction successfully
- [ ] You traced it through the logs

## Summary

After completing this workshop you should be able to:
- Trace a ledger transaction from creation to gossip propagation
- Explain Merkle-DAG structure and validation
- Understand CCL contract structure and capabilities
- Describe fuel metering and its purpose
- Debug ledger operations using logs

## Troubleshooting

### "Insufficient balance"
The source account doesn't have enough credits. Check balance with CLI.

### "Invalid signature"
The transaction must be signed by the debit account's private key.

### "Entry already exists"
Duplicate entries are rejected. Check if transaction was already processed.

### "Out of fuel"
Contract execution exceeded fuel limit. Reduce complexity or increase limit.

## Next steps
Proceed to Workshop 7: Gateway Auth and SDK Usage
