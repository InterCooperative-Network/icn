//! Compute Dispatcher
//!
//! Routes events to reducers and requests to services.
//!
//! # Determinism Boundary
//!
//! - **Reducers**: Pure, deterministic, synchronous. No async runtime access.
//!   Receive `&State`, return new `State`. Cannot call external systems.
//!
//! - **Services**: May have side effects. Can be async. Cannot mutate state
//!   directly. Must emit events that reducers consume.
//!
//! # Event Flow
//!
//! ```text
//! Request → Service → Event → Dispatcher → Reducer → State
//! ```
//!
//! # Performance Considerations
//!
//! `StateSnapshot::from_app_state()` clones the entire KV store for each
//! reducer invocation. This is acceptable for small state (<1000 keys).
//! For apps with large state, consider:
//!
//! - Keeping state small (use BlobService for large values)
//! - Batching multiple events before taking a snapshot
//! - Future optimization: Copy-on-write snapshots (not yet implemented)

use super::state_factory::AppState;
use icn_obs::metrics::apps as app_metrics;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch, RwLock};

/// Event type identifier (e.g., "trust:attestation").
pub type EventType = String;

/// Request type identifier (e.g., "trust:query").
pub type RequestType = String;

/// An event to be processed by a reducer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Event type (routes to reducer)
    pub event_type: EventType,
    /// Event payload (serialized)
    pub payload: Vec<u8>,
    /// Source (who emitted this event)
    pub source: String,
    /// Timestamp (milliseconds since epoch)
    pub timestamp: u64,
}

impl Event {
    /// Create a new event.
    pub fn new(event_type: impl Into<String>, payload: Vec<u8>, source: impl Into<String>) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
            source: source.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }

    /// Create an event with a typed payload.
    pub fn with_payload<T: Serialize>(
        event_type: impl Into<String>,
        payload: &T,
        source: impl Into<String>,
    ) -> Result<Self, DispatchError> {
        let bytes =
            serde_json::to_vec(payload).map_err(|e| DispatchError::Serialization(e.to_string()))?;
        Ok(Self::new(event_type, bytes, source))
    }

    /// Parse the payload as a typed value.
    pub fn parse_payload<T: DeserializeOwned>(&self) -> Result<T, DispatchError> {
        serde_json::from_slice(&self.payload)
            .map_err(|e| DispatchError::Deserialization(e.to_string()))
    }
}

/// A request to be handled by a service.
#[derive(Clone, Debug)]
pub struct Request {
    /// Request type (routes to service)
    pub request_type: RequestType,
    /// Request payload (serialized)
    pub payload: Vec<u8>,
    /// Requestor identity
    pub from: String,
    /// Request ID for correlation
    pub id: String,
}

impl Request {
    /// Create a new request.
    pub fn new(request_type: impl Into<String>, payload: Vec<u8>, from: impl Into<String>) -> Self {
        Self {
            request_type: request_type.into(),
            payload,
            from: from.into(),
            id: uuid_v4(),
        }
    }

    /// Create a request with a typed payload.
    pub fn with_payload<T: Serialize>(
        request_type: impl Into<String>,
        payload: &T,
        from: impl Into<String>,
    ) -> Result<Self, DispatchError> {
        let bytes =
            serde_json::to_vec(payload).map_err(|e| DispatchError::Serialization(e.to_string()))?;
        Ok(Self::new(request_type, bytes, from))
    }

    /// Parse the payload as a typed value.
    pub fn parse_payload<T: DeserializeOwned>(&self) -> Result<T, DispatchError> {
        serde_json::from_slice(&self.payload)
            .map_err(|e| DispatchError::Deserialization(e.to_string()))
    }
}

/// Response from a service.
#[derive(Clone, Debug)]
pub struct Response {
    /// Request ID this is responding to
    pub request_id: String,
    /// Response payload (serialized)
    pub payload: Vec<u8>,
    /// Whether the request succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl Response {
    /// Create a successful response.
    pub fn success(request_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            request_id: request_id.into(),
            payload,
            success: true,
            error: None,
        }
    }

    /// Create a successful response with typed payload.
    pub fn success_with<T: Serialize>(
        request_id: impl Into<String>,
        payload: &T,
    ) -> Result<Self, DispatchError> {
        let bytes =
            serde_json::to_vec(payload).map_err(|e| DispatchError::Serialization(e.to_string()))?;
        Ok(Self::success(request_id, bytes))
    }

    /// Create an error response.
    pub fn error(request_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            payload: vec![],
            success: false,
            error: Some(error.into()),
        }
    }

    /// Parse the payload as a typed value.
    pub fn parse_payload<T: DeserializeOwned>(&self) -> Result<T, DispatchError> {
        serde_json::from_slice(&self.payload)
            .map_err(|e| DispatchError::Deserialization(e.to_string()))
    }
}

/// Read-only state snapshot for reducers.
///
/// Reducers receive this immutable view and return a new state.
// TODO(#873): Implement copy-on-write for StateSnapshot to avoid O(n) clone per reducer.
// Currently acceptable for Phase 1, but should use Arc<HashMap> or persistent data
// structures (e.g., `im` crate) for production workloads with large state.
#[derive(Clone)]
pub struct StateSnapshot {
    /// KV data (cloned for isolation)
    kv_data: HashMap<String, HashMap<String, Vec<u8>>>,
}

impl StateSnapshot {
    /// Create a snapshot from app state.
    pub async fn from_app_state(state: &AppState) -> Self {
        let mut kv_data = HashMap::new();

        for (name, handle) in &state.kv {
            let keys = handle.keys().await.unwrap_or_default();
            let mut store_data = HashMap::new();
            for key in keys {
                if let Ok(Some(value)) = handle.get(&key).await {
                    store_data.insert(key, value);
                }
            }
            kv_data.insert(name.clone(), store_data);
        }

        Self { kv_data }
    }

    /// Get a value from a KV store.
    pub fn kv_get(&self, store: &str, key: &str) -> Option<&Vec<u8>> {
        self.kv_data.get(store).and_then(|s| s.get(key))
    }

    /// List keys in a KV store.
    pub fn kv_keys(&self, store: &str) -> Vec<&String> {
        self.kv_data
            .get(store)
            .map(|s| s.keys().collect())
            .unwrap_or_default()
    }

    /// Get total number of keys across all KV stores.
    ///
    /// Used for metrics to track snapshot size.
    pub fn total_key_count(&self) -> usize {
        self.kv_data.values().map(|store| store.len()).sum()
    }
}

/// Mutable state delta produced by a reducer.
#[derive(Clone, Default)]
pub struct StateDelta {
    /// KV operations
    pub kv_ops: Vec<KvOp>,
    /// Log operations
    pub log_ops: Vec<LogOp>,
}

impl StateDelta {
    /// Create an empty delta.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a KV value.
    pub fn kv_set(&mut self, store: impl Into<String>, key: impl Into<String>, value: Vec<u8>) {
        self.kv_ops.push(KvOp::Set {
            store: store.into(),
            key: key.into(),
            value,
        });
    }

    /// Delete a KV value.
    pub fn kv_delete(&mut self, store: impl Into<String>, key: impl Into<String>) {
        self.kv_ops.push(KvOp::Delete {
            store: store.into(),
            key: key.into(),
        });
    }

    /// Append to a log.
    pub fn log_append(&mut self, log: impl Into<String>, data: Vec<u8>) {
        self.log_ops.push(LogOp::Append {
            log: log.into(),
            data,
        });
    }
}

/// KV operation in a delta.
#[derive(Clone, Debug)]
pub enum KvOp {
    Set {
        store: String,
        key: String,
        value: Vec<u8>,
    },
    Delete {
        store: String,
        key: String,
    },
}

/// Log operation in a delta.
#[derive(Clone, Debug)]
pub enum LogOp {
    Append { log: String, data: Vec<u8> },
}

/// Reducer trait - pure, deterministic event handlers.
///
/// Reducers run synchronously with NO async runtime access.
/// They receive an immutable state snapshot and return a delta.
pub trait Reducer: Send + Sync {
    /// Process an event and produce state changes.
    ///
    /// MUST be deterministic: same (state, event) → same delta.
    fn reduce(&self, state: &StateSnapshot, event: &Event) -> Result<StateDelta, DispatchError>;
}

/// Type-erased reducer for registry.
pub type BoxedReducer = Box<dyn Reducer>;

/// Service trait - async request handlers.
///
/// Services can call external systems but must not mutate state directly.
/// They return a response and optionally emit events for reducers.
#[async_trait::async_trait]
pub trait Service: Send + Sync {
    /// Handle a request.
    ///
    /// May call external systems. Should emit events via event_tx for state changes.
    async fn handle(
        &self,
        request: Request,
        state: &StateSnapshot,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<Response, DispatchError>;
}

/// Type-erased service for registry.
pub type BoxedService = Box<dyn Service>;

/// A tracked event sender that increments/decrements a counter for queue depth metrics.
#[derive(Clone)]
pub struct TrackedEventSender {
    inner: mpsc::Sender<Event>,
    pending: Arc<AtomicUsize>,
}

impl TrackedEventSender {
    /// Send an event and increment the pending counter.
    pub async fn send(&self, event: Event) -> Result<(), mpsc::error::SendError<Event>> {
        self.pending.fetch_add(1, Ordering::Relaxed);
        match self.inner.send(event).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Decrement on send failure
                self.pending.fetch_sub(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

/// Compute dispatcher - routes events and requests.
pub struct ComputeDispatcher {
    /// App identifier for metrics
    app_id: String,
    /// Registered reducers by event type
    reducers: HashMap<EventType, BoxedReducer>,
    /// Registered services by request type
    services: HashMap<RequestType, BoxedService>,
    /// Event channel for services to emit events
    event_tx: mpsc::Sender<Event>,
    /// Event receiver for dispatcher loop. Option so it can be taken for the run loop.
    event_rx: Option<mpsc::Receiver<Event>>,
    /// App state reference
    state: AppState,
    /// Running flag
    running: Arc<RwLock<bool>>,
    /// Shutdown signal sender
    shutdown_tx: watch::Sender<bool>,
    /// Shutdown signal receiver
    shutdown_rx: watch::Receiver<bool>,
    /// Pending event counter for queue depth metrics
    pending_events: Arc<AtomicUsize>,
}

impl ComputeDispatcher {
    /// Create a new dispatcher.
    pub fn new(app_id: impl Into<String>, state: AppState) -> Self {
        let (event_tx, event_rx) = mpsc::channel(1000);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_id: app_id.into(),
            reducers: HashMap::new(),
            services: HashMap::new(),
            event_tx,
            event_rx: Some(event_rx),
            state,
            running: Arc::new(RwLock::new(false)),
            shutdown_tx,
            shutdown_rx,
            pending_events: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Register a reducer for an event type.
    pub fn register_reducer(&mut self, event_type: impl Into<String>, reducer: BoxedReducer) {
        self.reducers.insert(event_type.into(), reducer);
    }

    /// Register a service for a request type.
    pub fn register_service(&mut self, request_type: impl Into<String>, service: BoxedService) {
        self.services.insert(request_type.into(), service);
    }

    /// Get an event sender for emitting events.
    ///
    /// The returned sender tracks pending events for queue depth metrics.
    pub fn event_sender(&self) -> TrackedEventSender {
        TrackedEventSender {
            inner: self.event_tx.clone(),
            pending: Arc::clone(&self.pending_events),
        }
    }

    /// Get the current pending event count.
    pub fn pending_event_count(&self) -> usize {
        self.pending_events.load(Ordering::Relaxed)
    }

    /// Dispatch an event directly (for testing or internal use).
    pub async fn dispatch_event(&self, event: Event) -> Result<(), DispatchError> {
        let reducer = self
            .reducers
            .get(&event.event_type)
            .ok_or_else(|| DispatchError::NoHandler(event.event_type.clone()))?;

        // Create snapshot with metrics
        let snapshot_start = Instant::now();
        let snapshot = StateSnapshot::from_app_state(&self.state).await;
        let key_count = snapshot.total_key_count();
        app_metrics::snapshot_duration_observe(
            &self.app_id,
            snapshot_start.elapsed().as_secs_f64(),
        );
        app_metrics::snapshot_keys_count_observe(&self.app_id, key_count as u64);

        let delta = reducer.reduce(&snapshot, &event)?;

        // Apply delta to state
        self.apply_delta(delta).await?;

        Ok(())
    }

    /// Handle a request.
    pub async fn handle_request(&self, request: Request) -> Result<Response, DispatchError> {
        let service = self
            .services
            .get(&request.request_type)
            .ok_or_else(|| DispatchError::NoHandler(request.request_type.clone()))?;

        // Create snapshot with metrics
        let snapshot_start = Instant::now();
        let snapshot = StateSnapshot::from_app_state(&self.state).await;
        let key_count = snapshot.total_key_count();
        app_metrics::snapshot_duration_observe(
            &self.app_id,
            snapshot_start.elapsed().as_secs_f64(),
        );
        app_metrics::snapshot_keys_count_observe(&self.app_id, key_count as u64);

        service
            .handle(request, &snapshot, self.event_tx.clone())
            .await
    }

    /// Apply a state delta.
    async fn apply_delta(&self, delta: StateDelta) -> Result<(), DispatchError> {
        // Apply KV operations
        for op in delta.kv_ops {
            match op {
                KvOp::Set { store, key, value } => {
                    let handle = self
                        .state
                        .kv(&store)
                        .ok_or_else(|| DispatchError::StoreNotFound(store.clone()))?;
                    handle
                        .set(&key, value)
                        .await
                        .map_err(|e| DispatchError::State(e.to_string()))?;
                }
                KvOp::Delete { store, key } => {
                    let handle = self
                        .state
                        .kv(&store)
                        .ok_or_else(|| DispatchError::StoreNotFound(store.clone()))?;
                    handle
                        .delete(&key)
                        .await
                        .map_err(|e| DispatchError::State(e.to_string()))?;
                }
            }
        }

        // Apply log operations
        for op in delta.log_ops {
            match op {
                LogOp::Append { log, data } => {
                    let handle = self
                        .state
                        .log(&log)
                        .ok_or_else(|| DispatchError::StoreNotFound(log.clone()))?;
                    handle
                        .append(data)
                        .await
                        .map_err(|e| DispatchError::State(e.to_string()))?;
                }
            }
        }

        Ok(())
    }

    /// Start the event loop.
    ///
    /// Continuously processes events from the channel and routes to reducers.
    /// This method can only be called once - subsequent calls return an error.
    pub async fn run(&mut self) -> Result<(), DispatchError> {
        // Take the receiver - this ensures run() can only be called once
        let mut rx = self
            .event_rx
            .take()
            .ok_or_else(|| DispatchError::Handler("Dispatcher already running".to_string()))?;

        {
            let mut running = self.running.write().await;
            *running = true;
        }

        let mut shutdown_rx = self.shutdown_rx.clone();

        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                // Process incoming events
                event = rx.recv() => {
                    match event {
                        Some(event) => {
                            // Decrement pending counter (was incremented by TrackedEventSender)
                            self.pending_events.fetch_sub(1, Ordering::Relaxed);

                            let event_type = event.event_type.clone();
                            let start = Instant::now();

                            match self.dispatch_event(event).await {
                                Ok(()) => {
                                    app_metrics::dispatch_events_total_inc(&self.app_id, &event_type);
                                    app_metrics::dispatch_event_duration_observe(
                                        &self.app_id,
                                        &event_type,
                                        start.elapsed().as_secs_f64(),
                                    );
                                }
                                Err(e) => {
                                    let error_type = match &e {
                                        DispatchError::NoHandler(_) => "no_handler",
                                        DispatchError::Serialization(_) => "serialization",
                                        DispatchError::Deserialization(_) => "deserialization",
                                        DispatchError::State(_) => "state",
                                        DispatchError::StoreNotFound(_) => "store_not_found",
                                        DispatchError::Handler(_) => "handler",
                                    };
                                    app_metrics::dispatch_events_failed_total_inc(
                                        &self.app_id,
                                        &event_type,
                                        error_type,
                                    );
                                    tracing::error!(
                                        app_id = %self.app_id,
                                        event_type = %event_type,
                                        error_type = %error_type,
                                        "Failed to dispatch event: {}", e
                                    );
                                }
                            }

                            // Report current queue depth
                            let depth = self.pending_events.load(Ordering::Relaxed);
                            app_metrics::event_queue_depth_set(&self.app_id, depth);
                        }
                        None => break, // Channel closed
                    }
                }
            }
        }

        {
            let mut running = self.running.write().await;
            *running = false;
        }

        Ok(())
    }

    /// Stop the event loop.
    pub async fn stop(&self) {
        // Signal shutdown first (no lock needed) to wake up the run loop
        let _ = self.shutdown_tx.send(true);
        // Then update flag - avoids deadlock if run loop is waiting on write lock
        {
            let mut running = self.running.write().await;
            *running = false;
        }
    }

    /// Check if dispatcher is running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get registered event types.
    pub fn event_types(&self) -> Vec<&EventType> {
        self.reducers.keys().collect()
    }

    /// Get registered request types.
    pub fn request_types(&self) -> Vec<&RequestType> {
        self.services.keys().collect()
    }
}

/// Dispatch errors.
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    /// No handler registered for type
    #[error("No handler for: {0}")]
    NoHandler(String),

    /// Serialization error
    #[error("Serialization failed: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization failed: {0}")]
    Deserialization(String),

    /// State operation error
    #[error("State error: {0}")]
    State(String),

    /// Store not found
    #[error("Store not found: {0}")]
    StoreNotFound(String),

    /// Handler error
    #[error("Handler error: {0}")]
    Handler(String),
}

/// Generate a simple UUID v4 (random).
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let random: u64 = rand::random();
    format!("{:016x}-{:016x}", timestamp, random)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::manifest::{KvConfig, StateConfig};
    use crate::apps::state_factory::{AppNamespace, StateFactory};

    /// Test reducer that echoes events to KV.
    struct EchoReducer;

    impl Reducer for EchoReducer {
        fn reduce(
            &self,
            _state: &StateSnapshot,
            event: &Event,
        ) -> Result<StateDelta, DispatchError> {
            let mut delta = StateDelta::new();
            // Store event in KV
            let key = format!("event:{}", event.timestamp);
            delta.kv_set("echoes", key, event.payload.clone());
            Ok(delta)
        }
    }

    /// Test service that queries KV.
    struct QueryService;

    #[async_trait::async_trait]
    impl Service for QueryService {
        async fn handle(
            &self,
            request: Request,
            state: &StateSnapshot,
            _event_tx: mpsc::Sender<Event>,
        ) -> Result<Response, DispatchError> {
            // Get the key from request payload
            let key: String = request.parse_payload()?;

            // Look up in state
            let value = state.kv_get("echoes", &key);

            match value {
                Some(v) => Response::success_with(&request.id, &v),
                None => Ok(Response::error(&request.id, "not found")),
            }
        }
    }

    #[tokio::test]
    async fn test_event_dispatch() {
        let factory = StateFactory::new();
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let config = StateConfig {
            kv: vec![KvConfig {
                name: "echoes".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let state = factory.create_for_app(namespace, &config).await.unwrap();
        let mut dispatcher = ComputeDispatcher::new("test-app", state.clone());

        // Register echo reducer
        dispatcher.register_reducer("echo:message", Box::new(EchoReducer));

        // Dispatch an event
        let event = Event::new("echo:message", b"hello".to_vec(), "test");
        dispatcher.dispatch_event(event).await.unwrap();

        // Verify state was updated
        let keys = state.kv("echoes").unwrap().list("event:").await.unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[tokio::test]
    async fn test_request_handling() {
        let factory = StateFactory::new();
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let config = StateConfig {
            kv: vec![KvConfig {
                name: "echoes".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let state = factory.create_for_app(namespace, &config).await.unwrap();

        // Pre-populate some data
        state
            .kv("echoes")
            .unwrap()
            .set("mykey", b"myvalue".to_vec())
            .await
            .unwrap();

        let mut dispatcher = ComputeDispatcher::new("test-app", state);

        // Register query service
        dispatcher.register_service("echo:query", Box::new(QueryService));

        // Handle a request
        let request = Request::with_payload("echo:query", &"mykey".to_string(), "test").unwrap();
        let response = dispatcher.handle_request(request).await.unwrap();

        assert!(response.success);
        let value: Vec<u8> = response.parse_payload().unwrap();
        assert_eq!(value, b"myvalue");
    }

    #[tokio::test]
    async fn test_no_handler() {
        let factory = StateFactory::new();
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let config = StateConfig::default();

        let state = factory.create_for_app(namespace, &config).await.unwrap();
        let dispatcher = ComputeDispatcher::new("test-app", state);

        // Dispatch to non-existent handler
        let event = Event::new("unknown:event", b"data".to_vec(), "test");
        let result = dispatcher.dispatch_event(event).await;

        assert!(matches!(result, Err(DispatchError::NoHandler(_))));
    }

    #[test]
    fn test_event_typed_payload() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestPayload {
            message: String,
            count: u32,
        }

        let payload = TestPayload {
            message: "hello".to_string(),
            count: 42,
        };

        let event = Event::with_payload("test:event", &payload, "test").unwrap();
        let parsed: TestPayload = event.parse_payload().unwrap();

        assert_eq!(parsed, payload);
    }

    #[test]
    fn test_state_delta() {
        let mut delta = StateDelta::new();

        delta.kv_set("store1", "key1", b"value1".to_vec());
        delta.kv_delete("store1", "key2");
        delta.log_append("log1", b"entry1".to_vec());

        assert_eq!(delta.kv_ops.len(), 2);
        assert_eq!(delta.log_ops.len(), 1);
    }
}
