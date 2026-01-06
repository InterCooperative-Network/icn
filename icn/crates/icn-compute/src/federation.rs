//! Federation integration for cross-cooperative compute.
//!
//! This module enables compute tasks to be executed by executors from federated
//! cooperatives. It provides:
//!
//! - `FederatedExecutorAttestation`: Trust claims from source cooperatives
//! - `FederatedPaymentTerms`: Payment terms for cross-coop task execution
//! - `FederatedExecutorRegistry`: Discovery and caching of federated executors
//! - `FederatedExecutorInfo`: Information about federated executors
//!
//! # Trust Attenuation
//!
//! When evaluating federated executors, trust is attenuated based on:
//! - The executor's local trust score within their cooperative
//! - The source cooperative's trust score in the federation
//! - A configurable attenuation factor
//!
//! Formula: `federated_trust = local_trust × coop_trust × attenuation_factor`

use crate::error::ComputeError;
use crate::scheduler::NodeCapacity;
use crate::types::ExecutorCapability;

use icn_federation::{
    CooperativeInfo, CooperativeRegistry, FederatedDidResolver, FederatedTrustAttestation,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Information about a federated executor from another cooperative.
///
/// This is the federation-aware version of executor information that includes
/// both local trust (within the source cooperative) and attenuated trust
/// (computed based on cooperative trust).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedExecutorInfo {
    /// Executor's DID (federated format: did:icn:coop-id:key)
    pub did: String,

    /// Source cooperative ID
    pub cooperative_id: String,

    /// Capabilities this executor offers
    pub capabilities: Vec<ExecutorCapability>,

    /// Local trust score within source cooperative (0.0-1.0)
    pub local_trust_score: f64,

    /// Attenuated trust score for cross-coop placement (0.0-1.0)
    pub federated_trust_score: f64,

    /// Last announcement timestamp (milliseconds since epoch)
    pub last_seen: u64,

    /// Current capacity (if known)
    pub capacity: Option<NodeCapacity>,

    /// Gateway endpoint for task submission
    pub gateway_endpoint: Option<String>,
}

/// Attestation for a federated executor's capabilities and trust.
///
/// This is signed by the source cooperative to vouch for an executor's
/// identity, capabilities, and trust level within their cooperative.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedExecutorAttestation {
    /// Source cooperative ID
    pub source_coop_id: String,

    /// Executor DID (full format: did:icn:coop-id:key)
    pub executor_did: String,

    /// Trust attestation from source cooperative
    pub trust_attestation: FederatedTrustAttestation,

    /// Executor capabilities as attested by source
    pub capabilities: Vec<ExecutorCapability>,

    /// Optional capacity information
    pub capacity: Option<NodeCapacity>,

    /// Gateway endpoint for task submission
    pub gateway_endpoint: Option<String>,

    /// When this attestation was issued (Unix timestamp)
    pub issued_at: u64,

    /// When this attestation expires (Unix timestamp)
    pub expires_at: u64,

    /// Ed25519 signature from source cooperative
    pub signature: Vec<u8>,
}

impl FederatedExecutorAttestation {
    /// Check if this attestation has expired
    pub fn is_expired(&self) -> bool {
        let now = icn_time::current_timestamp_millis() / 1000;
        now > self.expires_at
    }

    /// Get remaining validity in seconds
    pub fn remaining_validity(&self) -> u64 {
        let now = icn_time::current_timestamp_millis() / 1000;
        self.expires_at.saturating_sub(now)
    }

    /// Get the bytes to sign (attestation without signature)
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut att = self.clone();
        att.signature = Vec::new();
        // Clear nested signature too for consistent hashing
        att.trust_attestation.signature = Vec::new();
        serde_json::to_vec(&att).unwrap_or_default()
    }

    /// Sign the attestation with the source cooperative's key
    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        use ed25519_dalek::Signer;
        let bytes = self.signing_bytes();
        let signature = signing_key.sign(&bytes);
        self.signature = signature.to_bytes().to_vec();
    }

    /// Verify the attestation signature against the source cooperative's public key
    ///
    /// Returns Ok(true) if signature is valid, Ok(false) if invalid,
    /// Err if signature format is wrong
    pub fn verify(
        &self,
        verifying_key: &ed25519_dalek::VerifyingKey,
    ) -> Result<bool, ComputeError> {
        use ed25519_dalek::{Signature, Verifier};

        if self.signature.len() != 64 {
            return Err(ComputeError::InvalidSignature(
                "Invalid signature length".to_string(),
            ));
        }

        let bytes = self.signing_bytes();
        let sig_bytes: [u8; 64] = self.signature[..64]
            .try_into()
            .map_err(|_| ComputeError::InvalidSignature("Invalid signature format".to_string()))?;

        let signature = Signature::from_bytes(&sig_bytes);
        match verifying_key.verify(&bytes, &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Check if attestation has a signature
    pub fn is_signed(&self) -> bool {
        !self.signature.is_empty()
    }
}

/// When payment for federated task execution is triggered.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum PaymentTrigger {
    /// Pay when executor reports completion
    #[default]
    Completion,

    /// Pay when result is verified by quorum
    Verification,

    /// Pay when submitter acknowledges receipt
    Acknowledgement,
}

/// Payment terms for cross-cooperative task execution.
///
/// These terms are agreed upon before task assignment and
/// determine how payment flows through the clearing system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedPaymentTerms {
    /// Payment amount in source currency (credits)
    pub amount: u64,

    /// Source cooperative's currency
    pub source_currency: String,

    /// Destination cooperative's currency (executor's)
    pub dest_currency: String,

    /// Clearing agreement ID (if exists between cooperatives)
    pub clearing_agreement_id: Option<String>,

    /// When payment is triggered
    pub payment_trigger: PaymentTrigger,

    /// Maximum cost multiplier for exchange rate variance
    pub max_exchange_variance: f64,
}

impl Default for FederatedPaymentTerms {
    fn default() -> Self {
        Self {
            amount: 0,
            source_currency: "ICN".to_string(),
            dest_currency: "ICN".to_string(),
            clearing_agreement_id: None,
            payment_trigger: PaymentTrigger::Completion,
            max_exchange_variance: 0.05, // 5% tolerance
        }
    }
}

/// Registry of federated executors discovered across cooperatives.
///
/// This registry maintains a cache of executors from federated cooperatives,
/// discovered via the `CooperativeRegistry` and `FederatedDidResolver`.
pub struct FederatedExecutorRegistry {
    /// Federated executor cache: coop_id -> executor_did -> FederatedExecutorInfo
    federated_cache: RwLock<HashMap<String, HashMap<String, FederatedExecutorInfo>>>,

    /// Cooperative registry for discovering compute providers
    coop_registry: Arc<CooperativeRegistry>,

    /// DID resolver for federated DIDs
    #[allow(dead_code)]
    did_resolver: Arc<FederatedDidResolver>,

    /// Own cooperative ID
    own_coop_id: String,

    /// Trust attenuation factor (default 0.9)
    trust_attenuation: f64,

    /// Cache TTL in seconds
    cache_ttl_secs: u64,

    /// When the cache was last refreshed
    last_refresh: RwLock<u64>,
}

impl FederatedExecutorRegistry {
    /// Default trust attenuation factor
    pub const DEFAULT_ATTENUATION: f64 = 0.9;

    /// Default cache TTL (5 minutes)
    pub const DEFAULT_CACHE_TTL_SECS: u64 = 300;

    /// Create a new federated executor registry
    pub fn new(
        coop_registry: Arc<CooperativeRegistry>,
        did_resolver: Arc<FederatedDidResolver>,
        own_coop_id: String,
    ) -> Self {
        Self {
            federated_cache: RwLock::new(HashMap::new()),
            coop_registry,
            did_resolver,
            own_coop_id,
            trust_attenuation: Self::DEFAULT_ATTENUATION,
            cache_ttl_secs: Self::DEFAULT_CACHE_TTL_SECS,
            last_refresh: RwLock::new(0),
        }
    }

    /// Set the trust attenuation factor
    pub fn with_attenuation(mut self, factor: f64) -> Self {
        self.trust_attenuation = factor.clamp(0.0, 1.0);
        self
    }

    /// Set the cache TTL
    pub fn with_cache_ttl(mut self, ttl_secs: u64) -> Self {
        self.cache_ttl_secs = ttl_secs;
        self
    }

    /// Compute attenuated trust score for a federated executor.
    ///
    /// Formula: `federated_trust = local_trust × coop_trust × attenuation_factor`
    ///
    /// # Arguments
    /// * `local_trust` - The executor's trust score within their cooperative (0.0-1.0)
    /// * `coop_trust` - The cooperative's trust score in the federation (0.0-1.0)
    ///
    /// # Returns
    /// The attenuated trust score (0.0-1.0)
    pub fn attenuate_trust(&self, local_trust: f64, coop_trust: f64) -> f64 {
        let local = local_trust.clamp(0.0, 1.0);
        let coop = coop_trust.clamp(0.0, 1.0);
        (local * coop * self.trust_attenuation).clamp(0.0, 1.0)
    }

    /// Compute attenuated trust score using default attenuation factor (static version).
    ///
    /// This is useful when you don't have access to a FederatedExecutorRegistry instance.
    /// Uses the default attenuation factor of 0.9.
    ///
    /// Formula: `federated_trust = local_trust × coop_trust × 0.9`
    ///
    /// # Arguments
    /// * `local_trust` - The executor's trust score within their cooperative (0.0-1.0)
    /// * `coop_trust` - The cooperative's trust score in the federation (0.0-1.0)
    ///
    /// # Returns
    /// The attenuated trust score (0.0-1.0)
    pub fn attenuate_trust_static(local_trust: f64, coop_trust: f64) -> f64 {
        const DEFAULT_ATTENUATION: f64 = 0.9;
        let local = local_trust.clamp(0.0, 1.0);
        let coop = coop_trust.clamp(0.0, 1.0);
        (local * coop * DEFAULT_ATTENUATION).clamp(0.0, 1.0)
    }

    /// Discover executors from federated cooperatives with compute capability.
    ///
    /// This queries the cooperative registry for cooperatives that advertise
    /// compute capability, then caches their executor information.
    pub async fn discover_federated_executors(
        &self,
    ) -> Result<Vec<FederatedExecutorInfo>, ComputeError> {
        // Find cooperatives with compute capability
        let compute_coops: Vec<CooperativeInfo> = self
            .coop_registry
            .list()
            .map_err(|e| ComputeError::Internal(format!("Failed to list cooperatives: {e}")))?
            .into_iter()
            .filter(|coop| {
                coop.capabilities.iter().any(|c| c == "compute") && coop.coop_id != self.own_coop_id
            })
            .collect();

        tracing::debug!(
            count = compute_coops.len(),
            "Discovered cooperatives with compute capability"
        );

        let mut all_executors = Vec::new();
        // Use read lock since we only read from cache here
        let cache = self.federated_cache.read().await;

        for coop in compute_coops {
            // For now, we don't have a way to query executors directly from cooperatives.
            // This would require a federation protocol extension or gateway API call.
            // For MVP, we rely on FederatedExecutorAnnounce messages received via gossip.

            // Placeholder: Just log that we found a compute-capable cooperative
            tracing::debug!(
                coop_id = %coop.coop_id,
                name = %coop.name,
                "Found compute-capable federated cooperative"
            );

            // Get cached executors for this coop if any
            if let Some(coop_executors) = cache.get(&coop.coop_id) {
                all_executors.extend(coop_executors.values().cloned());
            }
        }

        // Update last refresh timestamp
        *self.last_refresh.write().await = icn_time::current_timestamp_millis() / 1000;

        Ok(all_executors)
    }

    /// Add or update a federated executor from an announcement.
    ///
    /// Called when receiving a `FederatedExecutorAnnounce` message.
    pub async fn add_executor(
        &self,
        attestation: FederatedExecutorAttestation,
        coop_trust: f64,
    ) -> Result<(), ComputeError> {
        if attestation.is_expired() {
            return Err(ComputeError::Internal(
                "Federated executor attestation expired".to_string(),
            ));
        }

        // Get the trust score from attestation
        let local_trust = attestation.trust_attestation.trust_score;
        let federated_trust = self.attenuate_trust(local_trust, coop_trust);

        let now = icn_time::current_timestamp_millis();

        let info = FederatedExecutorInfo {
            did: attestation.executor_did.clone(),
            cooperative_id: attestation.source_coop_id.clone(),
            capabilities: attestation.capabilities,
            local_trust_score: local_trust,
            federated_trust_score: federated_trust,
            last_seen: now,
            capacity: attestation.capacity,
            gateway_endpoint: attestation.gateway_endpoint,
        };

        // Update cache
        let mut cache = self.federated_cache.write().await;
        let coop_executors = cache
            .entry(attestation.source_coop_id)
            .or_insert_with(HashMap::new);
        coop_executors.insert(attestation.executor_did, info);

        Ok(())
    }

    /// Remove a federated executor.
    pub async fn remove_executor(&self, coop_id: &str, executor_did: &str) {
        let mut cache = self.federated_cache.write().await;
        if let Some(coop_executors) = cache.get_mut(coop_id) {
            coop_executors.remove(executor_did);
        }
    }

    /// Get all cached federated executors.
    pub async fn get_all_executors(&self) -> Vec<FederatedExecutorInfo> {
        let cache = self.federated_cache.read().await;
        cache
            .values()
            .flat_map(|coop_executors| coop_executors.values().cloned())
            .collect()
    }

    /// Get executors from a specific cooperative.
    pub async fn get_executors_for_coop(&self, coop_id: &str) -> Vec<FederatedExecutorInfo> {
        let cache = self.federated_cache.read().await;
        cache
            .get(coop_id)
            .map(|execs| execs.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if cache needs refresh.
    pub async fn needs_refresh(&self) -> bool {
        let now = icn_time::current_timestamp_millis() / 1000;
        let last = *self.last_refresh.read().await;
        now - last > self.cache_ttl_secs
    }

    /// Get the own cooperative ID.
    pub fn own_coop_id(&self) -> &str {
        &self.own_coop_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to compute attenuated trust
    fn attenuate_trust_helper(local_trust: f64, coop_trust: f64, attenuation: f64) -> f64 {
        let local = local_trust.clamp(0.0, 1.0);
        let coop = coop_trust.clamp(0.0, 1.0);
        (local * coop * attenuation).clamp(0.0, 1.0)
    }

    #[test]
    fn test_attenuate_trust_formula() {
        // Test: high local trust, high coop trust
        let result = attenuate_trust_helper(0.9, 0.8, 0.5);
        assert!((result - 0.36).abs() < 0.001); // 0.9 * 0.8 * 0.5 = 0.36

        // Test: low local trust
        let result = attenuate_trust_helper(0.3, 0.8, 0.5);
        assert!((result - 0.12).abs() < 0.001); // 0.3 * 0.8 * 0.5 = 0.12

        // Test: low coop trust
        let result = attenuate_trust_helper(0.9, 0.2, 0.5);
        assert!((result - 0.09).abs() < 0.001); // 0.9 * 0.2 * 0.5 = 0.09

        // Test: both low
        let result = attenuate_trust_helper(0.3, 0.2, 0.5);
        assert!((result - 0.03).abs() < 0.001); // 0.3 * 0.2 * 0.5 = 0.03
    }

    #[test]
    fn test_attenuate_trust_clamps_input() {
        // Test: values > 1.0 are clamped
        let result = attenuate_trust_helper(1.5, 1.2, 0.5);
        assert!((result - 0.5).abs() < 0.001); // 1.0 * 1.0 * 0.5 = 0.5

        // Test: values < 0.0 are clamped
        let result = attenuate_trust_helper(-0.5, -0.3, 0.5);
        assert!((result - 0.0).abs() < 0.001); // 0.0 * 0.0 * 0.5 = 0.0
    }

    #[test]
    fn test_attenuate_trust_different_factors() {
        // Test with 1.0 attenuation (no reduction)
        let result = attenuate_trust_helper(0.8, 0.9, 1.0);
        assert!((result - 0.72).abs() < 0.001); // 0.8 * 0.9 * 1.0 = 0.72

        // Test with 0.25 attenuation (75% reduction)
        let result = attenuate_trust_helper(0.8, 0.8, 0.25);
        assert!((result - 0.16).abs() < 0.001); // 0.8 * 0.8 * 0.25 = 0.16
    }

    #[test]
    fn test_payment_trigger_default() {
        let trigger = PaymentTrigger::default();
        assert_eq!(trigger, PaymentTrigger::Completion);
    }

    #[test]
    fn test_federated_payment_terms_default() {
        let terms = FederatedPaymentTerms::default();
        assert_eq!(terms.amount, 0);
        assert_eq!(terms.source_currency, "ICN");
        assert_eq!(terms.dest_currency, "ICN");
        assert_eq!(terms.payment_trigger, PaymentTrigger::Completion);
        assert!((terms.max_exchange_variance - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_federated_executor_info() {
        let info = FederatedExecutorInfo {
            did: "did:icn:coop-a:z6MkTest".to_string(),
            cooperative_id: "coop-a".to_string(),
            capabilities: vec![ExecutorCapability::Ccl],
            local_trust_score: 0.8,
            federated_trust_score: 0.32, // 0.8 * 0.8 * 0.5 = 0.32
            last_seen: 1000000,
            capacity: None,
            gateway_endpoint: Some("https://coop-a.example.com".to_string()),
        };

        assert_eq!(info.cooperative_id, "coop-a");
        assert!(info.federated_trust_score < info.local_trust_score);
    }

    #[test]
    fn test_federated_executor_attestation_expiry() {
        let now_secs = icn_time::current_timestamp_millis() / 1000;

        // Generate test DIDs from keypairs
        let coop_keypair = icn_identity::KeyPair::generate().unwrap();
        let exec_keypair = icn_identity::KeyPair::generate().unwrap();
        let coop_did = coop_keypair.did();
        let exec_did = exec_keypair.did();

        // Create expired attestation
        let expired = FederatedExecutorAttestation {
            source_coop_id: "coop-a".to_string(),
            executor_did: "did:icn:coop-a:test".to_string(),
            trust_attestation: FederatedTrustAttestation::new(
                "coop-a".to_string(),
                coop_did.clone(), // Clone for first use
                exec_did.clone(), // Clone for first use
                0.8,
                icn_federation::TrustContext::General,
                0, // Already expired
            ),
            capabilities: vec![],
            capacity: None,
            gateway_endpoint: None,
            issued_at: now_secs - 3600,
            expires_at: now_secs - 1, // 1 second ago
            signature: vec![],
        };
        assert!(expired.is_expired());
        assert_eq!(expired.remaining_validity(), 0);

        // Create valid attestation
        let valid = FederatedExecutorAttestation {
            source_coop_id: "coop-a".to_string(),
            executor_did: "did:icn:coop-a:test".to_string(),
            trust_attestation: FederatedTrustAttestation::new(
                "coop-a".to_string(),
                coop_did.clone(),
                exec_did.clone(),
                0.8,
                icn_federation::TrustContext::General,
                3600,
            ),
            capabilities: vec![],
            capacity: None,
            gateway_endpoint: Some("https://coop-a.example.com".to_string()),
            issued_at: now_secs,
            expires_at: now_secs + 3600, // 1 hour from now
            signature: vec![],
        };
        assert!(!valid.is_expired());
        assert!(valid.remaining_validity() > 3500); // Should be close to 3600
    }

    #[test]
    fn test_federation_policy_default() {
        use crate::scheduler::FederationPolicy;
        let policy = FederationPolicy::default();
        assert_eq!(policy, FederationPolicy::PreferLocal);
    }

    #[test]
    fn test_federated_placement_constraints_allows_cooperative() {
        use crate::scheduler::{FederatedPlacementConstraints, FederationPolicy};

        // LocalOnly - only allows local
        let local_only = FederatedPlacementConstraints {
            federation_policy: FederationPolicy::LocalOnly,
            ..Default::default()
        };
        assert!(local_only.allows_cooperative(None, true));
        assert!(!local_only.allows_cooperative(Some("coop-a"), false));

        // AllowFederated - allows all
        let allow_fed = FederatedPlacementConstraints {
            federation_policy: FederationPolicy::AllowFederated,
            ..Default::default()
        };
        assert!(allow_fed.allows_cooperative(None, true));
        assert!(allow_fed.allows_cooperative(Some("coop-a"), false));

        // Whitelist - allows local + whitelisted
        let whitelist = FederatedPlacementConstraints {
            federation_policy: FederationPolicy::FederatedWhitelist {
                cooperatives: vec!["coop-a".to_string(), "coop-b".to_string()],
            },
            ..Default::default()
        };
        assert!(whitelist.allows_cooperative(None, true));
        assert!(whitelist.allows_cooperative(Some("coop-a"), false));
        assert!(whitelist.allows_cooperative(Some("coop-b"), false));
        assert!(!whitelist.allows_cooperative(Some("coop-c"), false));
    }
}
