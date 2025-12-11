//! Hybrid key derivation functions
//!
//! These functions derive keys by combining classical and post-quantum
//! shared secrets, ensuring security if EITHER component is secure.

use hkdf::Hkdf;
use sha3::Sha3_256;

/// Derive a key from hybrid shared secrets
///
/// This combines classical (X25519) and post-quantum (Kyber) shared secrets
/// using HKDF-SHA3-256. The result is secure if EITHER shared secret is secure.
///
/// # Arguments
///
/// * `classical_shared` - Shared secret from X25519 key exchange (32 bytes)
/// * `pq_shared` - Shared secret from ML-KEM key encapsulation (32 bytes)
/// * `context` - Application-specific context string
/// * `output_len` - Desired output length in bytes
///
/// # Returns
///
/// Derived key material of length `output_len`
pub fn derive_hybrid_key(
    classical_shared: &[u8; 32],
    pq_shared: &[u8; 32],
    context: &[u8],
    output_len: usize,
) -> Vec<u8> {
    // Concatenate shared secrets
    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(classical_shared);
    combined.extend_from_slice(pq_shared);

    // Use HKDF-SHA3-256
    let hk = Hkdf::<Sha3_256>::new(None, &combined);
    let mut output = vec![0u8; output_len];
    hk.expand(context, &mut output)
        .expect("HKDF output length should be valid");

    output
}

/// Derive all keys needed for a KeyBundle from a seed
///
/// This deterministically generates:
/// - Ed25519 signing key (32 bytes)
/// - ML-DSA key seed (64 bytes) - used for deterministic ML-DSA keygen
/// - X25519 encryption key (32 bytes)
///
/// # Arguments
///
/// * `master_seed` - 64-byte master seed
/// * `version` - KeyBundle version number
///
/// # Returns
///
/// Tuple of (ed25519_seed, ml_dsa_seed, x25519_seed)
pub fn derive_keybundle_keys(
    master_seed: &[u8; 64],
    version: u32,
) -> ([u8; 32], [u8; 64], [u8; 32]) {
    let hk = Hkdf::<Sha3_256>::new(None, master_seed);

    // Context includes version to get different keys per version
    let version_bytes = version.to_le_bytes();

    // Derive Ed25519 seed
    let mut ed25519_seed = [0u8; 32];
    let ed25519_context = [b"icn-sdis-ed25519-v", version_bytes.as_slice()].concat();
    hk.expand(&ed25519_context, &mut ed25519_seed)
        .expect("Ed25519 seed derivation failed");

    // Derive ML-DSA seed (larger for key expansion)
    let mut ml_dsa_seed = [0u8; 64];
    let ml_dsa_context = [b"icn-sdis-ml-dsa-v", version_bytes.as_slice()].concat();
    hk.expand(&ml_dsa_context, &mut ml_dsa_seed)
        .expect("ML-DSA seed derivation failed");

    // Derive X25519 seed
    let mut x25519_seed = [0u8; 32];
    let x25519_context = [b"icn-sdis-x25519-v", version_bytes.as_slice()].concat();
    hk.expand(&x25519_context, &mut x25519_seed)
        .expect("X25519 seed derivation failed");

    (ed25519_seed, ml_dsa_seed, x25519_seed)
}

/// Derive a session key from ephemeral and static keys
///
/// Used in protocols where both parties contribute ephemeral keys
/// and have long-term static keys.
pub fn derive_session_key(
    static_shared: &[u8; 32],
    ephemeral_shared: &[u8; 32],
    session_context: &[u8],
) -> [u8; 32] {
    let mut combined = Vec::with_capacity(64 + session_context.len());
    combined.extend_from_slice(static_shared);
    combined.extend_from_slice(ephemeral_shared);

    let hk = Hkdf::<Sha3_256>::new(None, &combined);
    let mut session_key = [0u8; 32];
    hk.expand(session_context, &mut session_key)
        .expect("Session key derivation failed");

    session_key
}

/// Derive a PRF output for VUI computation
///
/// This is used by the threshold PRF to compute VUIs.
/// Each steward computes a partial with their pepper share,
/// and the partials are combined by the user.
pub fn prf_partial(pepper_share: &[u8; 32], input: &[u8]) -> [u8; 32] {
    use hmac::{Hmac, Mac};

    let mut mac = Hmac::<Sha3_256>::new_from_slice(pepper_share)
        .expect("HMAC key length should be valid");
    mac.update(input);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_hybrid_key() {
        let classical = [1u8; 32];
        let pq = [2u8; 32];

        let key1 = derive_hybrid_key(&classical, &pq, b"test-context", 32);
        let key2 = derive_hybrid_key(&classical, &pq, b"test-context", 32);

        // Deterministic
        assert_eq!(key1, key2);

        // Different context gives different key
        let key3 = derive_hybrid_key(&classical, &pq, b"other-context", 32);
        assert_ne!(key1, key3);

        // Different input gives different key
        let key4 = derive_hybrid_key(&[3u8; 32], &pq, b"test-context", 32);
        assert_ne!(key1, key4);
    }

    #[test]
    fn test_derive_keybundle_keys() {
        let seed = [42u8; 64];

        let (ed1, ml1, x1) = derive_keybundle_keys(&seed, 1);
        let (ed2, ml2, x2) = derive_keybundle_keys(&seed, 1);

        // Deterministic
        assert_eq!(ed1, ed2);
        assert_eq!(ml1, ml2);
        assert_eq!(x1, x2);

        // Different version gives different keys
        let (ed3, ml3, x3) = derive_keybundle_keys(&seed, 2);
        assert_ne!(ed1, ed3);
        assert_ne!(ml1, ml3);
        assert_ne!(x1, x3);

        // All keys are different from each other
        assert_ne!(ed1.as_slice(), &ml1[..32]);
        assert_ne!(ed1, x1);
    }

    #[test]
    fn test_derive_session_key() {
        let static_shared = [1u8; 32];
        let ephemeral_shared = [2u8; 32];

        let key1 = derive_session_key(&static_shared, &ephemeral_shared, b"session-1");
        let key2 = derive_session_key(&static_shared, &ephemeral_shared, b"session-1");

        assert_eq!(key1, key2);

        // Different session context
        let key3 = derive_session_key(&static_shared, &ephemeral_shared, b"session-2");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_prf_partial() {
        let share1 = [1u8; 32];
        let share2 = [2u8; 32];
        let input = b"vui-input-data";

        let partial1 = prf_partial(&share1, input);
        let partial2 = prf_partial(&share2, input);

        // Different shares give different outputs
        assert_ne!(partial1, partial2);

        // Deterministic
        let partial1_again = prf_partial(&share1, input);
        assert_eq!(partial1, partial1_again);

        // Different input gives different output
        let partial1_other = prf_partial(&share1, b"other-input");
        assert_ne!(partial1, partial1_other);
    }
}
