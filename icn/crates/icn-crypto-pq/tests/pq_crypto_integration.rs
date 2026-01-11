//! Integration tests for post-quantum cryptography
//!
//! Tests end-to-end workflows for ML-DSA, ML-KEM, hybrid signatures,
//! hybrid key exchange, and threshold operations.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_crypto_pq::{
    combine_prf_partials, AdditiveSecretSharing, HybridKemKeypair, HybridKeypair, MlDsaKeypair,
    MlDsaPublicKey, MlDsaSignature, MlKemCiphertext, MlKemKeypair, MlKemPublicKey, PepperShare,
    PrfPartial, ShareCommitment,
};

// =============================================================================
// ML-DSA Integration Tests
// =============================================================================

mod ml_dsa_tests {
    use super::*;

    /// Test complete ML-DSA key lifecycle: generate, sign, verify
    #[test]
    fn test_ml_dsa_key_lifecycle() {
        // Generate keypair
        let keypair = MlDsaKeypair::generate().expect("Key generation should succeed");

        // Get public key
        let public_key = keypair.public_key();
        assert_eq!(
            public_key.as_bytes().len(),
            MlDsaPublicKey::SIZE,
            "Public key should be {} bytes",
            MlDsaPublicKey::SIZE
        );

        // Sign a message
        let message = b"This is a test message for ML-DSA signing";
        let signature = keypair.sign(message).expect("Signing should succeed");
        assert_eq!(
            signature.as_bytes().len(),
            MlDsaSignature::SIZE,
            "Signature should be {} bytes",
            MlDsaSignature::SIZE
        );

        // Verify the signature
        assert!(
            MlDsaKeypair::verify(public_key, message, &signature),
            "Valid signature should verify"
        );

        // Verify fails with wrong message
        let wrong_message = b"This is a different message";
        assert!(
            !MlDsaKeypair::verify(public_key, wrong_message, &signature),
            "Signature should not verify with wrong message"
        );
    }

    /// Test deterministic key generation from seed
    #[test]
    fn test_ml_dsa_deterministic_keygen() {
        let seed = [42u8; 32];

        let keypair1 = MlDsaKeypair::from_seed(&seed).expect("Seeded keygen should succeed");
        let keypair2 = MlDsaKeypair::from_seed(&seed).expect("Seeded keygen should succeed");

        // Same seed should produce identical public keys
        assert_eq!(
            keypair1.public_key().as_bytes(),
            keypair2.public_key().as_bytes(),
            "Same seed should produce identical keypairs"
        );

        // Different seeds should produce different keys
        let different_seed = [99u8; 32];
        let keypair3 =
            MlDsaKeypair::from_seed(&different_seed).expect("Seeded keygen should succeed");
        assert_ne!(
            keypair1.public_key().as_bytes(),
            keypair3.public_key().as_bytes(),
            "Different seeds should produce different keypairs"
        );
    }

    /// Test ML-DSA signature with wrong key fails
    #[test]
    fn test_ml_dsa_wrong_key_verification() {
        let keypair1 = MlDsaKeypair::generate().unwrap();
        let keypair2 = MlDsaKeypair::generate().unwrap();

        let message = b"Test message";
        let signature = keypair1.sign(message).unwrap();

        // Verification with wrong key should fail
        assert!(
            !MlDsaKeypair::verify(keypair2.public_key(), message, &signature),
            "Signature should not verify with wrong key"
        );
    }

    /// Test ML-DSA key serialization roundtrip
    #[test]
    fn test_ml_dsa_serialization_roundtrip() {
        let keypair = MlDsaKeypair::generate().unwrap();
        let public_key = keypair.public_key();

        // Serialize public key to bytes
        let bytes = public_key.as_bytes();

        // Deserialize back
        let restored = MlDsaPublicKey::from_bytes(bytes).expect("Deserialization should succeed");

        assert_eq!(
            public_key.as_bytes(),
            restored.as_bytes(),
            "Roundtrip should preserve public key"
        );

        // Test signature roundtrip
        let message = b"Test message";
        let signature = keypair.sign(message).unwrap();
        let sig_bytes = signature.as_bytes();

        let restored_sig = MlDsaSignature::from_bytes(sig_bytes)
            .expect("Signature deserialization should succeed");
        assert!(
            MlDsaKeypair::verify(public_key, message, &restored_sig),
            "Restored signature should verify"
        );
    }

    /// Test invalid key format rejection
    #[test]
    fn test_ml_dsa_invalid_key_rejected() {
        let too_short = vec![0u8; 100];
        let too_long = vec![0u8; 3000];

        assert!(
            MlDsaPublicKey::from_bytes(&too_short).is_err(),
            "Too short key should be rejected"
        );
        assert!(
            MlDsaPublicKey::from_bytes(&too_long).is_err(),
            "Too long key should be rejected"
        );
    }
}

// =============================================================================
// ML-KEM Integration Tests
// =============================================================================

mod ml_kem_tests {
    use super::*;

    /// Test complete ML-KEM key exchange lifecycle
    #[test]
    fn test_ml_kem_key_exchange_lifecycle() {
        // Alice generates keypair
        let alice_keypair = MlKemKeypair::generate().expect("Key generation should succeed");
        let alice_public = alice_keypair.public_key();

        assert_eq!(
            alice_public.as_bytes().len(),
            MlKemPublicKey::SIZE,
            "Public key should be {} bytes",
            MlKemPublicKey::SIZE
        );

        // Bob encapsulates to Alice's public key
        let (bob_shared_secret, ciphertext) =
            MlKemKeypair::encapsulate(alice_public).expect("Encapsulation should succeed");

        // Alice decapsulates
        let alice_shared_secret = alice_keypair
            .decapsulate(&ciphertext)
            .expect("Decapsulation should succeed");

        // Both parties should have the same shared secret
        assert_eq!(
            alice_shared_secret, bob_shared_secret,
            "Shared secrets should match"
        );
        assert_eq!(
            alice_shared_secret.len(),
            32,
            "Shared secret should be 32 bytes"
        );
    }

    /// Test ML-KEM with wrong keypair fails
    #[test]
    fn test_ml_kem_wrong_keypair() {
        let alice_keypair = MlKemKeypair::generate().unwrap();
        let bob_keypair = MlKemKeypair::generate().unwrap();

        // Encapsulate to Alice's public key
        let (_bob_shared_secret, ciphertext) =
            MlKemKeypair::encapsulate(alice_keypair.public_key()).unwrap();

        // Bob tries to decapsulate with his own keypair (wrong)
        let bob_decap_result = bob_keypair.decapsulate(&ciphertext);

        // ML-KEM decapsulation with wrong key produces a different secret (implicit rejection)
        // rather than an error - this is by design for IND-CCA2 security
        if let Ok(wrong_secret) = bob_decap_result {
            // The secrets should be different (with overwhelming probability)
            let alice_secret = alice_keypair.decapsulate(&ciphertext).unwrap();
            assert_ne!(
                wrong_secret, alice_secret,
                "Wrong key should produce different secret"
            );
        }
    }

    /// Test ML-KEM serialization roundtrip
    #[test]
    fn test_ml_kem_serialization_roundtrip() {
        let keypair = MlKemKeypair::generate().unwrap();
        let public_key = keypair.public_key();

        // Serialize and deserialize public key
        let bytes = public_key.as_bytes();
        let restored =
            MlKemPublicKey::from_bytes(bytes).expect("Public key deserialization should succeed");

        // Encapsulate with restored key
        let (shared_secret, ciphertext) = MlKemKeypair::encapsulate(&restored).unwrap();

        // Original keypair should be able to decapsulate
        let decap_secret = keypair.decapsulate(&ciphertext).unwrap();
        assert_eq!(
            shared_secret, decap_secret,
            "Roundtrip public key should work"
        );
    }

    /// Test ciphertext serialization roundtrip
    #[test]
    fn test_ml_kem_ciphertext_serialization() {
        let keypair = MlKemKeypair::generate().unwrap();
        let (shared_secret, ciphertext) = MlKemKeypair::encapsulate(keypair.public_key()).unwrap();

        // Serialize ciphertext
        let ct_bytes = ciphertext.as_bytes();
        let restored_ct = MlKemCiphertext::from_bytes(ct_bytes)
            .expect("Ciphertext deserialization should succeed");

        // Decapsulate with restored ciphertext
        let decap_secret = keypair.decapsulate(&restored_ct).unwrap();
        assert_eq!(
            shared_secret, decap_secret,
            "Restored ciphertext should work"
        );
    }

    /// Test invalid ML-KEM key format rejection
    #[test]
    fn test_ml_kem_invalid_key_rejected() {
        let too_short = vec![0u8; 100];
        let too_long = vec![0u8; 2000];

        assert!(
            MlKemPublicKey::from_bytes(&too_short).is_err(),
            "Too short key should be rejected"
        );
        assert!(
            MlKemPublicKey::from_bytes(&too_long).is_err(),
            "Too long key should be rejected"
        );
    }
}

// =============================================================================
// Hybrid Signature Integration Tests
// =============================================================================

mod hybrid_signature_tests {
    use super::*;

    /// Test hybrid signature lifecycle (Ed25519 + ML-DSA)
    #[test]
    fn test_hybrid_signature_lifecycle() {
        let keypair = HybridKeypair::generate().expect("Hybrid keygen should succeed");
        let public_key = keypair.public_key();

        let message = b"Important document requiring hybrid signature";
        let signature = keypair
            .sign(message)
            .expect("Hybrid signing should succeed");

        // Verify should pass
        assert!(
            signature.verify(message, &public_key),
            "Valid hybrid signature should verify"
        );

        // Tampered message should fail
        assert!(
            !signature.verify(b"tampered", &public_key),
            "Tampered message should fail verification"
        );
    }

    /// Test hybrid signature with wrong key fails
    #[test]
    fn test_hybrid_wrong_key() {
        let keypair1 = HybridKeypair::generate().unwrap();
        let keypair2 = HybridKeypair::generate().unwrap();

        let message = b"Test message";
        let signature = keypair1.sign(message).unwrap();

        assert!(
            !signature.verify(message, &keypair2.public_key()),
            "Wrong key should fail verification"
        );
    }

    /// Test hybrid signature serialization roundtrip
    #[test]
    fn test_hybrid_signature_serialization() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"Test message for serialization";

        let signature = keypair.sign(message).unwrap();

        // Serialize
        let json = serde_json::to_string(&signature).expect("JSON serialization should succeed");

        // Deserialize
        let restored: icn_crypto_pq::HybridSignature =
            serde_json::from_str(&json).expect("JSON deserialization should succeed");

        // Verify restored signature
        assert!(
            restored.verify(message, &keypair.public_key()),
            "Restored signature should verify"
        );
    }

    /// Test hybrid key serialization roundtrip
    #[test]
    fn test_hybrid_key_serialization() {
        let keypair = HybridKeypair::generate().unwrap();
        let public_key = keypair.public_key();

        // Serialize public key
        let json = serde_json::to_string(&public_key).expect("JSON serialization should succeed");

        // Deserialize
        let restored: icn_crypto_pq::HybridPublicKey =
            serde_json::from_str(&json).expect("JSON deserialization should succeed");

        // Sign with original, verify with restored key
        let message = b"Test";
        let signature = keypair.sign(message).unwrap();
        assert!(
            signature.verify(message, &restored),
            "Signature should verify with restored key"
        );
    }
}

// =============================================================================
// Hybrid KEM Integration Tests
// =============================================================================

mod hybrid_kem_tests {
    use super::*;

    /// Test hybrid KEM lifecycle (X25519 + ML-KEM)
    #[test]
    fn test_hybrid_kem_lifecycle() {
        // Alice generates hybrid KEM keypair
        let alice_keypair = HybridKemKeypair::generate().expect("Hybrid KEM keygen should succeed");

        let context = b"test-context";

        // Bob encapsulates to Alice
        let (bob_shared_secret, ciphertext) =
            HybridKemKeypair::encapsulate(alice_keypair.public_key(), context)
                .expect("Encapsulation should succeed");

        // Alice decapsulates
        let alice_shared_secret = alice_keypair
            .decapsulate(&ciphertext, context)
            .expect("Decapsulation should succeed");

        // Shared secrets should match
        assert_eq!(
            alice_shared_secret, bob_shared_secret,
            "Hybrid KEM shared secrets should match"
        );
    }

    /// Test hybrid KEM with wrong key
    #[test]
    fn test_hybrid_kem_wrong_key() {
        let alice = HybridKemKeypair::generate().unwrap();
        let bob = HybridKemKeypair::generate().unwrap();
        let context = b"test-context";

        let (alice_secret, ciphertext) =
            HybridKemKeypair::encapsulate(alice.public_key(), context).unwrap();

        // Bob decapsulates with wrong key - should produce different secret
        let bob_secret = bob.decapsulate(&ciphertext, context).unwrap();

        assert_ne!(
            alice_secret, bob_secret,
            "Wrong key should produce different secret"
        );
    }

    /// Test hybrid KEM with different context produces different secret
    #[test]
    fn test_hybrid_kem_context_binding() {
        let alice = HybridKemKeypair::generate().unwrap();
        let context1 = b"context-1";
        let context2 = b"context-2";

        let (secret1, ciphertext) =
            HybridKemKeypair::encapsulate(alice.public_key(), context1).unwrap();

        // Same ciphertext but different context should produce different secret
        let secret2 = alice.decapsulate(&ciphertext, context2).unwrap();

        assert_ne!(
            secret1, secret2,
            "Different context should produce different secret"
        );
    }

    /// Test hybrid KEM serialization roundtrip
    #[test]
    fn test_hybrid_kem_serialization() {
        let keypair = HybridKemKeypair::generate().unwrap();
        let public_key = keypair.public_key();
        let context = b"serialization-test";

        // Serialize public key
        let json = serde_json::to_string(&public_key).expect("Serialization should succeed");

        // Deserialize
        let restored: icn_crypto_pq::HybridKemPublicKey =
            serde_json::from_str(&json).expect("Deserialization should succeed");

        // Encapsulate with restored key
        let (shared_secret, ciphertext) =
            HybridKemKeypair::encapsulate(&restored, context).unwrap();

        // Original keypair should decapsulate correctly
        let decap_secret = keypair.decapsulate(&ciphertext, context).unwrap();
        assert_eq!(
            shared_secret, decap_secret,
            "Restored key encapsulation should work"
        );
    }
}

// =============================================================================
// Threshold Cryptography Integration Tests
// =============================================================================

mod threshold_tests {
    use super::*;

    /// Test n-of-n additive secret sharing
    #[test]
    fn test_additive_secret_sharing() {
        let secret = [42u8; 32];
        let n = 5u8;

        // Split secret into n shares
        let shares = AdditiveSecretSharing::split(&secret, n).expect("Splitting should succeed");

        assert_eq!(shares.len(), n as usize, "Should have n shares");

        // Generate commitments for each share
        let commitments: Vec<ShareCommitment> =
            shares.iter().map(ShareCommitment::commit).collect();

        assert_eq!(commitments.len(), n as usize, "Should have n commitments");

        // Verify each share against its commitment
        for (share, commitment) in shares.iter().zip(commitments.iter()) {
            assert!(
                commitment.verify(share),
                "Share {} should verify against commitment",
                share.index
            );
        }

        // Reconstruct secret
        let reconstructed =
            AdditiveSecretSharing::reconstruct(&shares).expect("Reconstruction should succeed");
        assert_eq!(reconstructed, secret, "Reconstructed secret should match");
    }

    /// Test threshold PRF computation
    #[test]
    fn test_threshold_prf() {
        // Create pepper shares using proper splitting
        let pepper = [0xABu8; 32];
        let shares = AdditiveSecretSharing::split(&pepper, 3).expect("Split should succeed");

        let input = b"credential_hash_input";

        // Each steward computes PRF partial
        let partials: Vec<PrfPartial> = shares
            .iter()
            .map(|s| PrfPartial::compute(s, input))
            .collect();

        assert_eq!(partials.len(), 3, "Should have 3 partials");

        // Combine partials
        let combined = combine_prf_partials(&partials).expect("Combine should succeed");
        assert_eq!(combined.len(), 32, "Combined output should be 32 bytes");

        // Same input should produce same output
        let partials2: Vec<PrfPartial> = shares
            .iter()
            .map(|s| PrfPartial::compute(s, input))
            .collect();
        let combined2 = combine_prf_partials(&partials2).expect("Combine should succeed");
        assert_eq!(combined, combined2, "Same input should produce same output");

        // Different input should produce different output
        let different_input = b"different_credential_hash";
        let partials3: Vec<PrfPartial> = shares
            .iter()
            .map(|s| PrfPartial::compute(s, different_input))
            .collect();
        let combined3 = combine_prf_partials(&partials3).expect("Combine should succeed");
        assert_ne!(
            combined, combined3,
            "Different input should produce different output"
        );
    }

    /// Test share commitment verification
    #[test]
    fn test_share_commitment_verification() {
        let share = PepperShare::new(1, [0x42u8; 32]);
        let commitment = ShareCommitment::commit(&share);

        // Valid share should verify
        assert!(commitment.verify(&share), "Valid share should verify");

        // Different share should not verify
        let wrong_share = PepperShare::new(1, [0x99u8; 32]);
        assert!(
            !commitment.verify(&wrong_share),
            "Wrong share should not verify"
        );

        // Same value but different index should not verify
        let wrong_index = PepperShare::new(2, [0x42u8; 32]);
        assert!(
            !commitment.verify(&wrong_index),
            "Wrong index should not verify"
        );
    }
}

// =============================================================================
// Cross-module Integration Tests
// =============================================================================

mod cross_module_tests {
    use super::*;

    /// Test using ML-DSA alongside ML-KEM in a combined protocol
    #[test]
    fn test_signed_key_exchange() {
        // Alice has signing keypair and KEM keypair
        let alice_sign = MlDsaKeypair::generate().unwrap();
        let alice_kem = MlKemKeypair::generate().unwrap();

        // Bob has signing keypair
        let bob_sign = MlDsaKeypair::generate().unwrap();

        // Alice signs her KEM public key
        let alice_kem_pub_bytes = alice_kem.public_key().as_bytes();
        let alice_sig = alice_sign.sign(alice_kem_pub_bytes).unwrap();

        // Bob verifies Alice's signed KEM key
        assert!(
            MlDsaKeypair::verify(alice_sign.public_key(), alice_kem_pub_bytes, &alice_sig),
            "Bob should verify Alice's signed KEM key"
        );

        // Bob encapsulates to Alice's verified KEM key
        let alice_kem_pub = MlKemPublicKey::from_bytes(alice_kem_pub_bytes).unwrap();
        let (bob_secret, ciphertext) = MlKemKeypair::encapsulate(&alice_kem_pub).unwrap();

        // Bob signs the ciphertext
        let bob_sig = bob_sign.sign(ciphertext.as_bytes()).unwrap();

        // Alice verifies Bob's signature and decapsulates
        assert!(
            MlDsaKeypair::verify(bob_sign.public_key(), ciphertext.as_bytes(), &bob_sig),
            "Alice should verify Bob's signed ciphertext"
        );
        let alice_secret = alice_kem.decapsulate(&ciphertext).unwrap();

        // Both have same shared secret
        assert_eq!(alice_secret, bob_secret, "Shared secrets should match");
    }

    /// Test hybrid operations preserve security properties
    #[test]
    fn test_hybrid_security_properties() {
        let keypair = HybridKeypair::generate().unwrap();
        let public_key = keypair.public_key();

        // Generate multiple signatures for same message
        let message = b"test message";
        let sig1 = keypair.sign(message).unwrap();
        let sig2 = keypair.sign(message).unwrap();

        // Both signatures should verify
        assert!(sig1.verify(message, &public_key));
        assert!(sig2.verify(message, &public_key));

        // Signatures may be different (due to randomness in signing)
        // or the same (if deterministic) - both are valid
    }
}

// =============================================================================
// Performance/Stress Tests
// =============================================================================

mod stress_tests {
    use super::*;

    /// Test many signature operations
    #[test]
    fn test_many_signatures() {
        let keypair = MlDsaKeypair::generate().unwrap();
        let public_key = keypair.public_key();

        for i in 0..10 {
            let message = format!("Message number {i}");
            let sig = keypair.sign(message.as_bytes()).unwrap();
            assert!(
                MlDsaKeypair::verify(public_key, message.as_bytes(), &sig),
                "Signature {i} should verify"
            );
        }
    }

    /// Test many key exchanges
    #[test]
    fn test_many_key_exchanges() {
        let alice = MlKemKeypair::generate().unwrap();

        for _ in 0..10 {
            let (bob_secret, ciphertext) = MlKemKeypair::encapsulate(alice.public_key()).unwrap();
            let alice_secret = alice.decapsulate(&ciphertext).unwrap();
            assert_eq!(alice_secret, bob_secret, "Secrets should match");
        }
    }
}
