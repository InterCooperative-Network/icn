# Workshop 6: Ledger and Contract Flow

## Learning Objectives

By the end of this workshop, you will be able to:

1. **Explain double-entry accounting** - Understand debits, credits, and balance computation
2. **Navigate the Merkle-DAG** - Trace entry relationships and verify integrity
3. **Validate transactions** - Know what checks are performed before entry acceptance
4. **Execute CCL contracts** - Understand capability-based contract execution
5. **Debug ledger issues** - Use logs to trace transaction flow and identify problems

## Goal
Trace a ledger payment flow from API request to gossip propagation, and understand
how CCL contracts execute with capabilities.

## Prerequisites
- Completed [Module 6: Ledger and CCL](../reference/module-06-ledger-contracts.md)
- ICN binaries built (`cargo build`)
- Understanding of double-entry accounting basics

## Estimated time
3-4 hours

## Related Materials
- [Module 6: Ledger and CCL](../reference/module-06-ledger-contracts.md) - Background reading
- [Workshop 5: Network and Gossip](./workshop-05-network-gossip.md) - How entries propagate
- [Workshop 7: Gateway and SDK](./workshop-07-gateway-sdk.md) - API for ledger operations

## Part 1: Ledger Entry Structure

### Steps
1. Open `icn/crates/icn-ledger/src/types.rs` (or search for LedgerEntry)
2. Find the `LedgerEntry` struct definition
3. Identify all fields and their purposes

### Actual structure

The actual code uses `JournalEntry` (in `icn/crates/icn-ledger/src/types.rs`):

```rust
pub struct JournalEntry {
    pub id: Option<ContentHash>,      // Hash-based identifier (computed)
    pub author: Did,                   // Who created this entry
    pub accounts: Vec<AccountDelta>,   // Multi-currency double-entry (debits/credits)
    pub timestamp: u64,                // When created
    pub parents: Vec<ContentHash>,     // Merkle-DAG references
    pub signature: Option<Signature>,  // Creator's signature
    pub contract_ref: Option<String>,  // Related CCL contract
}

pub struct AccountDelta {
    pub account_id: String,     // Account identifier
    pub currency: String,       // Currency code
    pub debit: Option<i64>,     // Amount debited (leaves account)
    pub credit: Option<i64>,    // Amount credited (enters account)
}
```

Key difference: Instead of a single debit/credit pair, entries use `Vec<AccountDelta>`
to support multi-currency, generalized double-entry bookkeeping.

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

In `icn/crates/icn-ledger/src/entry.rs`, find the hash computation:

```rust
impl JournalEntry {
    pub fn compute_hash(&self) -> ContentHash {
        // Hash of entry content (excluding signature and id)
        let mut hasher = Blake2b256::new();
        hasher.update(self.author.as_bytes());
        // Hash each account delta
        for delta in &self.accounts {
            hasher.update(delta.account_id.as_bytes());
            hasher.update(delta.currency.as_bytes());
            // ... hash amounts
        }
        // Hash parents for DAG integrity
        for parent in &self.parents {
            hasher.update(parent.as_bytes());
        }
        ContentHash::from(hasher.finalize())
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

### Balance computation (simplified concept)

The basic formula for a single currency:
```
Balance(Account, Currency) = Σ credits_to_Account - Σ debits_from_Account
```

**Conceptual example** (simplified - actual entries use `AccountDelta`):

> **Warning**: This example is conceptual. Real entries use `Vec<AccountDelta>`
> for multi-currency support. See the actual code pattern below.

```
Entry 1: Bob → Alice: 100 USD   (Alice's USD balance +100)
Entry 2: Alice → Carol: 30 USD  (Alice's USD balance -30)
Entry 3: Dan → Alice: 50 USD    (Alice's USD balance +50)
---
Balance(Alice, USD) = 100 - 30 + 50 = 120
```

**Important**: The actual ledger is multi-currency. Each `JournalEntry` contains
`Vec<AccountDelta>` where each delta specifies the currency. Balances are computed
per-account and per-currency.

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

### Code pattern (conceptual)

The actual validation in `icn/crates/icn-ledger/src/ledger.rs` uses `JournalEntry`:

```rust
fn validate_entry(&self, entry: &JournalEntry) -> Result<()> {
    // 1. Verify signature from author
    if let Some(sig) = &entry.signature {
        entry.author.verify(sig, &entry.compute_hash().as_bytes())?;
    }

    // 2. Check all account deltas have valid amounts
    for delta in &entry.accounts {
        if let Some(debit) = delta.debit {
            if debit <= 0 { return Err(LedgerError::InvalidAmount); }
        }
        if let Some(credit) = delta.credit {
            if credit <= 0 { return Err(LedgerError::InvalidAmount); }
        }
    }

    // 3. Check author has sufficient balance (per currency)
    // ... currency-specific balance checks

    Ok(())
}
```

**Note**: This is simplified. The actual validation includes trust checks,
witness requirements for high-value transactions, and more.

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

From `icn/crates/icn-ccl/src/ast.rs`:

```rust
pub struct Contract {
    pub name: String,
    pub participants: Vec<Did>,      // Who can participate
    pub currency: Option<String>,    // Optional currency code
    pub rules: Vec<Rule>,            // Conditional logic
    pub state_vars: Vec<StateVar>,   // Mutable state variables
    pub triggers: Vec<Trigger>,      // Event triggers
}

pub struct Rule {
    pub name: String,
    pub params: Vec<Param>,     // Rule parameters
    pub requires: Vec<Expr>,    // Preconditions
    pub body: Vec<Stmt>,        // Rule body (what happens)
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

# Note: Transactions are created via the Gateway API or SDK, not icnctl.
# Use icnctl to inspect ledger state after transactions:
./target/debug/icnctl --data-dir "$ICN_DATA" ledger head
./target/debug/icnctl --data-dir "$ICN_DATA" ledger history
./target/debug/icnctl --data-dir "$ICN_DATA" ledger balance <account-id>
```

### Available icnctl ledger commands
- `ledger head` - Show most recent ledger entry
- `ledger balance <account>` - Show balance for an account
- `ledger history` - Show recent ledger history
- `ledger quarantine` - Quarantine management

### Expected log output
- "Creating ledger entry"
- "Validating entry..."
- "Entry stored: <id>"
- "Announcing to gossip"

### Checkpoint
- [ ] You started the daemon with debug logging
- [ ] You understand how to inspect ledger state

## Summary

After completing this workshop you should be able to:
- Trace a ledger transaction from creation to gossip propagation
- Explain Merkle-DAG structure and validation
- Understand CCL contract structure and capabilities
- Describe fuel metering and its purpose
- Debug ledger operations using logs

## Key Takeaways

| Concept | Key Point |
|---------|-----------|
| **JournalEntry** | Uses `Vec<AccountDelta>` for multi-currency double-entry |
| **Merkle-DAG** | Each entry references parents; hash includes parent hashes |
| **Balance** | Computed from sum of credits minus sum of debits per currency |
| **Validation** | Signature + amount + balance + parents + uniqueness |
| **CCL Capabilities** | Contracts declare required permissions (ReadLedger, WriteLedger, etc.) |
| **Fuel Metering** | Prevents infinite loops; each operation costs fuel |

## Try It Yourself

**Challenge 1**: Trace a complete transaction
1. Start the daemon with ledger debug logging:
   ```bash
   RUST_LOG=icn_ledger=debug ./target/debug/icnd --data-dir "$ICN_DATA"
   ```
2. Create a transaction via the gateway API
3. Follow the logs to see: validation → storage → gossip announcement
4. Query the transaction using `icnctl ledger history`

**Challenge 2**: Explore the DAG structure
1. Create several transactions
2. Use `icnctl ledger head` to find the latest entry
3. Trace back through parents to understand the DAG shape
4. What happens if you create transactions in parallel?

**Challenge 3**: CCL contract exploration
Find a CCL contract in the codebase and analyze:
- What capabilities does it require?
- What operations does it perform?
- How much fuel would it consume?

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
