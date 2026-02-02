//! Service Discovery Manager
//!
//! In-memory registry for service endpoints with background expiry.

use icn_kernel_api::naming::{
    AsyncScopedDiscovery, NamingError, ScopedDiscovery, ServiceEndpoint, ServiceEndpointId,
    ServiceType,
};
use icn_kernel_api::scope::ScopeLevel;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Maximum number of service endpoints the registry will hold.
/// Prevents unbounded memory growth in long-running deployments.
const MAX_REGISTRY_SIZE: usize = 10_000;

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

/// In-memory service endpoint registry with TTL-based expiry.
#[derive(Clone)]
pub struct ServiceDiscoveryManager {
    /// service_id → ServiceEndpoint
    registry: Arc<RwLock<HashMap<String, ServiceEndpoint>>>,
}

impl ServiceDiscoveryManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a service endpoint.
    ///
    /// Returns an error if the registry has reached its capacity limit
    /// (`MAX_REGISTRY_SIZE`) and the service_id is not already registered
    /// (updates to existing entries are always allowed).
    pub async fn announce(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError> {
        let id = endpoint.service_id.clone();
        let mut reg = self.registry.write().await;
        if reg.len() >= MAX_REGISTRY_SIZE && !reg.contains_key(&id) {
            return Err(NamingError::RegistryFull(format!(
                "Service registry is full ({MAX_REGISTRY_SIZE} entries)"
            )));
        }
        reg.insert(id.clone(), endpoint);
        debug!("Service announced: {}", id);
        icn_obs::metrics::service_discovery::announcements_inc();
        Ok(())
    }

    /// Withdraw a service endpoint. Only the original provider may withdraw.
    pub async fn withdraw(&self, service_id: &str, provider: &str) -> Result<(), NamingError> {
        let mut reg = self.registry.write().await;
        match reg.get(service_id) {
            Some(ep) if ep.provider == provider => {
                reg.remove(service_id);
                debug!("Service withdrawn: {}", service_id);
                icn_obs::metrics::service_discovery::withdrawals_inc();
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
        reg.retain(|_, ep| !ep.is_expired());
        let removed = before - reg.len();
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

    fn make_endpoint(id: &str, scope: ScopeLevel, ttl: u64) -> ServiceEndpoint {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        ServiceEndpoint {
            service_id: id.to_string(),
            provider: "did:icn:test".to_string(),
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![Endpoint::new("https", "example.com", 8080)],
            capabilities: vec!["read".to_string()],
            trust_threshold: 0.1,
            scope_visibility: scope,
            ttl_secs: ttl,
            signature: Signature::new(vec![0; 64]),
            created_at: now,
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
        // Set created_at to the past so it's expired
        ep.created_at = 1000;
        mgr.announce(ep).await.unwrap();

        let results = mgr.discover(ScopeLevel::Commons, None, &[]).await;
        assert!(results.is_empty());

        assert!(mgr.get("expired-svc").await.is_none());
    }

    #[tokio::test]
    async fn test_remove_expired() {
        let mgr = ServiceDiscoveryManager::new();
        let mut ep = make_endpoint("expired-svc", ScopeLevel::Org, 1);
        ep.created_at = 1000;
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
        AsyncScopedDiscovery::announce_endpoint(&mgr, ep).await.unwrap();

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
        AsyncScopedDiscovery::announce_endpoint(&mgr, ep).await.unwrap();
        
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

        let results = AsyncScopedDiscovery::discover_endpoints(&mgr, ScopeLevel::Org, &service_type)
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
}
