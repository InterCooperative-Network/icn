//! Naming & Discovery Primitive
//!
//! Provides name resolution and service discovery.
//!
//! # Design
//!
//! Names are hierarchical and scoped (e.g., `/org-123/apps/ledger`).
//! Name records are signed by their authority, enabling verification
//! without trusting the resolution path.
//!
//! # Why Naming Matters
//!
//! Without naming, apps hardcode addresses. With naming:
//! - Apps compose via stable names that survive scaling
//! - Failover is transparent (name points to healthy endpoint)
//! - Federation discovery works across org boundaries
//!
//! # Non-Goals
//!
//! - Human-readable name policies (apps define these)
//! - Dispute resolution (just enforces signatures)
//! - Global namespace (scoped by design)

use crate::scope::{CellId, ScopeLevel};
use crate::types::{
    Did, Duration, Endpoint, Hash, Name, Namespace, Scope, Signature, Subscription,
};
use serde::{Deserialize, Serialize};

/// Target that a name resolves to.
#[derive(Clone, Debug)]
pub enum Target {
    /// Service endpoint
    Service { endpoint: Endpoint },
    /// Content-addressed blob
    Blob { hash: Hash },
    /// Namespace reference
    Namespace { ns: Namespace },
    /// Alias to another name
    Alias { name: Name },
    /// Multiple endpoints (for load balancing)
    MultiService { endpoints: Vec<Endpoint> },
}

/// Name record with authority signature.
#[derive(Clone, Debug)]
pub struct NameRecord {
    /// The name being registered
    pub name: Name,
    /// What the name resolves to
    pub target: Target,
    /// Who controls this name
    pub authority: Did,
    /// Proof of authority's signature
    pub signature: Signature,
    /// Caching hint (how long to cache)
    pub ttl: Duration,
    /// When this record was created
    pub created_at: u64,
    /// When this record was last updated
    pub updated_at: u64,
    /// Optional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Name resolution options.
#[derive(Clone, Debug)]
pub struct ResolveOptions {
    /// Maximum recursion depth for aliases
    pub max_depth: Option<u32>,
    /// Whether to verify signatures
    pub verify_signatures: bool,
    /// Scope to search in
    pub scope: Option<Scope>,
    /// Return cached result if available
    pub allow_cached: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolveOptions {
    /// Create default options.
    pub fn new() -> Self {
        Self {
            max_depth: Some(10),
            verify_signatures: true,
            scope: None,
            allow_cached: true,
        }
    }

    /// Set maximum alias recursion depth.
    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Disable signature verification (use with caution).
    pub fn without_signature_verification(mut self) -> Self {
        self.verify_signatures = false;
        self
    }

    /// Limit resolution to a specific scope.
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Require fresh resolution (no cache).
    pub fn no_cache(mut self) -> Self {
        self.allow_cached = false;
        self
    }
}

/// Service type for discovery.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ServiceType {
    /// Service type name (e.g., "ledger", "governance")
    pub name: String,
    /// Protocol version
    pub version: String,
}

/// Service announcement for discovery.
#[derive(Clone, Debug)]
pub struct ServiceAnnouncement {
    /// Type of service
    pub service_type: ServiceType,
    /// Where the service is available
    pub endpoint: Endpoint,
    /// Who is providing this service
    pub provider: Did,
    /// How long this announcement is valid
    pub ttl: Duration,
    /// Service capabilities/metadata
    pub capabilities: Vec<String>,
}

/// Unique identifier for a service endpoint registration.
pub type ServiceEndpointId = String;

/// Type of network endpoint for a service.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EndpointType {
    /// QUIC transport
    Quic,
    /// HTTP/HTTPS endpoint
    Http,
    /// gRPC endpoint
    Grpc,
    /// WebSocket endpoint
    WebSocket,
}

impl EndpointType {
    /// Convert to canonical byte representation for signing payloads.
    ///
    /// These values are part of the signing protocol and MUST NOT change
    /// without incrementing the signing payload version.
    #[inline]
    pub fn to_canonical_byte(&self) -> u8 {
        match self {
            EndpointType::Quic => 0,
            EndpointType::Http => 1,
            EndpointType::Grpc => 2,
            EndpointType::WebSocket => 3,
        }
    }
}

/// Service endpoint with signing, scope awareness, and multi-endpoint support.
///
/// Complements the existing `ServiceAnnouncement` with:
/// - Ed25519 signature for authenticity
/// - `ScopeLevel` for hierarchical visibility
/// - Multiple endpoints per service
/// - TTL-based expiry
///
/// # Example
///
/// ```
/// use icn_kernel_api::{ServiceEndpoint, EndpointType, ScopeLevel};
/// use icn_kernel_api::naming::ServiceType;
/// use icn_kernel_api::types::Signature;
///
/// // Get current timestamp
/// let now = std::time::SystemTime::now()
///     .duration_since(std::time::UNIX_EPOCH)
///     .unwrap()
///     .as_secs();
///
/// // Create a service endpoint
/// let endpoint = ServiceEndpoint {
///     service_id: "ledger-svc-1".to_string(),
///     provider: "did:icn:alice".to_string(),
///     endpoint_type: EndpointType::Grpc,
///     service_type: ServiceType {
///         name: "ledger".to_string(),
///         version: "1.0".to_string(),
///     },
///     endpoints: vec![],
///     addresses: vec!["grpc://ledger.coop.local:50051".to_string()],
///     capabilities: vec!["read".to_string(), "write".to_string()],
///     trust_threshold: 0.5,
///     scope_visibility: ScopeLevel::Org,
///     cell_id: None,
///     ttl_secs: 3600,
///     signature: Signature::new(vec![0; 64]), // Should be actual Ed25519 signature
///     created_at: now,
///     updated_at: now,
/// };
///
/// // Check if endpoint is stale
/// assert!(!endpoint.is_stale());
///
/// // Compute signing payload (for signing before creation)
/// let payload = endpoint.signing_payload();
/// assert!(!payload.is_empty());
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServiceEndpoint {
    /// Unique identifier for this service registration
    pub service_id: ServiceEndpointId,
    /// DID of the service provider
    pub provider: Did,
    /// Type of endpoint (transport protocol)
    pub endpoint_type: EndpointType,
    /// Type of service being provided
    pub service_type: ServiceType,
    /// Network endpoints where the service is reachable
    pub endpoints: Vec<Endpoint>,
    /// Simple string addresses (alternative to structured endpoints)
    pub addresses: Vec<String>,
    /// Capabilities offered by this service
    pub capabilities: Vec<String>,
    /// Minimum trust score required to access this service
    pub trust_threshold: f64,
    /// Scope at which this service is visible
    pub scope_visibility: ScopeLevel,
    /// Optional cell identifier for cell-scoped services
    pub cell_id: Option<CellId>,
    /// Time-to-live in seconds
    pub ttl_secs: u64,
    /// Ed25519 signature over the signing payload
    pub signature: Signature,
    /// Unix timestamp of creation
    pub created_at: u64,
    /// Unix timestamp of last update
    pub updated_at: u64,
}

/// Append a length-prefixed string to the payload.
///
/// Writes a 4-byte little-endian length prefix followed by the string bytes.
/// This prevents ambiguity attacks where different field values could produce
/// identical concatenated payloads (e.g., `"ab" + "cd"` vs `"a" + "bcd"`).
fn extend_with_length(payload: &mut Vec<u8>, s: &str) {
    payload.extend_from_slice(&(s.len() as u32).to_le_bytes());
    payload.extend_from_slice(s.as_bytes());
}

impl ServiceEndpoint {
    /// Compute deterministic signing payload.
    ///
    /// The payload is a byte concatenation of all fields except the signature,
    /// following the `ComputeResult` pattern for canonical signing.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();

        // All variable-length strings are length-prefixed (u32 LE) to prevent
        // ambiguity attacks where different field values produce identical payloads.
        extend_with_length(&mut payload, &self.service_id);
        extend_with_length(&mut payload, &self.provider);

        // Endpoint type (fixed 1 byte)
        payload.push(self.endpoint_type.to_canonical_byte());

        extend_with_length(&mut payload, &self.service_type.name);
        extend_with_length(&mut payload, &self.service_type.version);

        // Endpoints: encode count then each endpoint
        payload.extend_from_slice(&(self.endpoints.len() as u32).to_le_bytes());
        for ep in &self.endpoints {
            extend_with_length(&mut payload, &ep.protocol);
            extend_with_length(&mut payload, &ep.host);
            payload.extend_from_slice(&ep.port.to_le_bytes());
            if let Some(ref path) = ep.path {
                payload.push(1);
                extend_with_length(&mut payload, path);
            } else {
                payload.push(0);
            }
        }

        // Addresses: sorted for determinism
        let mut sorted_addrs = self.addresses.clone();
        sorted_addrs.sort();
        payload.extend_from_slice(&(sorted_addrs.len() as u32).to_le_bytes());
        for addr in &sorted_addrs {
            extend_with_length(&mut payload, addr);
        }

        // Capabilities: sorted for determinism
        let mut sorted_caps = self.capabilities.clone();
        sorted_caps.sort();
        payload.extend_from_slice(&(sorted_caps.len() as u32).to_le_bytes());
        for cap in &sorted_caps {
            extend_with_length(&mut payload, cap);
        }

        payload.extend_from_slice(&self.trust_threshold.to_le_bytes());
        payload.push(self.scope_visibility.as_u8());

        // Cell ID: optional, fixed 32 bytes (no length prefix needed)
        if let Some(ref cell_id) = self.cell_id {
            payload.push(1);
            payload.extend_from_slice(&cell_id.0);
        } else {
            payload.push(0);
        }

        payload.extend_from_slice(&self.ttl_secs.to_le_bytes());
        payload.extend_from_slice(&self.created_at.to_le_bytes());
        payload.extend_from_slice(&self.updated_at.to_le_bytes());
        payload
    }

    /// Check if this endpoint registration has expired (alias for `is_stale()` for backward compatibility).
    pub fn is_expired(&self) -> bool {
        self.is_stale()
    }

    /// Check if this endpoint registration is stale (TTL expired).
    ///
    /// Returns `true` if the current time exceeds `updated_at + ttl_secs`.
    pub fn is_stale(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now > self.updated_at.saturating_add(self.ttl_secs)
    }
}

/// Scope-aware service discovery using `ScopeLevel`.
///
/// Complements the existing `Discovery` trait with 5-level scope support.
/// Does not replace `Discovery` — both can coexist.
pub trait ScopedDiscovery: Send + Sync {
    /// Announce a service endpoint.
    fn announce_endpoint(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError>;

    /// Withdraw a service endpoint.
    fn withdraw_endpoint(
        &self,
        service_id: &ServiceEndpointId,
        provider: &Did,
    ) -> Result<(), NamingError>;

    /// Discover service endpoints at a given scope level and type.
    fn discover_endpoints(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
    ) -> Result<Vec<ServiceEndpoint>, NamingError>;

    /// Discover service endpoints with capability filtering.
    fn discover_endpoints_filtered(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
        required_capabilities: &[String],
    ) -> Result<Vec<ServiceEndpoint>, NamingError>;

    /// Get a specific service endpoint by ID.
    fn get_endpoint(&self, service_id: &ServiceEndpointId) -> Result<ServiceEndpoint, NamingError>;
}

/// Async scope-aware service discovery using `ScopeLevel`.
///
/// Provides async variants of `ScopedDiscovery` methods for use in async contexts.
/// Implementations that already use async primitives (like `tokio::sync::RwLock`)
/// can implement this trait to avoid forcing `blocking_read()`/`blocking_write()`
/// workarounds.
///
/// # Relationship to ScopedDiscovery
///
/// This trait mirrors `ScopedDiscovery` but with async methods. Types can implement
/// both traits if they want to support both sync and async callers.
///
/// - Use `ScopedDiscovery` when called from blocking/sync contexts
/// - Use `AsyncScopedDiscovery` when called from async contexts
///
/// # When to Implement
///
/// Implement this trait when:
/// - Your implementation uses `tokio::sync::RwLock` or other async primitives
/// - You want to avoid `blocking_read()`/`blocking_write()` in async contexts
/// - You need to perform async I/O during discovery operations
///
/// If your implementation is purely in-memory with sync locks (`parking_lot::RwLock`),
/// prefer implementing only `ScopedDiscovery`.
///
/// # Implementation Note
///
/// This trait uses `#[async_trait]` for `Send` futures and dyn-compatibility.
/// Implementations must also add `#[async_trait]` to their impl blocks:
///
/// ```rust,ignore
/// #[async_trait::async_trait]
/// impl AsyncScopedDiscovery for MyDiscovery {
///     async fn announce_endpoint(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError> {
///         // ...
///     }
///     // ... other methods
/// }
/// ```
// #[async_trait] provides Send bounds on futures for dyn-compatibility with trait objects
#[async_trait::async_trait]
pub trait AsyncScopedDiscovery: Send + Sync {
    /// Announce a service endpoint (async).
    async fn announce_endpoint(&self, endpoint: ServiceEndpoint) -> Result<(), NamingError>;

    /// Withdraw a service endpoint (async).
    async fn withdraw_endpoint(
        &self,
        service_id: &ServiceEndpointId,
        provider: &Did,
    ) -> Result<(), NamingError>;

    /// Discover service endpoints at a given scope level and type (async).
    async fn discover_endpoints(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
    ) -> Result<Vec<ServiceEndpoint>, NamingError>;

    /// Discover service endpoints with capability filtering (async).
    async fn discover_endpoints_filtered(
        &self,
        scope: ScopeLevel,
        service_type: &ServiceType,
        required_capabilities: &[String],
    ) -> Result<Vec<ServiceEndpoint>, NamingError>;

    /// Get a specific service endpoint by ID (async).
    async fn get_endpoint(
        &self,
        service_id: &ServiceEndpointId,
    ) -> Result<ServiceEndpoint, NamingError>;
}

/// Naming service for name registration and resolution.
pub trait NamingService: Send + Sync {
    /// Register a name.
    ///
    /// The authority must sign the registration.
    fn register(
        &self,
        name: &Name,
        target: Target,
        authority: &Did,
        signature: &Signature,
        ttl: Duration,
    ) -> Result<NameRecord, NamingError>;

    /// Resolve a name to its target.
    fn resolve(&self, name: &Name) -> Result<Target, NamingError>;

    /// Resolve with options.
    fn resolve_with_options(
        &self,
        name: &Name,
        options: ResolveOptions,
    ) -> Result<(Target, NameRecord), NamingError>;

    /// Update an existing name record.
    fn update(
        &self,
        name: &Name,
        new_target: Target,
        signature: &Signature,
    ) -> Result<NameRecord, NamingError>;

    /// Delete a name record.
    fn delete(&self, name: &Name, signature: &Signature) -> Result<(), NamingError>;

    /// Get the full name record.
    fn get_record(&self, name: &Name) -> Result<NameRecord, NamingError>;

    /// List names under a prefix.
    fn list(&self, prefix: &Name) -> Result<Vec<Name>, NamingError>;

    /// Watch for changes to a name.
    fn watch(&self, name: &Name) -> Result<Subscription, NamingError>;

    /// Verify a name record's signature.
    fn verify(&self, record: &NameRecord) -> Result<bool, NamingError>;
}

/// Service discovery.
pub trait Discovery: Send + Sync {
    /// Announce a service.
    fn announce(&self, scope: Scope, announcement: ServiceAnnouncement) -> Result<(), NamingError>;

    /// Withdraw a service announcement.
    fn withdraw(
        &self,
        scope: Scope,
        service_type: &ServiceType,
        endpoint: &Endpoint,
    ) -> Result<(), NamingError>;

    /// Discover services of a given type.
    fn discover(
        &self,
        scope: Scope,
        service_type: &ServiceType,
    ) -> Result<Vec<ServiceAnnouncement>, NamingError>;

    /// Discover services with filtering.
    fn discover_filtered(
        &self,
        scope: Scope,
        service_type: &ServiceType,
        required_capabilities: &[String],
    ) -> Result<Vec<ServiceAnnouncement>, NamingError>;

    /// Watch for new services of a type.
    fn watch_services(
        &self,
        scope: Scope,
        service_type: &ServiceType,
    ) -> Result<Subscription, NamingError>;
}

/// Federation capability advertisement.
///
/// Coops advertise their federation capabilities to enable
/// cross-org discovery and interoperability.
#[derive(Clone, Debug)]
pub struct FederationCapabilities {
    /// Organization DID
    pub org: Did,
    /// Economic bridge endpoint
    pub economic_bridge: Option<Endpoint>,
    /// Trust bridge endpoint
    pub trust_bridge: Option<Endpoint>,
    /// Governance bridge endpoint
    pub governance_bridge: Option<Endpoint>,
    /// Supported federation protocols
    pub supported_protocols: Vec<String>,
    /// Supported denominations (for economic bridging)
    pub supported_denominations: Vec<String>,
}

/// Errors from naming operations.
#[derive(Debug, thiserror::Error)]
pub enum NamingError {
    /// Name not found
    #[error("Name not found: {0}")]
    NotFound(String),

    /// Name already registered
    #[error("Name already registered: {0}")]
    AlreadyExists(String),

    /// Invalid signature
    #[error("Invalid signature for name: {0}")]
    InvalidSignature(String),

    /// Not authorized to modify name
    #[error("Not authorized to modify: {0}")]
    Unauthorized(String),

    /// Too many alias redirects
    #[error("Too many redirects (max: {0})")]
    TooManyRedirects(u32),

    /// Invalid name format
    #[error("Invalid name format: {0}")]
    InvalidName(String),

    /// Service type not found
    #[error("Service type not found: {0}")]
    ServiceNotFound(String),

    /// Resolution timeout
    #[error("Resolution timeout")]
    Timeout,

    /// Registry is at capacity
    #[error("Registry full: {0}")]
    RegistryFull(String),

    /// Internal error
    #[error("Naming error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_variants() {
        let service = Target::Service {
            endpoint: Endpoint::new("https", "example.com", 8080),
        };
        match service {
            Target::Service { endpoint } => {
                assert_eq!(endpoint.host, "example.com");
            }
            _ => panic!("Expected Service"),
        }

        let alias = Target::Alias {
            name: Name::new("/other/name"),
        };
        match alias {
            Target::Alias { name } => assert_eq!(name.as_str(), "/other/name"),
            _ => panic!("Expected Alias"),
        }
    }

    #[test]
    fn test_resolve_options() {
        let opts = ResolveOptions::new()
            .with_max_depth(5)
            .with_scope(Scope::Org)
            .no_cache();

        assert_eq!(opts.max_depth, Some(5));
        assert_eq!(opts.scope, Some(Scope::Org));
        assert!(!opts.allow_cached);
        assert!(opts.verify_signatures);
    }

    #[test]
    fn test_service_type() {
        let st = ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        };
        assert_eq!(st.name, "ledger");
        assert_eq!(st.version, "1.0");
    }

    #[test]
    fn test_service_endpoint_signing_payload_deterministic() {
        let ep = ServiceEndpoint {
            service_id: "svc-1".to_string(),
            provider: "did:icn:alice".to_string(),
            endpoint_type: EndpointType::Http,
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![Endpoint::new("https", "example.com", 8080)],
            addresses: vec![],
            capabilities: vec!["read".to_string(), "write".to_string()],
            trust_threshold: 0.1,
            scope_visibility: ScopeLevel::Org,
            cell_id: None,
            ttl_secs: 3600,
            signature: Signature::new(vec![0; 64]),
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let payload1 = ep.signing_payload();
        let payload2 = ep.signing_payload();
        assert_eq!(payload1, payload2);
        assert!(!payload1.is_empty());
    }

    #[test]
    fn test_service_endpoint_signing_payload_caps_sorted() {
        let make_ep = |caps: Vec<String>| ServiceEndpoint {
            service_id: "svc-1".to_string(),
            provider: "did:icn:alice".to_string(),
            endpoint_type: EndpointType::Quic,
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![],
            addresses: vec![],
            capabilities: caps,
            trust_threshold: 0.0,
            scope_visibility: ScopeLevel::Local,
            cell_id: None,
            ttl_secs: 60,
            signature: Signature::new(vec![]),
            created_at: 0,
            updated_at: 0,
        };

        let ep1 = make_ep(vec!["b".to_string(), "a".to_string()]);
        let ep2 = make_ep(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(ep1.signing_payload(), ep2.signing_payload());
    }

    #[test]
    fn test_service_endpoint_is_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let fresh = ServiceEndpoint {
            service_id: "svc-1".to_string(),
            provider: "did:icn:alice".to_string(),
            endpoint_type: EndpointType::WebSocket,
            service_type: ServiceType {
                name: "test".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![],
            addresses: vec![],
            capabilities: vec![],
            trust_threshold: 0.0,
            scope_visibility: ScopeLevel::Local,
            cell_id: None,
            ttl_secs: 3600,
            signature: Signature::new(vec![]),
            created_at: now,
            updated_at: now,
        };
        assert!(!fresh.is_expired());
        assert!(!fresh.is_stale());

        let expired = ServiceEndpoint {
            updated_at: now - 7200,
            ttl_secs: 3600,
            ..fresh.clone()
        };
        assert!(expired.is_expired());
        assert!(expired.is_stale());
    }

    #[test]
    fn test_service_endpoint_serde_roundtrip() {
        let ep = ServiceEndpoint {
            service_id: "svc-1".to_string(),
            provider: "did:icn:alice".to_string(),
            endpoint_type: EndpointType::Grpc,
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![Endpoint::new("https", "example.com", 443)],
            addresses: vec!["https://example.com:443".to_string()],
            capabilities: vec!["read".to_string()],
            trust_threshold: 0.5,
            scope_visibility: ScopeLevel::Federation,
            cell_id: None,
            ttl_secs: 1800,
            signature: Signature::new(vec![1, 2, 3]),
            created_at: 1700000000,
            updated_at: 1700000100,
        };

        let json = serde_json::to_string(&ep).unwrap();
        let parsed: ServiceEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(ep, parsed);
    }

    #[test]
    fn test_naming_error_display() {
        let err = NamingError::TooManyRedirects(10);
        assert!(err.to_string().contains("10"));

        let err = NamingError::NotFound("/org/app".to_string());
        assert!(err.to_string().contains("/org/app"));
    }

    // NOTE: Signature verification tests are in icn-gossip/src/service_discovery.rs
    // which properly handles DID-based key resolution per kernel/app separation.
    // These tests verify the signing payload functionality only.

    #[test]
    fn test_signing_payload_changes_on_tamper() {
        // Create an endpoint
        let ep = ServiceEndpoint {
            service_id: "svc-tamper-test".to_string(),
            provider: "did:icn:alice".to_string(),
            endpoint_type: EndpointType::Http,
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![],
            addresses: vec!["http://example.com:8080".to_string()],
            capabilities: vec!["read".to_string()],
            trust_threshold: 0.5,
            scope_visibility: ScopeLevel::Org,
            cell_id: None,
            ttl_secs: 3600,
            signature: Signature::new(vec![0; 64]),
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let original_payload = ep.signing_payload();

        // Tamper with the address
        let mut tampered = ep.clone();
        tampered.addresses = vec!["http://malicious.com:8080".to_string()];

        // Payload should be different after tampering
        let tampered_payload = tampered.signing_payload();
        assert_ne!(
            original_payload, tampered_payload,
            "Tampered endpoint should produce different signing payload"
        );
    }

    #[test]
    fn test_endpoint_type_variants() {
        let types = vec![
            EndpointType::Quic,
            EndpointType::Http,
            EndpointType::Grpc,
            EndpointType::WebSocket,
        ];

        // Ensure serde works
        for endpoint_type in types {
            let json = serde_json::to_string(&endpoint_type).unwrap();
            let parsed: EndpointType = serde_json::from_str(&json).unwrap();
            assert_eq!(endpoint_type, parsed);
        }
    }

    #[test]
    fn test_signing_payload_includes_cell_id() {
        let cell_id = CellId::derive(b"org123", "cell-alpha", &[42u8; 32]);

        let ep_with_cell = ServiceEndpoint {
            service_id: "svc-cell-test".to_string(),
            provider: "did:icn:bob".to_string(),
            endpoint_type: EndpointType::Grpc,
            service_type: ServiceType {
                name: "storage".to_string(),
                version: "2.0".to_string(),
            },
            endpoints: vec![],
            addresses: vec!["grpc://storage.local:5000".to_string()],
            capabilities: vec!["write".to_string(), "read".to_string()],
            trust_threshold: 0.7,
            scope_visibility: ScopeLevel::Cell,
            cell_id: Some(cell_id),
            ttl_secs: 1800,
            signature: Signature::new(vec![0; 64]),
            created_at: 1700000000,
            updated_at: 1700000500,
        };

        let mut ep_without_cell = ep_with_cell.clone();
        ep_without_cell.cell_id = None;

        // Payloads should differ when cell_id changes
        let payload_with = ep_with_cell.signing_payload();
        let payload_without = ep_without_cell.signing_payload();

        assert_ne!(
            payload_with, payload_without,
            "cell_id should affect signing payload"
        );
    }
}
