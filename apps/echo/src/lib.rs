//! ICN Echo App
//!
//! A minimal test app that validates the app runtime works correctly.
//!
//! # Purpose
//!
//! This app exists to prove the runtime works before we extract real apps
//! like trust and governance. It:
//!
//! - Has a reducer that echoes events to KV state
//! - Has a service that queries stored messages
//! - Registers no oracle (simpler than trust)
//!
//! If echo works, trust extraction becomes mechanical: same pattern,
//! just with PolicyOracle registration.

use icn_core::apps::{
    AppBuilder, DispatchError, Event, Reducer, Request, Response, Service, StateDelta,
    StateSnapshot,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Echo message payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoMessage {
    /// The message content
    pub content: String,
    /// Optional sender identifier
    pub sender: Option<String>,
}

/// Echo query request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoQuery {
    /// Query type: "count", "list", or "get:<key>"
    pub query: String,
}

/// Echo query response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoResponse {
    /// Number of stored messages
    pub count: usize,
    /// List of message keys (for "list" query)
    pub keys: Vec<String>,
    /// Single message content (for "get" query)
    pub message: Option<String>,
}

/// Echo reducer - stores messages in KV.
///
/// This is a pure, deterministic reducer. It receives events and
/// produces state deltas without side effects.
pub struct EchoReducer;

impl Reducer for EchoReducer {
    fn reduce(&self, _state: &StateSnapshot, event: &Event) -> Result<StateDelta, DispatchError> {
        // Parse the message
        let msg: EchoMessage = event.parse_payload()?;

        // Create state delta
        let mut delta = StateDelta::new();

        // Store message with timestamp as key
        let key = format!("msg:{}", event.timestamp);
        let value =
            serde_json::to_vec(&msg).map_err(|e| DispatchError::Serialization(e.to_string()))?;
        delta.kv_set("echoes", key, value);

        Ok(delta)
    }
}

/// Echo service - queries stored messages.
///
/// This is an async service that can read state but must not mutate it.
/// It can emit events if needed (but this simple service doesn't).
pub struct EchoService;

#[async_trait::async_trait]
impl Service for EchoService {
    async fn handle(
        &self,
        request: Request,
        state: &StateSnapshot,
        _event_tx: mpsc::Sender<Event>,
    ) -> Result<Response, DispatchError> {
        // Parse query
        let query: EchoQuery = request.parse_payload()?;

        // Get all keys
        let keys: Vec<String> = state.kv_keys("echoes").into_iter().cloned().collect();

        let response = match query.query.as_str() {
            "count" => EchoResponse {
                count: keys.len(),
                keys: vec![],
                message: None,
            },
            "list" => EchoResponse {
                count: keys.len(),
                keys,
                message: None,
            },
            q if q.starts_with("get:") => {
                let key = q.strip_prefix("get:").unwrap_or("");
                let message = state
                    .kv_get("echoes", key)
                    .and_then(|v| serde_json::from_slice::<EchoMessage>(v).ok())
                    .map(|m| m.content);
                EchoResponse {
                    count: keys.len(),
                    keys: vec![],
                    message,
                }
            }
            _ => {
                return Ok(Response::error(
                    &request.id,
                    format!("Unknown query: {}", query.query),
                ));
            }
        };

        Response::success_with(&request.id, &response)
    }
}

/// Register echo app handlers with a builder.
pub fn register_handlers(builder: AppBuilder) -> AppBuilder {
    builder
        .with_reducer("echo:message", Box::new(EchoReducer))
        .with_service("echo:query", Box::new(EchoService))
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_core::apps::{AppRuntime, Manifest};
    use icn_kernel_api::authz::{Capability, Constraints};
    use icn_kernel_api::bootstrap::{CapabilitySet, OracleRegistry};
    use std::sync::Arc;

    fn root_caps() -> CapabilitySet {
        let mut set = CapabilitySet::new();
        set.add(Capability {
            id: "root".to_string(),
            resource: "*".to_string(),
            action: "*".to_string(),
            constraints: Constraints::default(),
            holder: None,
            issuer: "did:icn:root".to_string(),
            expiration: u64::MAX,
            signature: vec![],
        });
        set
    }

    const ECHO_MANIFEST: &str = include_str!("../manifest.yaml");

    #[tokio::test]
    async fn test_echo_app_lifecycle() {
        // Create runtime
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        // Parse manifest
        let manifest = Manifest::parse(ECHO_MANIFEST).expect("Failed to parse manifest");
        assert_eq!(manifest.name, "echo");

        // Prepare and register handlers
        let builder = runtime.prepare(manifest).await.expect("Failed to prepare");
        let builder = register_handlers(builder);

        // Install
        let app_id = runtime
            .install(builder, &root_caps())
            .await
            .expect("Failed to install");

        // Start
        runtime.start(&app_id).await.expect("Failed to start");

        // Get dispatcher
        let dispatcher = runtime.dispatcher(&app_id).await.expect("No dispatcher");

        // Send a message event
        let msg = EchoMessage {
            content: "Hello, ICN!".to_string(),
            sender: Some("test".to_string()),
        };
        let event =
            Event::with_payload("echo:message", &msg, "test").expect("Failed to create event");

        {
            let d = dispatcher.read().await;
            d.dispatch_event(event).await.expect("Failed to dispatch");
        }

        // Query count
        let query = EchoQuery {
            query: "count".to_string(),
        };
        let request =
            Request::with_payload("echo:query", &query, "test").expect("Failed to create request");

        let response = {
            let d = dispatcher.read().await;
            d.handle_request(request).await.expect("Failed to query")
        };

        assert!(response.success);
        let result: EchoResponse = response.parse_payload().expect("Failed to parse response");
        assert_eq!(result.count, 1);

        // Stop
        runtime.stop(&app_id).await.expect("Failed to stop");

        // Uninstall
        runtime
            .uninstall(&app_id)
            .await
            .expect("Failed to uninstall");
    }

    #[tokio::test]
    async fn test_echo_reducer() {
        let reducer = EchoReducer;

        let msg = EchoMessage {
            content: "Test message".to_string(),
            sender: None,
        };

        let event =
            Event::with_payload("echo:message", &msg, "test").expect("Failed to create event");
        let snapshot = StateSnapshot::from_app_state(&create_empty_state().await).await;

        let delta = reducer.reduce(&snapshot, &event).expect("Reduce failed");

        assert_eq!(delta.kv_ops.len(), 1);
    }

    async fn create_empty_state() -> icn_core::apps::AppState {
        use icn_core::apps::{AppNamespace, KvConfig, StateConfig, StateFactory};

        let factory = StateFactory::new();
        let namespace = AppNamespace::new("did:icn:test", "echo");
        let config = StateConfig {
            kv: vec![KvConfig {
                name: "echoes".to_string(),
            }],
            ..Default::default()
        };

        factory
            .create_for_app(namespace, &config)
            .await
            .expect("Failed to create state")
    }
}
