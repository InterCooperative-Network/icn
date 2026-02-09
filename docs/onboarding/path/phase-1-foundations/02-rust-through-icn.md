# 02: Rust Through ICN's Lens

> **Phase**: 1 | **Tier**: Reader  
> **Patterns introduced**: [Builder](#builder-pattern), [Lifecycle State Machine](#lifecycle-state-machine)  
> **Prerequisite**: 01-environment.md

## Why This Matters

Most Rust tutorials teach ownership, borrowing, and enums as **abstract rules**. This layer reframes them as **system invariants** — properties ICN relies on for safety and correctness. When you see `&mut self`, you're not seeing "mutable borrow"; you're seeing "exclusive write access enforcing single-writer invariant."

By tracing three core types through the system, you'll learn Rust by reading production code, not toy examples.

→ See `manual.md` § "Why Rust?" for the rationale behind language choice.

## What You'll Read

### 1. Type: `Did` (Decentralized Identifier)

**File**: `icn/crates/icn-kernel-api/src/authz.rs` (also defined in `icn-identity`)

```rust
pub struct Did(String);  // Newtype pattern
```

**Ownership trace**:
- Created once during identity initialization: `Did::from_public_key()`
- **Moved** into `IdentityBundle` (no copies, no clones)
- **Borrowed** (`&Did`) when signing messages or checking authz
- **Never mutated** after creation (immutable by design)

**Rust concepts inline**:
- **Newtype pattern**: `Did(String)` prevents string misuse, adds type safety
- **Move semantics**: `Did` owns its `String`, transferring ownership transfers the string
- **Borrow checker**: `&Did` references don't outlive the original `Did`

**Invariant enforced**: DIDs are immutable identifiers — once created, they never change. Rust's ownership system guarantees this at compile time.

### 2. Type: `JournalEntry` (Ledger Transaction)

**File**: `icn/crates/icn-ledger/src/ledger.rs`

```rust
pub struct JournalEntry {
    pub id: EntryId,
    pub postings: Vec<Posting>,
    pub metadata: EntryMetadata,
    pub signatures: Vec<Signature>,
}
```

**Ownership trace**:
- Created by `Ledger::create_entry()`
- **Validated** via `&JournalEntry` borrow (no ownership transfer)
- **Cloned** for gossip broadcast (explicit copy because network needs independent data)
- **Moved** into storage via `store.put(key, bincode::serialize(&entry)?)`
- If validation fails, moved to **quarantine** (isolated, not dropped)

**Rust concepts inline**:
- **Struct ownership**: `JournalEntry` owns all fields (Vec, String, etc.)
- **Borrow for validation**: `validate_entry(&entry)` inspects without taking ownership
- **Clone for network**: `entry.clone()` creates independent copy for gossip
- **Enum for results**: `Result<(), LedgerError>` forces explicit error handling

**Invariant enforced**: Entries are validated before storage. Failed entries are quarantined, not silently dropped. The type system prevents unvalidated entries from entering storage.

### 3. Type: `GossipEntry` (Replicated State)

**File**: `icn/crates/icn-gossip/src/gossip.rs`

```rust
pub struct GossipEntry {
    pub id: EntryId,
    pub data: Vec<u8>,
    pub timestamp: LogicalTimestamp,
    pub signature: Signature,
}
```

**Ownership trace**:
- Created by app (e.g., Ledger) via `gossip.announce(topic, data)`
- **Moved** into GossipActor's internal storage (`HashMap<EntryId, GossipEntry>`)
- **Borrowed** (`&GossipEntry`) when serializing for network transmission
- **Cloned** when requested by peer via anti-entropy sync
- **Never deleted** (append-only log, pruning is explicit)

**Rust concepts inline**:
- **HashMap ownership**: `HashMap<K, V>` owns both keys and values
- **Entry API**: `map.entry(key).or_insert(value)` handles insert-if-absent idiomatically
- **Arc for shared state**: `Arc<RwLock<GossipActor>>` allows concurrent read/write access
- **Lifetime elision**: `&GossipEntry` borrow lifetime is inferred from function signature

**Invariant enforced**: Gossip entries are append-only. Once inserted, they persist until explicit pruning. Rust's ownership prevents accidental mutation or deletion.

## Patterns Introduced

### Builder Pattern
**Used for**: Complex struct construction with optional fields.

**Example**: `NetworkConfig::builder()` from `icn-net/src/config.rs`

```rust
let config = NetworkConfig::builder()
    .listen_addr("0.0.0.0:5000".parse()?)
    .enable_mdns(true)
    .max_connections(100)
    .build()?;
```

→ See `patterns.md` #5 for full template.

### Lifecycle State Machine
**Used for**: Tracking actor/connection state transitions.

**Example**: Proposal lifecycle in `icn-governance/src/proposal.rs`

```rust
pub enum ProposalState {
    Draft,
    Proposed { voting_ends: Timestamp },
    Active,
    Terminated { reason: String },
}
```

Transitions are explicit functions: `draft_to_proposed()`, `proposed_to_active()`. Invalid transitions return errors, not panics.

→ See `patterns.md` #6 for full template.

## What You'll Build

→ **Lab**: Completed in `labs/lab-01-workspace/` (no new lab)

Extend your workspace from Layer 01:
- Add a newtype (`UserId(String)`) with validation
- Add a builder for your actor config
- Add a state enum with transition methods

This reinforces Rust concepts through practical application.

## Checkpoint

You've completed this layer when you can:

1. **Identify invariants**: Given a Rust function, explain what invariants it enforces (e.g., "This function requires exclusive access to prevent concurrent writes")
2. **Trace ownership**: Draw a diagram showing how a `Did`, `JournalEntry`, or `GossipEntry` moves through the system
3. **Explain borrow checker errors**: When you see a borrow checker error, translate it to the invariant being protected
4. **Use patterns**: Write a small builder or state machine without consulting examples

**Artifact**: Code review exercise — given a snippet with an ownership/borrowing bug, identify and fix it. (Provided in checkpoint materials.)

## Deep Reference

→ `reference/module-01-rust-fundamentals.md` — Ownership, borrowing, lifetimes, async/await in depth  
→ `patterns.md` #5 (Builder), #6 (Lifecycle State Machine)  
→ [The Rust Book](https://doc.rust-lang.org/book/) chapters 4, 6, 10 for foundational concepts
