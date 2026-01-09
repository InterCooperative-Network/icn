//! Sybil resistance for trust computation
//!
//! This module implements Sybil attack mitigation through:
//! - VUI (Verified Unique Identity) status tracking
//! - Trust decay for unverified identities
//! - Integration with anomaly detection
//!
//! # Sybil Attack Mitigation
//!
//! Sybil attacks occur when an attacker creates many fake identities to
//! artificially inflate their trust or influence in the network. This
//! module provides several defenses:
//!
//! 1. **VUI Verification**: Identities can be marked as "verified unique"
//!    through external verification mechanisms (e.g., cooperative membership,
//!    phone verification, vouching by trusted members).
//!
//! 2. **Trust Decay**: Unverified identities receive a decay penalty on their
//!    trust scores, reducing the effectiveness of newly created fake identities.
//!
//! 3. **Anomaly Integration**: The `TrustGraphAnalyzer` detects suspicious patterns
//!    like circular vouching and tight clusters, which are flagged here.
//!
//! # Example
//!
//! ```ignore
//! use icn_trust::sybil::{SybilResistance, VerificationStatus, VerificationMethod};
//!
//! // Create Sybil resistance layer
//! let mut sybil = SybilResistance::new(store, config);
//!
//! // Mark an identity as verified through cooperative membership
//! sybil.set_verification_status(
//!     &member_did,
//!     VerificationStatus::Verified {
//!         method: VerificationMethod::CooperativeMembership,
//!         verified_at: timestamp,
//!         verified_by: Some(admin_did),
//!         expires_at: None,
//!     },
//! )?;
//!
//! // Base trust score from the trust computation (example value)
//! let base_trust_score: f64 = 0.8;
//!
//! // Compute trust with Sybil resistance
//! let effective_score = sybil.compute_effective_trust(base_trust_score, &target)?;
//! ```

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};

/// Storage key prefix for verification status
const VERIFICATION_PREFIX: &str = "sybil:verification:";

/// Storage key prefix for Sybil flags
const SYBIL_FLAG_PREFIX: &str = "sybil:flag:";

/// Verification method used to establish identity uniqueness
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    /// Verified through cooperative membership records
    CooperativeMembership,

    /// Verified through phone/SMS verification
    PhoneVerification,

    /// Vouched for by a trusted member (requires min trust threshold)
    TrustedVouch,

    /// Verified through government ID (privacy-preserving ZKP)
    GovernmentId,

    /// Verified through in-person meeting
    InPerson,

    /// External verification service
    External { service: String },
}

/// Verification status of an identity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum VerificationStatus {
    /// Identity has not been verified
    Unverified,

    /// Verification is pending
    Pending {
        /// Method being used for verification
        method: VerificationMethod,
        /// When verification was initiated
        initiated_at: u64,
    },

    /// Identity has been verified as unique
    Verified {
        /// Method used for verification
        method: VerificationMethod,
        /// When the identity was verified
        verified_at: u64,
        /// Who performed the verification (if applicable)
        verified_by: Option<Did>,
        /// When the verification expires (if applicable)
        expires_at: Option<u64>,
    },

    /// Verification was revoked
    Revoked {
        /// Reason for revocation
        reason: String,
        /// When revoked
        revoked_at: u64,
        /// Who revoked it
        revoked_by: Option<Did>,
    },
}

impl VerificationStatus {
    /// Check if this identity is currently verified
    pub fn is_verified(&self, now: u64) -> bool {
        match self {
            Self::Verified { expires_at, .. } => {
                // Check expiration if set
                expires_at.is_none_or(|exp| now <= exp)
            }
            _ => false,
        }
    }

    /// Check if this identity is unverified
    pub fn is_unverified(&self) -> bool {
        matches!(self, Self::Unverified)
    }
}

impl Default for VerificationStatus {
    fn default() -> Self {
        Self::Unverified
    }
}

/// Sybil flag indicating suspicious identity behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SybilFlag {
    /// The DID that was flagged
    pub did: Did,

    /// Type of suspicious behavior detected
    pub flag_type: SybilFlagType,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// When the flag was created
    pub flagged_at: u64,

    /// Additional evidence or context
    pub evidence: Vec<String>,
}

/// Types of Sybil-related suspicious behavior
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SybilFlagType {
    /// Part of a detected circular vouching pattern
    CircularVouching,

    /// Part of a detected Sybil cluster
    SybilCluster,

    /// Rapid trust growth detected
    RapidTrustGrowth,

    /// Multiple identities from same origin
    SharedOrigin,

    /// Coordinated activity pattern
    CoordinatedActivity,
}

/// Configuration for Sybil resistance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SybilResistanceConfig {
    /// Decay factor for unverified identities (0.0 - 1.0)
    /// Applied as: effective_trust = base_trust * (1.0 - unverified_decay)
    pub unverified_decay: f64,

    /// Decay factor for pending verification
    pub pending_decay: f64,

    /// Decay factor for flagged identities (per flag)
    pub flagged_decay: f64,

    /// Minimum trust required for a vouch to count as verification
    pub vouch_trust_threshold: f64,

    /// Maximum decay that can be applied (floor)
    pub max_decay: f64,

    /// Whether to require VUI for trust scores above threshold
    pub require_vui_above_threshold: f64,
}

impl Default for SybilResistanceConfig {
    fn default() -> Self {
        Self {
            unverified_decay: 0.3,            // 30% reduction for unverified
            pending_decay: 0.15,              // 15% reduction for pending
            flagged_decay: 0.2,               // 20% reduction per flag
            vouch_trust_threshold: 0.7,       // Partner level required to vouch
            max_decay: 0.8,                   // Never decay more than 80%
            require_vui_above_threshold: 0.5, // Require VUI for scores > 0.5
        }
    }
}

/// Sybil resistance layer for trust computation
///
/// Wraps trust graph queries with VUI verification and anomaly awareness.
pub struct SybilResistance {
    store: Arc<dyn Store>,
    config: SybilResistanceConfig,
}

impl SybilResistance {
    /// Create a new Sybil resistance layer
    pub fn new(store: Arc<dyn Store>, config: SybilResistanceConfig) -> Self {
        Self { store, config }
    }

    /// Create with default configuration
    pub fn with_defaults(store: Arc<dyn Store>) -> Self {
        Self::new(store, SybilResistanceConfig::default())
    }

    /// Get the configuration
    pub fn config(&self) -> &SybilResistanceConfig {
        &self.config
    }

    // ================================================================
    // Verification Status Management
    // ================================================================

    /// Get the verification status for a DID
    pub fn get_verification_status(&self, did: &Did) -> Result<VerificationStatus> {
        let key = format!("{}{}", VERIFICATION_PREFIX, did.as_str());
        match self.store.get(key.as_bytes())? {
            Some(value) => {
                let status: VerificationStatus = serde_json::from_slice(&value)?;
                Ok(status)
            }
            None => Ok(VerificationStatus::Unverified),
        }
    }

    /// Set the verification status for a DID
    pub fn set_verification_status(&self, did: &Did, status: VerificationStatus) -> Result<()> {
        let key = format!("{}{}", VERIFICATION_PREFIX, did.as_str());
        let value = serde_json::to_vec(&status)?;
        self.store.put(key.as_bytes(), &value)?;
        debug!("Set verification status for {}: {:?}", did, status);
        Ok(())
    }

    /// Mark an identity as verified
    pub fn verify_identity(
        &self,
        did: &Did,
        method: VerificationMethod,
        verified_by: Option<Did>,
        expires_at: Option<u64>,
    ) -> Result<()> {
        let status = VerificationStatus::Verified {
            method,
            verified_at: icn_time::current_timestamp_secs(),
            verified_by,
            expires_at,
        };
        self.set_verification_status(did, status)
    }

    /// Revoke verification for an identity
    pub fn revoke_verification(
        &self,
        did: &Did,
        reason: String,
        revoked_by: Option<Did>,
    ) -> Result<()> {
        let status = VerificationStatus::Revoked {
            reason,
            revoked_at: icn_time::current_timestamp_secs(),
            revoked_by,
        };
        self.set_verification_status(did, status)
    }

    // ================================================================
    // Sybil Flag Management
    // ================================================================

    /// Add a Sybil flag for a DID
    pub fn add_sybil_flag(&self, flag: SybilFlag) -> Result<()> {
        // Include flag_type in key to prevent collision when multiple flags
        // are added for the same DID within the same second
        let key = format!(
            "{}{}:{}:{:?}",
            SYBIL_FLAG_PREFIX,
            flag.did.as_str(),
            flag.flagged_at,
            flag.flag_type
        );
        let value = serde_json::to_vec(&flag)?;
        self.store.put(key.as_bytes(), &value)?;
        warn!(
            "Added Sybil flag for {}: {:?} (confidence: {})",
            flag.did, flag.flag_type, flag.confidence
        );
        Ok(())
    }

    /// Get all Sybil flags for a DID
    pub fn get_sybil_flags(&self, did: &Did) -> Result<Vec<SybilFlag>> {
        let prefix = format!("{}{}", SYBIL_FLAG_PREFIX, did.as_str());
        let results = self.store.scan(prefix.as_bytes())?;

        let mut flags = Vec::new();
        for (_key, value) in results {
            let flag: SybilFlag = serde_json::from_slice(&value)?;
            flags.push(flag);
        }
        Ok(flags)
    }

    /// Clear all Sybil flags for a DID
    pub fn clear_sybil_flags(&self, did: &Did) -> Result<usize> {
        let prefix = format!("{}{}", SYBIL_FLAG_PREFIX, did.as_str());
        let results = self.store.scan(prefix.as_bytes())?;

        let mut cleared = 0;
        for (key, _value) in results {
            self.store.delete(&key)?;
            cleared += 1;
        }

        if cleared > 0 {
            debug!("Cleared {} Sybil flags for {}", cleared, did);
        }
        Ok(cleared)
    }

    // ================================================================
    // Trust Computation with Sybil Resistance
    // ================================================================

    /// Compute the decay factor for a DID based on verification and flags
    ///
    /// Returns a value between 0.0 and 1.0 where:
    /// - 1.0 = no decay (fully verified, no flags)
    /// - 0.0 = maximum decay
    pub fn compute_decay_factor(&self, did: &Did) -> Result<f64> {
        let now = icn_time::current_timestamp_secs();
        let status = self.get_verification_status(did)?;
        let flags = self.get_sybil_flags(did)?;

        // Start with no decay
        let mut total_decay = 0.0;

        // Apply verification-based decay
        match &status {
            VerificationStatus::Unverified => {
                total_decay += self.config.unverified_decay;
            }
            VerificationStatus::Pending { .. } => {
                total_decay += self.config.pending_decay;
            }
            VerificationStatus::Verified { expires_at, .. } => {
                // Check if expired
                if let Some(exp) = expires_at {
                    if now > *exp {
                        total_decay += self.config.unverified_decay;
                    }
                }
                // Verified and not expired: no decay
            }
            VerificationStatus::Revoked { .. } => {
                // Revoked gets full unverified decay
                total_decay += self.config.unverified_decay;
            }
        }

        // Apply flag-based decay (weighted by confidence)
        for flag in &flags {
            total_decay += self.config.flagged_decay * flag.confidence;
        }

        // Cap at maximum decay
        total_decay = total_decay.min(self.config.max_decay);

        // Return the decay factor (1.0 - decay = the multiplier)
        Ok(1.0 - total_decay)
    }

    /// Compute effective trust score with Sybil resistance
    ///
    /// Takes a base trust score and applies:
    /// 1. Verification-based decay
    /// 2. Flag-based decay
    /// 3. VUI requirement enforcement
    pub fn compute_effective_trust(&self, base_score: f64, did: &Did) -> Result<f64> {
        let decay_factor = self.compute_decay_factor(did)?;
        let effective_score = base_score * decay_factor;

        // Enforce VUI requirement for high trust
        if effective_score > self.config.require_vui_above_threshold {
            let now = icn_time::current_timestamp_secs();
            let status = self.get_verification_status(did)?;

            if !status.is_verified(now) {
                // Cap at threshold if not verified
                debug!(
                    "Capping trust for {} at {} (VUI required for higher scores)",
                    did,
                    self.config.require_vui_above_threshold,
                );
                return Ok(self.config.require_vui_above_threshold);
            }
        }

        Ok(effective_score)
    }

    /// Check if an identity can vouch for others (for TrustedVouch verification)
    pub fn can_vouch(&self, voucher_did: &Did, voucher_trust_score: f64) -> Result<bool> {
        // Voucher must be verified
        let now = icn_time::current_timestamp_secs();
        let status = self.get_verification_status(voucher_did)?;

        if !status.is_verified(now) {
            return Ok(false);
        }

        // Voucher must have sufficient trust
        if voucher_trust_score < self.config.vouch_trust_threshold {
            return Ok(false);
        }

        // Voucher must not be flagged
        let flags = self.get_sybil_flags(voucher_did)?;
        if !flags.is_empty() {
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_verification_status_default() {
        let status = VerificationStatus::default();
        assert!(status.is_unverified());
    }

    #[test]
    fn test_verification_status_verified() {
        let now = icn_time::current_timestamp_secs();
        let status = VerificationStatus::Verified {
            method: VerificationMethod::CooperativeMembership,
            verified_at: now,
            verified_by: None,
            expires_at: None,
        };
        assert!(status.is_verified(now));
        assert!(!status.is_unverified());
    }

    #[test]
    fn test_verification_status_expired() {
        let now = icn_time::current_timestamp_secs();
        let status = VerificationStatus::Verified {
            method: VerificationMethod::PhoneVerification,
            verified_at: now - 1000,
            verified_by: None,
            expires_at: Some(now - 100), // Expired
        };
        assert!(!status.is_verified(now));
    }

    #[test]
    fn test_verification_status_expires_at_boundary() {
        let now = icn_time::current_timestamp_secs();
        // Expires exactly at 'now' - should still be valid (now <= expires_at)
        let status = VerificationStatus::Verified {
            method: VerificationMethod::PhoneVerification,
            verified_at: now - 1000,
            verified_by: None,
            expires_at: Some(now),
        };
        assert!(
            status.is_verified(now),
            "Verification should still be valid when expires_at == now"
        );
    }

    #[test]
    fn test_sybil_resistance_verification() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Initially unverified
        let status = sybil.get_verification_status(&did).unwrap();
        assert!(status.is_unverified());

        // Verify the identity
        sybil
            .verify_identity(&did, VerificationMethod::CooperativeMembership, None, None)
            .unwrap();

        // Now verified
        let now = icn_time::current_timestamp_secs();
        let status = sybil.get_verification_status(&did).unwrap();
        assert!(status.is_verified(now));
    }

    #[test]
    fn test_sybil_resistance_revocation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Verify first
        sybil
            .verify_identity(&did, VerificationMethod::InPerson, None, None)
            .unwrap();

        let now = icn_time::current_timestamp_secs();
        assert!(sybil
            .get_verification_status(&did)
            .unwrap()
            .is_verified(now));

        // Revoke
        sybil
            .revoke_verification(&did, "Suspicious activity".to_string(), None)
            .unwrap();

        // No longer verified
        let status = sybil.get_verification_status(&did).unwrap();
        assert!(!status.is_verified(now));
        assert!(matches!(status, VerificationStatus::Revoked { .. }));
    }

    #[test]
    fn test_decay_factor_verified() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Verify the identity
        sybil
            .verify_identity(&did, VerificationMethod::CooperativeMembership, None, None)
            .unwrap();

        // Verified identity should have no decay (factor = 1.0)
        let decay = sybil.compute_decay_factor(&did).unwrap();
        assert!((decay - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_decay_factor_unverified() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Unverified identity should have 30% decay (factor = 0.7)
        let decay = sybil.compute_decay_factor(&did).unwrap();
        assert!((decay - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_decay_factor_with_flags() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Add a Sybil flag with 100% confidence
        sybil
            .add_sybil_flag(SybilFlag {
                did: did.clone(),
                flag_type: SybilFlagType::CircularVouching,
                confidence: 1.0,
                flagged_at: icn_time::current_timestamp_secs(),
                evidence: vec!["cycle:A->B->C->A".to_string()],
            })
            .unwrap();

        // Unverified (30%) + 1 flag at 100% (20%) = 50% decay = 0.5 factor
        let decay = sybil.compute_decay_factor(&did).unwrap();
        assert!((decay - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_decay_factor_max_cap() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Add multiple flags
        for i in 0..10 {
            sybil
                .add_sybil_flag(SybilFlag {
                    did: did.clone(),
                    flag_type: SybilFlagType::SybilCluster,
                    confidence: 1.0,
                    flagged_at: icn_time::current_timestamp_secs() + i,
                    evidence: vec![],
                })
                .unwrap();
        }

        // Should be capped at max_decay (80%), so factor = 0.2
        let decay = sybil.compute_decay_factor(&did).unwrap();
        assert!((decay - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_effective_trust_unverified() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Base score of 0.6, unverified decay of 30%
        // Effective = 0.6 * 0.7 = 0.42
        let effective = sybil.compute_effective_trust(0.6, &did).unwrap();
        assert!((effective - 0.42).abs() < 0.001);
    }

    #[test]
    fn test_effective_trust_vui_cap() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Base score of 1.0, unverified
        // Without cap: 1.0 * 0.7 = 0.7
        // But VUI required above 0.5, so capped at 0.5
        let effective = sybil.compute_effective_trust(1.0, &did).unwrap();
        assert!((effective - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_effective_trust_verified_no_cap() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Verify the identity
        sybil
            .verify_identity(&did, VerificationMethod::CooperativeMembership, None, None)
            .unwrap();

        // Base score of 1.0, verified = no decay, no cap
        let effective = sybil.compute_effective_trust(1.0, &did).unwrap();
        assert!((effective - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_can_vouch() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let voucher = test_did();

        // Unverified voucher cannot vouch
        assert!(!sybil.can_vouch(&voucher, 0.8).unwrap());

        // Verify the voucher
        sybil
            .verify_identity(&voucher, VerificationMethod::InPerson, None, None)
            .unwrap();

        // Verified but low trust cannot vouch (threshold is 0.7)
        assert!(!sybil.can_vouch(&voucher, 0.5).unwrap());

        // Verified with high trust can vouch
        assert!(sybil.can_vouch(&voucher, 0.8).unwrap());
    }

    #[test]
    fn test_can_vouch_flagged() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let voucher = test_did();

        // Verify the voucher
        sybil
            .verify_identity(&voucher, VerificationMethod::InPerson, None, None)
            .unwrap();

        // Flag the voucher
        sybil
            .add_sybil_flag(SybilFlag {
                did: voucher.clone(),
                flag_type: SybilFlagType::SybilCluster,
                confidence: 0.5,
                flagged_at: icn_time::current_timestamp_secs(),
                evidence: vec![],
            })
            .unwrap();

        // Flagged voucher cannot vouch even with high trust
        assert!(!sybil.can_vouch(&voucher, 0.9).unwrap());
    }

    #[test]
    fn test_clear_sybil_flags() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let sybil = SybilResistance::with_defaults(store);
        let did = test_did();

        // Add multiple flags
        for i in 0..3 {
            sybil
                .add_sybil_flag(SybilFlag {
                    did: did.clone(),
                    flag_type: SybilFlagType::RapidTrustGrowth,
                    confidence: 0.5,
                    flagged_at: icn_time::current_timestamp_secs() + i,
                    evidence: vec![],
                })
                .unwrap();
        }

        assert_eq!(sybil.get_sybil_flags(&did).unwrap().len(), 3);

        // Clear flags
        let cleared = sybil.clear_sybil_flags(&did).unwrap();
        assert_eq!(cleared, 3);
        assert!(sybil.get_sybil_flags(&did).unwrap().is_empty());
    }

    #[test]
    fn test_verification_method_serialization() {
        let method = VerificationMethod::External {
            service: "gov-id-service".to_string(),
        };
        let json = serde_json::to_string(&method).unwrap();
        assert!(json.contains("external"));
        assert!(json.contains("gov-id-service"));

        let parsed: VerificationMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, method);
    }
}
