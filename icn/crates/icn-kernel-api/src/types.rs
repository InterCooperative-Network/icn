//! Common types used across all kernel primitives.
//!
//! These types form the shared vocabulary for kernel operations.
//! They are intentionally generic and domain-agnostic.

use serde::{Deserialize, Serialize};

/// Decentralized Identifier (e.g., `did:icn:z6Mk...`)
pub type Did = String;

/// Node identifier in the network
pub type NodeId = String;

/// Organization identifier
pub type OrgId = String;

/// Application identifier
pub type AppId = String;

/// Logical timestamp for ordering events
pub type LogicalTimestamp = u64;

/// Version number for CAS (Compare-And-Swap) operations
pub type Version = u64;

/// Offset position in a log
pub type Offset = u64;

/// Content-addressed hash for blobs (32 bytes, typically Blake3 or SHA-256)
pub type Hash = [u8; 32];

/// Generic key for KV storage
pub type Key = Vec<u8>;

/// Generic value for KV storage
pub type Value = Vec<u8>;

/// Capability identifier
pub type CapabilityId = String;

/// Log identifier
pub type LogId = String;

/// CRDT identifier
pub type CrdtId = String;

/// Service identifier
pub type ServiceId = String;

/// Job identifier for compute tasks
pub type JobId = String;

/// Topic identifier for pub/sub
pub type TopicId = String;

/// Message identifier
pub type MessageId = String;

/// Schedule identifier
pub type ScheduleId = String;

/// Callback identifier
pub type CallbackId = String;

/// Handler identifier
pub type HandlerId = String;

/// Group identifier for consensus
pub type GroupId = String;

/// Resource identifier
pub type ResourceId = String;

/// Namespace for isolation and access control.
///
/// Namespaces follow the pattern `/<org>/<app>/<sub>/` and provide
/// isolation boundaries for state, compute, and communication.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Namespace {
    /// Organization that owns this namespace
    pub org: String,
    /// Application within the organization
    pub app: String,
    /// Optional sub-namespace for further organization
    pub sub: Option<String>,
}

impl Namespace {
    /// Create a new namespace for an org and app.
    pub fn new(org: impl Into<String>, app: impl Into<String>) -> Self {
        Self {
            org: org.into(),
            app: app.into(),
            sub: None,
        }
    }

    /// Add a sub-namespace.
    pub fn with_sub(mut self, sub: impl Into<String>) -> Self {
        self.sub = Some(sub.into());
        self
    }

    /// Convert to path string (e.g., "/org/app/sub").
    pub fn to_path(&self) -> String {
        match &self.sub {
            Some(sub) => format!("/{}/{}/{}", self.org, self.app, sub),
            None => format!("/{}/{}", self.org, self.app),
        }
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_path())
    }
}

/// Reference to a schema definition.
///
/// Schemas are identified by name and version, allowing for
/// schema evolution while maintaining compatibility.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchemaRef {
    /// Schema name (e.g., "transfer-event")
    pub name: String,
    /// Schema version (semantic versioning)
    pub version: String,
}

impl SchemaRef {
    /// Create a new schema reference.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// Cryptographic signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(pub Vec<u8>);

impl Signature {
    /// Create a new signature from bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Get the signature bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Key identifier within a keystore.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyId(pub String);

impl KeyId {
    /// Create a new key identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for KeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Hierarchical name for naming service.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Name(pub String);

impl Name {
    /// Create a new name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Network endpoint for service communication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Protocol (e.g., "quic", "https")
    pub protocol: String,
    /// Host address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Optional path
    pub path: Option<String>,
}

impl Endpoint {
    /// Create a new endpoint.
    pub fn new(protocol: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        Self {
            protocol: protocol.into(),
            host: host.into(),
            port,
            path: None,
        }
    }

    /// Add a path to the endpoint.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// Path for request routing.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Path(pub String);

impl Path {
    /// Create a new path.
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Get the path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Scope for discovery and naming.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    /// Local to this node only
    Local,
    /// Within the organization
    Org,
    /// Across federated organizations
    Federation,
}

/// Duration type for time-related operations.
pub type Duration = std::time::Duration;

/// Subscription handle for event streams.
#[derive(Clone, Debug)]
pub struct Subscription {
    /// Unique identifier for this subscription
    pub id: String,
}

/// Lease for time-limited resource access.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lease {
    /// Lease identifier
    pub id: String,
    /// Resource being leased
    pub resource: ResourceId,
    /// Expiration timestamp
    pub expires_at: LogicalTimestamp,
}

/// Bidirectional stream handle.
#[derive(Clone, Debug)]
pub struct BidirectionalStream {
    /// Stream identifier
    pub id: String,
}

/// Leader lease for coordination.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaderLease {
    /// The node holding the lease
    pub holder: NodeId,
    /// Group this lease is for
    pub group: GroupId,
    /// When the lease expires
    pub expires_at: LogicalTimestamp,
}

/// Decision from a consensus group.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    /// The decided value
    pub value: Vec<u8>,
    /// Sequence number of this decision
    pub sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_path() {
        let ns = Namespace::new("my-org", "my-app");
        assert_eq!(ns.to_path(), "/my-org/my-app");

        let ns_with_sub = ns.with_sub("data");
        assert_eq!(ns_with_sub.to_path(), "/my-org/my-app/data");
    }

    #[test]
    fn test_schema_ref() {
        let schema = SchemaRef::new("transfer-event", "1.0.0");
        assert_eq!(schema.name, "transfer-event");
        assert_eq!(schema.version, "1.0.0");
    }

    #[test]
    fn test_endpoint() {
        let ep = Endpoint::new("https", "example.com", 8080);
        assert_eq!(ep.protocol, "https");
        assert_eq!(ep.host, "example.com");
        assert_eq!(ep.port, 8080);
        assert!(ep.path.is_none());

        let ep_with_path = ep.with_path("/api/v1");
        assert_eq!(ep_with_path.path, Some("/api/v1".to_string()));
    }
}
