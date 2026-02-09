# 04: Actors and Concurrency

> **Phase**: 2 | **Tier**: Fixer  
> **Patterns introduced**: [Actor](#actor-pattern), [Async Callbacks](#async-callbacks), [Shutdown Coordination](#shutdown-coordination), [Handle Holder](#handle-holder)  
> **Prerequisite**: Phase 1 complete (Reader tier)

## Why This Matters

ICN's central concurrency model is the **actor pattern** — each subsystem (Network, Gossip, Ledger, Governance, Compute) runs independently, processing messages sequentially from a bounded mailbox. This eliminates data races, simplifies reasoning, and enables graceful shutdown.

Understanding actors is prerequisite to understanding the Meaning Firewall (Layer 05) — the boundary between kernel and apps is enforced through actor communication, not direct function calls.

→ See `manual.md` § "Concurrency via Actors" for the design rationale.

## What You'll Read

### 1. The Actor Template

**File**: `docs/architecture/ARCHITECTURE_MAP.md` (search for "Actor Pattern")

Every ICN actor follows this template:

```rust
// 1. Message enum
enum ActorMsg {
    DoSomething { arg: T, reply: oneshot::Sender<Result<R>> },
    Shutdown,
}

// 2. Actor state struct
struct Actor {
    state: InternalState,
}

// 3. Handle for external communication
pub struct ActorHandle {
    tx: mpsc::Sender<ActorMsg>,
}

// 4. Spawn function
pub fn spawn_actor() -> ActorHandle {
    let (tx, mut rx) = mpsc::channel(32);  // Bounded mailbox
    
    tokio::spawn(async move {
        let mut actor = Actor::new();
        while let Some(msg) = rx.recv().await {
            match msg {
                ActorMsg::DoSomething { arg, reply } => {
                    let result = actor.do_something(arg);
                    let _ = reply.send(result);
                }
                ActorMsg::Shutdown => break,
            }
        }
    });
    
    ActorHandle { tx }
}
```

**Key properties**:
- **Sequential processing**: Messages processed one at a time (no internal locks needed)
- **Bounded mailbox**: Backpressure via `mpsc::channel(N)` prevents unbounded memory growth
- **Async API**: Handle provides `async fn` methods that send messages and await replies

### 2. Real Actors: Simplest to Most Complex

Trace these actors in order of complexity:

#### **Level 1: CoopActor** (`icn-coop/src/actor.rs`)
- Manages cooperative membership and lifecycle
- State: `HashMap<CoopId, Coop>`
- Messages: `CreateCoop`, `AddMember`, `UpdateStatus`
- No external callbacks, pure state management

#### **Level 2: ContractRegistryActor** (`icn-ccl/src/registry.rs`)
- Stores and retrieves CCL contracts
- State: `HashMap<ContractId, Contract>`
- Messages: `Register`, `Get`, `List`
- Simple pattern: request/response with no side effects

#### **Level 3: CommunityActor** (`icn-community/src/actor.rs`)
- Manages communities (groupings of coops/individuals)
- State: `HashMap<CommunityId, Community>`
- Messages: `Create`, `AddMember`, `ListMembers`
- Callback: Gossip send for replication

#### **Level 4: StewardActor** (`icn-steward/src/actor.rs`)
- SDIS steward network for VUI computation
- State: `HashMap<Did, StewardInfo>`, threshold crypto state
- Messages: `EnrollMember`, `ComputeVui`, `IssueToken`
- Complex: cryptographic operations, multi-party coordination

#### **Level 5: ComputeActor** (`icn-compute/src/actor.rs`)
- Distributed task execution with trust gating
- State: `HashMap<TaskId, Task>`, execution queue
- Messages: `Submit`, `Cancel`, `GetResult`
- Callbacks: PolicyOracle for trust checks, Network for distribution
- **Most complex**: scheduling, resource management, result aggregation

#### **Level 6: NetworkActor** (`icn-net/src/actor.rs`)
- QUIC transport, mDNS discovery, session management
- State: `HashMap<Did, Connection>`, peer discovery cache
- Messages: `Dial`, `Send`, `Disconnect`, `IncomingConnection`
- Callbacks: Incoming message handlers (routes to Gossip/RPC)
- **Deepest**: handles raw network I/O, certificate validation, retry logic

### 3. Four Communication Patterns

#### **Pattern A: Channel-Based (mpsc::Sender<Msg>)**
Used for: Actor-to-actor messaging within the same process.

```rust
let (tx, rx) = mpsc::channel(32);
// Send a message
tx.send(Msg::DoSomething { arg }).await?;
```

#### **Pattern B: Shared State (Arc<RwLock<T>>)**
Used for: Read-heavy state shared across actors.

```rust
let state = Arc::new(RwLock::new(SharedState::new()));
// Read
let data = state.read().await;
// Write
let mut data = state.write().await;
```

**Warning**: Don't hold locks across `.await` points — causes deadlocks.

#### **Pattern C: Callbacks**
Used for: Decoupling actors (e.g., Gossip → Network).

```rust
pub type SendCallback = Arc<dyn Fn(Did, Vec<u8>) -> Result<()> + Send + Sync>;

// Set callback
gossip_actor.set_send_callback(send_callback);

// Invoke callback
if let Some(ref cb) = self.send_callback {
    cb(recipient, data)?;
}
```

#### **Pattern D: Broadcast (Shutdown)**
Used for: Shutdown signal propagation.

```rust
let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

// In runtime.rs
shutdown_tx.send(()).ok();

// In each actor
tokio::select! {
    msg = rx.recv() => { /* process */ }
    _ = shutdown_rx.recv() => { break; }
}
```

### 4. Shutdown Coordination

**File**: `icn/crates/icn-core/src/runtime.rs`

The runtime waits for shutdown signal or error:

```rust
tokio::select! {
    result = supervisor.run() => {
        // Supervisor exited (error or completed)
    }
    _ = tokio::signal::ctrl_c() => {
        // User pressed Ctrl+C
    }
}

// Broadcast shutdown signal
shutdown_tx.send(()).ok();

// Wait for actors to finish
tokio::time::sleep(Duration::from_secs(2)).await;
```

Each actor receives the signal via `broadcast::Receiver` and exits gracefully:

```rust
loop {
    tokio::select! {
        msg = rx.recv() => {
            // Process message
        }
        _ = shutdown_rx.recv() => {
            tracing::info!("Shutdown signal received");
            break;
        }
    }
}
// Cleanup: flush buffers, close connections, etc.
```

### 5. The `parking_lot::RwLock` vs `tokio::sync::RwLock` Distinction

ICN has **two** `TrustPolicyOracle` implementations:

**`apps/trust/src/oracle.rs`** (uses `parking_lot::RwLock`):
```rust
pub struct TrustPolicyOracle {
    trust_graph: Arc<parking_lot::RwLock<TrustGraph>>,
}
```

**`apps/trust/src/oracle_tokio.rs`** (uses `tokio::sync::RwLock`):
```rust
pub struct TrustPolicyOracleTokio {
    trust_graph: Arc<tokio::sync::RwLock<TrustGraph>>,
}
```

**Why two versions?**

| Lock Type | When to Use | Trade-offs |
|-----------|-------------|------------|
| `parking_lot::RwLock` | Synchronous code, short critical sections | Faster, no async overhead, but blocks threads |
| `tokio::sync::RwLock` | Async code, long critical sections | Async-aware, but slower, more overhead |

**ICN's choice**: The trust oracle does **fast in-memory graph queries** (no I/O), so `parking_lot::RwLock` is appropriate. If the oracle needed to call async functions inside the lock, `tokio::sync::RwLock` would be required.

**Rule of thumb**: Use `parking_lot` for pure CPU-bound operations, `tokio::sync` if you need `.await` inside the lock.

## Patterns Introduced

### Actor Pattern
→ See `patterns.md` #1 for full template.

### Async Callbacks
→ See `patterns.md` #3 for full template.

### Shutdown Coordination
→ See `patterns.md` #9 for full template.

### Handle Holder
**Pattern**: `Arc<RwLock<Option<Handle>>>` for late-bound actors.

**Why**: During supervisor initialization, some actors depend on handles from others. Use `Option<Handle>` to allow phased initialization.

```rust
let gossip_handle_holder = Arc::new(RwLock::new(None));

// Spawn actor
let handle = spawn_gossip_actor();
*gossip_handle_holder.write().await = Some(handle);

// Later use
if let Some(ref handle) = *gossip_handle_holder.read().await {
    handle.announce(topic, data).await?;
}
```

→ See `patterns.md` #15 for full template.

## What You'll Build

→ **Lab**: `labs/lab-03-mini-actor/`

Build a mini actor runtime:
- Actor with bounded mailbox (`mpsc::channel`)
- Sequential message processing
- `tokio::select!` shutdown coordination via `broadcast::channel`
- Timeouts + retries to simulate network behavior

**Done when**: Actor processes messages, shuts down gracefully on signal, tests prove ordering.

## Checkpoint

You've completed this layer when you can:

1. **Identify actor boundaries**: Given ICN code, identify which subsystems are actors and which are pure functions
2. **Trace message flow**: Draw a sequence diagram showing a message from Gateway → GossipActor → NetworkActor
3. **Explain lock choices**: Justify when to use `parking_lot::RwLock` vs `tokio::sync::RwLock`
4. **Implement shutdown**: Add graceful shutdown to your lab actor

**Artifact**: Diagram showing message flow for a gossip announcement (Gateway → Gossip → Network → Peer).

## Deep Reference

→ `reference/module-03-runtime-actors.md` — Full actor lifecycle, supervisor details  
→ [Tokio tutorial](https://tokio.rs/tokio/tutorial) — Async Rust fundamentals  
→ [The Actor Model](https://www.brianstorti.com/the-actor-model/) — Conceptual overview
