# ICN Common Patterns Reference

This guide documents recurring patterns used throughout the ICN codebase. Use
it as a quick reference when reading or writing code.

## Actor Pattern

ICN uses an actor model for concurrent subsystems. Each actor encapsulates
state and exposes a handle for interaction.

### Structure
```rust
// The actor struct holds state
pub struct MyActor {
    state: InternalState,
    // ...
}

// The handle provides concurrent access
pub struct MyActorHandle {
    inner: Arc<RwLock<MyActor>>,
}

impl MyActorHandle {
    pub async fn do_something(&self) -> Result<()> {
        let actor = self.inner.write().await;
        actor.internal_operation()
    }
}
```

### Where to find examples
- `icn-gossip/src/gossip.rs`: GossipActor
- `icn-net/src/actor.rs`: NetworkActor
- `icn-ledger/src/ledger.rs`: Ledger

## Error Handling with anyhow/thiserror

### Domain errors with thiserror
```rust
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid state: {reason}")]
    InvalidState { reason: String },
}
```

### Propagation with anyhow
```rust
use anyhow::{Context, Result};

fn load_config(path: &Path) -> Result<Config> {
    let data = std::fs::read_to_string(path)
        .context("Failed to read config file")?;
    let config: Config = toml::from_str(&data)
        .context("Failed to parse config")?;
    Ok(config)
}
```

### Where to find examples
- `icn-federation/src/error.rs`: Domain errors
- `icn-core/src/runtime.rs`: Error propagation

## Async Callbacks for Inter-Actor Communication

Actors communicate via callbacks instead of direct dependencies.

### Pattern
```rust
pub type SendCallback = Arc<dyn Fn(&str, Vec<u8>) -> Result<()> + Send + Sync>;

impl MyActor {
    pub fn set_send_callback(&mut self, callback: SendCallback) {
        self.send_callback = Some(callback);
    }

    fn send_message(&self, topic: &str, data: Vec<u8>) -> Result<()> {
        if let Some(ref cb) = self.send_callback {
            cb(topic, data)?;
        }
        Ok(())
    }
}
```

### Where to find examples
- `icn-gossip/src/gossip.rs`: Send callbacks
- `icn-federation/src/gossip.rs`: GossipSendCallback

## Store Abstraction for Persistence

Storage is abstracted behind traits, allowing different backends.

### Pattern
```rust
pub trait Store: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
}

// Concrete implementation
pub struct SledStore {
    db: sled::Db,
}
```

### Namespaced storage
```rust
const KEY_PREFIX: &[u8] = b"agreement:";

fn make_key(id: &str) -> Vec<u8> {
    let mut key = KEY_PREFIX.to_vec();
    key.extend(id.as_bytes());
    key
}
```

### Where to find examples
- `icn-store/src/lib.rs`: Store trait
- `icn-federation/src/agreement/store.rs`: Namespaced storage

## Builder Pattern for Complex Configuration

### Pattern
```rust
pub struct ConfigBuilder {
    timeout: Option<Duration>,
    max_retries: Option<u32>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self { timeout: None, max_retries: None }
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    pub fn build(self) -> Config {
        Config {
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
            max_retries: self.max_retries.unwrap_or(3),
        }
    }
}
```

### Where to find examples
- `icn-obs/src/otel.rs`: TracingConfig
- `icn-core/src/config/`: Configuration builders

## Lifecycle State Machines

Many components have explicit lifecycle states.

### Pattern
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Draft,
    Pending,
    Active,
    Terminated,
}

impl MyEntity {
    pub fn transition_to(&mut self, new_status: Status) -> Result<()> {
        match (&self.status, &new_status) {
            (Status::Draft, Status::Pending) => {}
            (Status::Pending, Status::Active) => {}
            (Status::Active, Status::Terminated) => {}
            _ => return Err(Error::InvalidTransition),
        }
        self.status = new_status;
        Ok(())
    }
}
```

### Where to find examples
- `icn-federation/src/agreement/types.rs`: AgreementStatus
- `icn-governance/src/proposal.rs`: ProposalStatus

## Metrics Integration

Metrics follow a consistent naming and registration pattern.

### Pattern
```rust
use metrics::{counter, gauge, histogram};

pub fn init_descriptions() {
    metrics::describe_counter!(
        "icn_my_counter_total",
        "Description of what this counter measures"
    );
}

pub fn my_counter_inc() {
    counter!("icn_my_counter_total").increment(1);
}

pub fn my_gauge_set(value: f64) {
    gauge!("icn_my_gauge").set(value);
}

pub fn my_histogram_observe(value: f64) {
    histogram!("icn_my_duration_seconds").record(value);
}
```

### Naming convention
- `icn_{subsystem}_{metric}_{unit}`
- Counters end with `_total`
- Histograms for durations end with `_seconds`

### Where to find examples
- `icn-obs/src/metrics/`: All metric modules
- `icn-federation/src/metrics.rs`: Federation metrics

## Trust-Gated Operations

Operations may be gated based on trust scores.

### Pattern
```rust
pub enum TrustClass {
    Isolated,   // < 0.1
    Known,      // 0.1 - 0.4
    Partner,    // 0.4 - 0.7
    Federated,  // > 0.7
}

impl TrustClass {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s < 0.1 => Self::Isolated,
            s if s < 0.4 => Self::Known,
            s if s < 0.7 => Self::Partner,
            _ => Self::Federated,
        }
    }

    pub fn rate_limit(&self) -> u32 {
        match self {
            Self::Isolated => 10,
            Self::Known => 50,
            Self::Partner => 100,
            Self::Federated => 200,
        }
    }
}
```

### Where to find examples
- `icn-security/src/rate_limit.rs`: Trust-based rate limiting
- `icn-gossip/src/access.rs`: Topic access control

## Shutdown Coordination

Graceful shutdown uses broadcast channels.

### Pattern
```rust
use tokio::sync::broadcast;

let (shutdown_tx, _) = broadcast::channel::<()>(1);

// In each actor
let mut shutdown_rx = shutdown_tx.subscribe();
tokio::select! {
    _ = normal_work() => {}
    _ = shutdown_rx.recv() => {
        info!("Received shutdown signal");
        break;
    }
}

// To trigger shutdown
drop(shutdown_tx);
// Or explicit send
shutdown_tx.send(())?;
```

### Where to find examples
- `icn-core/src/runtime.rs`: Shutdown channel setup
- `icn-core/src/supervisor/init_federation.rs`: Shutdown receiver in tasks

## Serialization with Postcard

Binary serialization uses postcard for compact encoding.

### Pattern
```rust
use serde::{Serialize, Deserialize};
use postcard;

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub payload: Vec<u8>,
}

// Serialize
let bytes = postcard::to_allocvec(&msg)?;

// Deserialize
let msg: Message = postcard::from_bytes(&bytes)?;
```

### Where to find examples
- `icn-gossip/src/message.rs`: Gossip message encoding
- `icn-federation/src/agreement/types.rs`: Agreement serialization
