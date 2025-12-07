//! Federated DID Resolution (Phase F5)
//!
//! Extends DID resolution to work across federation boundaries using
//! the cooperative prefix format: `did:icn:coop-id:z6Mk...`

use crate::error::{FederationError, Result};
use crate::metrics;
use crate::registry::CooperativeRegistry;
use icn_identity::Did;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Default cache size for resolved DIDs
pub const DEFAULT_CACHE_SIZE: usize = 1000;

/// Default cache TTL in seconds
pub const DEFAULT_CACHE_TTL_SECS: u64 = 300;

/// A cached DID resolution result
#[derive(Debug, Clone)]
pub struct CachedDidResolution {
    /// The resolved DID information
    pub did: Did,

    /// The cooperative that owns this DID
    pub coop_id: Option<String>,

    /// When this entry was cached
    pub cached_at: Instant,

    /// The gateway endpoint used for resolution (if federated)
    pub resolved_via: Option<String>,
}

impl CachedDidResolution {
    /// Create a new cached resolution
    pub fn new(did: Did, coop_id: Option<String>, resolved_via: Option<String>) -> Self {
        Self {
            did,
            coop_id,
            cached_at: Instant::now(),
            resolved_via,
        }
    }

    /// Check if the cache entry has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }
}

/// Resolver for federated DIDs
///
/// Handles DID resolution across cooperative boundaries. For DIDs with
/// a cooperative prefix (`did:icn:coop-id:z6Mk...`), it routes the
/// resolution request to the appropriate cooperative's gateway.
pub struct FederatedDidResolver {
    /// The cooperative registry for looking up gateway endpoints
    registry: Arc<CooperativeRegistry>,

    /// LRU cache for resolved DIDs
    cache: RwLock<LruCache<String, CachedDidResolution>>,

    /// Cache TTL
    cache_ttl: Duration,

    /// Our own cooperative ID (for local resolution)
    own_coop_id: Option<String>,
}

impl FederatedDidResolver {
    /// Create a new federated DID resolver
    pub fn new(registry: Arc<CooperativeRegistry>) -> Self {
        Self::with_cache_size(registry, DEFAULT_CACHE_SIZE)
    }

    /// Create a resolver with a specific cache size
    pub fn with_cache_size(registry: Arc<CooperativeRegistry>, cache_size: usize) -> Self {
        let cache_size = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            registry,
            cache: RwLock::new(LruCache::new(cache_size)),
            cache_ttl: Duration::from_secs(DEFAULT_CACHE_TTL_SECS),
            own_coop_id: None,
        }
    }

    /// Set the cache TTL
    pub fn set_cache_ttl(&mut self, ttl: Duration) {
        self.cache_ttl = ttl;
    }

    /// Set our own cooperative ID
    pub fn set_own_coop_id(&mut self, coop_id: String) {
        self.own_coop_id = Some(coop_id);
    }

    /// Parse a federated DID to extract the cooperative ID
    ///
    /// Federated DID format: `did:icn:coop-id:z6Mk...`
    /// Standard DID format: `did:icn:z6Mk...`
    pub fn parse_federated_did(did_str: &str) -> Option<(String, String)> {
        if !did_str.starts_with("did:icn:") {
            return None;
        }

        let suffix = &did_str[8..]; // Remove "did:icn:"
        let parts: Vec<&str> = suffix.split(':').collect();

        if parts.len() == 2 {
            // Federated format: coop-id:key
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            // Standard format: just key (no coop prefix)
            None
        }
    }

    /// Check if a DID is federated (has coop prefix)
    pub fn is_federated_did(did_str: &str) -> bool {
        Self::parse_federated_did(did_str).is_some()
    }

    /// Get the cooperative ID from a federated DID
    pub fn extract_coop_id(did_str: &str) -> Option<String> {
        Self::parse_federated_did(did_str).map(|(coop_id, _)| coop_id)
    }

    /// Construct a federated DID from a standard DID and coop ID
    pub fn construct_federated_did(did: &Did, coop_id: &str) -> String {
        // Extract the key part from the DID
        let did_str = did.to_string();
        if let Some(key) = did_str.strip_prefix("did:icn:") {
            format!("did:icn:{coop_id}:{key}")
        } else {
            // Fallback - just append
            format!("did:icn:{coop_id}:{did_str}")
        }
    }

    /// Resolve a DID, potentially across federation boundaries
    pub async fn resolve(&self, did_str: &str) -> Result<CachedDidResolution> {
        // Check cache first
        {
            let mut cache = self.cache.write().unwrap();
            if let Some(cached) = cache.get(did_str) {
                if !cached.is_expired(self.cache_ttl) {
                    metrics::did_resolution::cache_hits_inc();
                    return Ok(cached.clone());
                }
            }
        }

        metrics::did_resolution::cache_misses_inc();

        // Determine if this is a federated DID
        let resolution = if let Some((coop_id, _key)) = Self::parse_federated_did(did_str) {
            // Federated DID - need to resolve via the cooperative's gateway
            self.resolve_federated(did_str, &coop_id).await?
        } else {
            // Local DID - resolve directly
            self.resolve_local(did_str).await?
        };

        // Cache the result
        {
            let mut cache = self.cache.write().unwrap();
            cache.put(did_str.to_string(), resolution.clone());
        }

        Ok(resolution)
    }

    /// Resolve a local DID
    async fn resolve_local(&self, did_str: &str) -> Result<CachedDidResolution> {
        metrics::did_resolution::total_inc(None);

        let did =
            Did::from_str(did_str).map_err(|e| FederationError::InvalidDidFormat(e.to_string()))?;

        Ok(CachedDidResolution::new(
            did,
            self.own_coop_id.clone(),
            None,
        ))
    }

    /// Resolve a federated DID via the cooperative's gateway
    async fn resolve_federated(&self, did_str: &str, coop_id: &str) -> Result<CachedDidResolution> {
        metrics::did_resolution::total_inc(Some(coop_id));

        // Check if it's our own cooperative
        if let Some(ref own_coop) = self.own_coop_id {
            if own_coop == coop_id {
                // Extract the key from federated format and resolve locally
                let local_did_str = if let Some((_, key)) = Self::parse_federated_did(did_str) {
                    format!("did:icn:{key}")
                } else {
                    did_str.to_string()
                };
                return self.resolve_local(&local_did_str).await;
            }
        }

        // Look up the cooperative's gateway endpoint
        let coop_info = self
            .registry
            .get(coop_id)?
            .ok_or_else(|| FederationError::CooperativeNotFound(coop_id.to_string()))?;

        if coop_info.gateway_endpoints.is_empty() {
            return Err(FederationError::DidResolutionFailed(format!(
                "No gateway endpoints for cooperative: {coop_id}"
            )));
        }

        // Try each gateway endpoint
        for endpoint in &coop_info.gateway_endpoints {
            match self.query_gateway(endpoint, did_str).await {
                Ok(did) => {
                    debug!(
                        "Resolved federated DID {} via gateway {}",
                        did_str, endpoint
                    );
                    return Ok(CachedDidResolution::new(
                        did,
                        Some(coop_id.to_string()),
                        Some(endpoint.to_string()),
                    ));
                }
                Err(e) => {
                    warn!("Failed to resolve DID via gateway {}: {}", endpoint, e);
                    continue;
                }
            }
        }

        metrics::did_resolution::failures_inc("no_gateway_available");
        Err(FederationError::DidResolutionFailed(format!(
            "Failed to resolve DID {did_str} via any gateway"
        )))
    }

    /// Query a gateway endpoint for DID resolution
    ///
    /// Makes an HTTP GET request to the remote gateway's identity resolution endpoint.
    /// The endpoint format is: `{gateway_endpoint}/v1/identity/resolve/{did}`
    async fn query_gateway(&self, endpoint: &str, did_str: &str) -> Result<Did> {
        // Extract the key from federated format to get the standard DID
        let standard_did_str = if let Some((_, key)) = Self::parse_federated_did(did_str) {
            format!("did:icn:{key}")
        } else {
            did_str.to_string()
        };

        // Build the resolution URL
        let url = format!(
            "{}/v1/identity/resolve/{}",
            endpoint.trim_end_matches('/'),
            urlencoding::encode(&standard_did_str)
        );

        debug!("Querying gateway for DID resolution: {}", url);

        // Create HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| FederationError::DidResolutionFailed(format!("HTTP client error: {e}")))?;

        // Make the request
        let response = client.get(&url).send().await.map_err(|e| {
            warn!("Failed to query gateway {}: {}", endpoint, e);
            FederationError::DidResolutionFailed(format!("Gateway request failed: {e}"))
        })?;

        // Check response status
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(
                "Gateway returned error status {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            );
            return Err(FederationError::DidResolutionFailed(format!(
                "Gateway returned status {status}"
            )));
        }

        // Parse response
        #[derive(serde::Deserialize)]
        struct DidResolutionResponse {
            did: String,
            valid: bool,
        }

        let resolution: DidResolutionResponse = response.json().await.map_err(|e| {
            FederationError::DidResolutionFailed(format!("Failed to parse gateway response: {e}"))
        })?;

        if !resolution.valid {
            return Err(FederationError::DidResolutionFailed(format!(
                "Gateway reports DID as invalid: {}",
                resolution.did
            )));
        }

        // Parse and return the DID
        Did::from_str(&resolution.did).map_err(|e| FederationError::InvalidDidFormat(e.to_string()))
    }

    /// Clear the resolution cache
    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get the current cache size
    pub fn cache_size(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// Remove expired entries from cache
    pub fn evict_expired(&self) -> usize {
        let mut cache = self.cache.write().unwrap();
        let ttl = self.cache_ttl;

        // Collect keys to remove (can't modify while iterating)
        let expired_keys: Vec<String> = cache
            .iter()
            .filter(|(_, v)| v.is_expired(ttl))
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            cache.pop(&key);
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CooperativeInfo, FederationPolicy};
    use icn_identity::KeyPair;
    use icn_store::{SledStore, Store};

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_coop(id: &str) -> CooperativeInfo {
        CooperativeInfo::new(
            id.to_string(),
            format!("{id} Cooperative"),
            test_did(),
            FederationPolicy::Open,
        )
    }

    fn create_test_registry() -> Arc<CooperativeRegistry> {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let own_info = create_test_coop("test-coop");
        Arc::new(CooperativeRegistry::new(store, own_info).unwrap())
    }

    #[test]
    fn test_parse_federated_did() {
        // Federated format
        let result = FederatedDidResolver::parse_federated_did(
            "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
        );
        assert!(result.is_some());
        let (coop_id, key) = result.unwrap();
        assert_eq!(coop_id, "food-coop");
        assert_eq!(key, "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");

        // Standard format (not federated)
        let result = FederatedDidResolver::parse_federated_did(
            "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
        );
        assert!(result.is_none());

        // Invalid format
        let result = FederatedDidResolver::parse_federated_did("not-a-did");
        assert!(result.is_none());
    }

    #[test]
    fn test_is_federated_did() {
        assert!(FederatedDidResolver::is_federated_did(
            "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
        ));
        assert!(!FederatedDidResolver::is_federated_did(
            "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
        ));
    }

    #[test]
    fn test_extract_coop_id() {
        let coop_id = FederatedDidResolver::extract_coop_id(
            "did:icn:tech-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
        );
        assert_eq!(coop_id, Some("tech-coop".to_string()));

        let coop_id = FederatedDidResolver::extract_coop_id(
            "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
        );
        assert!(coop_id.is_none());
    }

    #[test]
    fn test_construct_federated_did() {
        let did = test_did();
        let federated = FederatedDidResolver::construct_federated_did(&did, "food-coop");
        // Should start with "did:icn:food-coop:" and contain the key from the DID
        assert!(federated.starts_with("did:icn:food-coop:"));
        let did_str = did.to_string();
        let key = did_str.strip_prefix("did:icn:").unwrap();
        assert!(federated.ends_with(key));
    }

    #[tokio::test]
    async fn test_resolve_local_did() {
        let registry = create_test_registry();
        let resolver = FederatedDidResolver::new(registry);

        // Use a generated DID for resolution
        let did = test_did();
        let result = resolver.resolve(&did.to_string()).await;
        assert!(result.is_ok());

        let resolution = result.unwrap();
        assert!(resolution.coop_id.is_none());
        assert!(resolution.resolved_via.is_none());
    }

    #[tokio::test]
    async fn test_resolve_federated_did_own_coop() {
        let registry = create_test_registry();
        let mut resolver = FederatedDidResolver::new(registry);
        resolver.set_own_coop_id("my-coop".to_string());

        // Construct a federated DID for our own coop
        let did = test_did();
        let federated_did = FederatedDidResolver::construct_federated_did(&did, "my-coop");

        // Resolving our own coop's DID should work locally
        let result = resolver.resolve(&federated_did).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_federated_did_unknown_coop() {
        let registry = create_test_registry();
        let resolver = FederatedDidResolver::new(registry);

        // Construct a federated DID for an unknown coop
        let did = test_did();
        let federated_did = FederatedDidResolver::construct_federated_did(&did, "unknown-coop");

        // Resolving unknown coop's DID should fail
        let result = resolver.resolve(&federated_did).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cache_behavior() {
        let registry = create_test_registry();
        let resolver = FederatedDidResolver::new(registry);

        // Use a generated DID string
        let did = test_did();
        let did_str = did.to_string();

        // First resolution
        let _ = resolver.resolve(&did_str).await.unwrap();
        assert_eq!(resolver.cache_size(), 1);

        // Second resolution should hit cache
        let _ = resolver.resolve(&did_str).await.unwrap();
        assert_eq!(resolver.cache_size(), 1);

        // Clear cache
        resolver.clear_cache();
        assert_eq!(resolver.cache_size(), 0);
    }

    #[tokio::test]
    async fn test_resolve_federated_did_with_gateway() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Start a mock server
        let mock_server = MockServer::start().await;

        // Create a test DID
        let did = test_did();
        let did_str = did.to_string();

        // Set up the mock response
        Mock::given(method("GET"))
            .and(path_regex(r"/v1/identity/resolve/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "did": did_str,
                "valid": true,
                "coop_id": "food-coop",
                "has_attestations": false
            })))
            .mount(&mock_server)
            .await;

        let registry = create_test_registry();
        let resolver = FederatedDidResolver::new(registry.clone());

        // Register a cooperative with the mock server as gateway
        let coop = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        )
        .with_gateway(mock_server.uri());

        registry.register(coop).unwrap();

        // Construct a federated DID for that coop
        let federated_did = FederatedDidResolver::construct_federated_did(&did, "food-coop");

        // Now resolve a DID from that coop
        let result = resolver.resolve(&federated_did).await;

        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert_eq!(resolution.coop_id, Some("food-coop".to_string()));
        assert!(resolution.resolved_via.is_some());
    }

    #[tokio::test]
    async fn test_resolve_federated_did_gateway_error() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Start a mock server
        let mock_server = MockServer::start().await;

        // Set up the mock to return an error
        Mock::given(method("GET"))
            .and(path_regex(r"/v1/identity/resolve/.*"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "Invalid DID format"
            })))
            .mount(&mock_server)
            .await;

        let registry = create_test_registry();
        let resolver = FederatedDidResolver::new(registry.clone());

        // Register a cooperative with the mock server as gateway
        let coop = CooperativeInfo::new(
            "error-coop".to_string(),
            "Error Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        )
        .with_gateway(mock_server.uri());

        registry.register(coop).unwrap();

        // Construct a federated DID
        let did = test_did();
        let federated_did = FederatedDidResolver::construct_federated_did(&did, "error-coop");

        // Resolution should fail
        let result = resolver.resolve(&federated_did).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_federated_did_gateway_timeout() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Start a mock server
        let mock_server = MockServer::start().await;

        // Set up the mock to delay (longer than timeout)
        Mock::given(method("GET"))
            .and(path_regex(r"/v1/identity/resolve/.*"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(15)))
            .mount(&mock_server)
            .await;

        let registry = create_test_registry();
        let resolver = FederatedDidResolver::new(registry.clone());

        // Register a cooperative with the mock server as gateway
        let coop = CooperativeInfo::new(
            "slow-coop".to_string(),
            "Slow Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        )
        .with_gateway(mock_server.uri());

        registry.register(coop).unwrap();

        // Construct a federated DID
        let did = test_did();
        let federated_did = FederatedDidResolver::construct_federated_did(&did, "slow-coop");

        // Resolution should fail due to timeout
        let result = resolver.resolve(&federated_did).await;
        assert!(result.is_err());
    }
}
