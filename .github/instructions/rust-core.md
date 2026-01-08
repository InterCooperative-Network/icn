---
applyTo: "icn/crates/**/*.rs"
---

# Rust Core Crates Instructions

These instructions apply to the Rust crates in `icn/crates/` directory.

## Rust Version and Edition

- **Rust Edition**: 2021
- **MSRV (Minimum Supported Rust Version)**: Check `rust-toolchain.toml`
- Use stable Rust features only (no nightly)

## Code Style and Conventions

### Formatting

- Run `cargo fmt` before committing
- 100 character line length (configured in `rustfmt.toml`)
- Use default rustfmt settings

### Linting

- Code must pass `cargo clippy -- -D warnings` (warnings as errors)
- Address all clippy suggestions or explicitly allow with justification
- Common allows: `#[allow(clippy::too_many_arguments)]` with comment explaining why

### Naming Conventions

- **Types**: `PascalCase` (e.g., `NetworkActor`, `GossipMessage`)
- **Functions/methods**: `snake_case` (e.g., `send_message`, `compute_trust`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_PEERS`, `DEFAULT_TIMEOUT`)
- **Modules**: `snake_case` (e.g., `gossip`, `ledger`)
- **Lifetimes**: Single letter or descriptive (e.g., `'a`, `'msg`)

## Async/Await Patterns

### Tokio Runtime

- All async code uses Tokio runtime
- **Never use blocking operations in async contexts**
- Use `tokio::spawn` for background tasks
- Use `tokio::task::spawn_blocking` for CPU-intensive work

Example:
```rust
// Good: Non-blocking async I/O
async fn read_data() -> Result<Vec<u8>> {
    tokio::fs::read("data.bin").await
}

// Bad: Blocking in async context
async fn bad_read_data() -> Result<Vec<u8>> {
    std::fs::read("data.bin") // DON'T DO THIS
}

// Good: CPU work in blocking task
async fn compute_hash(data: Vec<u8>) -> String {
    tokio::task::spawn_blocking(move || {
        expensive_hash_computation(&data)
    }).await.unwrap()
}
```

### Actor Pattern

Actors are the core abstraction. Follow this pattern:

```rust
// Actor internal message enum
enum ActorMsg {
    DoSomething { arg: T, reply: oneshot::Sender<Result<R>> },
    Stop,
}

// Actor struct (private)
struct Actor {
    state: ActorState,
    rx: mpsc::Receiver<ActorMsg>,
}

impl Actor {
    fn new(rx: mpsc::Receiver<ActorMsg>) -> Self {
        Self { state: ActorState::default(), rx }
    }

    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                ActorMsg::DoSomething { arg, reply } => {
                    let result = self.handle_do_something(arg).await;
                    let _ = reply.send(result);
                }
                ActorMsg::Stop => break,
            }
        }
    }
}

// Public handle
#[derive(Clone)]
pub struct ActorHandle {
    tx: mpsc::Sender<ActorMsg>,
}

impl ActorHandle {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(100);
        let actor = Actor::new(rx);
        tokio::spawn(actor.run());
        Self { tx }
    }

    pub async fn do_something(&self, arg: T) -> Result<R> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(ActorMsg::DoSomething { arg, reply: tx }).await?;
        rx.await?
    }
}
```

## Error Handling

### Error Types

- Use `thiserror` crate for error types
- Provide context with error variants
- Implement `std::error::Error` via `thiserror::Error`

Example:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Failed to connect to peer {did}: {source}")]
    ConnectionFailed {
        did: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Invalid message format")]
    InvalidMessage,
    #[error("Timeout after {0:?}")]
    Timeout(Duration),
}
```

### Error Propagation

- Use `?` operator for error propagation
- Add context with `.context()` or `.with_context()` from `anyhow`
- **Never panic in protocol/actor code** (tests are okay)
- Use `Result<T, E>` return types

## Testing

### Unit Tests

- Test module in same file: `#[cfg(test)] mod tests { ... }`
- Use descriptive test names: `test_actor_handles_invalid_message`
- Test error conditions, not just happy path

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_processes_message() {
        let handle = ActorHandle::spawn();
        let result = handle.do_something(42).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_actor_rejects_invalid_input() {
        let handle = ActorHandle::spawn();
        let result = handle.do_something(-1).await;
        assert!(matches!(result, Err(Error::InvalidInput)));
    }
}
```

### Integration Tests

- Located in `tests/` directory at crate root
- Test multi-component interactions
- Use `TestNode` helper from `icn-testkit`
- Each test gets unique ports to avoid conflicts

```rust
#[tokio::test]
async fn test_two_node_gossip_convergence() {
    let node1 = TestNode::spawn(8001).await;
    let node2 = TestNode::spawn(8002).await;
    
    node1.dial(&node2).await.unwrap();
    
    node1.publish("topic", "data").await.unwrap();
    
    // Verify convergence with timeout
    tokio::time::timeout(
        Duration::from_secs(5),
        node2.wait_for_entry("topic")
    ).await.unwrap();
}
```

## Concurrency and Synchronization

### Preferred Patterns

- **Message passing** (mpsc channels) over shared state
- **Arc<RwLock<T>>** for shared state (rarely needed with actors)
- **tokio::sync primitives** (not std::sync)

### Anti-patterns to Avoid

- ❌ Blocking mutexes in async code
- ❌ Shared mutable state without synchronization
- ❌ Nested locks (deadlock risk)
- ❌ Long-held locks across await points

## Serialization

- Use `serde` with `bincode` for internal protocol messages
- Use `serde_json` for human-readable config/API
- Derive `Serialize` and `Deserialize` for data types
- Use `#[serde(rename_all = "camelCase")]` for JSON APIs

## Documentation

### Public API

- All public items must have doc comments
- Include examples in doc comments
- Document errors with `# Errors` section
- Document panics with `# Panics` section (though avoid panics)

```rust
/// Sends a message to the specified peer.
///
/// # Arguments
///
/// * `recipient` - DID of the recipient peer
/// * `payload` - Message payload to send
///
/// # Errors
///
/// Returns `NetworkError::ConnectionFailed` if connection cannot be established.
/// Returns `NetworkError::Timeout` if send times out.
///
/// # Examples
///
/// ```
/// # use icn_net::{NetworkActor, MessagePayload};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let actor = NetworkActor::spawn();
/// actor.send_message("did:icn:abc123", payload).await?;
/// # Ok(())
/// # }
/// ```
pub async fn send_message(
    &self,
    recipient: &str,
    payload: MessagePayload,
) -> Result<(), NetworkError> {
    // Implementation
}
```

## Performance Considerations

- Profile before optimizing
- Use `#[inline]` judiciously (hot paths only)
- Consider `SmallVec` for small, stack-allocated vectors
- Use `Arc::clone()` not `.clone()` for clarity
- Benchmark with `cargo bench` for critical paths

## Security Best Practices

- **Validate all inputs** from network/external sources
- **Use constant-time comparisons** for secrets/signatures
- **Sanitize error messages** (no sensitive data in errors)
- **Rate limiting** on all external inputs
- **Bounds checking** on all buffer operations

## Metrics and Observability

- Add Prometheus metrics for key operations
- Use structured logging with `tracing` crate
- Log at appropriate levels:
  - `error!`: Unrecoverable errors
  - `warn!`: Recoverable errors, degraded operation
  - `info!`: Important state changes
  - `debug!`: Detailed diagnostic info
  - `trace!`: Very verbose debugging

```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(self), fields(recipient = %recipient))]
async fn send_message(&self, recipient: &str) -> Result<()> {
    info!("Sending message to {}", recipient);
    
    match self.do_send(recipient).await {
        Ok(_) => {
            MESSAGES_SENT.inc();
            Ok(())
        }
        Err(e) => {
            warn!("Failed to send message: {}", e);
            MESSAGES_FAILED.inc();
            Err(e)
        }
    }
}
```

## Common Patterns in ICN Codebase

### Gossip Messages

- All gossip messages are content-addressed (hashed)
- Vector clocks track causality
- Bloom filters for efficient anti-entropy

### Ledger Operations

- Double-entry bookkeeping
- All entries are immutable
- Merkle-DAG structure for verification

### Trust Computation

- Scores between 0.0 and 1.0
- Transitive trust via weighted paths
- Used for rate limiting and access control

## Dependencies

- Keep dependencies minimal
- Prefer well-maintained crates
- Pin major versions in Cargo.toml
- Run `cargo audit` regularly
- Document why each dependency is needed

## Important Notes

- All commands run from `icn/` directory (not repo root)
- Integration tests need unique ports per node
- Shutdown propagates via broadcast channel
- DID format: `did:icn:<base58-pubkey>`
