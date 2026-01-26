//! ICN Trust App
//!
//! Policy oracle for trust-based authorization in the ICN kernel/app separation.
//!
//! # Purpose
//!
//! This app implements the trust domain's `PolicyOracle`, converting trust graph
//! computations into kernel-enforceable constraints. The kernel enforces these
//! constraints without understanding trust semantics (the "meaning firewall").
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         Trust App                                │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
//! │  │ TrustEdgeReducer│  │ TrustQuerySvc   │  │ TrustPolicyOracle│ │
//! │  │ (state updates) │  │ (score queries) │  │ (authz decisions)│ │
//! │  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘  │
//! │           │                    │                    │            │
//! │           ▼                    ▼                    ▼            │
//! │  ┌─────────────────────────────────────────────────────────────┐│
//! │  │                    MultiTrustGraph                          ││
//! │  │  ┌───────────┐  ┌───────────────┐  ┌─────────────────────┐  ││
//! │  │  │ Social    │  │ Economic      │  │ Technical           │  ││
//! │  │  │ Graph     │  │ Graph         │  │ Graph               │  ││
//! │  │  └───────────┘  └───────────────┘  └─────────────────────┘  ││
//! │  └─────────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼ PolicyDecision
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         Kernel                                   │
//! │  Enforces: rate_limit, credit_multiplier, voting_weight, etc.   │
//! │  Does NOT understand: trust classes, graph semantics            │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use icn_app_trust::{register_handlers, TrustPolicyOracle};
//!
//! // In app runtime initialization:
//! let builder = runtime.prepare(manifest).await?;
//! let builder = register_handlers(builder, graph.clone());
//! runtime.install(builder, &caps).await?;
//!
//! // Oracle is automatically registered with OracleRegistry
//! ```

pub mod oracle;

use icn_core::apps::{
    AppBuilder, DispatchError, Event, Reducer, Request, Response, Service, StateDelta,
    StateSnapshot,
};
use icn_trust::{MultiTrustGraph, TrustEdge, TrustGraphType, TrustScore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::debug;

pub use oracle::{SimpleTrustOracle, TrustPolicyOracle};

/// Trust edge update event payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustEdgeUpdate {
    /// Source DID (truster)
    pub source: String,
    /// Target DID (trustee)
    pub target: String,
    /// Trust score (0.0 to 1.0)
    pub score: f64,
    /// Which graph type (social, economic, technical)
    pub graph_type: String,
    /// Optional labels for the relationship
    #[serde(default)]
    pub labels: Vec<String>,
    /// Optional expiration timestamp
    pub expires_at: Option<u64>,
}

/// Trust query request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustQuery {
    /// Query type: "score", "class", "edges", "all_trusted"
    pub query_type: String,
    /// Target DID for score/class queries
    pub target: Option<String>,
    /// Graph type filter (optional)
    pub graph_type: Option<String>,
    /// Threshold for "all_trusted" queries
    pub threshold: Option<f64>,
}

/// Trust query response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustQueryResponse {
    /// Trust score (for "score" query)
    pub score: Option<f64>,
    /// Trust class (for "class" query)
    pub trust_class: Option<String>,
    /// List of DIDs (for "edges" or "all_trusted" queries)
    pub dids: Vec<String>,
    /// Success indicator
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Trust edge reducer - updates trust graph from events.
///
/// This reducer handles trust:edge:update events and applies them
/// to the appropriate trust graph (social, economic, or technical).
pub struct TrustEdgeReducer {
    graph: Arc<RwLock<MultiTrustGraph>>,
}

impl TrustEdgeReducer {
    /// Create a new trust edge reducer.
    pub fn new(graph: Arc<RwLock<MultiTrustGraph>>) -> Self {
        Self { graph }
    }
}

impl Reducer for TrustEdgeReducer {
    fn reduce(&self, _state: &StateSnapshot, event: &Event) -> Result<StateDelta, DispatchError> {
        // Parse the update
        let update: TrustEdgeUpdate = event.parse_payload()?;

        debug!(
            "Processing trust edge update: {} -> {} (score: {}, type: {})",
            update.source, update.target, update.score, update.graph_type
        );

        // Validate score
        if !(0.0..=1.0).contains(&update.score) {
            return Err(DispatchError::Handler(format!(
                "Invalid trust score: {} (must be 0.0-1.0)",
                update.score
            )));
        }

        // Parse graph type
        let graph_type = match update.graph_type.to_lowercase().as_str() {
            "social" => TrustGraphType::Social,
            "economic" => TrustGraphType::EconomicReliability,
            "technical" => TrustGraphType::TechnicalReliability,
            _ => {
                return Err(DispatchError::Handler(format!(
                    "Invalid graph type: {} (must be social, economic, or technical)",
                    update.graph_type
                )));
            }
        };

        // Parse DIDs
        let source_did = icn_identity::Did::from_str(&update.source)
            .map_err(|e| DispatchError::Handler(format!("Invalid source DID: {e}")))?;
        let target_did = icn_identity::Did::from_str(&update.target)
            .map_err(|e| DispatchError::Handler(format!("Invalid target DID: {e}")))?;

        // Build trust edge
        let mut edge = TrustEdge::new_typed(
            source_did,
            target_did,
            TrustScore::unchecked(update.score),
            graph_type,
        );

        for label in &update.labels {
            edge = edge.with_label(label.clone());
        }

        if let Some(expires_at) = update.expires_at {
            edge = edge.with_expiry(expires_at);
        }

        // Apply to graph (blocking) - use block_in_place for tokio compatibility
        tokio::task::block_in_place(|| {
            let mut graph = self.graph.blocking_write();
            let typed_graph = match graph_type {
                TrustGraphType::Social => graph.social_mut(),
                TrustGraphType::EconomicReliability => graph.economic_mut(),
                TrustGraphType::TechnicalReliability => graph.technical_mut(),
            };

            typed_graph
                .add_edge(edge)
                .map_err(|e| DispatchError::Handler(format!("Failed to add edge: {e}")))
        })?;

        // Create state delta for persistence
        // Store a record in the appropriate KV store
        let mut delta = StateDelta::new();

        let store_name = match graph_type {
            TrustGraphType::Social => "social_edges",
            TrustGraphType::EconomicReliability => "economic_edges",
            TrustGraphType::TechnicalReliability => "technical_edges",
        };

        let key = format!("{}:{}", update.source, update.target);
        let value = serde_json::to_vec(&update)
            .map_err(|e| DispatchError::Serialization(e.to_string()))?;

        delta.kv_set(store_name, key, value);

        Ok(delta)
    }
}

/// Trust query service - handles trust score queries.
pub struct TrustQueryService {
    graph: Arc<RwLock<MultiTrustGraph>>,
}

impl TrustQueryService {
    /// Create a new trust query service.
    pub fn new(graph: Arc<RwLock<MultiTrustGraph>>) -> Self {
        Self { graph }
    }
}

#[async_trait::async_trait]
impl Service for TrustQueryService {
    async fn handle(
        &self,
        request: Request,
        _state: &StateSnapshot,
        _event_tx: mpsc::Sender<Event>,
    ) -> Result<Response, DispatchError> {
        let query: TrustQuery = request.parse_payload()?;

        let response = match query.query_type.as_str() {
            "score" => {
                let target_str = query.target.ok_or_else(|| {
                    DispatchError::Handler("Missing target for score query".to_string())
                })?;

                // Parse target as DID
                let target = icn_identity::Did::from_str(&target_str)
                    .map_err(|e| DispatchError::Handler(format!("Invalid DID: {e}")))?;

                let graph = self.graph.read().await;
                let score = graph
                    .compute_combined_trust_score(&target)
                    .unwrap_or_default();

                TrustQueryResponse {
                    score: Some(score),
                    trust_class: None,
                    dids: vec![],
                    success: true,
                    error: None,
                }
            }

            "class" => {
                let target_str = query.target.ok_or_else(|| {
                    DispatchError::Handler("Missing target for class query".to_string())
                })?;

                // Parse target as DID
                let target = icn_identity::Did::from_str(&target_str)
                    .map_err(|e| DispatchError::Handler(format!("Invalid DID: {e}")))?;

                let graph = self.graph.read().await;
                let score = graph
                    .compute_combined_trust_score(&target)
                    .unwrap_or_default();

                let class = icn_trust::TrustClass::from_score(score);

                TrustQueryResponse {
                    score: Some(score),
                    trust_class: Some(format!("{class:?}")),
                    dids: vec![],
                    success: true,
                    error: None,
                }
            }

            "all_trusted" => {
                let threshold = query.threshold.unwrap_or(0.1);
                let graph = self.graph.read().await;

                // Get all DIDs above threshold and convert to strings
                let dids: Vec<String> = graph
                    .social()
                    .get_dids_above_threshold(threshold)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|d| d.to_string())
                    .collect();

                TrustQueryResponse {
                    score: None,
                    trust_class: None,
                    dids,
                    success: true,
                    error: None,
                }
            }

            _ => TrustQueryResponse {
                score: None,
                trust_class: None,
                dids: vec![],
                success: false,
                error: Some(format!("Unknown query type: {}", query.query_type)),
            },
        };

        Response::success_with(&request.id, &response)
    }
}

/// Register trust app handlers with a builder.
///
/// This sets up:
/// - TrustEdgeReducer for processing edge update events
/// - TrustQueryService for handling trust queries
/// - TrustPolicyOracle for authorization decisions
pub fn register_handlers(
    builder: AppBuilder,
    graph: Arc<RwLock<MultiTrustGraph>>,
) -> AppBuilder {
    let oracle = Arc::new(TrustPolicyOracle::new(graph.clone()));

    builder
        .with_reducer("trust:edge:update", Box::new(TrustEdgeReducer::new(graph.clone())))
        .with_service("trust:query", Box::new(TrustQueryService::new(graph)))
        .with_oracle(oracle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_core::apps::{
        AppNamespace, AppRuntime, KvConfig, Manifest, StateConfig, StateFactory, StateSnapshot,
    };
    use icn_identity::KeyPair;
    use icn_kernel_api::authz::{Capability, Constraints};
    use icn_kernel_api::bootstrap::{CapabilitySet, OracleRegistry};
    use icn_store::SledStore;

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

    fn create_test_graph() -> Arc<RwLock<MultiTrustGraph>> {
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_did = KeyPair::generate().unwrap().did().clone();
        Arc::new(RwLock::new(MultiTrustGraph::new(store, own_did)))
    }

    async fn create_empty_state() -> icn_core::apps::AppState {
        let factory = StateFactory::new();
        let namespace = AppNamespace::new("did:icn:test", "trust");
        let config = StateConfig {
            kv: vec![
                KvConfig { name: "social_edges".to_string() },
                KvConfig { name: "economic_edges".to_string() },
                KvConfig { name: "technical_edges".to_string() },
                KvConfig { name: "score_cache".to_string() },
            ],
            ..Default::default()
        };

        factory
            .create_for_app(namespace, &config)
            .await
            .expect("Failed to create state")
    }

    const TRUST_MANIFEST: &str = include_str!("../manifest.yaml");

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_trust_edge_reducer() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let graph = Arc::new(RwLock::new(MultiTrustGraph::new(store, alice.clone())));
        let reducer = TrustEdgeReducer::new(graph.clone());

        // Create edge update event with valid DIDs
        let update = TrustEdgeUpdate {
            source: alice.to_string(),
            target: bob.to_string(),
            score: 0.75,
            graph_type: "social".to_string(),
            labels: vec!["partner".to_string()],
            expires_at: None,
        };

        let event = Event::with_payload("trust:edge:update", &update, "test")
            .expect("Failed to create event");

        // Create snapshot for reducer
        let snapshot = StateSnapshot::from_app_state(&create_empty_state().await).await;

        // Reduce
        let delta = reducer.reduce(&snapshot, &event).expect("Reduce failed");

        // Verify delta
        assert_eq!(delta.kv_ops.len(), 1);

        // Verify graph was updated
        let g = graph.read().await;
        let edge = g.social().get_edge(&alice, &bob).unwrap();

        assert!(edge.is_some());
        let edge = edge.unwrap();
        assert_eq!(edge.score.value(), 0.75);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_trust_query_service() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_did = KeyPair::generate().unwrap().did().clone();
        let peer_did = KeyPair::generate().unwrap().did().clone();

        let graph = Arc::new(RwLock::new(MultiTrustGraph::new(store, own_did.clone())));

        // Add trust edge
        {
            let mut g = graph.write().await;
            g.social_mut()
                .add_edge(TrustEdge::new(
                    own_did.clone(),
                    peer_did.clone(),
                    TrustScore::unchecked(0.8),
                ))
                .unwrap();
        }

        let service = TrustQueryService::new(graph);

        // Query score - use String for the target
        let query = TrustQuery {
            query_type: "score".to_string(),
            target: Some(peer_did.to_string()),
            graph_type: None,
            threshold: None,
        };

        let request = Request::with_payload("trust:query", &query, "test")
            .expect("Failed to create request");

        let (tx, _rx) = mpsc::channel(1);
        let snapshot = StateSnapshot::from_app_state(&create_empty_state().await).await;

        let response = service
            .handle(request, &snapshot, tx)
            .await
            .expect("Handle failed");

        assert!(response.success);

        let result: TrustQueryResponse = response.parse_payload().expect("Failed to parse");
        assert!(result.success);
        assert!(result.score.is_some());
        // Combined score calculation:
        // - Social: 0.8 * 0.6 (60/40 weighting) = 0.48, then * 0.5 (combined weight) = 0.24
        // - Economic: 0.05 fallback * 0.3 = 0.015
        // - Technical: 0.05 fallback * 0.2 = 0.01
        // Total: 0.24 + 0.015 + 0.01 = 0.265
        let score = result.score.unwrap();
        assert!(score > 0.2 && score < 0.35, "Expected score ~0.265, got {}", score);
    }

    #[tokio::test]
    async fn test_trust_app_manifest_parsing() {
        let manifest = Manifest::parse(TRUST_MANIFEST).expect("Failed to parse manifest");

        assert_eq!(manifest.name, "trust");
        assert_eq!(manifest.version, "0.1.0");
        assert!(manifest.capabilities_required.contains(&"state:kv:read:self".to_string()));
        assert!(manifest.capabilities_provided.contains(&"oracle:trust:policy".to_string()));
    }

    #[tokio::test]
    async fn test_trust_app_lifecycle() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry.clone());

        let manifest = Manifest::parse(TRUST_MANIFEST).expect("Failed to parse manifest");

        // Prepare app
        let builder = runtime.prepare(manifest).await.expect("Failed to prepare");

        // Register handlers
        let graph = create_test_graph();
        let builder = register_handlers(builder, graph);

        // Install
        let app_id = runtime
            .install(builder, &root_caps())
            .await
            .expect("Failed to install");

        // Start
        runtime.start(&app_id).await.expect("Failed to start");

        // Verify oracle is registered
        let oracle = registry.get(&icn_kernel_api::authz::Domain::trust());
        assert!(oracle.domain() == icn_kernel_api::authz::Domain::trust());

        // Stop
        runtime.stop(&app_id).await.expect("Failed to stop");

        // Uninstall
        runtime
            .uninstall(&app_id)
            .await
            .expect("Failed to uninstall");
    }
}
