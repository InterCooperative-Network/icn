//! Service Discovery Manager
//!
//! In-memory registry for service endpoints with background expiry
//! and gossip-based remote discovery aggregation.

use icn_kernel_api::naming::{
    AsyncScopedDiscovery, NamingError, ScopedDiscovery, ServiceEndpoint, ServiceEndpointId,
    ServiceType,
};
use icn_kernel_api::scope::ScopeLevel;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Maximum number of service endpoints the registry will hold.
/// Prevents unbounded memory growth in long-running deployments.
const MAX_REGISTRY_SIZE: usize = 10_000;

/// Default timeout for remote discovery queries via gossip.
const DEFAULT_REMOTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Maximum number of concurrent pending remote queries.
const MAX_PENDING_QUERIES: usize = 64;

/// Debug-only check that warns if called from within a Tokio async runtime.
/// This catches accidental use of blocking ScopedDiscovery methods from async code.
#[allow(unused_variables)]
fn debug_assert_blocking_context(method: &str) {
    #[cfg(debug_assertions)]
    if tokio::runtime::Handle::try_current().is_ok() {
        tracing::warn!(
            method = %method,
            "ScopedDiscovery::{} called from async context; use async methods instead",
            method
        );
    }
}

/// Outcome of a remote discovery query.
#[derive(Debug)]
pub enum RemoteDiscoverOutcome {
    /// Results arrived within the timeout.
    Results(Vec<ServiceEndpoint>),
    /// No results arrived (peers responded but had nothing).
    NoResults,
    /// The timeout elapsed before all expected responses arrived.
    Timeout(Vec<ServiceEndpoint>),
}

/// Sled tree name for persisted service endpoints.
const SLED_TREE_NAME: &str = "service_endpoints";

/// In-memory service endpoint registry with TTL-based expiry,
/// gossip-based remote discovery aggregation, and optional sled persistence.
#[derive(Clone)]
pub struct ServiceDiscoveryManager {
    /// service_id → ServiceEndpoint
    registry: Arc<RwLock<HashMap<String, ServiceEndpoint>>>,
    /// Optional gossip handle for propagating announcements/withdrawals
    gossip_handle: Option<icn_gossip::GossipHandle>,
    /// Pending remote discovery queries: query_id → response sender
    pending_queries: Arc<RwLock<HashMap<String, mpsc::Sender<Vec<ServiceEndpoint>>>>>,
    /// Own signing key for generating signed responses to incoming queries
    own_signing_key: Option<Arc<ed25519_dalek::SigningKey>>,
    /// Own DID for response attribution
    own_did: Option<icn_identity::Did>,
    /// Optional sled tree for write-through persistence
    sled_tree: Option<sled::Tree>,
}

impl ServiceDiscoveryManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            gossip_handle: None,
            pending_queries: Arc::new(RwLock::new(HashMap::new())),
            own_signing_key: None,
            own_did: None,
            sled_tree: None,
        }
    }

    /// Create a new manager backed by sled for persistence.
    ///
    /// On construction, loads all non-expired endpoints from sled into memory.
    /// Subsequent announce/withdraw calls write through to sled.
    pub fn with_sled(db: &sled::Db) -> Result<Self, NamingError> {
        let tree = db
            .open_tree(SLED_TREE_NAME)
            .map_err(|e| NamingError::Internal(format!("Failed to open sled tree: {e}")))?;

        // Load existing entries from sled
        let mut registry = HashMap::new();
        let mut expired_keys = Vec::new();

        for entry in tree.iter() {
            let (key, value) = entry
                .map_err(|e| NamingError::Internal(format!("Failed to read sled entry: {e}")))?;
            match serde_json::from_slice::<ServiceEndpoint>(&value) {
                Ok(ep) => {
                    if ep.is_expired() {
                        expired_keys.push(key);
                    } else {
                        registry.insert(ep.service_id.clone(), ep);
                    }
                }
                Err(e) => {
                    warn!("Skipping corrupt sled entry: {}", e);
                    expired_keys.push(key);
                }
            }
        }

        // Clean up expired/corrupt entries
        for key in expired_keys {
            let _ = tree.remove(key);
        }

        info!(
            "Loaded {} service endpoints from sled persistence",
            registry.len()
        );

        Ok(Self {
            registry: Arc::new(RwLock::new(registry)),
            gossip_handle: None,
            pending_queries: Arc::new(RwLock::new(HashMap::new())),
            own_signing_key: None,
            own_did: None,
            sled_tree: Some(tree),
        })
    }

    /// Create a new manager with gossip propagation enabled.
    ///
    /// When a gossip handle is provided:
    /// - The manager subscribes to `services:announce` topic
    /// - Successful `announce()` calls propagate via gossip
    /// - Successful `withdraw()` calls propagate via gossip
    ///
    /// **Important**: This method does NOT set up notification callbacks. The supervisor
    /// must route incoming service discovery messages to `handle_gossip_entry()` via
    /// its central notification callback. This follows the established pattern where
    /// the supervisor owns the single global gossip callback and routes messages to
    /// appropriate handlers based on topic.
    pub async fn with_gossip(
        gossip_handle: icn_gossip::GossipHandle,
        own_did: icn_identity::Did,
    ) -> Result<Self, NamingError> {
        let registry = Arc::new(RwLock::new(HashMap::new()));

        let manager = Self {
            registry: registry.clone(),
            gossip_handle: Some(gossip_handle.clone()),
            pending_queries: Arc::new(RwLock::new(HashMap::new())),
            own_signing_key: None,
            own_did: Some(own_did.clone()),
            sled_tree: None,
        };

        let mut gossip = gossip_handle.write().await;

        // Create announce topic
        let announce_topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
        let topic = icn_gossip::types::Topic::new(
            announce_topic.to_string(),
            icn_gossip::AccessControl::Public,
        );
        gossip.create_topic(topic);
        gossip
            .subscribe(announce_topic, own_did.clone())
            .await
            .map_err(|e| {
                NamingError::Internal(format!("Failed to subscribe to gossip topic: {e}"))
            })?;

        // Create query topic
        let query_topic = icn_gossip::service_discovery_topics::SERVICES_QUERY;
        let topic = icn_gossip::types::Topic::new(
            query_topic.to_string(),
            icn_gossip::AccessControl::Public,
        );
        gossip.create_topic(topic);
        gossip.subscribe(query_topic, own_did).await.map_err(|e| {
            NamingError::Internal(format!("Failed to subscribe to gossip query topic: {e}"))
        })?;

        info!(
            "ServiceDiscoveryManager subscribed to gossip topics: {}, {}",
            announce_topic, query_topic
        );

        Ok(manager)
    }

    /// Attach sled persistence to an already-constructed manager.
    ///
    /// Loads all non-expired endpoints from sled into the existing in-memory
    /// registry and wires up write-through for future announce/withdraw calls.
    /// Call this after `with_gossip()` to combine gossip propagation with
    /// durable persistence across daemon restarts.
    ///
    /// The sled scan happens outside the registry lock to keep the lock window
    /// short; the lock is only held for the final merge into memory.
    pub async fn with_persistence(&mut self, db: &sled::Db) -> Result<(), NamingError> {
        let tree = db
            .open_tree(SLED_TREE_NAME)
            .map_err(|e| NamingError::Internal(format!("Failed to open sled tree: {e}")))?;

        // Scan sled outside the registry lock — this is the slow part.
        let mut loaded: HashMap<String, ServiceEndpoint> = HashMap::new();
        let mut expired_keys = Vec::new();

        for entry in tree.iter() {
            let (key, value) = entry
                .map_err(|e| NamingError::Internal(format!("Failed to read sled entry: {e}")))?;
            match serde_json::from_slice::<ServiceEndpoint>(&value) {
                Ok(ep) => {
                    if ep.is_expired() {
                        expired_keys.push(key);
                    } else {
                        loaded.insert(ep.service_id.clone(), ep);
                    }
                }
                Err(e) => {
                    warn!("Skipping corrupt sled entry: {}", e);
                    expired_keys.push(key);
                }
            }
        }

        // Prune expired/corrupt entries from sled now that we've finished scanning.
        for key in &expired_keys {
            if let Err(e) = tree.remove(key) {
                warn!(
                    "Failed to remove expired sled entry for key {:?}: {}",
                    key, e
                );
            }
        }

        // Merge loaded endpoints into registry — brief lock window, no I/O inside.
        // Note: extend() overwrites in-memory entries on service_id collision.
        // This is intentional: persistence is called at startup before gossip has
        // had a chance to announce anything, so there should be no conflicting entries.
        let loaded_count = loaded.len();
        {
            let mut registry = self.registry.write().await;
            registry.extend(loaded);
        }

        info!(
            "Loaded {} service endpoints from sled persistence ({} expired entries pruned)",
            loaded_count,
            expired_keys.len()
        );

        self.sled_tree = Some(tree);
        Ok(())
    }

    /// Set the signing key for generating signed responses to incoming queries.
    ///
    /// Without a signing key, the manager cannot respond to remote queries.
    pub fn set_signing_key(&mut self, key: ed25519_dalek::SigningKey) {
        self.own_signing_key = Some(Arc::new(key));
    }

    /// Handle incoming gossip entry for service discovery.
    ///
    /// This is the primary method for supervisor integration. The supervisor's
    /// central notification callback should call this method when a service
    /// discovery message is received on the `services:announce` or
    /// `services:query` topic.
    ///
    /// Returns an error if message processing fails (for logging purposes).
    pub async fn handle_incoming_gossip(
        &self,
        entry: icn_gossip::GossipEntry,
    ) -> Result<(), String> {
        Self::handle_gossip_entry_internal(
            self.registry.clone(),
            Some(self.pending_queries.clone()),
            self.own_signing_key.clone(),
            self.own_did.clone(),
            self.gossip_handle.clone(),
            entry,
        )
        .await
    }

    /// Internal static handler for gossip entries (exposed for testing).
    ///
    /// For production use, prefer `handle_incoming_gossip(&self, entry)` which
    /// uses the manager's internal registry.
    ///
    /// Returns an error if message processing fails (for logging purposes).
    pub async fn handle_gossip_entry(
        registry: Arc<RwLock<HashMap<String, ServiceEndpoint>>>,
        entry: icn_gossip::GossipEntry,
    ) -> Result<(), String> {
        Self::handle_gossip_entry_internal(registry, None, None, None, None, entry).await
    }

    /// Discover services remotely via gossip query fanout.
    ///
    /// Sends a `Query` message on the `services:query` gossip topic and
    /// aggregates signed `Response` messages from peers within a timeout.
    ///
    /// Returns a `RemoteDiscoverOutcome` distinguishing between:
    /// - `Results`: at least one valid response arrived
    /// - `NoResults`: timeout elapsed, no responses received
    /// - `Timeout`: timeout elapsed but some partial results arrived
    pub async fn discover_remote(
        &self,
        scope: ScopeLevel,
        service_type: Option<&ServiceType>,
        required_capabilities: &[String],
        timeout: Option<std::time::Duration>,
    ) -> Result<RemoteDiscoverOutcome, NamingError> {
        let gossip_handle = self.gossip_handle.as_ref().ok_or_else(|| {
            NamingError::Internal("Gossip not configured for remote discovery".to_string())
        })?;

        let own_did = self.own_did.as_ref().ok_or_else(|| {
            NamingError::Internal("Own DID not configured for remote discovery".to_string())
        })?;

        // Enforce pending query limit
        {
            let pq = self.pending_queries.read().await;
            if pq.len() >= MAX_PENDING_QUERIES {
                return Err(NamingError::Internal(
                    "Too many pending remote queries".to_string(),
                ));
            }
        }

        let query_id = format!("q-{}", uuid::Uuid::new_v4());
        let timeout_duration = timeout.unwrap_or(DEFAULT_REMOTE_TIMEOUT);
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| NamingError::Internal(format!("System time error: {e}")))?
            .as_secs();
        let expires_at = now_secs + timeout_duration.as_secs() + 30; // pad expiry beyond timeout

        // Create response channel
        let (tx, mut rx) = mpsc::channel::<Vec<ServiceEndpoint>>(32);
        {
            let mut pq = self.pending_queries.write().await;
            pq.insert(query_id.clone(), tx);
        }

        // Build and publish query
        let query_msg = icn_gossip::ServiceDiscoveryMessage::Query {
            requester: own_did.clone(),
            service_type: service_type.cloned().unwrap_or_else(|| ServiceType {
                name: "*".to_string(),
                version: String::new(),
            }),
            max_scope: scope,
            required_capabilities: required_capabilities.to_vec(),
            query_id: query_id.clone(),
            expires_at,
        };

        let encoded = icn_encoding::encode(&query_msg)
            .map_err(|e| NamingError::Internal(format!("Failed to encode query: {e}")))?;

        {
            let topic = icn_gossip::service_discovery_topics::SERVICES_QUERY;
            let mut gossip = gossip_handle.write().await;
            gossip
                .publish(topic, encoded)
                .await
                .map_err(|e| NamingError::Internal(format!("Failed to publish query: {e}")))?;
        }

        debug!(query_id = %query_id, "Published remote discovery query");

        // Collect responses within timeout
        let mut all_endpoints = Vec::new();
        let deadline = tokio::time::Instant::now() + timeout_duration;
        let timed_out;

        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(endpoints)) => {
                    all_endpoints.extend(endpoints);
                }
                Ok(None) => {
                    // Channel closed (all senders dropped)
                    timed_out = false;
                    break;
                }
                Err(_) => {
                    // Timeout elapsed
                    timed_out = true;
                    break;
                }
            }
        }

        // Clean up pending query
        {
            let mut pq = self.pending_queries.write().await;
            pq.remove(&query_id);
        }

        // Deduplicate by service_id (keep first seen)
        let mut seen = std::collections::HashSet::new();
        all_endpoints.retain(|ep| seen.insert(ep.service_id.clone()));

        debug!(
            query_id = %query_id,
            results = all_endpoints.len(),
            timed_out,
            "Remote discovery completed"
        );

        if all_endpoints.is_empty() {
            Ok(RemoteDiscoverOutcome::NoResults)
        } else if timed_out {
            Ok(RemoteDiscoverOutcome::Timeout(all_endpoints))
        } else {
            Ok(RemoteDiscoverOutcome::Results(all_endpoints))
        }
    }

    /// Internal implementation for processing gossip entries.
    #[allow(clippy::type_complexity)]
    async fn handle_gossip_entry_internal(
        registry: Arc<RwLock<HashMap<String, ServiceEndpoint>>>,
        pending_queries: Option<Arc<RwLock<HashMap<String, mpsc::Sender<Vec<ServiceEndpoint>>>>>>,
        own_signing_key: Option<Arc<ed25519_dalek::SigningKey>>,
        own_did: Option<icn_identity::Did>,
        gossip_handle: Option<icn_gossip::GossipHandle>,
        entry: icn_gossip::GossipEntry,
    ) -> Result<(), String> {
        // Decode the message (handling possible compression via get_data)
        let data = entry
            .get_data()
            .map_err(|e| format!("Failed to get entry data: {e}"))?;
        let msg: icn_gossip::ServiceDiscoveryMessage =
            icn_encoding::decode(&data).map_err(|e| format!("Failed to decode: {e}"))?;

        match msg {
            icn_gossip::ServiceDiscoveryMessage::Announce { endpoint } => {
                // Verify signature (supports key rotation)
                if let Err(e) = icn_gossip::verify_service_endpoint_with_rotation(&endpoint, None) {
                    return Err(format!(
                        "Invalid signature for {}: {}",
                        endpoint.service_id, e
                    ));
                }

                // Check if expired
                if endpoint.is_expired() {
                    debug!(
                        "Ignoring expired service announcement: {}",
                        endpoint.service_id
                    );
                    return Ok(());
                }

                // Store in registry (deduplication by service_id)
                let mut reg = registry.write().await;

                let is_update = reg.contains_key(&endpoint.service_id);
                if !is_update && reg.len() >= MAX_REGISTRY_SIZE {
                    return Err(format!(
                        "Registry full, ignoring announcement: {}",
                        endpoint.service_id
                    ));
                }

                let service_id = endpoint.service_id.clone();
                reg.insert(service_id.clone(), endpoint);

                debug!("Received service announcement via gossip: {}", service_id);
                icn_obs::metrics::service_discovery::gossip_announcements_received_inc();
                Ok(())
            }
            icn_gossip::ServiceDiscoveryMessage::Withdraw {
                service_id,
                provider,
                ..
            } => {
                // SECURITY: Verify that the gossip entry author matches the provider DID
                if entry.author != provider {
                    return Err(format!(
                        "Withdrawal author mismatch: entry.author={}, provider={} (service: {})",
                        entry.author, provider, service_id
                    ));
                }

                let mut reg = registry.write().await;
                if let Some(ep) = reg.get(&service_id) {
                    if ep.provider == provider.as_str() {
                        reg.remove(&service_id);
                        debug!("Received service withdrawal via gossip: {}", service_id);
                        icn_obs::metrics::service_discovery::gossip_withdrawals_received_inc();
                        Ok(())
                    } else {
                        Err(format!(
                            "Ignoring withdrawal from non-owner: {} (owner: {}, requestor: {})",
                            service_id, ep.provider, provider
                        ))
                    }
                } else {
                    Ok(())
                }
            }
            icn_gossip::ServiceDiscoveryMessage::Query {
                service_type,
                max_scope,
                required_capabilities,
                query_id,
                expires_at,
                ..
            } => {
                // Check if query has expired
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if now_secs > expires_at {
                    debug!(query_id = %query_id, "Ignoring expired query");
                    return Ok(());
                }

                // Only respond if we have a signing key
                let (signing_key, responder_did) =
                    match (own_signing_key.as_ref(), own_did.as_ref()) {
                        (Some(k), Some(d)) => (k.clone(), d.clone()),
                        _ => {
                            debug!("No signing key configured, cannot respond to query");
                            return Ok(());
                        }
                    };

                // Search local registry for matching endpoints
                let reg = registry.read().await;
                let matching: Vec<ServiceEndpoint> = reg
                    .values()
                    .filter(|ep| max_scope.includes(ep.scope_visibility))
                    .filter(|ep| {
                        if service_type.name == "*" {
                            true
                        } else {
                            ep.service_type.name == service_type.name
                        }
                    })
                    .filter(|ep| {
                        required_capabilities
                            .iter()
                            .all(|cap| ep.capabilities.contains(cap))
                    })
                    .filter(|ep| !ep.is_expired())
                    .cloned()
                    .collect();
                drop(reg);

                if matching.is_empty() {
                    debug!(query_id = %query_id, "No matching endpoints for query");
                    return Ok(());
                }

                // Build and sign response
                let response_expires = now_secs + 300; // 5 min TTL on responses
                let mut response = icn_gossip::ServiceDiscoveryMessage::Response {
                    query_id: query_id.clone(),
                    endpoints: matching,
                    responder: responder_did,
                    signature: Vec::new(),
                    expires_at: response_expires,
                    scope: max_scope,
                };

                if let Err(e) = icn_gossip::sign_service_response(&mut response, &signing_key) {
                    warn!(query_id = %query_id, error = %e, "Failed to sign response");
                    return Err(format!("Failed to sign response: {e}"));
                }

                // Publish response on query topic
                if let Some(gossip) = gossip_handle.as_ref() {
                    let encoded = icn_encoding::encode(&response)
                        .map_err(|e| format!("Failed to encode response: {e}"))?;
                    let topic = icn_gossip::service_discovery_topics::SERVICES_QUERY;
                    let mut gossip_guard = gossip.write().await;
                    gossip_guard
                        .publish(topic, encoded)
                        .await
                        .map_err(|e| format!("Failed to publish response: {e}"))?;
                    debug!(query_id = %query_id, "Published response to query");
                }

                Ok(())
            }
            ref response @ icn_gossip::ServiceDiscoveryMessage::Response {
                ref query_id,
                ref scope,
                ..
            } => {
                // Validate the response signature, TTL, scope
                if !icn_gossip::validate_service_response(response, scope) {
                    debug!(
                        query_id = %query_id,
                        "Dropping invalid/expired/unsigned response"
                    );
                    return Ok(());
                }

                // Extract endpoints for routing (after validation)
                let (qid, eps) = match msg {
                    icn_gossip::ServiceDiscoveryMessage::Response {
                        query_id,
                        endpoints,
                        ..
                    } => (query_id, endpoints),
                    _ => unreachable!(),
                };

                // Route to pending query if we have one
                if let Some(ref pq) = pending_queries {
                    let pq_read = pq.read().await;
                    if let Some(sender) = pq_read.get(&qid) {
                        if sender.try_send(eps).is_err() {
                            warn!(
                                query_id = %qid,
                                "Pending query response dropped: channel full or closed"
                            );
                        }
                    }
                }

                Ok(())
            }
        }
    }

    /// Register a service endpoint.
    ///
    /// Returns an error if the registry has reached its capacity limit
    /// (`MAX_REGISTRY_SIZE`) and the service_id is not already registered
    /// (updates to existing entries are always allowed).
    ///
    /// If gossip is enabled, the announcement is propagated to peers.
    pub async fn announce(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError> {
        let id = endpoint.service_id.clone();
        let mut reg = self.registry.write().await;
        if reg.len() >= MAX_REGISTRY_SIZE && !reg.contains_key(&id) {
            return Err(NamingError::RegistryFull(format!(
                "Service registry is full ({MAX_REGISTRY_SIZE} entries)"
            )));
        }
        reg.insert(id.clone(), endpoint.clone());
        drop(reg); // Release lock before gossip

        // Write-through to sled if configured
        if let Some(ref tree) = self.sled_tree {
            let encoded = serde_json::to_vec(&endpoint)
                .map_err(|e| NamingError::Internal(format!("Failed to serialize endpoint: {e}")))?;
            tree.insert(id.as_bytes(), encoded)
                .map_err(|e| NamingError::Internal(format!("Failed to persist endpoint: {e}")))?;
        }

        debug!("Service announced: {}", id);
        icn_obs::metrics::service_discovery::announcements_inc();

        // Propagate to gossip if enabled
        if let Some(ref gossip_handle) = self.gossip_handle {
            let msg = icn_gossip::ServiceDiscoveryMessage::Announce {
                endpoint: endpoint.clone(),
            };
            let encoded = icn_encoding::encode(&msg).map_err(|e| {
                NamingError::Internal(format!("Failed to encode gossip message: {e}"))
            })?;

            let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
            let mut gossip = gossip_handle.write().await;
            gossip
                .publish(topic, encoded)
                .await
                .map_err(|e| NamingError::Internal(format!("Failed to publish to gossip: {e}")))?;

            debug!("Service announcement propagated via gossip: {}", id);
        }

        Ok(())
    }

    /// Withdraw a service endpoint. Only the original provider may withdraw.
    ///
    /// If gossip is enabled, the withdrawal is propagated to peers.
    pub async fn withdraw(&self, service_id: &str, provider: &str) -> Result<(), NamingError> {
        let mut reg = self.registry.write().await;
        match reg.get(service_id) {
            Some(ep) if ep.provider == provider => {
                reg.remove(service_id);
                drop(reg); // Release lock before gossip

                // Remove from sled if configured
                if let Some(ref tree) = self.sled_tree {
                    let _ = tree.remove(service_id.as_bytes());
                }

                debug!("Service withdrawn: {}", service_id);
                icn_obs::metrics::service_discovery::withdrawals_inc();

                // Propagate to gossip if enabled
                if let Some(ref gossip_handle) = self.gossip_handle {
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| {
                            NamingError::Internal(format!(
                                "System time error while computing withdrawal timestamp: {e}"
                            ))
                        })?
                        .as_secs();

                    let provider_did = icn_identity::Did::from_str(provider).map_err(|e| {
                        NamingError::InvalidName(format!("Failed to parse provider DID: {e}"))
                    })?;

                    let msg = icn_gossip::ServiceDiscoveryMessage::Withdraw {
                        service_id: service_id.to_string(),
                        provider: provider_did,
                        timestamp,
                    };
                    let encoded = icn_encoding::encode(&msg).map_err(|e| {
                        NamingError::Internal(format!("Failed to encode gossip message: {e}"))
                    })?;

                    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
                    let mut gossip = gossip_handle.write().await;
                    gossip.publish(topic, encoded).await.map_err(|e| {
                        NamingError::Internal(format!("Failed to publish to gossip: {e}"))
                    })?;

                    debug!("Service withdrawal propagated via gossip: {}", service_id);
                }

                Ok(())
            }
            Some(_) => Err(NamingError::Unauthorized(
                "Only the provider can withdraw a service".to_string(),
            )),
            None => Err(NamingError::NotFound(service_id.to_string())),
        }
    }

    /// Discover service endpoints matching type and scope.
    pub async fn discover(
        &self,
        scope: ScopeLevel,
        service_type: Option<&ServiceType>,
        required_capabilities: &[String],
    ) -> Vec<ServiceEndpoint> {
        let reg = self.registry.read().await;
        reg.values()
            .filter(|ep| {
                // Scope filter: endpoint visible at requested scope or narrower
                scope.includes(ep.scope_visibility)
            })
            .filter(|ep| {
                // Type filter (if specified)
                match service_type {
                    Some(st) => ep.service_type.name == st.name,
                    None => true,
                }
            })
            .filter(|ep| {
                // Capability filter
                required_capabilities
                    .iter()
                    .all(|cap| ep.capabilities.contains(cap))
            })
            .filter(|ep| !ep.is_expired())
            .cloned()
            .collect()
    }

    /// Get a specific service endpoint by ID.
    pub async fn get(&self, service_id: &str) -> Option<ServiceEndpoint> {
        let reg = self.registry.read().await;
        reg.get(service_id).filter(|ep| !ep.is_expired()).cloned()
    }

    /// Remove all expired endpoints. Returns the number removed.
    pub async fn remove_expired(&self) -> usize {
        let mut reg = self.registry.write().await;
        let before = reg.len();
        let mut expired_ids = Vec::new();
        reg.retain(|id, ep| {
            if ep.is_expired() {
                expired_ids.push(id.clone());
                false
            } else {
                true
            }
        });
        let removed = before - reg.len();

        // Remove expired entries from sled
        if let Some(ref tree) = self.sled_tree {
            for id in &expired_ids {
                let _ = tree.remove(id.as_bytes());
            }
        }

        if removed > 0 {
            debug!("Removed {} expired service endpoints", removed);
            icn_obs::metrics::service_discovery::expired_removed_add(removed as u64);
        }
        icn_obs::metrics::service_discovery::registry_size_set(reg.len() as u64);
        removed
    }
}

impl Default for ServiceDiscoveryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `ScopedDiscovery` implementation using `blocking_read()`/`blocking_write()`.
///
/// **Important:** These methods must only be called from a blocking (non-async) context
/// or from within `tokio::task::spawn_blocking`. Calling them directly from an async
/// task risks deadlocking under high contention. For async callers, use the `announce()`,
/// `withdraw()`, `discover()`, and `get()` async methods directly.
///
/// In debug builds, these methods will log a warning if called from within an active
/// Tokio runtime context (which may indicate accidental use from an async task).
impl ScopedDiscovery for ServiceDiscoveryManager {
    fn announce_endpoint(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError> {
        debug_assert_blocking_context("announce_endpoint");
        let id = endpoint.service_id.clone();
        // Use blocking write since the trait is not async
        let mut reg = self.registry.blocking_write();
        if reg.len() >= MAX_REGISTRY_SIZE && !reg.contains_key(&id) {
            return Err(NamingError::RegistryFull(format!(
                "Service registry is full ({MAX_REGISTRY_SIZE} entries)"
            )));
        }
        reg.insert(id, endpoint);
        Ok(())
    }

    fn withdraw_endpoint(
        &self,
        service_id: &ServiceEndpointId,
        provider: &icn_kernel_api::types::Did,
    ) -> Result<(), NamingError> {
        debug_assert_blocking_context("withdraw_endpoint");
        let mut reg = self.registry.blocking_write();
        match reg.get(service_id) {
            Some(ep) if ep.provider == *provider => {
                reg.remove(service_id);
                Ok(())
            }
            Some(_) => Err(NamingError::Unauthorized(
                "Only the provider can withdraw a service".to_string(),
            )),
            None => Err(NamingError::NotFound(service_id.to_string())),
        }
    }

    fn discover_endpoints(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
    ) -> Result<Vec<ServiceEndpoint>, NamingError> {
        debug_assert_blocking_context("discover_endpoints");
        let reg = self.registry.blocking_read();
        Ok(reg
            .values()
            .filter(|ep| scope.includes(ep.scope_visibility))
            .filter(|ep| ep.service_type.name == service_type.name)
            .filter(|ep| !ep.is_expired())
            .cloned()
            .collect())
    }

    fn discover_endpoints_filtered(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
        required_capabilities: &[String],
    ) -> Result<Vec<ServiceEndpoint>, NamingError> {
        debug_assert_blocking_context("discover_endpoints_filtered");
        let reg = self.registry.blocking_read();
        Ok(reg
            .values()
            .filter(|ep| scope.includes(ep.scope_visibility))
            .filter(|ep| ep.service_type.name == service_type.name)
            .filter(|ep| {
                required_capabilities
                    .iter()
                    .all(|cap| ep.capabilities.contains(cap))
            })
            .filter(|ep| !ep.is_expired())
            .cloned()
            .collect())
    }

    fn get_endpoint(&self, service_id: &ServiceEndpointId) -> Result<ServiceEndpoint, NamingError> {
        debug_assert_blocking_context("get_endpoint");
        let reg = self.registry.blocking_read();
        reg.get(service_id)
            .filter(|ep| !ep.is_expired())
            .cloned()
            .ok_or_else(|| NamingError::NotFound(service_id.to_string()))
    }
}

/// `AsyncScopedDiscovery` implementation using native async methods.
///
/// This implementation delegates to the existing async methods (`announce()`,
/// `withdraw()`, `discover()`, `get()`), avoiding the need for `blocking_read()`
/// or `blocking_write()` calls in async contexts.
///
/// Use this trait when calling from async contexts (e.g., async HTTP handlers,
/// async actor methods). Use the sync `ScopedDiscovery` trait only from blocking
/// contexts or inside `tokio::task::spawn_blocking`.
#[async_trait::async_trait]
impl AsyncScopedDiscovery for ServiceDiscoveryManager {
    async fn announce_endpoint(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError> {
        self.announce(endpoint).await
    }

    async fn withdraw_endpoint(
        &self,
        service_id: &ServiceEndpointId,
        provider: &icn_kernel_api::types::Did,
    ) -> Result<(), NamingError> {
        self.withdraw(service_id, provider).await
    }

    async fn discover_endpoints(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
    ) -> Result<Vec<ServiceEndpoint>, NamingError> {
        Ok(self.discover(scope, Some(service_type), &[]).await)
    }

    async fn discover_endpoints_filtered(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
        required_capabilities: &[String],
    ) -> Result<Vec<ServiceEndpoint>, NamingError> {
        Ok(self
            .discover(scope, Some(service_type), required_capabilities)
            .await)
    }

    async fn get_endpoint(
        &self,
        service_id: &ServiceEndpointId,
    ) -> Result<ServiceEndpoint, NamingError> {
        self.get(service_id)
            .await
            .ok_or_else(|| NamingError::NotFound(service_id.to_string()))
    }
}

/// Start a background task that periodically removes expired endpoints.
pub fn start_expiry_task(
    manager: Arc<ServiceDiscoveryManager>,
    interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let removed = manager.remove_expired().await;
            if removed > 0 {
                info!(
                    "Service discovery expiry: removed {} expired endpoints",
                    removed
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::types::{Endpoint, Signature};

    fn make_gossip_entry(
        author: icn_identity::Did,
        data: Vec<u8>,
        topic: &str,
    ) -> icn_gossip::GossipEntry {
        // Simple hash for test entries (not cryptographic, just unique)
        let mut hash = [0u8; 32];
        for (i, byte) in data.iter().enumerate() {
            hash[i % 32] ^= byte;
        }
        icn_gossip::GossipEntry {
            hash,
            author,
            clock: icn_gossip::VectorClock::new(),
            topic: topic.to_string(),
            data,
            compressed: false,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            replica_offered: None,
        }
    }

    fn make_endpoint(id: &str, scope: ScopeLevel, ttl: u64) -> ServiceEndpoint {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        ServiceEndpoint {
            service_id: id.to_string(),
            provider: "did:icn:test".to_string(),
            endpoint_type: icn_kernel_api::naming::EndpointType::Http,
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![Endpoint::new("https", "example.com", 8080)],
            addresses: vec![],
            capabilities: vec!["read".to_string()],
            trust_threshold: 0.1,
            scope_visibility: scope,
            cell_id: None,
            ttl_secs: ttl,
            signature: Signature::new(vec![0; 64]),
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn test_announce_and_get() {
        let mgr = ServiceDiscoveryManager::new();
        let ep = make_endpoint("svc-1", ScopeLevel::Org, 3600);

        mgr.announce(ep.clone()).await.unwrap();

        let result = mgr.get("svc-1").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().service_id, "svc-1");
    }

    #[tokio::test]
    async fn test_withdraw() {
        let mgr = ServiceDiscoveryManager::new();
        let ep = make_endpoint("svc-1", ScopeLevel::Org, 3600);

        mgr.announce(ep).await.unwrap();
        mgr.withdraw("svc-1", "did:icn:test").await.unwrap();

        assert!(mgr.get("svc-1").await.is_none());
    }

    #[tokio::test]
    async fn test_withdraw_wrong_provider() {
        let mgr = ServiceDiscoveryManager::new();
        let ep = make_endpoint("svc-1", ScopeLevel::Org, 3600);

        mgr.announce(ep).await.unwrap();

        let result = mgr.withdraw("svc-1", "did:icn:other").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_discover_scope_filter() {
        let mgr = ServiceDiscoveryManager::new();
        mgr.announce(make_endpoint("local-svc", ScopeLevel::Local, 3600))
            .await
            .unwrap();
        mgr.announce(make_endpoint("org-svc", ScopeLevel::Org, 3600))
            .await
            .unwrap();
        mgr.announce(make_endpoint("fed-svc", ScopeLevel::Federation, 3600))
            .await
            .unwrap();

        // Org scope should include Local and Org, but not Federation
        let results = mgr.discover(ScopeLevel::Org, None, &[]).await;
        assert_eq!(results.len(), 2);

        // Federation scope should include all three
        let results = mgr.discover(ScopeLevel::Federation, None, &[]).await;
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_discover_capability_filter() {
        let mgr = ServiceDiscoveryManager::new();
        let mut ep = make_endpoint("svc-rw", ScopeLevel::Org, 3600);
        ep.capabilities = vec!["read".to_string(), "write".to_string()];
        mgr.announce(ep).await.unwrap();

        let mut ep2 = make_endpoint("svc-r", ScopeLevel::Org, 3600);
        ep2.capabilities = vec!["read".to_string()];
        mgr.announce(ep2).await.unwrap();

        let results = mgr
            .discover(
                ScopeLevel::Commons,
                None,
                &["read".to_string(), "write".to_string()],
            )
            .await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].service_id, "svc-rw");
    }

    #[tokio::test]
    async fn test_expired_endpoints_filtered() {
        let mgr = ServiceDiscoveryManager::new();
        let mut ep = make_endpoint("expired-svc", ScopeLevel::Org, 1);
        // Set updated_at to the past so it's expired
        ep.updated_at = 1000;
        mgr.announce(ep).await.unwrap();

        let results = mgr.discover(ScopeLevel::Commons, None, &[]).await;
        assert!(results.is_empty());

        assert!(mgr.get("expired-svc").await.is_none());
    }

    #[tokio::test]
    async fn test_remove_expired() {
        let mgr = ServiceDiscoveryManager::new();
        let mut ep = make_endpoint("expired-svc", ScopeLevel::Org, 1);
        ep.updated_at = 1000;
        mgr.announce(ep).await.unwrap();

        let fresh = make_endpoint("fresh-svc", ScopeLevel::Org, 3600);
        mgr.announce(fresh).await.unwrap();

        let removed = mgr.remove_expired().await;
        assert_eq!(removed, 1);

        // Fresh endpoint should still be there
        assert!(mgr.get("fresh-svc").await.is_some());
    }

    #[tokio::test]
    async fn test_async_scoped_discovery_announce() {
        // Test that AsyncScopedDiscovery trait works correctly
        let mgr = ServiceDiscoveryManager::new();
        let ep = make_endpoint("async-svc", ScopeLevel::Org, 3600);

        // Use the trait method
        use icn_kernel_api::naming::AsyncScopedDiscovery;
        AsyncScopedDiscovery::announce_endpoint(&mgr, ep)
            .await
            .unwrap();

        // Verify via trait method
        let service_id = "async-svc".to_string();
        let result = AsyncScopedDiscovery::get_endpoint(&mgr, &service_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().service_id, "async-svc");
    }

    #[tokio::test]
    async fn test_async_scoped_discovery_withdraw() {
        let mgr = ServiceDiscoveryManager::new();
        let ep = make_endpoint("async-svc", ScopeLevel::Org, 3600);

        use icn_kernel_api::naming::AsyncScopedDiscovery;
        AsyncScopedDiscovery::announce_endpoint(&mgr, ep)
            .await
            .unwrap();

        let service_id = "async-svc".to_string();
        let provider = "did:icn:test".to_string();
        AsyncScopedDiscovery::withdraw_endpoint(&mgr, &service_id, &provider)
            .await
            .unwrap();

        let result = AsyncScopedDiscovery::get_endpoint(&mgr, &service_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_scoped_discovery_discover_endpoints() {
        let mgr = ServiceDiscoveryManager::new();
        mgr.announce(make_endpoint("svc-1", ScopeLevel::Local, 3600))
            .await
            .unwrap();
        mgr.announce(make_endpoint("svc-2", ScopeLevel::Org, 3600))
            .await
            .unwrap();

        use icn_kernel_api::naming::AsyncScopedDiscovery;
        let service_type = ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        };

        let results =
            AsyncScopedDiscovery::discover_endpoints(&mgr, ScopeLevel::Org, &service_type)
                .await
                .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_async_scoped_discovery_discover_filtered() {
        let mgr = ServiceDiscoveryManager::new();
        let mut ep1 = make_endpoint("svc-rw", ScopeLevel::Org, 3600);
        ep1.capabilities = vec!["read".to_string(), "write".to_string()];
        mgr.announce(ep1).await.unwrap();

        let mut ep2 = make_endpoint("svc-r", ScopeLevel::Org, 3600);
        ep2.capabilities = vec!["read".to_string()];
        mgr.announce(ep2).await.unwrap();

        use icn_kernel_api::naming::AsyncScopedDiscovery;
        let service_type = ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        };

        let results = AsyncScopedDiscovery::discover_endpoints_filtered(
            &mgr,
            ScopeLevel::Commons,
            &service_type,
            &["read".to_string(), "write".to_string()],
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].service_id, "svc-rw");
    }

    // ========== B3: Remote Discovery Aggregator Tests (#1081) ==========

    #[tokio::test]
    async fn test_discover_remote_without_gossip_returns_error() {
        let mgr = ServiceDiscoveryManager::new();

        let result = mgr.discover_remote(ScopeLevel::Org, None, &[], None).await;

        assert!(result.is_err());
        match result {
            Err(NamingError::Internal(msg)) => {
                assert!(msg.contains("Gossip not configured"));
            }
            other => panic!("Expected Internal error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pending_queries_state_management() {
        let mgr = ServiceDiscoveryManager::new();

        // Verify initial state is empty
        let pq = mgr.pending_queries.read().await;
        assert!(pq.is_empty());
    }

    #[tokio::test]
    async fn test_remote_discover_outcome_variants() {
        // Verify RemoteDiscoverOutcome can represent all states
        let eps = vec![make_endpoint("svc-1", ScopeLevel::Org, 3600)];

        let results = RemoteDiscoverOutcome::Results(eps.clone());
        match results {
            RemoteDiscoverOutcome::Results(r) => assert_eq!(r.len(), 1),
            _ => panic!("Expected Results"),
        }

        let timeout = RemoteDiscoverOutcome::Timeout(eps);
        match timeout {
            RemoteDiscoverOutcome::Timeout(r) => assert_eq!(r.len(), 1),
            _ => panic!("Expected Timeout"),
        }

        let no_results = RemoteDiscoverOutcome::NoResults;
        assert!(matches!(no_results, RemoteDiscoverOutcome::NoResults));
    }

    #[tokio::test]
    async fn test_query_handling_responds_with_local_results() {
        // Test that handle_gossip_entry_internal processes Query messages
        // and responds with matching local endpoints
        let registry = Arc::new(RwLock::new(HashMap::new()));
        let ep = make_endpoint("local-svc", ScopeLevel::Org, 3600);
        {
            let mut reg = registry.write().await;
            reg.insert("local-svc".to_string(), ep);
        }

        // Build a Query message
        let kp = icn_identity::KeyPair::generate().unwrap();
        let query = icn_gossip::ServiceDiscoveryMessage::Query {
            requester: kp.did().clone(),
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            max_scope: ScopeLevel::Org,
            required_capabilities: vec![],
            query_id: "test-q-1".to_string(),
            expires_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 300,
        };

        let encoded = icn_encoding::encode(&query).unwrap();
        let entry = make_gossip_entry(kp.did().clone(), encoded, "services:query");

        // Without a signing key, should succeed but not generate a response
        let result = ServiceDiscoveryManager::handle_gossip_entry_internal(
            registry, None, None, None, None, entry,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_expired_query_ignored() {
        let registry = Arc::new(RwLock::new(HashMap::new()));
        let ep = make_endpoint("local-svc", ScopeLevel::Org, 3600);
        {
            let mut reg = registry.write().await;
            reg.insert("local-svc".to_string(), ep);
        }

        let kp = icn_identity::KeyPair::generate().unwrap();
        // Query expired 10 seconds ago
        let query = icn_gossip::ServiceDiscoveryMessage::Query {
            requester: kp.did().clone(),
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            max_scope: ScopeLevel::Org,
            required_capabilities: vec![],
            query_id: "expired-q".to_string(),
            expires_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 10,
        };

        let encoded = icn_encoding::encode(&query).unwrap();
        let entry = make_gossip_entry(kp.did().clone(), encoded, "services:query");

        let result = ServiceDiscoveryManager::handle_gossip_entry_internal(
            registry, None, None, None, None, entry,
        )
        .await;

        // Should succeed (expired queries are silently ignored)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_response_routed_to_pending_query() {
        let registry = Arc::new(RwLock::new(HashMap::new()));
        let pending = Arc::new(RwLock::new(HashMap::new()));

        // Set up a pending query with a channel
        let (tx, mut rx) = mpsc::channel::<Vec<ServiceEndpoint>>(8);
        {
            let mut pq = pending.write().await;
            pq.insert("route-test-q".to_string(), tx);
        }

        // Build a signed response
        let kp = icn_identity::KeyPair::generate().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes());
        let ep = make_endpoint("remote-svc", ScopeLevel::Org, 3600);

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut response = icn_gossip::ServiceDiscoveryMessage::Response {
            query_id: "route-test-q".to_string(),
            endpoints: vec![ep],
            responder: kp.did().clone(),
            signature: Vec::new(),
            expires_at: now_secs + 300,
            scope: ScopeLevel::Org,
        };
        icn_gossip::sign_service_response(&mut response, &signing_key).unwrap();

        let encoded = icn_encoding::encode(&response).unwrap();
        let entry = make_gossip_entry(kp.did().clone(), encoded, "services:query");

        let result = ServiceDiscoveryManager::handle_gossip_entry_internal(
            registry,
            Some(pending.clone()),
            None,
            None,
            None,
            entry,
        )
        .await;

        assert!(result.is_ok());

        // Check that the response was routed to the channel
        match rx.try_recv() {
            Ok(endpoints) => {
                assert_eq!(endpoints.len(), 1);
                assert_eq!(endpoints[0].service_id, "remote-svc");
            }
            Err(_) => panic!("Expected endpoints to be routed to pending query channel"),
        }
    }

    #[tokio::test]
    async fn test_unsigned_response_dropped() {
        let registry = Arc::new(RwLock::new(HashMap::new()));
        let pending = Arc::new(RwLock::new(HashMap::new()));

        let (tx, mut rx) = mpsc::channel::<Vec<ServiceEndpoint>>(8);
        {
            let mut pq = pending.write().await;
            pq.insert("unsigned-q".to_string(), tx);
        }

        // Build an unsigned response (empty signature)
        let kp = icn_identity::KeyPair::generate().unwrap();
        let ep = make_endpoint("unsigned-svc", ScopeLevel::Org, 3600);
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let response = icn_gossip::ServiceDiscoveryMessage::Response {
            query_id: "unsigned-q".to_string(),
            endpoints: vec![ep],
            responder: kp.did().clone(),
            signature: vec![0; 64], // Invalid signature
            expires_at: now_secs + 300,
            scope: ScopeLevel::Org,
        };

        let encoded = icn_encoding::encode(&response).unwrap();
        let entry = make_gossip_entry(kp.did().clone(), encoded, "services:query");

        let result = ServiceDiscoveryManager::handle_gossip_entry_internal(
            registry,
            Some(pending),
            None,
            None,
            None,
            entry,
        )
        .await;

        // Should succeed (invalid responses silently dropped)
        assert!(result.is_ok());

        // But nothing should have been sent to the channel
        assert!(
            rx.try_recv().is_err(),
            "Unsigned response should not reach pending query"
        );
    }

    #[tokio::test]
    async fn test_set_signing_key() {
        let mut mgr = ServiceDiscoveryManager::new();
        assert!(mgr.own_signing_key.is_none());

        let kp = icn_identity::KeyPair::generate().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes());
        mgr.set_signing_key(signing_key);

        assert!(mgr.own_signing_key.is_some());
    }

    // ========== N3: Sled Persistence Tests ==========

    #[tokio::test]
    async fn test_sled_persistence_announce_and_reload() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        // Create manager with sled, announce an endpoint
        {
            let mgr = ServiceDiscoveryManager::with_sled(&db).unwrap();
            let ep = make_endpoint("persist-svc", ScopeLevel::Org, 3600);
            mgr.announce(ep).await.unwrap();
        }

        // Create a new manager from the same sled — endpoint should be loaded
        let mgr2 = ServiceDiscoveryManager::with_sled(&db).unwrap();
        let result = mgr2.get("persist-svc").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().service_id, "persist-svc");
    }

    #[tokio::test]
    async fn test_sled_persistence_withdraw_removes_entry() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        let mgr = ServiceDiscoveryManager::with_sled(&db).unwrap();
        let ep = make_endpoint("withdraw-svc", ScopeLevel::Org, 3600);
        mgr.announce(ep).await.unwrap();
        mgr.withdraw("withdraw-svc", "did:icn:test").await.unwrap();

        // Reload from sled — should be gone
        let mgr2 = ServiceDiscoveryManager::with_sled(&db).unwrap();
        assert!(mgr2.get("withdraw-svc").await.is_none());
    }

    #[tokio::test]
    async fn test_sled_persistence_expired_cleaned_on_load() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        // Write an expired endpoint directly to sled
        {
            let mgr = ServiceDiscoveryManager::with_sled(&db).unwrap();
            let mut ep = make_endpoint("expired-persist", ScopeLevel::Org, 1);
            ep.updated_at = 1000; // Way in the past → expired
                                  // Bypass announce to force it into sled even though expired
            let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
            let encoded = serde_json::to_vec(&ep).unwrap();
            tree.insert(ep.service_id.as_bytes(), encoded).unwrap();
            // Also add a fresh one
            let fresh = make_endpoint("fresh-persist", ScopeLevel::Org, 3600);
            mgr.announce(fresh).await.unwrap();
        }

        // Reload — expired entry should be cleaned up
        let mgr2 = ServiceDiscoveryManager::with_sled(&db).unwrap();
        assert!(mgr2.get("expired-persist").await.is_none());
        assert!(mgr2.get("fresh-persist").await.is_some());

        // Verify sled is also cleaned
        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        assert!(tree.get("expired-persist").unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sled_remove_expired_cleans_sled() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        let mgr = ServiceDiscoveryManager::with_sled(&db).unwrap();
        let mut ep = make_endpoint("will-expire", ScopeLevel::Org, 1);
        ep.updated_at = 1000; // Already expired
                              // Insert directly into registry + sled to test remove_expired
        {
            let mut reg = mgr.registry.write().await;
            reg.insert("will-expire".to_string(), ep.clone());
        }
        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        let encoded = serde_json::to_vec(&ep).unwrap();
        tree.insert("will-expire".as_bytes(), encoded).unwrap();

        let removed = mgr.remove_expired().await;
        assert_eq!(removed, 1);

        // Verify sled is also cleaned
        assert!(tree.get("will-expire").unwrap().is_none());
    }

    // ========== N4: with_persistence() Tests ==========

    /// Attaching persistence to a fresh manager loads previously persisted endpoints.
    #[tokio::test]
    async fn test_with_persistence_loads_existing_endpoints() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        // Seed sled directly with a valid endpoint
        let ep = make_endpoint("pre-existing-svc", ScopeLevel::Org, 3600);
        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        tree.insert(ep.service_id.as_bytes(), serde_json::to_vec(&ep).unwrap())
            .unwrap();
        drop(tree);

        // Attach persistence to a freshly constructed manager
        let mut mgr = ServiceDiscoveryManager::new();
        mgr.with_persistence(&db).await.unwrap();

        // Previously persisted endpoint should be visible
        assert!(
            mgr.get("pre-existing-svc").await.is_some(),
            "with_persistence should load previously persisted endpoints"
        );
    }

    /// with_persistence merges with any in-memory entries already in the manager
    /// (e.g. from with_gossip bootstrapping).
    #[tokio::test]
    async fn test_with_persistence_merges_with_existing_memory() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        // Seed sled
        let sled_ep = make_endpoint("sled-svc", ScopeLevel::Org, 3600);
        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        tree.insert(
            sled_ep.service_id.as_bytes(),
            serde_json::to_vec(&sled_ep).unwrap(),
        )
        .unwrap();
        drop(tree);

        // Pre-populate registry in-memory
        let mut mgr = ServiceDiscoveryManager::new();
        let mem_ep = make_endpoint("memory-svc", ScopeLevel::Org, 3600);
        mgr.announce(mem_ep).await.unwrap();

        // Attach persistence
        mgr.with_persistence(&db).await.unwrap();

        // Both endpoints should be present
        assert!(
            mgr.get("sled-svc").await.is_some(),
            "sled entry should be loaded"
        );
        assert!(
            mgr.get("memory-svc").await.is_some(),
            "in-memory entry should survive"
        );
    }

    /// with_persistence prunes expired entries from sled on attach.
    #[tokio::test]
    async fn test_with_persistence_prunes_expired_on_attach() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        // Seed sled with one expired and one valid endpoint
        let mut expired_ep = make_endpoint("expired-svc", ScopeLevel::Org, 1);
        expired_ep.updated_at = 1000; // Far in the past → expired
        let valid_ep = make_endpoint("valid-svc", ScopeLevel::Org, 3600);

        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        tree.insert(
            expired_ep.service_id.as_bytes(),
            serde_json::to_vec(&expired_ep).unwrap(),
        )
        .unwrap();
        tree.insert(
            valid_ep.service_id.as_bytes(),
            serde_json::to_vec(&valid_ep).unwrap(),
        )
        .unwrap();
        drop(tree);

        let mut mgr = ServiceDiscoveryManager::new();
        mgr.with_persistence(&db).await.unwrap();

        // Expired entry should not be in memory
        assert!(mgr.get("expired-svc").await.is_none());
        // Valid entry should be
        assert!(mgr.get("valid-svc").await.is_some());
        // Expired entry should be gone from sled too
        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        assert!(
            tree.get("expired-svc").unwrap().is_none(),
            "expired entry should be pruned from sled"
        );
    }

    /// After with_persistence, announce() and withdraw() write-through to sled.
    #[tokio::test]
    async fn test_with_persistence_enables_write_through() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db = sled::open(tmpdir.path()).unwrap();

        let mut mgr = ServiceDiscoveryManager::new();
        mgr.with_persistence(&db).await.unwrap();

        // Announce a new endpoint
        let ep = make_endpoint("write-through-svc", ScopeLevel::Org, 3600);
        mgr.announce(ep).await.unwrap();

        // Verify it landed in sled
        let tree = db.open_tree(super::SLED_TREE_NAME).unwrap();
        assert!(
            tree.get("write-through-svc").unwrap().is_some(),
            "announce should write-through to sled"
        );

        // Withdraw it
        mgr.withdraw("write-through-svc", "did:icn:test")
            .await
            .unwrap();

        // Verify it was removed from sled
        assert!(
            tree.get("write-through-svc").unwrap().is_none(),
            "withdraw should remove from sled"
        );
    }
}
