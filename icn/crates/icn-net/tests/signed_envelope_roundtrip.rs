//! Roundtrip tests for SignedEnvelope (issue #1065)
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Validates signing, verification, serialization, and tamper detection
//! across all payload types and typed payload workflows.

use icn_identity::KeyPair;
use icn_net::envelope::{PayloadType, SignedEnvelope};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Helper: all PayloadType variants for exhaustive coverage.
fn all_payload_types() -> Vec<PayloadType> {
    vec![
        PayloadType::Gossip,
        PayloadType::Ledger,
        PayloadType::Trust,
        PayloadType::Contract,
        PayloadType::Rpc,
        PayloadType::Control,
        PayloadType::Encrypted,
    ]
}

// --------------------------------------------------------------------------
// 1. Roundtrip across every PayloadType variant
// --------------------------------------------------------------------------

#[test]
fn test_roundtrip_all_payload_types() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did();

    for (seq, pt) in all_payload_types().into_iter().enumerate() {
        let payload = format!("payload-for-{pt:?}").into_bytes();
        let env = SignedEnvelope::new(did, &kp, seq as u64, pt, payload.clone()).unwrap();

        // Structural checks
        assert_eq!(env.from, *did, "from DID mismatch for {pt:?}");
        assert_eq!(env.sequence, seq as u64, "sequence mismatch for {pt:?}");
        assert_eq!(env.payload_type, pt, "payload_type mismatch for {pt:?}");
        assert_eq!(env.payload, payload, "payload bytes mismatch for {pt:?}");

        // Signature must verify
        env.verify(300)
            .unwrap_or_else(|e| panic!("verify failed for {pt:?}: {e}"));
    }
}

// --------------------------------------------------------------------------
// 2. Serialize (postcard) -> deserialize -> verify signature still valid
// --------------------------------------------------------------------------

#[test]
fn test_serialize_deserialize_roundtrip() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did();

    let env = SignedEnvelope::new(
        did,
        &kp,
        42,
        PayloadType::Gossip,
        b"serde-roundtrip".to_vec(),
    )
    .unwrap();

    // Serialize with the project's canonical encoding (postcard via icn-encoding)
    let bytes = icn_encoding::encode(&env).unwrap();
    assert!(!bytes.is_empty(), "serialized bytes must be non-empty");

    // Deserialize back
    let restored: SignedEnvelope = icn_encoding::decode(&bytes).unwrap();

    // All fields must survive the roundtrip
    assert_eq!(restored.from, env.from);
    assert_eq!(restored.sequence, env.sequence);
    assert_eq!(restored.timestamp, env.timestamp);
    assert_eq!(restored.payload_type, env.payload_type);
    assert_eq!(restored.payload, env.payload);
    assert_eq!(restored.signature, env.signature);
    assert_eq!(restored.pq_signature, env.pq_signature);

    // Signature must still verify after deserialization
    restored
        .verify(300)
        .expect("signature must verify after serde roundtrip");
}

// --------------------------------------------------------------------------
// 3. Tampered payload bytes -> verification must fail
// --------------------------------------------------------------------------

#[test]
fn test_tampered_payload_fails_verification() {
    let kp = KeyPair::generate().unwrap();
    let mut env =
        SignedEnvelope::new(kp.did(), &kp, 1, PayloadType::Ledger, b"original".to_vec()).unwrap();

    // Flip one bit in the payload
    env.payload[0] ^= 0xFF;

    let result = env.verify(300);
    assert!(
        result.is_err(),
        "tampered payload must fail verification: got Ok(())"
    );
}

// --------------------------------------------------------------------------
// 4. Tampered sequence number -> verification must fail
// --------------------------------------------------------------------------

#[test]
fn test_tampered_sequence_fails() {
    let kp = KeyPair::generate().unwrap();
    let mut env =
        SignedEnvelope::new(kp.did(), &kp, 10, PayloadType::Trust, b"seq-test".to_vec()).unwrap();

    // Bump sequence without re-signing
    env.sequence += 1;

    let result = env.verify(300);
    assert!(
        result.is_err(),
        "tampered sequence must fail verification: got Ok(())"
    );
}

// --------------------------------------------------------------------------
// 5. Sign with key A, but set from-DID to key B -> wrong key must fail
// --------------------------------------------------------------------------

#[test]
fn test_wrong_key_fails() {
    let kp_a = KeyPair::generate().unwrap();
    let kp_b = KeyPair::generate().unwrap();

    // Sign with kp_a but claim to be kp_b
    let env = SignedEnvelope::new(
        kp_b.did(), // Claim B's identity
        &kp_a,      // Sign with A's private key
        1,
        PayloadType::Rpc,
        b"impersonation-attempt".to_vec(),
    )
    .unwrap();

    let result = env.verify(300);
    assert!(
        result.is_err(),
        "wrong signer must fail verification: got Ok(())"
    );
}

// --------------------------------------------------------------------------
// 6. Expired message -> age check must reject
// --------------------------------------------------------------------------

#[test]
fn test_expired_message_rejected() {
    let kp = KeyPair::generate().unwrap();

    // Build an envelope with a timestamp 10 minutes in the past, then re-sign
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let old_timestamp = now_ms.saturating_sub(600_000); // 10 minutes ago

    let mut env = SignedEnvelope {
        from: kp.did().clone(),
        sequence: 1,
        timestamp: old_timestamp,
        payload_type: PayloadType::Control,
        payload: b"old-message".to_vec(),
        signature_type: icn_net::envelope::SignatureType::Classical,
        signature: Vec::new(),
        pq_signature: None,
    };

    // Manually compute canonical encoding and sign (mimics what `new()` does internally)
    let mut sig_input = Vec::new();
    sig_input.extend_from_slice(&env.sequence.to_be_bytes());
    sig_input.extend_from_slice(&env.timestamp.to_be_bytes());
    sig_input.push(env.payload_type as u8);
    sig_input.extend_from_slice(&env.payload);
    env.signature = kp.sign(&sig_input).to_vec();

    // Verify with a tight max_age (60 seconds) -- the 10-minute-old message must be rejected
    let result = env.verify(60);
    assert!(
        result.is_err(),
        "expired message must be rejected: got Ok(())"
    );

    // Verify with a generous max_age (900 seconds = 15 minutes) -- should pass
    env.verify(900)
        .expect("message within generous max_age should verify");
}

// --------------------------------------------------------------------------
// 7. from_payload() + decode_payload() typed roundtrip
// --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LedgerEntry {
    from_account: String,
    to_account: String,
    amount: u64,
    memo: String,
}

#[test]
fn test_decode_typed_payload_roundtrip() {
    let kp = KeyPair::generate().unwrap();

    let entry = LedgerEntry {
        from_account: "alice".into(),
        to_account: "bob".into(),
        amount: 500,
        memo: "mutual credit transfer".into(),
    };

    // Create envelope with typed payload
    let env = SignedEnvelope::from_payload(kp.did(), &kp, 7, PayloadType::Ledger, &entry).unwrap();

    // Verify signature
    env.verify(300).expect("typed payload envelope must verify");

    // Decode back to the original type
    let decoded: LedgerEntry = env.decode_payload().unwrap();
    assert_eq!(decoded, entry, "decoded typed payload must match original");
}

// --------------------------------------------------------------------------
// Bonus: typed payload roundtrip survives serde
// --------------------------------------------------------------------------

#[test]
fn test_typed_payload_survives_serde() {
    let kp = KeyPair::generate().unwrap();

    let entry = LedgerEntry {
        from_account: "coop-treasury".into(),
        to_account: "member-42".into(),
        amount: 1000,
        memo: "dividend".into(),
    };

    let env = SignedEnvelope::from_payload(kp.did(), &kp, 1, PayloadType::Ledger, &entry).unwrap();

    // Serialize the whole envelope
    let bytes = icn_encoding::encode(&env).unwrap();

    // Deserialize on the "receiving" side
    let restored: SignedEnvelope = icn_encoding::decode(&bytes).unwrap();

    // Verify signature after transit
    restored
        .verify(300)
        .expect("signature must survive serde roundtrip");

    // Decode typed payload after transit
    let decoded: LedgerEntry = restored.decode_payload().unwrap();
    assert_eq!(decoded, entry, "typed payload must survive full roundtrip");
}
