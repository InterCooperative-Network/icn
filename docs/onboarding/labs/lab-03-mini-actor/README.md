# Lab 03: Mini Actor Runtime

## Goal
Build a mini actor with bounded mailbox, sequential message processing, and graceful shutdown.

## Structure
```
lab-03-mini-actor/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── actor.rs        # Actor struct and message enum
    └── handle.rs       # Handle for external API
```

## Requirements

### 1. Message Enum
```rust
enum ActorMsg {
    Echo { text: String, reply: oneshot::Sender<String> },
    GetState { reply: oneshot::Sender<State> },
    Shutdown,
}
```

### 2. Actor Implementation
- Bounded mailbox: `mpsc::channel(32)`
- Sequential processing loop
- State mutations via messages only

### 3. Handle API
```rust
pub struct ActorHandle {
    tx: mpsc::Sender<ActorMsg>,
}

impl ActorHandle {
    pub async fn echo(&self, text: String) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(ActorMsg::Echo { text, reply: tx }).await?;
        Ok(rx.await?)
    }
}
```

### 4. Shutdown Coordination
- Use `broadcast::channel` for shutdown signal
- Actor uses `tokio::select!` to wait for messages or shutdown
- Graceful cleanup on shutdown

### 5. Timeouts and Retries
Add operation with configurable timeout:
```rust
pub async fn echo_with_timeout(
    &self,
    text: String,
    timeout: Duration,
) -> Result<String> {
    tokio::time::timeout(timeout, self.echo(text)).await?
}
```

## Done When
- Actor processes messages sequentially (tests prove ordering)
- Shutdown signal causes graceful exit
- Tests verify timeout behavior
- No panics, all errors are `Result`

## Tests to Write
1. `test_sequential_processing` — messages processed in order
2. `test_shutdown_graceful` — actor stops on signal
3. `test_timeout` — operations respect timeout
4. `test_backpressure` — bounded mailbox applies backpressure

## Resources
- `icn/crates/icn-core/src/runtime.rs` (shutdown pattern)
- `icn-gossip/src/gossip.rs` (actor pattern)
- [Tokio tutorial](https://tokio.rs/tokio/tutorial)
