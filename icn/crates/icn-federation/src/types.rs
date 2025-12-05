//! Core federation types
//!
//! This module defines the fundamental types used throughout the federation layer.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Information about a federated cooperative
///
/// This represents a cooperative's public identity and federation metadata.
/// Cooperatives announce this information via the `federation:registry` gossip topic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CooperativeInfo {
    /// Unique identifier for the cooperative (e.g., "food-coop", "tech-coop")
    pub coop_id: String,

    /// Human-readable name of the cooperative
    pub name: String,

    /// The cooperative's institutional DID (signs on behalf of the coop)
    pub public_did: Did,

    /// Gateway API endpoints for this cooperative
    pub gateway_endpoints: Vec<String>,

    /// Federation policy determining how other coops can join
    pub federation_policy: FederationPolicy,

    /// Currencies supported by this cooperative
    pub currencies: Vec<CurrencyInfo>,

    /// Capability flags (e.g., "clearing", "attestations", "compute")
    pub capabilities: Vec<String>,

    /// Unix timestamp when this cooperative was last seen
    pub last_seen: u64,

    /// Ed25519 signature over the serialized content (excluding signature field)
    pub signature: Vec<u8>,
}

impl CooperativeInfo {
    /// Create a new CooperativeInfo (unsigned)
    pub fn new(
        coop_id: String,
        name: String,
        public_did: Did,
        federation_policy: FederationPolicy,
    ) -> Self {
        Self {
            coop_id,
            name,
            public_did,
            gateway_endpoints: Vec::new(),
            federation_policy,
            currencies: Vec::new(),
            capabilities: Vec::new(),
            last_seen: current_timestamp(),
            signature: Vec::new(),
        }
    }

    /// Add a gateway endpoint
    pub fn with_gateway(mut self, endpoint: String) -> Self {
        self.gateway_endpoints.push(endpoint);
        self
    }

    /// Add a supported currency
    pub fn with_currency(mut self, currency: CurrencyInfo) -> Self {
        self.currencies.push(currency);
        self
    }

    /// Add a capability
    pub fn with_capability(mut self, capability: &str) -> Self {
        self.capabilities.push(capability.to_string());
        self
    }

    /// Get bytes to sign (excludes signature field)
    pub fn signing_bytes(&self) -> Vec<u8> {
        // Create a version without signature for signing
        let mut signable = self.clone();
        signable.signature = Vec::new();
        serde_json::to_vec(&signable).unwrap_or_default()
    }

    /// Sign this cooperative info using the provided keypair
    ///
    /// Returns a new CooperativeInfo with the signature field populated.
    pub fn sign(mut self, keypair: &icn_identity::KeyPair) -> Self {
        let bytes = self.signing_bytes();
        let signature = keypair.sign(&bytes);
        self.signature = signature.to_vec();
        self
    }

    /// Verify the signature on this cooperative info
    ///
    /// Returns Ok(()) if the signature is valid, Err if invalid or missing.
    pub fn verify_signature(&self) -> Result<(), String> {
        if self.signature.is_empty() {
            return Err("Missing signature".to_string());
        }

        let verifying_key = self
            .public_did
            .to_verifying_key()
            .map_err(|e| format!("Failed to extract public key from DID: {e}"))?;

        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .map_err(|e| format!("Invalid signature format: {e}"))?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|e| format!("Signature verification failed: {e}"))
    }

    /// Check if this cooperative supports a specific capability
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// Federation policy determining how cooperatives can join
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type")]
pub enum FederationPolicy {
    /// Any cooperative can federate (no restrictions)
    #[default]
    Open,

    /// Requires vouches from existing federation partners
    Vouched {
        /// Minimum number of vouches required
        min_vouches: u8,
    },

    /// Federation is closed - no new cooperatives accepted
    Closed,
}

impl FederationPolicy {
    /// Create a Vouched policy with the specified minimum vouches
    pub fn vouched(min_vouches: u8) -> Self {
        FederationPolicy::Vouched { min_vouches }
    }

    /// Check if federation is possible under this policy
    pub fn allows_federation(&self) -> bool {
        !matches!(self, FederationPolicy::Closed)
    }
}

/// Information about a currency supported by a cooperative
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CurrencyInfo {
    /// Currency symbol (e.g., "hours", "USD", "kWh")
    pub symbol: String,

    /// Human-readable currency name
    pub name: String,

    /// Number of decimal places
    pub decimals: u8,
}

impl CurrencyInfo {
    /// Create a new CurrencyInfo
    pub fn new(symbol: &str, name: &str, decimals: u8) -> Self {
        Self {
            symbol: symbol.to_string(),
            name: name.to_string(),
            decimals,
        }
    }

    /// Common currency: Hours (mutual credit time banking)
    pub fn hours() -> Self {
        Self::new("hours", "Labor Hours", 2)
    }

    /// Common currency: USD equivalent
    pub fn usd() -> Self {
        Self::new("USD", "US Dollars", 2)
    }
}

/// Gossip messages for federation coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FederationMessage {
    /// Announce cooperative existence and metadata
    CoopAnnounce(CooperativeInfo),

    /// Query for cooperative(s) - None means "list all"
    CoopQuery {
        /// Specific coop_id to query, or None for all
        coop_id: Option<String>,
    },

    /// Response to a CoopQuery
    CoopResponse {
        /// List of cooperative info
        cooperatives: Vec<CooperativeInfo>,
    },

    /// Vouch for another cooperative
    Vouch(Vouch),

    /// Request to federate with a cooperative
    FederationRequest {
        /// The requesting cooperative's info
        requester: CooperativeInfo,
    },

    /// Accept a federation request
    FederationAccept {
        /// The accepting cooperative's coop_id
        accepter_coop_id: String,
        /// The requesting cooperative's coop_id
        requester_coop_id: String,
        /// Signature of acceptance
        signature: Vec<u8>,
    },

    /// Reject a federation request
    FederationReject {
        /// The rejecting cooperative's coop_id
        rejecter_coop_id: String,
        /// The requesting cooperative's coop_id
        requester_coop_id: String,
        /// Reason for rejection
        reason: String,
    },
}

/// A vouch from one cooperative for another
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Vouch {
    /// The cooperative doing the vouching
    pub voucher_coop_id: String,

    /// The cooperative's DID (for signature verification)
    pub voucher_did: Did,

    /// The cooperative being vouched for
    pub target_coop_id: String,

    /// Trust score assigned by the vouching cooperative (0.0-1.0)
    pub trust_score: f64,

    /// Unix timestamp when the vouch was created
    pub timestamp: u64,

    /// Optional expiry timestamp (vouches can be time-limited)
    pub expires_at: Option<u64>,

    /// Signature over (voucher_coop_id, target_coop_id, trust_score, timestamp, expires_at)
    pub signature: Vec<u8>,
}

impl Vouch {
    /// Create a new Vouch with trust score (unsigned)
    pub fn new(
        voucher_coop_id: String,
        voucher_did: Did,
        target_coop_id: String,
        trust_score: f64,
    ) -> Self {
        Self {
            voucher_coop_id,
            voucher_did,
            target_coop_id,
            trust_score: trust_score.clamp(0.0, 1.0),
            timestamp: current_timestamp(),
            expires_at: None,
            signature: Vec::new(),
        }
    }

    /// Set an expiry time
    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set the trust score (clamped to 0.0-1.0)
    pub fn with_trust_score(mut self, trust_score: f64) -> Self {
        self.trust_score = trust_score.clamp(0.0, 1.0);
        self
    }

    /// Get bytes to sign
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(self.voucher_coop_id.as_bytes());
        bytes.extend(self.target_coop_id.as_bytes());
        bytes.extend(self.trust_score.to_le_bytes());
        bytes.extend(self.timestamp.to_le_bytes());
        if let Some(exp) = self.expires_at {
            bytes.extend(exp.to_le_bytes());
        }
        bytes
    }

    /// Sign this vouch using the provided keypair
    ///
    /// Returns a new Vouch with the signature field populated.
    pub fn sign(mut self, keypair: &icn_identity::KeyPair) -> Self {
        let bytes = self.signing_bytes();
        let signature = keypair.sign(&bytes);
        self.signature = signature.to_vec();
        self
    }

    /// Verify the signature on this vouch
    ///
    /// Returns Ok(()) if the signature is valid, Err if invalid or missing.
    pub fn verify_signature(&self) -> Result<(), String> {
        if self.signature.is_empty() {
            return Err("Missing signature".to_string());
        }

        let verifying_key = self
            .voucher_did
            .to_verifying_key()
            .map_err(|e| format!("Failed to extract public key from DID: {e}"))?;

        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .map_err(|e| format!("Invalid signature format: {e}"))?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|e| format!("Signature verification failed: {e}"))
    }

    /// Check if the vouch has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            current_timestamp() > expires_at
        } else {
            false
        }
    }
}

/// Result of checking federation policy
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyResult {
    /// Federation allowed
    Allowed,

    /// Requires vouches - returns how many more are needed
    NeedsVouches { required: u8, current: u8 },

    /// Federation is closed
    Closed,

    /// Other policy violation
    Denied(String),
}

impl PolicyResult {
    /// Check if federation is allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, PolicyResult::Allowed)
    }
}

/// Federation status for a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationStatus {
    /// Whether federation is enabled
    pub enabled: bool,

    /// Own cooperative ID
    pub coop_id: String,

    /// Own cooperative name
    pub coop_name: String,

    /// Current federation policy
    pub policy: FederationPolicy,

    /// Number of known cooperatives
    pub known_coops: usize,

    /// Number of active federation channels
    pub active_channels: usize,

    /// Number of pending federation requests
    pub pending_requests: usize,
}

/// Helper function to get current Unix timestamp
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_cooperative_info_builder() {
        let did = test_did();

        let coop = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            did,
            FederationPolicy::Open,
        )
        .with_gateway("https://food-coop.example.com:8080".to_string())
        .with_currency(CurrencyInfo::hours())
        .with_capability("clearing");

        assert_eq!(coop.coop_id, "food-coop");
        assert_eq!(coop.gateway_endpoints.len(), 1);
        assert_eq!(coop.currencies.len(), 1);
        assert!(coop.has_capability("clearing"));
        assert!(!coop.has_capability("compute"));
    }

    #[test]
    fn test_federation_policy() {
        assert!(FederationPolicy::Open.allows_federation());
        assert!(FederationPolicy::vouched(2).allows_federation());
        assert!(!FederationPolicy::Closed.allows_federation());
    }

    #[test]
    fn test_vouch_expiry() {
        let did = test_did();

        let vouch = Vouch::new(
            "food-coop".to_string(),
            did.clone(),
            "tech-coop".to_string(),
            0.7,
        );
        assert!(!vouch.is_expired());
        assert!((vouch.trust_score - 0.7).abs() < f64::EPSILON);

        let expired_vouch =
            Vouch::new("food-coop".to_string(), did, "tech-coop".to_string(), 0.8).with_expiry(1); // Expired in 1970
        assert!(expired_vouch.is_expired());
    }

    #[test]
    fn test_vouch_trust_score_clamping() {
        let did = test_did();

        // Trust score above 1.0 should be clamped
        let high_trust = Vouch::new("coop-a".to_string(), did.clone(), "coop-b".to_string(), 1.5);
        assert!((high_trust.trust_score - 1.0).abs() < f64::EPSILON);

        // Trust score below 0.0 should be clamped
        let low_trust = Vouch::new(
            "coop-a".to_string(),
            did.clone(),
            "coop-b".to_string(),
            -0.5,
        );
        assert!(low_trust.trust_score.abs() < f64::EPSILON);

        // with_trust_score builder method
        let modified =
            Vouch::new("coop-a".to_string(), did, "coop-b".to_string(), 0.5).with_trust_score(2.0);
        assert!((modified.trust_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_policy_result() {
        assert!(PolicyResult::Allowed.is_allowed());
        assert!(!PolicyResult::NeedsVouches {
            required: 3,
            current: 1
        }
        .is_allowed());
        assert!(!PolicyResult::Closed.is_allowed());
        assert!(!PolicyResult::Denied("test".to_string()).is_allowed());
    }

    #[test]
    fn test_currency_info() {
        let hours = CurrencyInfo::hours();
        assert_eq!(hours.symbol, "hours");
        assert_eq!(hours.decimals, 2);

        let usd = CurrencyInfo::usd();
        assert_eq!(usd.symbol, "USD");
    }

    #[test]
    fn test_federation_message_serialization() {
        let did = test_did();

        let coop = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            did,
            FederationPolicy::Open,
        );

        let msg = FederationMessage::CoopAnnounce(coop);
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: FederationMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            FederationMessage::CoopAnnounce(c) => {
                assert_eq!(c.coop_id, "food-coop");
            }
            _ => panic!("Expected CoopAnnounce"),
        }
    }
}
