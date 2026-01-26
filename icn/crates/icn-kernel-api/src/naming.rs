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

use crate::types::{
    Did, Duration, Endpoint, Hash, Name, Namespace, Scope, Signature, Subscription,
};

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
#[derive(Clone, Debug)]
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
    fn test_naming_error_display() {
        let err = NamingError::TooManyRedirects(10);
        assert!(err.to_string().contains("10"));

        let err = NamingError::NotFound("/org/app".to_string());
        assert!(err.to_string().contains("/org/app"));
    }
}
