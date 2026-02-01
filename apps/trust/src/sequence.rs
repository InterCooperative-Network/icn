//! Sequence number tracking for trust attestations
//!
//! Provides per-issuer monotonic sequence tracking to prevent replay attacks.
//! Each issuer's sequence is stored in sled and incremented atomically.

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Sequence tracker for trust attestations
///
/// Manages per-issuer sequence numbers for replay protection:
/// - Issuers increment their own sequence when creating attestations
/// - Receivers track last-seen sequence per issuer and reject stale attestations
pub struct SequenceTracker {
    store: Arc<dyn Store>,
    /// Own DID for issuing attestations
    own_did: Did,
}

impl SequenceTracker {
    /// Storage key prefix for sequence numbers
    const SEQUENCE_PREFIX: &'static str = "trust/sequences";

    /// Create a new sequence tracker
    pub fn new(store: Arc<dyn Store>, own_did: Did) -> Self {
        Self { store, own_did }
    }

    /// Get the next sequence number for this node as an issuer
    ///
    /// This atomically increments and returns the sequence number.
    /// Used when creating outgoing attestations.
    pub async fn next_issuer_sequence(&self) -> Result<u64> {
        let key = format!("{}/issuer/{}", Self::SEQUENCE_PREFIX, self.own_did);

        // Get current sequence or start at 0
        let current = match self.store.get(key.as_bytes())? {
            Some(bytes) => {
                let slice: &[u8] = bytes.as_ref();
                let arr: [u8; 8] = slice
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid sequence data"))?;
                u64::from_le_bytes(arr)
            }
            None => 0,
        };

        // Increment and store
        let next = current + 1;
        self.store.put(key.as_bytes(), &next.to_le_bytes())?;

        Ok(next)
    }

    /// Get the last seen sequence for a given issuer
    ///
    /// Returns None if we've never seen an attestation from this issuer.
    pub async fn last_seen_sequence(&self, issuer: &Did) -> Result<Option<u64>> {
        let key = format!("{}/receiver/{}", Self::SEQUENCE_PREFIX, issuer);

        match self.store.get(key.as_bytes())? {
            Some(bytes) => {
                let slice: &[u8] = bytes.as_ref();
                let arr: [u8; 8] = slice
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid sequence data"))?;
                Ok(Some(u64::from_le_bytes(arr)))
            }
            None => Ok(None),
        }
    }

    /// Update the last seen sequence for a given issuer
    ///
    /// This should be called after successfully processing an attestation.
    /// Returns an error if the sequence is not monotonically increasing.
    pub async fn update_last_seen(&self, issuer: &Did, sequence: u64) -> Result<()> {
        let key = format!("{}/receiver/{}", Self::SEQUENCE_PREFIX, issuer);

        // Check if sequence is monotonically increasing
        if let Some(last_seen) = self.last_seen_sequence(issuer).await? {
            if sequence <= last_seen {
                anyhow::bail!(
                    "Replay attack detected: sequence {} <= last seen {} for issuer {}",
                    sequence,
                    last_seen,
                    issuer
                );
            }
        }

        // Store the new sequence
        self.store.put(key.as_bytes(), &sequence.to_le_bytes())?;

        Ok(())
    }

    /// Check if a sequence is valid (greater than last seen)
    ///
    /// Returns Ok(()) if valid, Err if replay detected.
    pub async fn validate_sequence(&self, issuer: &Did, sequence: u64) -> Result<()> {
        if let Some(last_seen) = self.last_seen_sequence(issuer).await? {
            if sequence <= last_seen {
                anyhow::bail!(
                    "Replay attack: sequence {} <= last seen {} for issuer {}",
                    sequence,
                    last_seen,
                    issuer
                );
            }
        }
        // If no last_seen, any sequence is valid (first attestation from this issuer)
        Ok(())
    }

    /// Reset sequence tracking for an issuer (used during key rotation)
    ///
    /// This clears the last-seen sequence for an issuer, allowing them to
    /// restart from sequence 1 after key rotation.
    pub async fn reset_issuer(&self, issuer: &Did) -> Result<()> {
        let key = format!("{}/receiver/{}", Self::SEQUENCE_PREFIX, issuer);
        self.store.delete(key.as_bytes())?;
        Ok(())
    }
}

/// Shared sequence tracker for async contexts
pub type SharedSequenceTracker = Arc<RwLock<SequenceTracker>>;

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[tokio::test]
    async fn test_next_issuer_sequence() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // First call should return 1
        let seq1 = tracker.next_issuer_sequence().await.unwrap();
        assert_eq!(seq1, 1);

        // Second call should return 2
        let seq2 = tracker.next_issuer_sequence().await.unwrap();
        assert_eq!(seq2, 2);

        // Third call should return 3
        let seq3 = tracker.next_issuer_sequence().await.unwrap();
        assert_eq!(seq3, 3);
    }

    #[tokio::test]
    async fn test_validate_sequence_first_attestation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // First attestation from Bob should be valid regardless of sequence
        assert!(tracker.validate_sequence(bob.did(), 1).await.is_ok());
        assert!(tracker.validate_sequence(bob.did(), 5).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_sequence_monotonic() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update to sequence 5
        tracker.update_last_seen(bob.did(), 5).await.unwrap();

        // Sequence 6 should be valid
        assert!(tracker.validate_sequence(bob.did(), 6).await.is_ok());

        // Sequence 5 should be invalid (replay)
        assert!(tracker.validate_sequence(bob.did(), 5).await.is_err());

        // Sequence 4 should be invalid (replay)
        assert!(tracker.validate_sequence(bob.did(), 4).await.is_err());
    }

    #[tokio::test]
    async fn test_update_last_seen_rejects_replay() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update to sequence 10
        tracker.update_last_seen(bob.did(), 10).await.unwrap();

        // Trying to update with sequence 9 should fail
        let result = tracker.update_last_seen(bob.did(), 9).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay attack"));

        // Last seen should still be 10
        let last = tracker.last_seen_sequence(bob.did()).await.unwrap();
        assert_eq!(last, Some(10));
    }

    #[tokio::test]
    async fn test_reset_issuer() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update to sequence 100
        tracker.update_last_seen(bob.did(), 100).await.unwrap();

        // Reset Bob's sequence
        tracker.reset_issuer(bob.did()).await.unwrap();

        // Should be able to accept sequence 1 now
        assert!(tracker.validate_sequence(bob.did(), 1).await.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_issuers_independent() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let carol = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update sequences for different issuers
        tracker.update_last_seen(bob.did(), 5).await.unwrap();
        tracker.update_last_seen(carol.did(), 10).await.unwrap();

        // Bob's sequence 6 should be valid
        assert!(tracker.validate_sequence(bob.did(), 6).await.is_ok());

        // Carol's sequence 11 should be valid
        assert!(tracker.validate_sequence(carol.did(), 11).await.is_ok());

        // Bob's sequence 5 should be invalid
        assert!(tracker.validate_sequence(bob.did(), 5).await.is_err());
    }
}
