# Workshop 2: Architecture Mapping Exercise

## Goal
Build a mental model of ICN's layered architecture by mapping documentation to
code and drawing system diagrams.

## Prerequisites
- Completed Module 2 reading
- Access to docs/ARCHITECTURE.md

## Estimated time
1-2 hours

## Part 1: Layer Stack Mapping

### Steps
1. Open `docs/ARCHITECTURE.md`
2. Read the "Layer Stack" section
3. Create a table mapping each layer to its crate:

| Layer | Crate | Primary Responsibility |
|-------|-------|----------------------|
| Identity | icn-identity | DID generation, key management |
| Trust | icn-trust | ? |
| Transport | icn-net | ? |
| Gossip | icn-gossip | ? |
| Ledger | icn-ledger | ? |
| Contracts | icn-ccl | ? |
| Gateway | icn-gateway | ? |

4. Fill in the "Primary Responsibility" column for each layer

### Expected output
Your table should show clear separation of concerns - each crate handles one
layer's functionality without crossing boundaries.

### Checkpoint
- [ ] You completed the mapping table
- [ ] You can explain why layers are separated into distinct crates

## Part 2: Crate Dependency Analysis

### Steps
1. Open `icn/crates/icn-core/Cargo.toml`
2. List all workspace dependencies (dependencies starting with `icn-`)
3. Draw arrows showing which crates `icn-core` depends on

### Expected diagram
```
icn-core
  ├── icn-identity
  ├── icn-trust
  ├── icn-gossip
  ├── icn-ledger
  ├── icn-net
  ├── icn-obs
  └── ...
```

### Checkpoint
- [ ] You can identify at least 5 internal crate dependencies
- [ ] You understand why icn-core is the "orchestration" crate

## Part 3: Draw the Architecture from Memory

### Exercise
Without looking at the documentation, draw the ICN layer stack on paper or in
a text diagram. Include:

1. All 7+ major layers
2. Direction of dependencies (which layer depends on which)
3. At least one "cross-cutting" concern (storage, observability)

### Self-check
Compare your diagram to `docs/ARCHITECTURE.md`. Note any layers you missed or
misplaced.

### Checkpoint
- [ ] Your diagram includes identity, trust, transport, gossip, ledger, contracts
- [ ] You correctly showed that ledger depends on gossip (not vice versa)

## Part 4: Integration Boundary Analysis

### Steps
1. Open `icn/crates/icn-gateway/src/lib.rs` or `README.md`
2. Identify what internal crates the gateway integrates
3. Answer: Why is the gateway a separate crate from icn-core?

### Guiding questions
- What external protocols does the gateway expose (REST, WebSocket)?
- What internal systems does it wrap (identity, ledger, trust)?
- Who are the consumers of the gateway API?

### Checkpoint
- [ ] You can explain the gateway's role as an "integration boundary"
- [ ] You understand why external API is separate from internal runtime

## Part 5: Quick Architecture Quiz

Answer without looking:

1. Which crate handles DID creation and key rotation?
2. Which crate implements the double-entry mutual credit ledger?
3. Which crate provides the REST API for external clients?
4. Which layer provides "social weighting" for peer connections?
5. What synchronization protocol does icn-gossip implement?

### Answers
1. icn-identity
2. icn-ledger
3. icn-gateway
4. Trust (icn-trust)
5. Gossip with vector clocks and anti-entropy

## Summary
After completing this workshop you should be able to:
- Map each architectural layer to its implementing crate
- Explain why ICN uses a layered crate structure
- Draw the layer stack from memory
- Identify integration boundaries (core, gateway)

## Next steps
Proceed to Module 3: Runtime and Actor Model
