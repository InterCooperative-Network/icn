//! Cooperative configuration for cooperative-specific settings

use serde::{Deserialize, Serialize};

/// Cooperative configuration for cooperative-specific settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CooperativeConfig {
    /// Treasury DID for the cooperative
    /// This DID is used as the source for budget payouts and other financial operations.
    /// If not set, the node's own DID will be used as a fallback.
    /// Format: "did:icn:<base58-pubkey>"
    #[serde(default)]
    pub treasury_did: Option<String>,

    /// Cooperative display name (human-readable)
    #[serde(default)]
    pub name: Option<String>,

    /// Cooperative description
    #[serde(default)]
    pub description: Option<String>,
}
