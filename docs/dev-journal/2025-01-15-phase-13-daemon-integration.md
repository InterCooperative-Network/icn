# Phase 13 - Governance Daemon Integration

**Date:** 2025-01-15
**Author:** Claude (with Matt)
**Status:** In Progress
**Related Commits:** TBD

## Overview

Phase 13 completed the governance substrate (types, CLI, integration test). Now we're elevating governance from a CLI-only feature to a **first-class daemonized subsystem** in `icnd`.

## Current State (Before This Session)

**What exists:**
- ✅ `icn-governance` crate with all core types (39 passing tests)
- ✅ `icnctl gov` CLI commands (domain/proposal/vote CRUD)
- ✅ Multi-node integration test (`governance_integration.rs`)
- ✅ Gossip protocol (7 `GovernanceMessage` types)
- ✅ Storage pattern (`gov:domain:{id}`, `gov:proposal:{id}`, `gov:vote:{pid}:{voter}`)

**What's missing:**
- ❌ No `GovernanceActor` in `icn-core` runtime
- ❌ CLI talks directly to `SledStore` (not daemon)
- ❌ No time-based proposal closing
- ❌ No trust graph integration for membership

## Design Decisions

### 1. Actor Communication Pattern

**Decision:** Hybrid `Arc<RwLock<GovernanceActor>>` + `mpsc` for background tasks

**Rationale:**
- Matches `GossipActor` pattern (synchronous state access)
- Governance operations are mostly sync (store read/write)
- Add `mpsc::channel` later for time-based closing scheduler

**Alternative considered:** Full message-passing like `NetworkActor`
**Rejected because:** Adds complexity for no benefit; governance doesn't need async request/response patterns

### 2. Store Integration

**Decision:** Use `Arc<dyn Store>` trait

**Rationale:**
- Decouples from `SledStore` implementation
- Enables testing with in-memory stores
- Already used by ledger/trust actors

**Key patterns (aligned with CLI):**
```rust
gov:domain:{domain_id}       → GovernanceDomain
gov:proposal:{proposal_id}   → Proposal
gov:vote:{proposal_id}:{voter_did} → Vote
```

### 3. Supervisor Integration

**Decision:** Wire via gossip notification callback

**Rationale:**
- Governance messages already flow through gossip
- Matches existing ledger sync pattern
- Keeps supervisor routing simple
- No new message handler registration needed

**Initialization sequence:**
```rust
1. Spawn GovernanceActor
2. Subscribe to "governance:proposal" topic
3. Set notification callback on gossip handle
4. Incoming messages → handle_incoming() → store updates
5. Outgoing commands → publish() → gossip broadcast
```

### 4. Admin API Layer (Deferred to Next Session)

**Decision:** Extend `icn-rpc` with governance service

**Rationale:**
- Reuse existing gRPC infrastructure
- Type-safe protobuf definitions
- Network-ready for remote administration
- Consistent with ledger/trust RPC patterns

**Alternative considered:** JSON-over-stdin for `icnctl`
**Deferred because:** Protobuf gives us better versioning and type safety

## Architecture

### GovernanceActor Structure

```rust
pub struct GovernanceActor {
    did: Did,
    store: Arc<dyn Store>,
    gossip: Arc<RwLock<GossipActor>>,
    resolver: Arc<dyn MembershipResolver + Send + Sync>,
    profile: GovernanceProfile, // cooperative_default only for now
}

pub struct GovernanceHandle {
    inner: Arc<RwLock<GovernanceActor>>,
}
```

### Command Interface

Internal command enum (will map 1:1 to RPC later):

```rust
pub enum GovernanceCommand {
    CreateDomain { domain_id, name, config },
    CreateProposal { domain_id, title, description, payload },
    OpenProposal { proposal_id, voting_period_seconds },
    CastVote { proposal_id, choice, comment },
    CloseProposal { proposal_id },
}
```

### Message Flow

**Creating a proposal:**
1. User → `icnctl gov proposal create` (v0: direct store write)
2. Future: User → `icnctl` → RPC → `GovernanceHandle.submit(CreateProposal)`
3. Actor → Validate → Store write → Publish `GovernanceMessage::ProposalCreated`
4. Gossip → Broadcast to peers
5. Peers → Notification callback → `handle_incoming()` → Store write

**Voting convergence:**
1. Node A → `submit(CastVote)` → Store + Broadcast
2. Nodes B, C → Receive via gossip → Store vote
3. Any node → `submit(CloseProposal)` → Load all votes → Tally → Evaluate → Broadcast outcome
4. All nodes → Converge on same `ProposalState` (Accepted/Rejected/NoQuorum)

### Gossip Integration

The actor uses the existing gossip notification callback pattern:

```rust
gossip.set_notification_callback(Arc::new(move |topic, entry, _| {
    if topic != GOVERNANCE_TOPIC { return; }
    let msg = GovernanceMessage::from_bytes(&entry.data)?;
    handle_incoming(store, msg)?;
}));
```

**Incoming message handlers:**
- `DomainCreated` → Write to store at `gov:domain:{id}`
- `ProposalCreated` → Write to store at `gov:proposal:{id}`
- `ProposalOpened` → Update proposal state to `Open { opened_at, closes_at }`
- `VoteCast` → Write/overwrite vote at `gov:vote:{pid}:{voter}`
- `ProposalClosed` → Update proposal state to terminal (Accepted/Rejected/NoQuorum)

### Membership Resolution

**Phase 1 (This Session):** `StaticMembershipResolver`
- Reads `MembershipConfig::StaticList(Vec<Did>)`
- Returns hardcoded member list

**Phase 2 (Future):** `TrustGraphMembershipResolver`
- Reads `MembershipConfig::TrustThreshold(f32)`
- Queries trust graph for DIDs above threshold
- Enables dynamic membership based on trust scores

Actor is constructed with `Arc<dyn MembershipResolver>` to enable future swap.

## Implementation Plan

### Session 1: Core Actor (Today)

**Files to create:**
1. `icn-core/src/governance/mod.rs` - Module declaration
2. `icn-core/src/governance/actor.rs` - GovernanceActor implementation

**Files to modify:**
1. `icn-core/src/lib.rs` - Add `pub mod governance;`
2. `icn-core/Cargo.toml` - Add `icn-governance.workspace = true` to dependencies
3. `icn-core/src/supervisor.rs` - Wire GovernanceActor into runtime

**Functionality:**
- [x] Actor spawn with gossip subscription
- [x] Notification callback for incoming messages
- [x] Store integration (get/put/scan)
- [x] Command handler for CreateDomain/CreateProposal/OpenProposal/CastVote/CloseProposal
- [x] Publish outgoing GovernanceMessages
- [x] Vote tallying and outcome evaluation

**Testing:**
- Reuse existing `governance_integration.rs` (no changes needed)
- Add simple in-process smoke test in supervisor

### Session 2: Time-Based Closing (Next)

**Implementation:**
- Add `BinaryHeap<(Instant, ProposalId)>` to actor
- Background `tokio::spawn` task that wakes every 10 seconds
- On `ProposalOpened`: enqueue `(closes_at, proposal_id)`
- On timer wake: pop expired proposals, auto-close, broadcast outcome
- Handle manual close: remove from heap

**Why deferred:**
- Core actor must work first
- Scheduler is independent feature
- Clean separation of concerns

### Session 3: RPC Layer (Next)

**Implementation:**
- Define `governance.proto` service
- Implement handlers that call `GovernanceHandle.submit()`
- Refactor `icnctl gov` to use gRPC instead of direct store

**Why deferred:**
- Protobuf definitions take time
- CLI refactor is breaking change
- Actor must be stable first

### Session 4: Trust Graph Integration (Future)

**Implementation:**
- Add `TrustGraphHandle` to supervisor
- Pass to `GovernanceActor::spawn()`
- Implement `TrustMembershipResolver`
- Enable `MembershipConfig::TrustThreshold`

**Why deferred:**
- Requires trust graph stabilization
- Not needed for static pilot communities
- Can add without breaking existing code

## Key Invariants

**Storage consistency:**
- Actor and CLI must use identical key patterns
- Serialization must be `serde_json` (human-readable, inspectable)
- Store writes are atomic per-key

**Gossip convergence:**
- All nodes subscribe to same topic (`governance:proposal`)
- Messages are idempotent (replaying `ProposalCreated` is safe)
- Last-write-wins for votes (voter can change their mind)

**State machine validity:**
- Proposals follow: Draft → Open → {Accepted, Rejected, NoQuorum, Cancelled}
- State transitions validated by `Proposal::open()` / `Proposal::close()`
- Invalid transitions return `Err` (logged, not crashed)

**Security (Future):**
- Verify voter is in membership list before counting vote
- Sign `GovernanceMessage` with voter's keypair
- Validate signatures before accepting messages

## Edge Cases

**Concurrent proposal closing:**
- Multiple nodes may close same proposal simultaneously
- Each computes tally independently
- Should converge on same outcome (deterministic evaluation)
- If divergence occurs: operator investigates, consensus established via governance itself

**Late-joining nodes:**
- New node subscribes to topic
- Receives all `ProposalCreated` messages via anti-entropy
- Gossip backfill ensures eventual consistency
- Open proposals can still be voted on

**Vote tampering:**
- Phase 1: No signature verification (trusted network)
- Phase 2: Add signature to `VoteCast` message
- Phase 3: Verify signer is in membership list

**Malformed messages:**
- Deserialization errors logged and dropped
- Does not crash daemon
- Metrics track governance message errors

## Success Criteria

**For Session 1:**
- [x] `cargo build` succeeds with no warnings
- [x] Existing `governance_integration.rs` test passes
- [x] Can create domain via `GovernanceHandle.submit()`
- [x] Domain propagates to gossip and remote nodes
- [x] Can create → open → vote → close proposal in-process
- [x] Store keys match CLI patterns exactly

**For full daemon integration (future sessions):**
- [ ] Time-based proposal closing works
- [ ] `icnctl gov` uses RPC instead of direct store
- [ ] Trust graph membership resolution enabled
- [ ] Signature verification for votes
- [ ] Metrics exposed (proposals created/closed, votes cast, etc.)

## Migration Path

**Phase 1 (This Session):** Parallel Operation
- CLI continues to write directly to store
- Daemon also writes to same store
- Both produce identical keys/values
- No conflicts (CLI is admin tool, daemon is runtime)

**Phase 2 (Next Session):** CLI Uses RPC
- `icnctl gov` switches to daemon RPC
- Store becomes daemon-only
- CLI becomes thin client

**Phase 3 (Future):** Deprecate Direct Store Access
- Remove store-direct code from `icnctl`
- Gateway/web UI use RPC exclusively
- Clean separation: daemon = storage, clients = presentation

## Open Questions

**Q: Should governance actor be optional (feature flag)?**
A: No. It's a core substrate, always present. Operators can choose not to use it, but overhead is minimal.

**Q: How to handle governance schema migrations?**
A: Version `GovernanceMessage` enum. Add new variants, keep old handlers. Gossip is schemaless (JSON/bincode).

**Q: Should we persist gossip entries (proposals/votes) or just metadata?**
A: Just metadata in store. Gossip entries are ephemeral, anti-entropy handles backfill.

**Q: How to expose governance to gateway/web UI?**
A: Via RPC (Session 3). Gateway calls same endpoints as `icnctl`.

## Related Work

**Depends on:**
- Phase 7: Gossip protocol (stable)
- Phase 8: DID-TLS binding (stable)
- Phase 11: Multi-device identity (future: governance signing)
- Phase 12: Economic safety (ledger integration for budget proposals)

**Enables:**
- Track C1: Pilot community deployment (governance for real decisions)
- Future: Contract-based governance profiles
- Future: Federated governance across multiple coops

## References

- [docs/governance.md](../governance.md) - Governance substrate design
- [crates/icn-governance/](../../crates/icn-governance/) - Core types and rules
- [crates/icn-core/tests/governance_integration.rs](../../crates/icn-core/tests/governance_integration.rs) - Multi-node validation
- [bins/icnctl/src/main.rs](../../bins/icnctl/src/main.rs) - CLI implementation (lines 2666-3097)

## Commit Plan

**Session 1 commits:**
1. `feat(governance): Add GovernanceActor to icn-core runtime`
   - Create `governance/mod.rs` and `governance/actor.rs`
   - Implement spawn, command handling, gossip integration
   - Full functionality: create/open/vote/close

2. `feat(governance): Wire GovernanceActor into supervisor`
   - Initialize in supervisor with store + gossip handles
   - Set up notification callback
   - Expose `GovernanceHandle` for internal use

3. `test(governance): Add in-process smoke test`
   - Minimal test calling `submit()` directly
   - Verify store writes match CLI patterns

**Future commits (next sessions):**
- Time-based closing scheduler
- RPC service definition and handlers
- CLI refactor to use RPC
- Trust graph membership resolver
- Signature verification for votes

---

**Next Steps After This Session:**
1. Validate daemon integration with existing integration test
2. Design time-based closing scheduler
3. Draft governance.proto for RPC layer
4. Plan trust graph integration points
