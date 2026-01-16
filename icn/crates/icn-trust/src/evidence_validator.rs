//! Evidence Validation
//!
//! Validates trust evidence against actual records in the system.
//! Each evidence type has specific validation requirements.
//!
//! ## Signature Verification (Issue #680)
//!
//! This module implements cryptographic signature verification for trust evidence:
//!
//! - **External attestations**: Verified against known provider public keys
//! - **Peer endorsements**: Verified against the endorser's DID public key
//! - **Technical observations**: Verified against the observer's DID public key
//!
//! ### Signed Message Formats
//!
//! Each evidence type has a canonical message format for signing:
//!
//! - **External attestation**: `"attestation:{provider}:{attestation_id}:{target_did}"`
//! - **Peer endorsement**: `"endorsement:{source_did}:{target_did}:{timestamp}"`
//! - **Technical observation**: `"observation:{target_did}:{metric_type}:{value:.17}:{timestamp}"`
//!   (value uses IEEE 754 double precision formatting for cross-platform determinism)

use crate::evidence::{
    EvidenceValidationError, EvidenceValidationResult, TechnicalMetricType, TrustEvidence,
};
use crate::TrustGraph;
use ed25519_dalek::{Signature, VerifyingKey};
use icn_identity::Did;
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for evidence validation
#[derive(Debug, Clone)]
pub struct EvidenceValidatorConfig {
    /// Whether to accept legacy string evidence
    pub accept_legacy: bool,
    /// Maximum age for technical observations (default: 7 days)
    pub max_observation_age: Duration,
    /// Minimum trust score required for peer endorsements
    pub min_endorser_trust: f64,
    /// Minimum trust score required for technical observers
    pub min_observer_trust: f64,
    /// Known external attestation providers
    pub known_providers: Vec<String>,
    /// Public keys for attestation providers (provider name -> Ed25519 public key bytes)
    ///
    /// External attestations are verified against these keys. If a provider is in
    /// `known_providers` but not in `provider_keys`, signatures are still checked
    /// for non-emptiness but not cryptographically verified.
    pub provider_keys: HashMap<String, [u8; 32]>,
    /// Whether to allow unsigned technical observations
    ///
    /// When `true` (development mode), unsigned observations are accepted with a warning.
    /// When `false` (production), all observations must be signed.
    pub allow_unsigned_observations: bool,
}

impl Default for EvidenceValidatorConfig {
    fn default() -> Self {
        Self {
            accept_legacy: true, // Allow legacy during migration
            max_observation_age: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            min_endorser_trust: 0.4, // Partner level
            min_observer_trust: 0.3, // Known level
            known_providers: vec![
                "sdis".to_string(),
                "keybase".to_string(),
                "github".to_string(),
            ],
            provider_keys: HashMap::new(), // No provider keys by default
            allow_unsigned_observations: true, // Allow unsigned during development
        }
    }
}

impl EvidenceValidatorConfig {
    /// Create a production-ready config that requires all signatures
    pub fn production() -> Self {
        Self {
            accept_legacy: false,
            max_observation_age: Duration::from_secs(24 * 60 * 60), // 1 day
            min_endorser_trust: 0.5,
            min_observer_trust: 0.4,
            known_providers: vec![
                "sdis".to_string(),
                "keybase".to_string(),
                "github".to_string(),
            ],
            provider_keys: HashMap::new(),
            allow_unsigned_observations: false, // Require signatures in production
        }
    }

    /// Add a provider's public key for signature verification
    ///
    /// # Panics
    ///
    /// Panics if the provided public key is not a valid Ed25519 public key.
    /// This follows the "fail fast" principle - invalid configuration should be
    /// caught at startup, not at runtime during signature verification.
    pub fn with_provider_key(mut self, provider: &str, public_key: [u8; 32]) -> Self {
        // Validate the key is a valid Ed25519 public key at configuration time
        VerifyingKey::from_bytes(&public_key).unwrap_or_else(|e| {
            panic!("Invalid Ed25519 public key for provider '{provider}': {e}")
        });

        if !self.known_providers.contains(&provider.to_string()) {
            self.known_providers.push(provider.to_string());
        }
        self.provider_keys.insert(provider.to_string(), public_key);
        self
    }
}

/// Validates trust evidence against system records
pub struct EvidenceValidator {
    config: EvidenceValidatorConfig,
    store: Arc<dyn Store>,
}

impl EvidenceValidator {
    /// Create a new evidence validator
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self {
            config: EvidenceValidatorConfig::default(),
            store,
        }
    }

    /// Create a new evidence validator with custom config
    pub fn with_config(store: Arc<dyn Store>, config: EvidenceValidatorConfig) -> Self {
        Self { config, store }
    }

    /// Validate a single evidence item
    ///
    /// This validates that the evidence:
    /// 1. References something that exists in the system
    /// 2. Is relevant to the trust relationship (source and target DIDs)
    /// 3. Has valid signatures where applicable
    pub fn validate_evidence(
        &self,
        evidence: &TrustEvidence,
        source: &Did,
        target: &Did,
        trust_graph: Option<&TrustGraph>,
    ) -> EvidenceValidationResult {
        match evidence {
            TrustEvidence::ContractExecution {
                contract_id,
                agreement_id,
                ..
            } => self.validate_contract(contract_id, agreement_id.as_deref(), source, target),

            TrustEvidence::LedgerTransaction {
                ledger_id,
                entry_hash,
                ..
            } => self.validate_ledger_transaction(ledger_id, entry_hash, source, target),

            TrustEvidence::GovernanceVote {
                scope_id,
                proposal_id,
                vote_hash,
            } => self.validate_governance_vote(scope_id, proposal_id, vote_hash, source, target),

            TrustEvidence::ExternalAttestation {
                provider,
                attestation_id,
                signature,
                ..
            } => self.validate_external_attestation(provider, attestation_id, signature, target),

            TrustEvidence::PeerEndorsement {
                endorser,
                signature,
                endorsed_at,
                ..
            } => self.validate_peer_endorsement(
                endorser,
                signature,
                *endorsed_at,
                source,
                target,
                trust_graph,
            ),

            TrustEvidence::TechnicalObservation {
                observer,
                metric_type,
                value,
                observed_at,
                signature,
            } => self.validate_technical_observation(
                observer,
                metric_type,
                *value,
                *observed_at,
                signature,
                target,
                trust_graph,
            ),

            TrustEvidence::Legacy { reference, .. } => self.validate_legacy(reference),
        }
    }

    /// Validate all evidence for an edge
    ///
    /// Returns overall validation result combining all evidence items.
    pub fn validate_all_evidence(
        &self,
        evidence: &[TrustEvidence],
        source: &Did,
        target: &Did,
        trust_graph: Option<&TrustGraph>,
    ) -> EvidenceValidationResult {
        if evidence.is_empty() {
            // No evidence is allowed (edges can exist without evidence)
            return EvidenceValidationResult::valid();
        }

        let mut all_errors = Vec::new();
        let mut total_adjustment = 0.0;
        let mut valid_count = 0;

        for e in evidence {
            let result = self.validate_evidence(e, source, target, trust_graph);
            if result.valid {
                valid_count += 1;
                total_adjustment += result.score_adjustment;
            } else {
                all_errors.extend(result.errors);
            }
        }

        // Require at least one valid evidence if any evidence is provided
        if valid_count == 0 {
            EvidenceValidationResult::invalid_multiple(all_errors)
        } else {
            // Some evidence is valid, return success with average adjustment
            let avg_adjustment = total_adjustment / valid_count as f64;
            EvidenceValidationResult::valid_with_adjustment(avg_adjustment)
        }
    }

    /// Validate contract execution evidence
    fn validate_contract(
        &self,
        contract_id: &[u8; 32],
        agreement_id: Option<&str>,
        source: &Did,
        target: &Did,
    ) -> EvidenceValidationResult {
        // Try to find the contract in storage
        let contract_key = format!("contracts/{}", hex::encode(contract_id));

        match self.store.get(contract_key.as_bytes()) {
            Ok(Some(_data)) => {
                // Contract exists, we'd need to parse and verify parties
                // For now, accept if contract exists
                debug!(
                    "Contract {} found for edge {} -> {}",
                    hex::encode(contract_id),
                    source,
                    target
                );
                EvidenceValidationResult::valid_with_adjustment(0.1) // Boost for contract evidence
            }
            Ok(None) => {
                // Contract not found, try agreement store if agreement_id provided
                if let Some(aid) = agreement_id {
                    let agreement_key = format!("agreements/{aid}");
                    if self
                        .store
                        .get(agreement_key.as_bytes())
                        .ok()
                        .flatten()
                        .is_some()
                    {
                        debug!("Agreement {} found for edge {} -> {}", aid, source, target);
                        return EvidenceValidationResult::valid_with_adjustment(0.1);
                    }
                }

                warn!(
                    "Contract {} not found for edge {} -> {}",
                    hex::encode(contract_id),
                    source,
                    target
                );
                // Don't fail, just don't boost - contract may be on another node
                EvidenceValidationResult::valid()
            }
            Err(e) => {
                warn!("Error checking contract: {}", e);
                EvidenceValidationResult::invalid(EvidenceValidationError::StorageError {
                    details: e.to_string(),
                })
            }
        }
    }

    /// Validate ledger transaction evidence
    fn validate_ledger_transaction(
        &self,
        ledger_id: &Did,
        entry_hash: &[u8; 32],
        source: &Did,
        target: &Did,
    ) -> EvidenceValidationResult {
        // Try to find the transaction in the ledger
        let tx_key = format!("ledger/{}/entries/{}", ledger_id, hex::encode(entry_hash));

        match self.store.get(tx_key.as_bytes()) {
            Ok(Some(_data)) => {
                // Transaction exists
                debug!(
                    "Ledger transaction {} found for edge {} -> {}",
                    hex::encode(entry_hash),
                    source,
                    target
                );
                // Economic evidence is stronger for EconomicReliability graph
                EvidenceValidationResult::valid_with_adjustment(0.15)
            }
            Ok(None) => {
                // Transaction not found locally, may be on another node
                debug!(
                    "Ledger transaction {} not found locally",
                    hex::encode(entry_hash)
                );
                EvidenceValidationResult::valid()
            }
            Err(e) => {
                warn!("Error checking ledger transaction: {}", e);
                EvidenceValidationResult::invalid(EvidenceValidationError::StorageError {
                    details: e.to_string(),
                })
            }
        }
    }

    /// Validate governance vote evidence
    fn validate_governance_vote(
        &self,
        scope_id: &str,
        proposal_id: &str,
        vote_hash: &[u8; 32],
        source: &Did,
        target: &Did,
    ) -> EvidenceValidationResult {
        // Try to find the vote record
        let vote_key = format!(
            "governance/{}/proposals/{}/votes/{}",
            scope_id,
            proposal_id,
            hex::encode(vote_hash)
        );

        match self.store.get(vote_key.as_bytes()) {
            Ok(Some(_data)) => {
                debug!(
                    "Governance vote found for proposal {} in {} for edge {} -> {}",
                    proposal_id, scope_id, source, target
                );
                EvidenceValidationResult::valid_with_adjustment(0.05)
            }
            Ok(None) => {
                // Vote not found locally
                EvidenceValidationResult::valid()
            }
            Err(e) => {
                warn!("Error checking governance vote: {}", e);
                EvidenceValidationResult::invalid(EvidenceValidationError::StorageError {
                    details: e.to_string(),
                })
            }
        }
    }

    /// Validate external attestation evidence
    ///
    /// Verifies that:
    /// 1. The provider is known
    /// 2. The signature is non-empty
    /// 3. If a provider public key is configured, the signature is cryptographically valid
    ///
    /// Signed message format: `"attestation:{provider}:{attestation_id}:{target_did}"`
    fn validate_external_attestation(
        &self,
        provider: &str,
        attestation_id: &str,
        signature: &[u8],
        target: &Did,
    ) -> EvidenceValidationResult {
        // Check if provider is known
        if !self.config.known_providers.contains(&provider.to_string()) {
            warn!("Unknown attestation provider: {}", provider);
            return EvidenceValidationResult::invalid(EvidenceValidationError::UnknownProvider {
                provider: provider.to_string(),
            });
        }

        // Signature must be non-empty
        if signature.is_empty() {
            return EvidenceValidationResult::invalid(
                EvidenceValidationError::InvalidAttestationSignature,
            );
        }

        // If we have the provider's public key, verify the signature cryptographically
        if let Some(public_key_bytes) = self.config.provider_keys.get(provider) {
            // Construct the canonical message that was signed
            let message = format!("attestation:{provider}:{attestation_id}:{target}");

            // Parse the verifying key
            let verifying_key = match VerifyingKey::from_bytes(public_key_bytes) {
                Ok(key) => key,
                Err(e) => {
                    warn!("Invalid provider public key for {}: {}", provider, e);
                    return EvidenceValidationResult::invalid(
                        EvidenceValidationError::InvalidAttestationSignature,
                    );
                }
            };

            // Parse the signature
            let sig = match Signature::from_slice(signature) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Invalid attestation signature format: {}", e);
                    return EvidenceValidationResult::invalid(
                        EvidenceValidationError::InvalidAttestationSignature,
                    );
                }
            };

            // Verify the signature
            if verifying_key
                .verify_strict(message.as_bytes(), &sig)
                .is_err()
            {
                warn!(
                    "Attestation signature verification failed for provider {} (target: {})",
                    provider, target
                );
                return EvidenceValidationResult::invalid(
                    EvidenceValidationError::InvalidAttestationSignature,
                );
            }

            debug!(
                "External attestation from {} for {} cryptographically verified (attestation: {})",
                provider, target, attestation_id
            );
        } else {
            // No public key configured - accept with warning (legacy behavior)
            debug!(
                "External attestation from {} for {} accepted without cryptographic verification (attestation: {})",
                provider, target, attestation_id
            );
        }

        EvidenceValidationResult::valid_with_adjustment(0.2) // External attestations are valuable
    }

    /// Validate peer endorsement evidence
    ///
    /// Verifies that:
    /// 1. The endorser is different from source and target
    /// 2. The endorsement is not too old (cheap check, done early for DoS protection)
    /// 3. The endorser has sufficient trust (if trust graph available)
    /// 4. The signature is cryptographically valid (expensive, done last)
    ///
    /// Signed message format: `"endorsement:{source_did}:{target_did}:{timestamp}"`
    fn validate_peer_endorsement(
        &self,
        endorser: &Did,
        signature: &[u8],
        endorsed_at: u64,
        source: &Did,
        target: &Did,
        trust_graph: Option<&TrustGraph>,
    ) -> EvidenceValidationResult {
        // Endorser must be different from source and target
        // Self-endorsement is accepted but provides no additional trust adjustment
        if endorser == source || endorser == target {
            return EvidenceValidationResult::valid();
        }

        // Check endorsement age FIRST (cheap check, DoS protection)
        // This prevents attackers from forcing expensive signature verification
        // by sending expired endorsements with valid signatures.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let age = now.saturating_sub(endorsed_at);
        let max_age = 365 * 24 * 60 * 60; // 1 year max for endorsements
        if age > max_age {
            return EvidenceValidationResult::invalid(EvidenceValidationError::EvidenceExpired);
        }

        // Check endorser's trust score if we have a trust graph
        if let Some(graph) = trust_graph {
            match graph.compute_trust_score(endorser) {
                Ok(score) if score < self.config.min_endorser_trust => {
                    warn!(
                        "Endorser {} has insufficient trust score: {} (required: {})",
                        endorser, score, self.config.min_endorser_trust
                    );
                    return EvidenceValidationResult::invalid(
                        EvidenceValidationError::EndorserNotTrusted {
                            endorser: endorser.to_string(),
                            score,
                            required: self.config.min_endorser_trust,
                        },
                    );
                }
                Ok(score) => {
                    debug!("Endorser {} has trust score {}", endorser, score);
                }
                Err(e) => {
                    warn!("Error computing endorser trust: {}", e);
                }
            }
        }

        // Signature must be non-empty
        if signature.is_empty() {
            return EvidenceValidationResult::invalid(
                EvidenceValidationError::InvalidEndorsementSignature {
                    endorser: endorser.to_string(),
                },
            );
        }

        // Verify signature cryptographically against endorser's DID (expensive, do last)
        let verifying_key = match endorser.to_verifying_key() {
            Ok(key) => key,
            Err(e) => {
                warn!("Failed to extract verifying key from endorser DID: {}", e);
                return EvidenceValidationResult::invalid(
                    EvidenceValidationError::InvalidEndorsementSignature {
                        endorser: endorser.to_string(),
                    },
                );
            }
        };

        let sig = match Signature::from_slice(signature) {
            Ok(s) => s,
            Err(e) => {
                warn!("Invalid endorsement signature format: {}", e);
                return EvidenceValidationResult::invalid(
                    EvidenceValidationError::InvalidEndorsementSignature {
                        endorser: endorser.to_string(),
                    },
                );
            }
        };

        // Construct the canonical message that was signed
        let message = format!("endorsement:{source}:{target}:{endorsed_at}");

        if verifying_key
            .verify_strict(message.as_bytes(), &sig)
            .is_err()
        {
            warn!(
                "Endorsement signature verification failed for endorser {} (edge: {} -> {})",
                endorser, source, target
            );
            return EvidenceValidationResult::invalid(
                EvidenceValidationError::InvalidEndorsementSignature {
                    endorser: endorser.to_string(),
                },
            );
        }

        debug!(
            "Peer endorsement from {} for {} -> {} cryptographically verified",
            endorser, source, target
        );
        EvidenceValidationResult::valid_with_adjustment(0.1)
    }

    /// Validate technical observation evidence
    ///
    /// Verifies that:
    /// 1. The observation is not too old
    /// 2. The observer has sufficient trust (if trust graph available)
    /// 3. The signature is cryptographically valid (signed by observer's DID key)
    /// 4. The value is within valid range for the metric type
    ///
    /// Signed message format: `"observation:{target_did}:{metric_type}:{value}:{timestamp}"`
    fn validate_technical_observation(
        &self,
        observer: &Did,
        metric_type: &TechnicalMetricType,
        value: f64,
        observed_at: u64,
        signature: &[u8],
        target: &Did,
        trust_graph: Option<&TrustGraph>,
    ) -> EvidenceValidationResult {
        // Check observation age
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let age_secs = now.saturating_sub(observed_at);
        let max_secs = self.config.max_observation_age.as_secs();

        if age_secs > max_secs {
            return EvidenceValidationResult::invalid(EvidenceValidationError::ObservationTooOld {
                age_secs,
                max_secs,
            });
        }

        // Check observer trust if we have a trust graph
        if let Some(graph) = trust_graph {
            match graph.compute_trust_score(observer) {
                Ok(score) if score < self.config.min_observer_trust => {
                    warn!(
                        "Observer {} has insufficient trust score: {} for technical observations",
                        observer, score
                    );
                    return EvidenceValidationResult::invalid(
                        EvidenceValidationError::ObserverNotTrusted {
                            observer: observer.to_string(),
                        },
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("Error computing observer trust: {}", e);
                }
            }
        }

        // Verify observer signature cryptographically
        if signature.is_empty() {
            if self.config.allow_unsigned_observations {
                debug!(
                    "Technical observation without signature accepted (allow_unsigned_observations=true)"
                );
            } else {
                warn!("Technical observation without signature rejected (production mode)");
                return EvidenceValidationResult::invalid(
                    EvidenceValidationError::MissingObservationSignature {
                        observer: observer.to_string(),
                    },
                );
            }
        } else {
            // Verify the signature cryptographically
            let verifying_key = match observer.to_verifying_key() {
                Ok(key) => key,
                Err(e) => {
                    warn!("Failed to extract verifying key from observer DID: {}", e);
                    return EvidenceValidationResult::invalid(
                        EvidenceValidationError::InvalidObservationSignature {
                            observer: observer.to_string(),
                        },
                    );
                }
            };

            let sig = match Signature::from_slice(signature) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Invalid observation signature format: {}", e);
                    return EvidenceValidationResult::invalid(
                        EvidenceValidationError::InvalidObservationSignature {
                            observer: observer.to_string(),
                        },
                    );
                }
            };

            // Construct the canonical message that was signed
            // Use IEEE 754 double precision formatting ({:.17}) for cross-platform determinism
            let message = format!(
                "observation:{target}:{}:{value:.17}:{observed_at}",
                metric_type.as_str()
            );

            if verifying_key
                .verify_strict(message.as_bytes(), &sig)
                .is_err()
            {
                warn!(
                    "Observation signature verification failed for observer {} (target: {})",
                    observer, target
                );
                return EvidenceValidationResult::invalid(
                    EvidenceValidationError::InvalidObservationSignature {
                        observer: observer.to_string(),
                    },
                );
            }

            debug!(
                "Technical observation {} = {} for {} from {} cryptographically verified",
                metric_type.as_str(),
                value,
                target,
                observer
            );
        }

        // Validate value range based on metric type
        let valid_range = match metric_type {
            TechnicalMetricType::Uptime => 0.0..=1.0,
            TechnicalMetricType::Latency => 0.0..=f64::MAX,
            TechnicalMetricType::TaskSuccessRate => 0.0..=1.0,
            TechnicalMetricType::StorageReliability => 0.0..=1.0,
            TechnicalMetricType::ProtocolCompliance => 0.0..=1.0,
        };

        if !valid_range.contains(&value) {
            // Out-of-range values are tolerated to avoid rejecting edges due to measurement errors
            warn!(
                "Invalid {} value {} for {}",
                metric_type.as_str(),
                value,
                target
            );
            return EvidenceValidationResult::valid();
        }

        debug!(
            "Technical observation {} = {} for {} from {} accepted",
            metric_type.as_str(),
            value,
            target,
            observer
        );
        EvidenceValidationResult::valid_with_adjustment(0.05)
    }

    /// Validate legacy string evidence
    fn validate_legacy(&self, reference: &str) -> EvidenceValidationResult {
        if !self.config.accept_legacy {
            return EvidenceValidationResult::invalid(EvidenceValidationError::LegacyNotAccepted);
        }

        // Legacy evidence is accepted but provides no boost
        debug!("Legacy evidence accepted: {}", reference);
        EvidenceValidationResult::valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn test_store() -> Arc<dyn Store> {
        Arc::new(SledStore::temporary().unwrap())
    }

    #[test]
    fn test_validator_accepts_no_evidence() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let result = validator.validate_all_evidence(&[], alice.did(), bob.did(), None);
        assert!(result.valid);
    }

    #[test]
    fn test_validator_accepts_legacy_evidence() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let evidence = vec![TrustEvidence::Legacy {
            reference: "old_contract_123".to_string(),
            migrated_at: 12345,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid);
    }

    #[test]
    fn test_validator_rejects_legacy_when_disabled() {
        let store = test_store();
        let config = EvidenceValidatorConfig {
            accept_legacy: false,
            ..Default::default()
        };
        let validator = EvidenceValidator::with_config(store, config);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let evidence = vec![TrustEvidence::Legacy {
            reference: "old_contract_123".to_string(),
            migrated_at: 12345,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
    }

    #[test]
    fn test_validator_rejects_unknown_provider() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let evidence = vec![TrustEvidence::ExternalAttestation {
            provider: "unknown_provider".to_string(),
            attestation_id: "test123".to_string(),
            signature: vec![1, 2, 3],
            metadata: None,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::UnknownProvider { .. }
        ));
    }

    #[test]
    fn test_validator_accepts_known_provider() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let evidence = vec![TrustEvidence::ExternalAttestation {
            provider: "github".to_string(),
            attestation_id: "test123".to_string(),
            signature: vec![1, 2, 3],
            metadata: None,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid);
        assert!(result.score_adjustment > 0.0); // Should have boost
    }

    #[test]
    fn test_validator_rejects_old_observation() {
        let store = test_store();
        let config = EvidenceValidatorConfig {
            max_observation_age: Duration::from_secs(60), // 1 minute
            ..Default::default()
        };
        let validator = EvidenceValidator::with_config(store, config);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        // Observation from 2 minutes ago
        let old_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 120;

        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(),
            metric_type: TechnicalMetricType::Uptime,
            value: 0.99,
            observed_at: old_time,
            signature: vec![], // Empty signature (allowed by default config)
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::ObservationTooOld { .. }
        ));
    }

    #[test]
    fn test_validator_accepts_recent_observation() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(),
            metric_type: TechnicalMetricType::Uptime,
            value: 0.99,
            observed_at: now,
            signature: vec![], // Empty signature (allowed by default config)
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid);
    }

    #[test]
    fn test_validator_contract_not_found_still_valid() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Contract that doesn't exist locally (may be on another node)
        let evidence = vec![TrustEvidence::ContractExecution {
            contract_id: [42u8; 32],
            agreement_id: None,
            executed_at: 12345,
        }];

        // Should still be valid (contract may be on another node)
        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid);
    }

    // ============================================================================
    // Cryptographic Signature Verification Tests (Issue #680)
    // ============================================================================

    #[test]
    fn test_peer_endorsement_valid_signature() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let endorser = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create properly signed endorsement
        let message = format!("endorsement:{}:{}:{}", alice.did(), bob.did(), now);
        let signature = endorser.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::PeerEndorsement {
            endorser: endorser.did().clone(),
            signature: signature.to_bytes().to_vec(),
            endorsed_at: now,
            reason: Some("Test endorsement".to_string()),
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid, "Valid signature should be accepted");
        assert!(
            result.score_adjustment > 0.0,
            "Should have positive score adjustment"
        );
    }

    #[test]
    fn test_peer_endorsement_invalid_signature() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let endorser = KeyPair::generate().unwrap();
        let wrong_signer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Sign with wrong key
        let message = format!("endorsement:{}:{}:{}", alice.did(), bob.did(), now);
        let wrong_signature = wrong_signer.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::PeerEndorsement {
            endorser: endorser.did().clone(), // Claims to be endorser
            signature: wrong_signature.to_bytes().to_vec(), // But signed by wrong_signer
            endorsed_at: now,
            reason: None,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid, "Invalid signature should be rejected");
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::InvalidEndorsementSignature { .. }
        ));
    }

    #[test]
    fn test_technical_observation_valid_signature() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create properly signed observation
        // Note: value must use IEEE 754 double precision formatting ({:.17}) to match validator
        let value = 0.99_f64;
        let message = format!("observation:{}:uptime:{value:.17}:{}", bob.did(), now);
        let signature = observer.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(),
            metric_type: TechnicalMetricType::Uptime,
            value,
            observed_at: now,
            signature: signature.to_bytes().to_vec(),
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(
            result.valid,
            "Valid observation signature should be accepted"
        );
    }

    #[test]
    fn test_technical_observation_invalid_signature() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();
        let wrong_signer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Sign with wrong key (using deterministic float format)
        let value = 0.99_f64;
        let message = format!("observation:{}:uptime:{value:.17}:{}", bob.did(), now);
        let wrong_signature = wrong_signer.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(), // Claims to be observer
            signature: wrong_signature.to_bytes().to_vec(), // But signed by wrong_signer
            metric_type: TechnicalMetricType::Uptime,
            value,
            observed_at: now,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(
            !result.valid,
            "Invalid observation signature should be rejected"
        );
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::InvalidObservationSignature { .. }
        ));
    }

    #[test]
    fn test_production_config_rejects_unsigned_observations() {
        let store = test_store();
        let config = EvidenceValidatorConfig::production();
        let validator = EvidenceValidator::with_config(store, config);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Unsigned observation
        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(),
            metric_type: TechnicalMetricType::Uptime,
            value: 0.99,
            observed_at: now,
            signature: vec![], // Empty signature
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(
            !result.valid,
            "Production config should reject unsigned observations"
        );
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::MissingObservationSignature { .. }
        ));
    }

    #[test]
    fn test_external_attestation_with_provider_key() {
        let store = test_store();

        // Create a mock provider key
        let provider_keypair = KeyPair::generate().unwrap();
        let provider_public_key: [u8; 32] = provider_keypair.verifying_key().as_bytes().to_owned();

        let config = EvidenceValidatorConfig::default()
            .with_provider_key("test_provider", provider_public_key);
        let validator = EvidenceValidator::with_config(store, config);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create properly signed attestation
        let message = format!("attestation:test_provider:attest123:{}", bob.did());
        let signature = provider_keypair.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::ExternalAttestation {
            provider: "test_provider".to_string(),
            attestation_id: "attest123".to_string(),
            signature: signature.to_bytes().to_vec(),
            metadata: None,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid, "Valid provider signature should be accepted");
    }

    #[test]
    fn test_external_attestation_invalid_provider_signature() {
        let store = test_store();

        // Create a mock provider key
        let provider_keypair = KeyPair::generate().unwrap();
        let wrong_keypair = KeyPair::generate().unwrap();
        let provider_public_key: [u8; 32] = provider_keypair.verifying_key().as_bytes().to_owned();

        let config = EvidenceValidatorConfig::default()
            .with_provider_key("test_provider", provider_public_key);
        let validator = EvidenceValidator::with_config(store, config);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Sign with wrong key
        let message = format!("attestation:test_provider:attest123:{}", bob.did());
        let wrong_signature = wrong_keypair.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::ExternalAttestation {
            provider: "test_provider".to_string(),
            attestation_id: "attest123".to_string(),
            signature: wrong_signature.to_bytes().to_vec(),
            metadata: None,
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(
            !result.valid,
            "Invalid provider signature should be rejected"
        );
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::InvalidAttestationSignature
        ));
    }
}
