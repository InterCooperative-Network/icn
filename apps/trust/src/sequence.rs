//! Sequence number tracking for trust attestations
//!
//! Provides per-issuer monotonic sequence tracking to prevent replay attacks.
//! Each issuer's sequence is stored in sled and incremented atomically.
//!
//! # Concurrency Safety
//!
//! The `SequenceTracker` struct itself is **not** internally synchronized.
//! Callers must acquire a **write lock** on `SharedSequenceTracker` before
//! calling any mutating method (`next_issuer_sequence`, `update_last_seen`,
//! `validate_and_update`).  Read-only methods (`last_seen_sequence`,
//! `validate_sequence`) may use a read lock.

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
///
/// # Revocation Semantics
///
/// Sequences are strictly monotonic and never reset on revocation.
/// If an issuer revokes trust at sequence 10 and later re-attests,
/// the new attestation must have sequence >= 11.  This prevents an
/// attacker from replaying a pre-revocation attestation after the
/// sequence tracker has advanced past it.
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

    /// Get the next sequence number for this node as an issuer.
    ///
    /// Reads the current sequence, increments, and writes back.
    ///
    /// **Caller MUST hold a write lock** on `SharedSequenceTracker` to
    /// prevent concurrent callers from reading the same value and producing
    /// duplicate sequence numbers.
    pub fn next_issuer_sequence(&self) -> Result<u64> {
        let key = format!("{}/issuer/{}", Self::SEQUENCE_PREFIX, self.own_did);

        // Get current sequence or start at 0
        let current = self.read_u64(&key)?;

        // Increment and store
        let next = current + 1;
        self.store.put(key.as_bytes(), &next.to_le_bytes())?;

        Ok(next)
    }

    /// Get the last seen sequence for a given issuer
    ///
    /// Returns None if we've never seen an attestation from this issuer.
    pub fn last_seen_sequence(&self, issuer: &Did) -> Result<Option<u64>> {
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

    /// Atomically validate and update the last-seen sequence for an issuer.
    ///
    /// Combines the check + write into a single method so the caller cannot
    /// accidentally split them.  Returns `Ok(())` on success or an error if
    /// the sequence is stale (replay detected).
    ///
    /// **Caller MUST hold a write lock** on `SharedSequenceTracker`.
    pub fn validate_and_update(&self, issuer: &Did, sequence: u64) -> Result<()> {
        let key = format!("{}/receiver/{}", Self::SEQUENCE_PREFIX, issuer);

        if let Some(last_seen) = self.last_seen_sequence(issuer)? {
            if sequence <= last_seen {
                anyhow::bail!(
                    "Replay attack detected: sequence {} <= last seen {} for issuer {}",
                    sequence,
                    last_seen,
                    issuer
                );
            }
        }

        self.store.put(key.as_bytes(), &sequence.to_le_bytes())?;
        Ok(())
    }

    /// Check if a sequence is valid (greater than last seen)
    ///
    /// Returns Ok(()) if valid, Err if replay detected.
    /// This is a read-only check; use `validate_and_update` when you also
    /// need to persist the new sequence.
    pub fn validate_sequence(&self, issuer: &Did, sequence: u64) -> Result<()> {
        if let Some(last_seen) = self.last_seen_sequence(issuer)? {
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
    pub fn reset_issuer(&self, issuer: &Did) -> Result<()> {
        let key = format!("{}/receiver/{}", Self::SEQUENCE_PREFIX, issuer);
        self.store.delete(key.as_bytes())?;
        Ok(())
    }

    // ------ helpers ------

    fn read_u64(&self, key: &str) -> Result<u64> {
        match self.store.get(key.as_bytes())? {
            Some(bytes) => {
                let slice: &[u8] = bytes.as_ref();
                let arr: [u8; 8] = slice
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid sequence data"))?;
                Ok(u64::from_le_bytes(arr))
            }
            None => Ok(0),
        }
    }
}

/// Shared sequence tracker for async contexts.
///
/// **Write-lock** before calling `next_issuer_sequence`,
/// `validate_and_update`, or `reset_issuer`.
pub type SharedSequenceTracker = Arc<RwLock<SequenceTracker>>;

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[test]
    fn test_next_issuer_sequence() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // First call should return 1
        let seq1 = tracker.next_issuer_sequence().unwrap();
        assert_eq!(seq1, 1);

        // Second call should return 2
        let seq2 = tracker.next_issuer_sequence().unwrap();
        assert_eq!(seq2, 2);

        // Third call should return 3
        let seq3 = tracker.next_issuer_sequence().unwrap();
        assert_eq!(seq3, 3);
    }

    #[test]
    fn test_validate_sequence_first_attestation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // First attestation from Bob should be valid regardless of sequence
        assert!(tracker.validate_sequence(bob.did(), 1).is_ok());
        assert!(tracker.validate_sequence(bob.did(), 5).is_ok());
    }

    #[test]
    fn test_validate_and_update_monotonic() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update to sequence 5
        tracker.validate_and_update(bob.did(), 5).unwrap();

        // Sequence 6 should be valid
        assert!(tracker.validate_sequence(bob.did(), 6).is_ok());

        // Sequence 5 should be invalid (replay)
        assert!(tracker.validate_sequence(bob.did(), 5).is_err());

        // Sequence 4 should be invalid (replay)
        assert!(tracker.validate_sequence(bob.did(), 4).is_err());
    }

    #[test]
    fn test_validate_and_update_rejects_replay() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update to sequence 10
        tracker.validate_and_update(bob.did(), 10).unwrap();

        // Trying to update with sequence 9 should fail
        let result = tracker.validate_and_update(bob.did(), 9);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay attack"));

        // Last seen should still be 10
        let last = tracker.last_seen_sequence(bob.did()).unwrap();
        assert_eq!(last, Some(10));
    }

    #[test]
    fn test_reset_issuer() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update to sequence 100
        tracker.validate_and_update(bob.did(), 100).unwrap();

        // Reset Bob's sequence
        tracker.reset_issuer(bob.did()).unwrap();

        // Should be able to accept sequence 1 now
        assert!(tracker.validate_sequence(bob.did(), 1).is_ok());
    }

    #[test]
    fn test_multiple_issuers_independent() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let carol = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        // Update sequences for different issuers
        tracker.validate_and_update(bob.did(), 5).unwrap();
        tracker.validate_and_update(carol.did(), 10).unwrap();

        // Bob's sequence 6 should be valid
        assert!(tracker.validate_sequence(bob.did(), 6).is_ok());

        // Carol's sequence 11 should be valid
        assert!(tracker.validate_sequence(carol.did(), 11).is_ok());

        // Bob's sequence 5 should be invalid
        assert!(tracker.validate_sequence(bob.did(), 5).is_err());
    }

    #[test]
    fn test_concurrent_issuer_sequences_are_unique() {
        // Verify that when accessed serially (simulating write-lock serialization),
        // all sequence numbers are unique.
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap();
        let tracker = SequenceTracker::new(store, alice.did().clone());

        let mut sequences = Vec::new();
        for _ in 0..100 {
            sequences.push(tracker.next_issuer_sequence().unwrap());
        }

        // All must be unique
        let mut deduped = sequences.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(
            sequences.len(),
            deduped.len(),
            "All 100 sequences must be unique"
        );

        // Must be 1..=100
        assert_eq!(*sequences.first().unwrap(), 1);
        assert_eq!(*sequences.last().unwrap(), 100);
    }
}
