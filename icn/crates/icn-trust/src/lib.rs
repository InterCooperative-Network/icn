//! ICN Trust - Trust graph management and policy enforcement

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Trust classification for a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustClass {
    /// Not yet evaluated
    Isolated,
    /// Known but not trusted
    Known,
    /// Trusted partner
    Partner,
    /// Federated peer (high trust)
    Federated,
}

/// A trust edge in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEdge {
    pub source: Did,
    pub target: Did,
    pub label: String,
    pub score: f64,
    pub expiry: Option<u64>,
}

/// Trust graph manager
pub struct TrustGraph {
    // Stub - will be implemented with store backend
}

impl TrustGraph {
    pub fn new() -> Self {
        TrustGraph {}
    }

    /// Compute the trust class for a given DID
    pub fn trust_class(&self, _did: &Did) -> TrustClass {
        // Stub implementation
        TrustClass::Isolated
    }
}

impl Default for TrustGraph {
    fn default() -> Self {
        Self::new()
    }
}
