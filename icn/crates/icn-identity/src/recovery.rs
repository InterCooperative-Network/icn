//! Social recovery implementation for ICN identities
//!
//! Allows users to recover lost identities with help from M-of-N trusted friends (trustees).
//!
//! ## Recovery Flow
//!
//! 1. **Setup**: User configures N trustees and threshold M
//! 2. **Loss**: User loses all devices, creates new identity on new device
//! 3. **Request**: User initiates recovery, contacting trustees out-of-band
//! 4. **Attest**: Each trustee verifies user's identity and signs attestation
//! 5. **Collect**: Once M attestations received, recovery enters delay period
//! 6. **Finalize**: After delay expires, new DID inherits old identity
//! 7. **Cancel**: Original owner can cancel fraudulent recovery attempts

use crate::{Did, KeyPair};
use anyhow::{bail, Result};
use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};

/// A recovery event represents an attempt to recover a lost identity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryEvent {
    /// Unique ID for this recovery event (hash of old_did + new_did + initiated_at)
    pub id: String,

    /// The DID being recovered (lost identity)
    pub old_did: Did,

    /// The new DID that will inherit the identity
    pub new_did: Did,

    /// When recovery was initiated (Unix timestamp in seconds)
    pub initiated_at: u64,

    /// When recovery was finalized (Unix timestamp in seconds)
    pub finalized_at: Option<u64>,

    /// Recovery attestations from trustees
    pub attestations: Vec<RecoveryAttestation>,

    /// Required number of attestations (M)
    pub threshold: usize,

    /// Delay period in seconds before finalization
    pub delay_period: u64,

    /// Current status of this recovery
    pub status: RecoveryStatus,
}

/// An attestation from a recovery trustee
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryAttestation {
    /// Trustee's DID
    pub trustee: Did,

    /// Old DID being recovered
    pub old_did: Did,

    /// New DID to inherit identity
    pub new_did: Did,

    /// Trustee's signature over (old_did, new_did, timestamp)
    pub signature: Vec<u8>, // Serialized Signature

    /// When attestation was created (Unix timestamp in seconds)
    pub timestamp: u64,

    /// How the trustee verified the user's identity
    /// Examples: "in-person meeting", "video call", "shared secret answer"
    pub verification_method: String,
}

/// Status of a recovery event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Collecting attestations (need M signatures)
    Pending {
        /// How many attestations collected so far
        collected: usize,
        /// How many more needed
        needed: usize,
    },

    /// Waiting for delay period to expire before finalization
    Delayed {
        /// When delay period expires (Unix timestamp in seconds)
        expires_at: u64,
    },

    /// Ready to finalize (M attestations collected, delay expired)
    ReadyToFinalize,

    /// Recovery completed successfully
    Finalized,

    /// Recovery cancelled (fraudulent attempt or user recovered device)
    Cancelled {
        /// Who cancelled (should be someone with authority over old_did)
        cancelled_by: Did,
        /// Reason for cancellation
        reason: String,
        /// When cancelled (Unix timestamp in seconds)
        cancelled_at: u64,
    },
}

impl RecoveryEvent {
    /// Create a new recovery event
    pub fn new(old_did: Did, new_did: Did, threshold: usize, delay_period: u64) -> Self {
        let initiated_at = current_timestamp();

        // Generate unique ID (hash of key fields)
        let id = Self::generate_id(&old_did, &new_did, initiated_at);

        RecoveryEvent {
            id,
            old_did,
            new_did,
            initiated_at,
            finalized_at: None,
            attestations: Vec::new(),
            threshold,
            delay_period,
            status: RecoveryStatus::Pending {
                collected: 0,
                needed: threshold,
            },
        }
    }

    /// Generate recovery event ID
    fn generate_id(old_did: &Did, new_did: &Did, initiated_at: u64) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(old_did.as_str().as_bytes());
        hasher.update(new_did.as_str().as_bytes());
        hasher.update(initiated_at.to_le_bytes());

        let hash = hasher.finalize();
        hex::encode(&hash[0..16]) // Use first 16 bytes for compact ID
    }

    /// Add an attestation to this recovery event
    ///
    /// Returns `Ok(true)` if this attestation completes the threshold (M signatures collected)
    pub fn add_attestation(&mut self, attestation: RecoveryAttestation) -> Result<bool> {
        // Verify attestation is for this recovery
        if attestation.old_did != self.old_did {
            bail!("Attestation old_did does not match recovery old_did");
        }
        if attestation.new_did != self.new_did {
            bail!("Attestation new_did does not match recovery new_did");
        }

        // Check if already cancelled
        if matches!(self.status, RecoveryStatus::Cancelled { .. }) {
            bail!("Cannot add attestation to cancelled recovery");
        }

        // Check if already finalized
        if matches!(self.status, RecoveryStatus::Finalized) {
            bail!("Cannot add attestation to finalized recovery");
        }

        // Check for duplicate trustee
        if self
            .attestations
            .iter()
            .any(|a| a.trustee == attestation.trustee)
        {
            bail!("Trustee {} has already attested", attestation.trustee);
        }

        // Add attestation
        self.attestations.push(attestation);

        // Check if we've reached threshold
        if self.attestations.len() >= self.threshold {
            // Enter delay period
            let expires_at = current_timestamp() + self.delay_period;

            self.status = if self.delay_period == 0 {
                // No delay configured, ready to finalize immediately
                RecoveryStatus::ReadyToFinalize
            } else {
                RecoveryStatus::Delayed { expires_at }
            };

            Ok(true) // Threshold reached
        } else {
            // Update pending status
            self.status = RecoveryStatus::Pending {
                collected: self.attestations.len(),
                needed: self.threshold - self.attestations.len(),
            };

            Ok(false) // Still collecting
        }
    }

    /// Check if delay period has expired and update status if ready
    pub fn check_delay_expired(&mut self) -> bool {
        if let RecoveryStatus::Delayed { expires_at } = self.status {
            if current_timestamp() >= expires_at {
                self.status = RecoveryStatus::ReadyToFinalize;
                return true;
            }
        }
        false
    }

    /// Finalize recovery (mark as complete)
    pub fn finalize(&mut self) -> Result<()> {
        // Can only finalize if ready
        if !matches!(self.status, RecoveryStatus::ReadyToFinalize) {
            bail!("Recovery not ready to finalize (status: {:?})", self.status);
        }

        self.finalized_at = Some(current_timestamp());
        self.status = RecoveryStatus::Finalized;

        Ok(())
    }

    /// Cancel this recovery event
    pub fn cancel(&mut self, cancelled_by: Did, reason: String) -> Result<()> {
        // Can't cancel if already finalized
        if matches!(self.status, RecoveryStatus::Finalized) {
            bail!("Cannot cancel finalized recovery");
        }

        // Can't cancel if already cancelled
        if matches!(self.status, RecoveryStatus::Cancelled { .. }) {
            bail!("Recovery already cancelled");
        }

        self.status = RecoveryStatus::Cancelled {
            cancelled_by,
            reason,
            cancelled_at: current_timestamp(),
        };

        Ok(())
    }

    /// Check if this recovery is active (not finalized or cancelled)
    pub fn is_active(&self) -> bool {
        !matches!(
            self.status,
            RecoveryStatus::Finalized | RecoveryStatus::Cancelled { .. }
        )
    }

    /// Check if this recovery is finalized
    pub fn is_finalized(&self) -> bool {
        matches!(self.status, RecoveryStatus::Finalized)
    }

    /// Get progress summary
    pub fn progress_summary(&self) -> String {
        match &self.status {
            RecoveryStatus::Pending {
                collected,
                needed: _,
            } => {
                format!("Collecting attestations: {}/{}", collected, self.threshold)
            }
            RecoveryStatus::Delayed { expires_at } => {
                let remaining = expires_at.saturating_sub(current_timestamp());
                format!("Delay period: {remaining} seconds remaining")
            }
            RecoveryStatus::ReadyToFinalize => "Ready to finalize".to_string(),
            RecoveryStatus::Finalized => {
                format!("Finalized at {}", self.finalized_at.unwrap_or(0))
            }
            RecoveryStatus::Cancelled { reason, .. } => {
                format!("Cancelled: {reason}")
            }
        }
    }
}

impl RecoveryAttestation {
    /// Create a new recovery attestation
    pub fn new(
        trustee_keypair: &KeyPair,
        old_did: Did,
        new_did: Did,
        verification_method: String,
    ) -> Result<Self> {
        let timestamp = current_timestamp();

        // Create message to sign: (old_did, new_did, timestamp)
        let message = Self::signing_message(&old_did, &new_did, timestamp);

        // Sign message
        let signature = trustee_keypair.sign(&message);

        Ok(RecoveryAttestation {
            trustee: trustee_keypair.did().clone(),
            old_did,
            new_did,
            signature: signature.to_vec(),
            timestamp,
            verification_method,
        })
    }

    /// Get the message that should be signed for this attestation
    fn signing_message(old_did: &Did, new_did: &Did, timestamp: u64) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(b"ICN_RECOVERY_ATTESTATION:");
        message.extend_from_slice(old_did.as_str().as_bytes());
        message.push(b':');
        message.extend_from_slice(new_did.as_str().as_bytes());
        message.push(b':');
        message.extend_from_slice(&timestamp.to_le_bytes());
        message
    }

    /// Verify this attestation's signature
    pub fn verify(&self, trustee_public_key: &ed25519_dalek::VerifyingKey) -> Result<()> {
        // Reconstruct message
        let message = Self::signing_message(&self.old_did, &self.new_did, self.timestamp);

        // Parse signature
        let signature: Signature = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature format"))?;

        // Verify
        use ed25519_dalek::Verifier;
        trustee_public_key
            .verify(&message, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {e}"))?;

        Ok(())
    }

    /// Check if attestation is still fresh (not too old)
    pub fn is_fresh(&self, max_age_seconds: u64) -> bool {
        let age = current_timestamp().saturating_sub(self.timestamp);
        age <= max_age_seconds
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    icn_time::current_timestamp_secs()
}

// ============================================================================
// Gossip Sync Protocol
// ============================================================================

/// Gossip topic for recovery events
pub const IDENTITY_RECOVERY_TOPIC: &str = "identity:recovery";

/// Recovery message broadcast via gossip
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryMessage {
    /// Recovery initiated by a user who lost their identity
    Initiated {
        /// Unique recovery ID
        id: String,
        /// Old DID being recovered
        old_did: Did,
        /// New DID that will inherit the identity
        new_did: Did,
        /// Recovery threshold (M-of-N)
        threshold: usize,
        /// Delay period in seconds
        delay_period: u64,
        /// When initiated
        timestamp: u64,
    },

    /// Trustee signed an attestation
    Attestation {
        /// Recovery ID this attestation applies to
        recovery_id: String,
        /// The attestation data
        attestation: RecoveryAttestation,
        /// When attestation was added
        timestamp: u64,
    },

    /// Recovery has been finalized (threshold reached + delay expired)
    Finalized {
        /// Recovery ID
        id: String,
        /// Old DID
        old_did: Did,
        /// New DID
        new_did: Did,
        /// When finalized
        timestamp: u64,
    },

    /// Recovery was cancelled (detected fraud or user found original key)
    Cancelled {
        /// Recovery ID
        id: String,
        /// Who cancelled it
        cancelled_by: Did,
        /// Reason for cancellation
        reason: String,
        /// When cancelled
        timestamp: u64,
    },
}

impl RecoveryMessage {
    /// Create an "Initiated" message from a recovery event
    pub fn initiated(event: &RecoveryEvent) -> Self {
        RecoveryMessage::Initiated {
            id: event.id.clone(),
            old_did: event.old_did.clone(),
            new_did: event.new_did.clone(),
            threshold: event.threshold,
            delay_period: event.delay_period,
            timestamp: event.initiated_at,
        }
    }

    /// Create an "Attestation" message
    pub fn attestation(recovery_id: String, attestation: RecoveryAttestation) -> Self {
        RecoveryMessage::Attestation {
            recovery_id,
            attestation,
            timestamp: current_timestamp(),
        }
    }

    /// Create a "Finalized" message from a recovery event
    pub fn finalized(event: &RecoveryEvent) -> Result<Self> {
        match event.status {
            RecoveryStatus::Finalized => Ok(RecoveryMessage::Finalized {
                id: event.id.clone(),
                old_did: event.old_did.clone(),
                new_did: event.new_did.clone(),
                timestamp: event.finalized_at.unwrap_or_else(current_timestamp),
            }),
            _ => bail!("Cannot create Finalized message from non-finalized recovery"),
        }
    }

    /// Create a "Cancelled" message from a recovery event
    pub fn cancelled(id: String, cancelled_by: Did, reason: String, timestamp: u64) -> Self {
        RecoveryMessage::Cancelled {
            id,
            cancelled_by,
            reason,
            timestamp,
        }
    }

    /// Serialize to bytes for gossip
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(icn_encoding::encode(self)?)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(icn_encoding::decode(bytes)?)
    }

    /// Get a human-readable summary of this message
    pub fn summary(&self) -> String {
        match self {
            RecoveryMessage::Initiated {
                old_did,
                new_did,
                threshold,
                ..
            } => {
                format!("Recovery initiated: {old_did} → {new_did} ({threshold})")
            }
            RecoveryMessage::Attestation {
                recovery_id,
                attestation,
                ..
            } => {
                format!(
                    "Attestation from {} for recovery {}",
                    attestation.trustee, recovery_id
                )
            }
            RecoveryMessage::Finalized {
                old_did, new_did, ..
            } => {
                format!("Recovery finalized: {old_did} → {new_did}")
            }
            RecoveryMessage::Cancelled { id, reason, .. } => {
                format!("Recovery {id} cancelled: {reason}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_message_serialization() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();

        // Test Initiated message
        let recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 3, 86400);
        let msg = RecoveryMessage::initiated(&recovery);

        let bytes = msg.to_bytes().unwrap();
        let msg2 = RecoveryMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg, msg2);

        // Test Attestation message
        let trustee_kp = KeyPair::generate().unwrap();
        let attestation = RecoveryAttestation::new(
            &trustee_kp,
            old_did.clone(),
            new_did.clone(),
            "video call".to_string(),
        )
        .unwrap();

        let msg = RecoveryMessage::attestation("recovery123".to_string(), attestation.clone());
        let bytes = msg.to_bytes().unwrap();
        let msg2 = RecoveryMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg, msg2);

        // Test message summary
        match msg2 {
            RecoveryMessage::Attestation { recovery_id, .. } => {
                assert_eq!(recovery_id, "recovery123");
            }
            _ => panic!("Expected Attestation message"),
        }
    }

    #[test]
    fn test_recovery_message_finalized() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();

        let mut recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 1, 0);

        // Add one attestation to reach threshold
        let trustee_kp = KeyPair::generate().unwrap();
        let attestation = RecoveryAttestation::new(
            &trustee_kp,
            old_did.clone(),
            new_did.clone(),
            "video call".to_string(),
        )
        .unwrap();
        recovery.add_attestation(attestation).unwrap();

        // Check delay expired (delay is 0, so should be ready immediately)
        recovery.check_delay_expired();

        // Now finalize
        recovery.finalize().unwrap();

        let msg = RecoveryMessage::finalized(&recovery).unwrap();

        match msg {
            RecoveryMessage::Finalized {
                old_did: old,
                new_did: new,
                ..
            } => {
                assert_eq!(old, old_did);
                assert_eq!(new, new_did);
            }
            _ => panic!("Expected Finalized message"),
        }

        // Should not create Finalized message from non-finalized recovery
        let recovery2 = RecoveryEvent::new(old_did.clone(), new_did.clone(), 3, 86400);
        assert!(RecoveryMessage::finalized(&recovery2).is_err());
    }

    #[test]
    fn test_recovery_message_cancelled() {
        let did = KeyPair::generate().unwrap().did().clone();
        let msg = RecoveryMessage::cancelled(
            "recovery123".to_string(),
            did.clone(),
            "Fraud detected".to_string(),
            1234567890,
        );

        let summary = msg.summary();
        assert!(summary.contains("cancelled"));
        assert!(summary.contains("Fraud detected"));

        // Test serialization
        let bytes = msg.to_bytes().unwrap();
        let msg2 = RecoveryMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_create_recovery_event() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();

        let recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 3, 86400);

        assert_eq!(recovery.old_did, old_did);
        assert_eq!(recovery.new_did, new_did);
        assert_eq!(recovery.threshold, 3);
        assert_eq!(recovery.delay_period, 86400);
        assert!(matches!(
            recovery.status,
            RecoveryStatus::Pending {
                collected: 0,
                needed: 3
            }
        ));
    }

    #[test]
    fn test_add_attestations() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();
        let mut recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 3, 0); // No delay

        // Create trustees
        let trustee1 = KeyPair::generate().unwrap();
        let trustee2 = KeyPair::generate().unwrap();
        let trustee3 = KeyPair::generate().unwrap();

        // Add first attestation
        let att1 = RecoveryAttestation::new(
            &trustee1,
            old_did.clone(),
            new_did.clone(),
            "video call".to_string(),
        )
        .unwrap();

        let reached_threshold = recovery.add_attestation(att1).unwrap();
        assert!(!reached_threshold); // Only 1/3
        assert!(matches!(
            recovery.status,
            RecoveryStatus::Pending {
                collected: 1,
                needed: 2
            }
        ));

        // Add second attestation
        let att2 = RecoveryAttestation::new(
            &trustee2,
            old_did.clone(),
            new_did.clone(),
            "in-person".to_string(),
        )
        .unwrap();

        let reached_threshold = recovery.add_attestation(att2).unwrap();
        assert!(!reached_threshold); // Only 2/3

        // Add third attestation (reaches threshold)
        let att3 = RecoveryAttestation::new(
            &trustee3,
            old_did.clone(),
            new_did.clone(),
            "phone call".to_string(),
        )
        .unwrap();

        let reached_threshold = recovery.add_attestation(att3).unwrap();
        assert!(reached_threshold); // 3/3!

        // Should be ready to finalize (no delay configured)
        assert!(matches!(recovery.status, RecoveryStatus::ReadyToFinalize));
    }

    #[test]
    fn test_duplicate_trustee_rejected() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();
        let mut recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 3, 0);

        let trustee = KeyPair::generate().unwrap();

        // Add first attestation
        let att1 = RecoveryAttestation::new(
            &trustee,
            old_did.clone(),
            new_did.clone(),
            "video call".to_string(),
        )
        .unwrap();
        recovery.add_attestation(att1).unwrap();

        // Try to add another from same trustee
        let att2 = RecoveryAttestation::new(
            &trustee,
            old_did.clone(),
            new_did.clone(),
            "video call again".to_string(),
        )
        .unwrap();

        let result = recovery.add_attestation(att2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already attested"));
    }

    #[test]
    fn test_attestation_signature_verification() {
        let trustee = KeyPair::generate().unwrap();
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();

        let attestation =
            RecoveryAttestation::new(&trustee, old_did, new_did, "in-person".to_string()).unwrap();

        // Verify with correct public key
        let result = attestation.verify(trustee.verifying_key());
        assert!(result.is_ok());

        // Verify with wrong public key
        let wrong_trustee = KeyPair::generate().unwrap();
        let result = attestation.verify(wrong_trustee.verifying_key());
        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_cancellation() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();
        let mut recovery = RecoveryEvent::new(old_did.clone(), new_did, 3, 0);

        // Cancel recovery
        recovery
            .cancel(old_did.clone(), "I found my device".to_string())
            .unwrap();

        assert!(matches!(recovery.status, RecoveryStatus::Cancelled { .. }));

        // Can't add attestations after cancellation
        let trustee = KeyPair::generate().unwrap();
        let att = RecoveryAttestation::new(
            &trustee,
            recovery.old_did.clone(),
            recovery.new_did.clone(),
            "test".to_string(),
        )
        .unwrap();

        let result = recovery.add_attestation(att);
        assert!(result.is_err());
    }

    #[test]
    fn test_finalization() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();
        let mut recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 2, 0);

        // Add 2 attestations
        for i in 0..2 {
            let trustee = KeyPair::generate().unwrap();
            let att = RecoveryAttestation::new(
                &trustee,
                old_did.clone(),
                new_did.clone(),
                format!("method {i}"),
            )
            .unwrap();
            recovery.add_attestation(att).unwrap();
        }

        // Should be ready to finalize
        assert!(matches!(recovery.status, RecoveryStatus::ReadyToFinalize));

        // Finalize
        recovery.finalize().unwrap();

        assert!(recovery.is_finalized());
        assert!(recovery.finalized_at.is_some());
    }

    #[test]
    fn test_progress_summary() {
        let old_kp = KeyPair::generate().unwrap();
        let new_kp = KeyPair::generate().unwrap();
        let old_did = old_kp.did().clone();
        let new_did = new_kp.did().clone();
        let mut recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), 3, 0);

        // Initially pending
        let summary = recovery.progress_summary();
        assert!(summary.contains("0/3"));

        // Add one attestation
        let trustee = KeyPair::generate().unwrap();
        let att = RecoveryAttestation::new(&trustee, old_did.clone(), new_did, "test".to_string())
            .unwrap();
        recovery.add_attestation(att).unwrap();

        let summary = recovery.progress_summary();
        assert!(summary.contains("1/3"));
    }
}
