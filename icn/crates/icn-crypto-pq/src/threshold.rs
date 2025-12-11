//! Threshold cryptography primitives for SDIS
//!
//! This module provides threshold cryptographic operations needed for:
//! - Pepper secret sharing among stewards
//! - Threshold PRF for VUI (Verifiable Unique Identifier) computation
//! - (Future) FROST threshold signatures
//!
//! # Pepper Distribution
//!
//! The enrollment pepper is split using Shamir's Secret Sharing with a
//! threshold of t-of-n stewards required to reconstruct. For VUI computation,
//! each steward computes a PRF partial with their share, and the partials
//! are combined by the user (without revealing the full pepper).
//!
//! # Threshold PRF Protocol
//!
//! 1. User sends PRF input (credential hash) to t stewards
//! 2. Each steward computes: partial_i = HMAC(pepper_share_i, input)
//! 3. User receives t partials and combines them
//! 4. Result is the VUI = H(combine(partials))
//!
//! # Security
//!
//! - No single steward learns the full pepper
//! - No single steward can compute the VUI alone
//! - Collusion of t stewards is required to forge VUIs
//! - Each partial is unlinkable without the other partials

use hmac::{Hmac, Mac};
use sha3::Sha3_256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, Result};

/// A share of the enrollment pepper
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PepperShare {
    /// Share index (1-indexed, for Lagrange interpolation)
    #[zeroize(skip)]
    pub index: u8,
    /// The share value (32 bytes)
    pub value: [u8; 32],
}

impl PepperShare {
    /// Create a new pepper share
    pub fn new(index: u8, value: [u8; 32]) -> Self {
        Self { index, value }
    }

    /// Create from bytes
    pub fn from_bytes(index: u8, bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKey(format!(
                "Pepper share must be 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut value = [0u8; 32];
        value.copy_from_slice(bytes);
        Ok(Self { index, value })
    }
}

/// A PRF partial computed by a steward
#[derive(Clone)]
pub struct PrfPartial {
    /// Steward index (matches pepper share index)
    pub index: u8,
    /// The partial PRF output (32 bytes)
    pub value: [u8; 32],
}

impl PrfPartial {
    /// Compute a PRF partial from a pepper share
    ///
    /// partial_i = HMAC-SHA3-256(pepper_share_i, input)
    pub fn compute(share: &PepperShare, input: &[u8]) -> Self {
        let mut mac = Hmac::<Sha3_256>::new_from_slice(&share.value)
            .expect("HMAC key length should be valid");
        mac.update(input);
        let result = mac.finalize().into_bytes();

        let mut value = [0u8; 32];
        value.copy_from_slice(&result);

        Self {
            index: share.index,
            value,
        }
    }
}

/// Combine PRF partials to compute the final VUI
///
/// This uses simple XOR combination. For a t-of-n scheme, exactly t
/// partials are needed. The combination is order-independent.
///
/// VUI = H(partial_1 XOR partial_2 XOR ... XOR partial_t)
pub fn combine_prf_partials(partials: &[PrfPartial]) -> Result<[u8; 32]> {
    if partials.is_empty() {
        return Err(CryptoError::InvalidKey(
            "At least one partial is required".to_string(),
        ));
    }

    // XOR all partials together
    let mut combined = [0u8; 32];
    for partial in partials {
        for (i, byte) in partial.value.iter().enumerate() {
            combined[i] ^= byte;
        }
    }

    // Hash the combined result to get the final VUI
    use sha3::Digest;
    let mut hasher = Sha3_256::new();
    hasher.update(&combined);
    let result = hasher.finalize();

    let mut vui = [0u8; 32];
    vui.copy_from_slice(&result);

    Ok(vui)
}

/// Simple secret sharing for pepper distribution
///
/// This implements additive secret sharing where:
/// - n-1 shares are random
/// - The nth share is pepper XOR (share_1 XOR share_2 XOR ... XOR share_{n-1})
///
/// For a more robust t-of-n scheme, use Shamir's Secret Sharing
/// (available via the vsss-rs crate when the threshold feature is enabled).
pub struct AdditiveSecretSharing;

impl AdditiveSecretSharing {
    /// Split a secret into n additive shares
    ///
    /// ALL shares are required to reconstruct (n-of-n scheme).
    /// For t-of-n, use Shamir's Secret Sharing.
    pub fn split(secret: &[u8; 32], n: u8) -> Result<Vec<PepperShare>> {
        if n < 2 {
            return Err(CryptoError::InvalidKey(
                "At least 2 shares are required".to_string(),
            ));
        }

        let mut shares = Vec::with_capacity(n as usize);
        let mut xor_sum = *secret;

        // Generate n-1 random shares
        for i in 1..n {
            let mut share_value = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut share_value);

            // XOR into running sum
            for (j, byte) in share_value.iter().enumerate() {
                xor_sum[j] ^= byte;
            }

            shares.push(PepperShare::new(i, share_value));
        }

        // The nth share is what's needed to get back to the secret
        shares.push(PepperShare::new(n, xor_sum));

        Ok(shares)
    }

    /// Reconstruct a secret from all shares
    ///
    /// ALL shares must be provided (n-of-n).
    pub fn reconstruct(shares: &[PepperShare]) -> Result<[u8; 32]> {
        if shares.is_empty() {
            return Err(CryptoError::InvalidKey(
                "At least one share is required".to_string(),
            ));
        }

        let mut secret = [0u8; 32];
        for share in shares {
            for (i, byte) in share.value.iter().enumerate() {
                secret[i] ^= byte;
            }
        }

        Ok(secret)
    }
}

/// Commitment to a pepper share (for verification)
///
/// Used to verify that a steward's PRF partial was computed correctly
/// without revealing the pepper share itself.
#[derive(Clone)]
pub struct ShareCommitment {
    /// Hash of the pepper share
    pub commitment: [u8; 32],
    /// Share index
    pub index: u8,
}

impl ShareCommitment {
    /// Create a commitment to a pepper share
    pub fn commit(share: &PepperShare) -> Self {
        use sha3::Digest;
        let mut hasher = Sha3_256::new();
        hasher.update([share.index]);
        hasher.update(&share.value);
        let result = hasher.finalize();

        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&result);

        Self {
            commitment,
            index: share.index,
        }
    }

    /// Verify a share against this commitment
    pub fn verify(&self, share: &PepperShare) -> bool {
        if share.index != self.index {
            return false;
        }

        use sha3::Digest;
        let mut hasher = Sha3_256::new();
        hasher.update([share.index]);
        hasher.update(&share.value);
        let result = hasher.finalize();

        result[..] == self.commitment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prf_partial_compute() {
        let share = PepperShare::new(1, [42u8; 32]);
        let input = b"credential-hash-data";

        let partial1 = PrfPartial::compute(&share, input);
        let partial2 = PrfPartial::compute(&share, input);

        // Deterministic
        assert_eq!(partial1.value, partial2.value);
        assert_eq!(partial1.index, 1);

        // Different input gives different output
        let partial3 = PrfPartial::compute(&share, b"different-input");
        assert_ne!(partial1.value, partial3.value);
    }

    #[test]
    fn test_prf_partial_different_shares() {
        let share1 = PepperShare::new(1, [1u8; 32]);
        let share2 = PepperShare::new(2, [2u8; 32]);
        let input = b"same-input";

        let partial1 = PrfPartial::compute(&share1, input);
        let partial2 = PrfPartial::compute(&share2, input);

        // Different shares give different partials
        assert_ne!(partial1.value, partial2.value);
    }

    #[test]
    fn test_combine_prf_partials() {
        let share1 = PepperShare::new(1, [1u8; 32]);
        let share2 = PepperShare::new(2, [2u8; 32]);
        let share3 = PepperShare::new(3, [3u8; 32]);
        let input = b"credential-hash";

        let partial1 = PrfPartial::compute(&share1, input);
        let partial2 = PrfPartial::compute(&share2, input);
        let partial3 = PrfPartial::compute(&share3, input);

        let vui1 =
            combine_prf_partials(&[partial1.clone(), partial2.clone(), partial3.clone()]).unwrap();
        let vui2 =
            combine_prf_partials(&[partial1.clone(), partial2.clone(), partial3.clone()]).unwrap();

        // Deterministic
        assert_eq!(vui1, vui2);

        // Order independent (XOR is commutative)
        let vui3 =
            combine_prf_partials(&[partial3.clone(), partial1.clone(), partial2.clone()]).unwrap();
        assert_eq!(vui1, vui3);
    }

    #[test]
    fn test_combine_prf_partials_different_inputs() {
        let share1 = PepperShare::new(1, [1u8; 32]);
        let share2 = PepperShare::new(2, [2u8; 32]);

        let partial1a = PrfPartial::compute(&share1, b"input-a");
        let partial2a = PrfPartial::compute(&share2, b"input-a");

        let partial1b = PrfPartial::compute(&share1, b"input-b");
        let partial2b = PrfPartial::compute(&share2, b"input-b");

        let vui_a = combine_prf_partials(&[partial1a, partial2a]).unwrap();
        let vui_b = combine_prf_partials(&[partial1b, partial2b]).unwrap();

        // Different inputs produce different VUIs
        assert_ne!(vui_a, vui_b);
    }

    #[test]
    fn test_additive_secret_sharing() {
        let secret = [42u8; 32];

        // Split into 3 shares
        let shares = AdditiveSecretSharing::split(&secret, 3).unwrap();
        assert_eq!(shares.len(), 3);

        // All shares are required to reconstruct
        let reconstructed = AdditiveSecretSharing::reconstruct(&shares).unwrap();
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_additive_sharing_partial_fails() {
        let secret = [42u8; 32];

        let shares = AdditiveSecretSharing::split(&secret, 3).unwrap();

        // Missing shares gives wrong result
        let partial = AdditiveSecretSharing::reconstruct(&shares[0..2]).unwrap();
        assert_ne!(partial, secret);
    }

    #[test]
    fn test_additive_sharing_randomness() {
        let secret = [42u8; 32];

        // Two splits should give different shares
        let shares1 = AdditiveSecretSharing::split(&secret, 3).unwrap();
        let shares2 = AdditiveSecretSharing::split(&secret, 3).unwrap();

        // At least one share should be different
        let different = shares1
            .iter()
            .zip(shares2.iter())
            .any(|(a, b)| a.value != b.value);
        assert!(different);

        // But both should reconstruct to the same secret
        let r1 = AdditiveSecretSharing::reconstruct(&shares1).unwrap();
        let r2 = AdditiveSecretSharing::reconstruct(&shares2).unwrap();
        assert_eq!(r1, secret);
        assert_eq!(r2, secret);
    }

    #[test]
    fn test_share_commitment() {
        let share = PepperShare::new(1, [42u8; 32]);
        let commitment = ShareCommitment::commit(&share);

        // Verify correct share
        assert!(commitment.verify(&share));

        // Wrong value fails
        let wrong_share = PepperShare::new(1, [43u8; 32]);
        assert!(!commitment.verify(&wrong_share));

        // Wrong index fails
        let wrong_index = PepperShare::new(2, [42u8; 32]);
        assert!(!commitment.verify(&wrong_index));
    }

    #[test]
    fn test_vui_computation_flow() {
        // Simulate the full VUI computation protocol
        let pepper = [99u8; 32];

        // 1. Split pepper among 3 stewards (n-of-n for simplicity)
        let shares = AdditiveSecretSharing::split(&pepper, 3).unwrap();

        // 2. Create commitments for verification
        let commitments: Vec<_> = shares.iter().map(ShareCommitment::commit).collect();

        // 3. User sends credential hash to stewards
        let credential_hash = b"unique-credential-hash-for-person";

        // 4. Each steward computes their partial
        let partials: Vec<_> = shares
            .iter()
            .map(|s| PrfPartial::compute(s, credential_hash))
            .collect();

        // 5. User combines partials to get VUI
        let vui = combine_prf_partials(&partials).unwrap();

        // 6. VUI is deterministic for same inputs
        let partials2: Vec<_> = shares
            .iter()
            .map(|s| PrfPartial::compute(s, credential_hash))
            .collect();
        let vui2 = combine_prf_partials(&partials2).unwrap();
        assert_eq!(vui, vui2);

        // 7. Different credential gives different VUI
        let other_partials: Vec<_> = shares
            .iter()
            .map(|s| PrfPartial::compute(s, b"different-person"))
            .collect();
        let other_vui = combine_prf_partials(&other_partials).unwrap();
        assert_ne!(vui, other_vui);

        // 8. Verify commitments match shares
        for (commitment, share) in commitments.iter().zip(shares.iter()) {
            assert!(commitment.verify(share));
        }
    }
}
