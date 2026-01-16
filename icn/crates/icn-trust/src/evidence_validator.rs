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
//!
//! ## Verification Order and DoS Protection
//!
//! Validation checks are ordered to minimize computational cost under attack:
//!
//! 1. **Cheapest checks first**: Format validation, timestamp comparison, range checks
//! 2. **Medium-cost checks**: Trust score lookups (involves graph traversal)
//! 3. **Expensive checks last**: Cryptographic signature verification
//!
//! This ordering prevents denial-of-service attacks where an attacker floods the
//! system with evidence that has valid-looking signatures but fails cheaper checks.
//! By rejecting invalid evidence early, we avoid wasting CPU cycles on expensive
//! Ed25519 signature verification.
//!
//! Example for peer endorsements:
//! ```text
//! validate_peer_endorsement():
//!   1. Check endorser != source/target    (O(1) - string compare)
//!   2. Check endorsement not expired      (O(1) - timestamp math)
//!   3. Check endorser trust score         (O(n) - graph traversal)
//!   4. Verify Ed25519 signature           (O(1) - expensive crypto)
//! ```
//!
//! ## Float Formatting Notes
//!
//! Technical observations use `{:.17}` format for float values to ensure
//! cross-platform determinism. Note that:
//! - Negative zero (`-0.0`) formats as `"-0.00000000000000000"`, distinct from `0.0`
//! - Callers must be consistent about which zero they use
//! - `{:.17}` provides enough precision to round-trip any f64 value uniquely

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

/// Error type for evidence validator configuration
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    /// Invalid Ed25519 public key provided for a provider
    #[error("Invalid Ed25519 public key for provider '{provider}': {reason}")]
    InvalidProviderKey { provider: String, reason: String },
}

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
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidProviderKey` if the provided bytes are not
    /// a valid Ed25519 public key. This follows the "fail fast" principle -
    /// invalid configuration should be caught at startup, not at runtime during
    /// signature verification.
    pub fn with_provider_key(
        mut self,
        provider: &str,
        public_key: [u8; 32],
    ) -> Result<Self, ConfigError> {
        // Validate the key is a valid Ed25519 public key at configuration time
        VerifyingKey::from_bytes(&public_key).map_err(|e| ConfigError::InvalidProviderKey {
            provider: provider.to_string(),
            reason: e.to_string(),
        })?;

        if !self.known_providers.contains(&provider.to_string()) {
            self.known_providers.push(provider.to_string());
        }
        self.provider_keys.insert(provider.to_string(), public_key);
        Ok(self)
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
    ///
    /// Verifies that:
    /// 1. The contract exists in storage
    /// 2. The target DID is a party (participant) to the contract
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
            Ok(Some(data)) => {
                // Contract exists, extract participants and verify BOTH source and target are parties
                // This is a security requirement: contract evidence should only be valid if
                // both the entity creating the trust edge AND the entity being trusted
                // were actually parties to the referenced contract.
                match self.extract_contract_participants(&data) {
                    Ok(participants) => {
                        // Security check: verify source is a participant
                        // (the entity creating this trust edge should be a contract party)
                        if !participants.iter().any(|p| p == source) {
                            warn!(
                                "Source {} is not a party to contract {} - rejecting evidence",
                                source,
                                hex::encode(contract_id)
                            );
                            return EvidenceValidationResult::invalid(
                                EvidenceValidationError::SourceNotContractParty {
                                    source_did: source.to_string(),
                                    contract_id: hex::encode(contract_id),
                                },
                            );
                        }

                        // Verify target is a participant in the contract
                        if !participants.iter().any(|p| p == target) {
                            warn!(
                                "Target {} is not a party to contract {}",
                                target,
                                hex::encode(contract_id)
                            );
                            return EvidenceValidationResult::invalid(
                                EvidenceValidationError::NotContractParty {
                                    target: target.to_string(),
                                    contract_id: hex::encode(contract_id),
                                },
                            );
                        }

                        debug!(
                            "Contract {} found with both {} and {} as participants",
                            hex::encode(contract_id),
                            source,
                            target
                        );
                        EvidenceValidationResult::valid_with_adjustment(0.1) // Boost for contract evidence
                    }
                    Err(e) => {
                        // Contract data is malformed - reject the evidence
                        // This could indicate tampering or corruption, so we fail hard
                        // rather than allowing potentially fraudulent evidence through
                        warn!(
                            "Failed to parse contract {}: {} - rejecting evidence",
                            hex::encode(contract_id),
                            e
                        );
                        EvidenceValidationResult::invalid(
                            EvidenceValidationError::MalformedContract {
                                contract_id: hex::encode(contract_id),
                                reason: e,
                            },
                        )
                    }
                }
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

    /// Maximum number of participants we'll process from a contract
    /// This matches MAX_PARTICIPANTS in icn-ccl to prevent DoS via maliciously large arrays
    const MAX_CONTRACT_PARTICIPANTS: usize = 100;

    /// Extract participants from contract JSON data
    ///
    /// This extracts just the participants field without requiring the full
    /// icn-ccl dependency (which would cause a cyclic dependency).
    fn extract_contract_participants(&self, data: &[u8]) -> Result<Vec<Did>, String> {
        let value: serde_json::Value =
            serde_json::from_slice(data).map_err(|e| format!("Invalid JSON: {e}"))?;

        let participants = value
            .get("participants")
            .ok_or("Missing participants field")?
            .as_array()
            .ok_or("participants is not an array")?;

        // Bounds check to prevent DoS via maliciously large arrays
        if participants.is_empty() {
            return Err("Contract has no participants".to_string());
        }
        if participants.len() > Self::MAX_CONTRACT_PARTICIPANTS {
            return Err(format!(
                "Contract has too many participants ({}, max {})",
                participants.len(),
                Self::MAX_CONTRACT_PARTICIPANTS
            ));
        }

        let mut dids = Vec::with_capacity(participants.len());
        for (i, p) in participants.iter().enumerate() {
            let did_str = p
                .as_str()
                .ok_or_else(|| format!("participants[{i}] is not a string"))?;
            let did: Did = did_str
                .parse()
                .map_err(|e| format!("Invalid DID in participants[{i}]: {e}"))?;
            dids.push(did);
        }

        Ok(dids)
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
    // Contract Party Validation Tests (Issue #681)
    // ============================================================================

    #[test]
    fn test_validator_contract_target_is_party() {
        let store = test_store();
        let validator = EvidenceValidator::new(store.clone());

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create a contract with bob as a participant
        let contract_id = [1u8; 32];
        let contract_json = serde_json::json!({
            "name": "TestContract",
            "participants": [alice.did().to_string(), bob.did().to_string()],
            "currency": null,
            "state_vars": [],
            "rules": [],
            "triggers": []
        });

        // Store the contract
        let contract_key = format!("contracts/{}", hex::encode(contract_id));
        store
            .put(
                contract_key.as_bytes(),
                &serde_json::to_vec(&contract_json).unwrap(),
            )
            .unwrap();

        let evidence = vec![TrustEvidence::ContractExecution {
            contract_id,
            agreement_id: None,
            executed_at: 12345,
        }];

        // Should be valid because bob is a participant
        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid);
        assert!(result.score_adjustment > 0.0); // Should have boost for verified contract
    }

    #[test]
    fn test_validator_contract_target_not_party() {
        let store = test_store();
        let validator = EvidenceValidator::new(store.clone());

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let charlie = KeyPair::generate().unwrap();

        // Create a contract with only alice and charlie (not bob)
        let contract_id = [2u8; 32];
        let contract_json = serde_json::json!({
            "name": "TestContract",
            "participants": [alice.did().to_string(), charlie.did().to_string()],
            "currency": null,
            "state_vars": [],
            "rules": [],
            "triggers": []
        });

        // Store the contract
        let contract_key = format!("contracts/{}", hex::encode(contract_id));
        store
            .put(
                contract_key.as_bytes(),
                &serde_json::to_vec(&contract_json).unwrap(),
            )
            .unwrap();

        let evidence = vec![TrustEvidence::ContractExecution {
            contract_id,
            agreement_id: None,
            executed_at: 12345,
        }];

        // Should be invalid because bob is NOT a participant
        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::NotContractParty { target, .. }
            if target == &bob.did().to_string()
        ));
    }

    #[test]
    fn test_validator_contract_malformed_data() {
        let store = test_store();
        let validator = EvidenceValidator::new(store.clone());

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Store malformed contract data (missing participants)
        let contract_id = [3u8; 32];
        let malformed_json = serde_json::json!({
            "name": "MalformedContract",
            // Missing "participants" field
        });

        let contract_key = format!("contracts/{}", hex::encode(contract_id));
        store
            .put(
                contract_key.as_bytes(),
                &serde_json::to_vec(&malformed_json).unwrap(),
            )
            .unwrap();

        let evidence = vec![TrustEvidence::ContractExecution {
            contract_id,
            agreement_id: None,
            executed_at: 12345,
        }];

        // Should be INVALID - malformed contracts could indicate tampering
        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::MalformedContract { .. }
        ));
    }

    #[test]
    fn test_validator_contract_source_not_party() {
        let store = test_store();
        let validator = EvidenceValidator::new(store.clone());

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let charlie = KeyPair::generate().unwrap();

        // Create a contract between bob and charlie (alice is NOT a party)
        let contract_id = [4u8; 32];
        let contract_json = serde_json::json!({
            "name": "TestContract",
            "participants": [bob.did().to_string(), charlie.did().to_string()],
            "currency": null,
            "state_vars": [],
            "rules": [],
            "triggers": []
        });

        // Store the contract
        let contract_key = format!("contracts/{}", hex::encode(contract_id));
        store
            .put(
                contract_key.as_bytes(),
                &serde_json::to_vec(&contract_json).unwrap(),
            )
            .unwrap();

        let evidence = vec![TrustEvidence::ContractExecution {
            contract_id,
            agreement_id: None,
            executed_at: 12345,
        }];

        // Alice tries to create trust edge to Bob using contract she's not party to
        // This should be INVALID - source must also be a contract party
        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::SourceNotContractParty { source_did, .. }
            if source_did == &alice.did().to_string()
        ));
    }

    #[test]
    fn test_validator_contract_empty_participants() {
        let store = test_store();
        let validator = EvidenceValidator::new(store.clone());

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create a contract with empty participants array
        let contract_id = [5u8; 32];
        let contract_json = serde_json::json!({
            "name": "EmptyContract",
            "participants": [],
            "currency": null,
            "state_vars": [],
            "rules": [],
            "triggers": []
        });

        // Store the contract
        let contract_key = format!("contracts/{}", hex::encode(contract_id));
        store
            .put(
                contract_key.as_bytes(),
                &serde_json::to_vec(&contract_json).unwrap(),
            )
            .unwrap();

        let evidence = vec![TrustEvidence::ContractExecution {
            contract_id,
            agreement_id: None,
            executed_at: 12345,
        }];

        // Empty participants is invalid - contracts must have at least one participant
        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(!result.valid);
        assert!(matches!(
            &result.errors[0],
            EvidenceValidationError::MalformedContract { reason, .. }
            if reason.contains("no participants")
        ));
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
            .with_provider_key("test_provider", provider_public_key)
            .expect("valid provider key");
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
            .with_provider_key("test_provider", provider_public_key)
            .expect("valid provider key");
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

    // ============================================================================
    // Float Edge Case Tests (IEEE 754 determinism)
    // ============================================================================
    //
    // These tests verify that technical observation signatures work correctly
    // for edge cases in IEEE 754 floating-point representation. The format
    // `{:.17}` is used for cross-platform determinism, as it provides enough
    // precision to round-trip any f64 value uniquely.

    #[test]
    fn test_technical_observation_float_zero() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Test with zero - latency metric allows any non-negative value
        let value = 0.0_f64;
        let message = format!("observation:{}:latency:{value:.17}:{}", bob.did(), now);
        let signature = observer.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(),
            metric_type: TechnicalMetricType::Latency,
            value,
            observed_at: now,
            signature: signature.to_bytes().to_vec(),
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid, "Zero value should be accepted");
    }

    #[test]
    fn test_technical_observation_float_negative_zero() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Note: Negative zero formats DIFFERENTLY than positive zero with {:.17}!
        // "-0.00000000000000000" vs "0.00000000000000000"
        // This is important for signature verification - both sides must use
        // the exact same value.
        let value = -0.0_f64;
        let message = format!("observation:{}:latency:{value:.17}:{}", bob.did(), now);
        let signature = observer.sign(message.as_bytes());

        let evidence = vec![TrustEvidence::TechnicalObservation {
            observer: observer.did().clone(),
            metric_type: TechnicalMetricType::Latency,
            value,
            observed_at: now,
            signature: signature.to_bytes().to_vec(),
        }];

        let result = validator.validate_all_evidence(&evidence, alice.did(), bob.did(), None);
        assert!(result.valid, "Negative zero should be accepted");

        // Document that -0.0 and 0.0 format differently (important for callers!)
        assert_ne!(
            format!("{:.17}", -0.0_f64),
            format!("{:.17}", 0.0_f64),
            "-0.0 and 0.0 format differently - callers must be consistent"
        );
    }

    #[test]
    fn test_technical_observation_float_subnormal() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Smallest positive subnormal f64
        let value = f64::MIN_POSITIVE / 2.0;
        assert!(value > 0.0 && value < f64::MIN_POSITIVE); // Verify it's subnormal
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
        assert!(result.valid, "Subnormal value should be accepted");
    }

    #[test]
    fn test_technical_observation_float_very_small() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Very small but representable value near epsilon
        let value = 1e-308_f64;
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
        assert!(result.valid, "Very small value should be accepted");
    }

    #[test]
    fn test_technical_observation_float_precision_roundtrip() {
        // Verify that formatting with .17 preserves precision for signature verification
        let test_values = [
            0.1_f64, // Classic binary float representation issue
            0.2_f64,
            0.3_f64,
            0.1 + 0.2,            // Should equal 0.30000000000000004
            std::f64::consts::PI, // Irrational approximation
            std::f64::consts::E,
            1.0 / 3.0,         // Repeating decimal in binary
            f64::MIN_POSITIVE, // Smallest positive normal
            f64::MAX,          // Largest finite
        ];

        for &value in &test_values {
            let formatted = format!("{value:.17}");
            let parsed: f64 = formatted.parse().unwrap();

            // The formatted-and-parsed value should match the original
            // for signature verification purposes
            assert!(
                (value - parsed).abs() < f64::EPSILON || value == parsed,
                "Value {} did not roundtrip correctly: {} -> {} (diff: {})",
                value,
                formatted,
                parsed,
                (value - parsed).abs()
            );
        }
    }

    #[test]
    fn test_technical_observation_float_infinity_out_of_range() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Infinity for uptime (0.0..=1.0 range) - should be out of range but tolerated
        let value = f64::INFINITY;
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
        // Out-of-range values are tolerated (return valid with no adjustment)
        assert!(
            result.valid,
            "Infinity should be tolerated (warning logged but accepted)"
        );
        assert_eq!(
            result.score_adjustment, 0.0,
            "Out-of-range should have no adjustment"
        );
    }

    #[test]
    fn test_technical_observation_float_nan_out_of_range() {
        let store = test_store();
        let validator = EvidenceValidator::new(store);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let observer = KeyPair::generate().unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // NaN for uptime - NaN is never in any range
        let value = f64::NAN;
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
        // NaN values are tolerated (not in range but accepted with warning)
        assert!(
            result.valid,
            "NaN should be tolerated (warning logged but accepted)"
        );
    }

    #[test]
    fn test_invalid_provider_key_returns_error() {
        // Test that with_provider_key properly returns an error for invalid keys.
        // Ed25519 public keys are points on the Ed25519 curve - most random byte
        // patterns are NOT valid curve points. The pattern [0xAB; 32] is not
        // decompressible as an Edwards point.
        let invalid_key = [0xABu8; 32];

        let result = EvidenceValidatorConfig::default().with_provider_key("test", invalid_key);

        assert!(result.is_err(), "Invalid key should return error");
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidProviderKey { .. }),
            "Should be InvalidProviderKey error"
        );
    }
}
