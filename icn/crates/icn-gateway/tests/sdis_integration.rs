//! SDIS Integration Tests
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Tests for the Sovereign Digital Identity System credential presentation flow.

use actix_web::{web, App};
use base64::Engine;
use icn_gateway::api::sdis::{
    decode_from_qr, encode_for_qr, Channel, EphemeralBinding, EphemeralProof, EphemeralVerifier,
    QrEstimate, SdisState, VerifyLevel1Request, VerifyLevel2Request, VerifyResponse,
};
use icn_identity::{Anchor, KeyBundle};
use icn_zkp::ProofType;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Test Helpers
// ============================================================================

fn test_keybundle() -> KeyBundle {
    let anchor = Anchor::genesis("sdis-integration-test");
    KeyBundle::generate(anchor, 1).unwrap()
}

fn generate_proof_and_binding(
    keybundle: &KeyBundle,
    proof_type: ProofType,
    validity: Duration,
    channels: Vec<Channel>,
) -> (EphemeralProof, EphemeralBinding) {
    EphemeralProof::generate(keybundle, proof_type, validity, channels).unwrap()
}

// ============================================================================
// Ephemeral Proof Tests
// ============================================================================

#[test]
fn test_ephemeral_proof_size_budget() {
    // Test that ephemeral proofs fit within QR code size budget
    let keybundle = test_keybundle();

    let (proof, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![
            Channel::Nfc,
            Channel::Ble,
            Channel::WifiDirect,
            Channel::Http,
        ],
    );

    // Proof should be under 200 bytes (compact)
    assert!(
        proof.size() < 200,
        "Proof size {} exceeds budget",
        proof.size()
    );

    // QR encoding should be exactly 137 bytes
    let qr_bytes = encode_for_qr(&proof).unwrap();
    assert_eq!(qr_bytes.len(), 137, "QR encoding should be 137 bytes");
}

#[test]
fn test_qr_encoding_all_proof_types() {
    let keybundle = test_keybundle();

    let proof_types = vec![
        ProofType::AgeAtLeast { threshold: 18 },
        ProofType::AgeAtLeast { threshold: 21 },
        ProofType::AgeAtLeast { threshold: 65 },
        ProofType::Citizenship {
            country_code: [b'U', b'S'],
        },
        ProofType::NonRevocation,
    ];

    for proof_type in proof_types {
        let (proof, binding) = generate_proof_and_binding(
            &keybundle,
            proof_type.clone(),
            Duration::from_secs(3600),
            vec![],
        );

        // Encode
        let encoded = encode_for_qr(&proof).unwrap();
        assert_eq!(encoded.len(), 137);

        // Decode
        let decoded = decode_from_qr(&encoded).unwrap();

        // Verify roundtrip
        assert_eq!(decoded.v, proof.v, "Version mismatch");
        assert_eq!(decoded.a, proof.a, "Anchor mismatch");
        assert_eq!(decoded.k, proof.k, "Key mismatch");
        assert_eq!(decoded.x, proof.x, "Expiry mismatch");
        assert_eq!(decoded.n, proof.n, "Nonce mismatch");
        assert_eq!(decoded.s, proof.s, "Signature mismatch");

        // Verify binding matches
        assert!(binding.matches_proof(&proof));
    }
}

#[test]
fn test_channel_combinations() {
    let keybundle = test_keybundle();

    let channel_sets = vec![
        vec![],
        vec![Channel::Nfc],
        vec![Channel::Ble],
        vec![Channel::WifiDirect],
        vec![Channel::Http],
        vec![Channel::Nfc, Channel::Ble],
        vec![Channel::Nfc, Channel::Ble, Channel::WifiDirect],
        vec![
            Channel::Nfc,
            Channel::Ble,
            Channel::WifiDirect,
            Channel::Http,
        ],
    ];

    for channels in channel_sets {
        let (proof, _) = generate_proof_and_binding(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 18 },
            Duration::from_secs(3600),
            channels.clone(),
        );

        let encoded = encode_for_qr(&proof).unwrap();
        let decoded = decode_from_qr(&encoded).unwrap();

        assert_eq!(decoded.c.len(), channels.len());
        for ch in &channels {
            assert!(decoded.c.contains(ch));
        }
    }
}

// ============================================================================
// Verification Level Tests
// ============================================================================

#[test]
fn test_verification_level1_valid_proof() {
    let keybundle = test_keybundle();
    let (proof, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level1(&proof);

    assert!(result.valid, "Valid proof should pass L1 verification");
    assert_eq!(result.level, 1);
    assert!(result.proof_type.is_some());
}

#[test]
fn test_verification_level1_expired_proof() {
    let keybundle = test_keybundle();
    let (mut proof, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    // Force expiry
    proof.x = 1;

    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level1(&proof);

    assert!(!result.valid, "Expired proof should fail L1 verification");
    assert!(result.warnings.iter().any(|w| w.contains("expired")));
}

#[test]
fn test_verification_level1_replay_detection() {
    let keybundle = test_keybundle();
    let (proof, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    let verifier = EphemeralVerifier::new();

    // First verification succeeds
    let result1 = verifier.verify_level1(&proof);
    assert!(result1.valid);

    // Second verification (replay) fails
    let result2 = verifier.verify_level1(&proof);
    assert!(!result2.valid);
    assert!(result2.warnings.iter().any(|w| w.contains("Replay")));
}

#[test]
fn test_verification_level2_with_binding() {
    let keybundle = test_keybundle();
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::Citizenship {
            country_code: [b'U', b'S'],
        },
        Duration::from_secs(3600),
        vec![Channel::Nfc],
    );

    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level2(&proof, &binding);

    assert!(
        result.valid,
        "Valid proof+binding should pass L2 verification"
    );
    assert_eq!(result.level, 2);
}

#[test]
fn test_verification_level2_binding_mismatch() {
    let keybundle1 = test_keybundle();
    let keybundle2 = test_keybundle();

    let (proof, _) = generate_proof_and_binding(
        &keybundle1,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    let (_, wrong_binding) = generate_proof_and_binding(
        &keybundle2,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level2(&proof, &wrong_binding);

    assert!(
        !result.valid,
        "Mismatched binding should fail L2 verification"
    );
    assert!(result.warnings.iter().any(|w| w.contains("match")));
}

#[test]
fn test_verification_level2_cached_binding() {
    let keybundle = test_keybundle();
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 18 },
        Duration::from_secs(3600),
        vec![Channel::Ble],
    );

    let verifier = EphemeralVerifier::new();

    // Cache the binding
    verifier.cache_binding(binding.clone());

    // Verify using cached binding
    let result = verifier.verify_level2_cached(&proof);

    assert!(result.valid, "Cached binding verification should succeed");
    assert_eq!(result.level, 2);
}

// ============================================================================
// QR Size Estimation Tests
// ============================================================================

#[test]
fn test_qr_estimate() {
    let keybundle = test_keybundle();
    let (proof, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 18 },
        Duration::from_secs(3600),
        vec![],
    );

    let estimate = QrEstimate::for_proof(&proof);

    // Should fit in QR Version 10 or smaller
    assert!(
        estimate.qr_version <= 10,
        "QR version {} too high",
        estimate.qr_version
    );
    assert_eq!(estimate.data_size, 137);

    // Modules formula: 17 + 4 * version
    let expected_modules = 17 + 4 * (estimate.qr_version as u16);
    assert_eq!(estimate.modules, expected_modules);
}

// ============================================================================
// API Integration Tests
// ============================================================================

#[actix_web::test]
async fn test_sdis_health_endpoint() {
    let state = Arc::new(SdisState::new());
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(icn_gateway::api::sdis::configure),
    )
    .await;

    let req = actix_web::test::TestRequest::get()
        .uri("/v1/sdis/health")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "sdis");
    assert_eq!(body["version"], "1.0");
}

#[actix_web::test]
async fn test_level1_verification_api() {
    let state = Arc::new(SdisState::new());
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(icn_gateway::api::sdis::configure),
    )
    .await;

    // Generate a proof
    let keybundle = test_keybundle();
    let (proof, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    let qr_data = encode_for_qr(&proof).unwrap();
    let qr_b64 = base64::engine::general_purpose::STANDARD.encode(&qr_data);

    let req = actix_web::test::TestRequest::post()
        .uri("/v1/sdis/verify/level1")
        .set_json(VerifyLevel1Request { qr_data: qr_b64 })
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: VerifyResponse = actix_web::test::read_body_json(resp).await;
    assert!(body.valid);
    assert_eq!(body.level, 1);
    assert!(body.proof_type.is_some());
}

#[actix_web::test]
async fn test_level2_verification_api() {
    let state = Arc::new(SdisState::new());
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(icn_gateway::api::sdis::configure),
    )
    .await;

    // Generate proof and binding
    // Note: Using AgeAtLeast because QR encoding preserves the threshold,
    // whereas Citizenship loses the country code (becomes placeholder)
    let keybundle = test_keybundle();
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![Channel::Nfc],
    );

    let qr_data = encode_for_qr(&proof).unwrap();
    let qr_b64 = base64::engine::general_purpose::STANDARD.encode(&qr_data);

    let binding_bytes = icn_encoding::encode_bincode_legacy(&binding).unwrap();
    let binding_b64 = base64::engine::general_purpose::STANDARD.encode(&binding_bytes);

    let req = actix_web::test::TestRequest::post()
        .uri("/v1/sdis/verify/level2")
        .set_json(VerifyLevel2Request {
            qr_data: qr_b64,
            binding: Some(binding_b64),
        })
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: VerifyResponse = actix_web::test::read_body_json(resp).await;
    assert!(
        body.valid,
        "L2 verification failed: {:?}, warnings: {:?}",
        body.proof_type, body.warnings
    );
    assert_eq!(body.level, 2);
}

#[actix_web::test]
async fn test_level1_verification_api_invalid_base64() {
    let state = Arc::new(SdisState::new());
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(icn_gateway::api::sdis::configure),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/v1/sdis/verify/level1")
        .set_json(VerifyLevel1Request {
            qr_data: "not-valid-base64!!!".to_string(),
        })
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_level1_verification_api_invalid_qr_data() {
    let state = Arc::new(SdisState::new());
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(icn_gateway::api::sdis::configure),
    )
    .await;

    // Valid base64 but invalid QR data (too short)
    let invalid_qr = base64::engine::general_purpose::STANDARD.encode([0u8; 50]);

    let req = actix_web::test::TestRequest::post()
        .uri("/v1/sdis/verify/level1")
        .set_json(VerifyLevel1Request {
            qr_data: invalid_qr,
        })
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

// ============================================================================
// Multiple Identity Tests
// ============================================================================

#[test]
fn test_multiple_identities_independent_verification() {
    // Test that multiple identities can generate and verify proofs independently
    let keybundles: Vec<KeyBundle> = (0..5)
        .map(|i| {
            let anchor = Anchor::genesis(&format!("test-identity-{i}"));
            KeyBundle::generate(anchor, 1).unwrap()
        })
        .collect();

    let verifier = EphemeralVerifier::new();

    for (i, keybundle) in keybundles.iter().enumerate() {
        // Generate separate proofs for L1 and L2 tests (since L2 internally calls L1)
        let (proof_l1, _) = generate_proof_and_binding(
            keybundle,
            ProofType::AgeAtLeast {
                threshold: 18 + (i as u8),
            },
            Duration::from_secs(3600),
            vec![],
        );

        let (proof_l2, binding) = generate_proof_and_binding(
            keybundle,
            ProofType::AgeAtLeast {
                threshold: 18 + (i as u8),
            },
            Duration::from_secs(3600),
            vec![],
        );

        // L1 verification (with first proof)
        let result = verifier.verify_level1(&proof_l1);
        assert!(result.valid, "Identity {i} should pass L1");

        // L2 verification (with second proof - L2 internally calls L1)
        let result = verifier.verify_level2(&proof_l2, &binding);
        assert!(result.valid, "Identity {i} should pass L2");
    }
}

#[test]
fn test_proof_validity_boundaries() {
    let keybundle = test_keybundle();

    // Test minimum validity (1 second)
    let (proof_min, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 18 },
        Duration::from_secs(1),
        vec![],
    );
    assert!(!proof_min.is_expired());

    // Test maximum validity (24 hours)
    let (proof_max, _) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 18 },
        Duration::from_secs(86400),
        vec![],
    );
    assert!(!proof_max.is_expired());

    // Test exceeding maximum validity (should fail)
    let result = EphemeralProof::generate(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 18 },
        Duration::from_secs(100000), // > 24 hours
        vec![],
    );
    assert!(result.is_err(), "Should reject validity > 24 hours");
}

// ============================================================================
// Binding Tests
// ============================================================================

#[test]
fn test_binding_size() {
    let keybundle = test_keybundle();
    let (_, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![],
    );

    // Binding size should be reasonable for NFC/BLE transfer
    let size = binding.size();
    assert!(size < 5000, "Binding size {size} too large for NFC");

    // Should have both classical and hybrid signatures
    assert!(
        !binding.signature.is_empty(),
        "Should have classical signature"
    );
    assert!(
        !binding.hybrid_signature.is_empty(),
        "Should have hybrid signature"
    );
}

#[test]
fn test_binding_serialization() {
    let keybundle = test_keybundle();
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::Citizenship {
            country_code: [b'G', b'B'],
        },
        Duration::from_secs(3600),
        vec![Channel::Nfc],
    );

    // Serialize and deserialize
    let serialized = icn_encoding::encode_bincode_legacy(&binding).unwrap();
    let deserialized: EphemeralBinding = icn_encoding::decode_bincode_legacy(&serialized).unwrap();

    // Should still match the proof
    assert!(deserialized.matches_proof(&proof));
    assert_eq!(deserialized.anchor, binding.anchor);
    assert_eq!(deserialized.ephemeral_pk, binding.ephemeral_pk);
    assert_eq!(deserialized.expires, binding.expires);
}

// ============================================================================
// Level 3 STARK Verification Tests
// ============================================================================

#[test]
fn test_verification_level3_with_stark_proof() {
    use icn_zkp::{
        proof_capability, AgeAttestation, ProofCapability, ProofContext, RsaAccumulator, ZkProver,
    };

    // Skip if no proof capability
    let capability = proof_capability();
    if !capability.can_prove() {
        eprintln!(
            "Skipping STARK test: no proof capability ({:?})",
            capability.description()
        );
        return;
    }

    let keybundle = test_keybundle();
    let issuer_pk = [1u8; 32];

    // Generate ephemeral proof and binding
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![Channel::Nfc],
    );

    // Generate STARK proof for age verification
    let prover = ZkProver::new();
    let context = ProofContext::new(None);

    // Create attestation for someone 25 years old
    let current_days = (context.timestamp / 86400) as u32;
    let birthdate_days = current_days - (25 * 365);

    let attestation = AgeAttestation {
        birthdate_days,
        signature: vec![0u8; 64],
    };

    let stark_proof = prover
        .prove_age(21, &attestation, issuer_pk, &context)
        .expect("STARK proof generation should succeed");

    // Create accumulator (empty - no revoked credentials)
    let accumulator = RsaAccumulator::new_test();

    // Verify at Level 3
    let verifier = EphemeralVerifier::new();
    verifier.add_trusted_issuer(issuer_pk);

    let result = verifier.verify_level3(&proof, &binding, &stark_proof, &accumulator, issuer_pk);

    assert!(
        result.valid,
        "Level 3 verification should pass. Warnings: {:?}",
        result.warnings
    );
    assert_eq!(result.level, 3, "Should report Level 3 verification");

    // Check proof capability is reported correctly
    if matches!(capability, ProofCapability::Stark) {
        println!("Using production STARK proofs");
    } else {
        println!("WARNING: Using simulated proofs for testing");
    }
}

#[test]
fn test_verification_level3_empty_stark_proof_fails() {
    use icn_zkp::{RsaAccumulator, StarkProof};

    let keybundle = test_keybundle();
    let issuer_pk = [1u8; 32];

    // Generate ephemeral proof and binding
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 18 },
        Duration::from_secs(3600),
        vec![],
    );

    // Create an empty/invalid STARK proof
    let empty_stark = StarkProof {
        proof_bytes: vec![], // Empty!
        public_inputs_hash: [0u8; 32],
        version: 1,
        simulated: false,
    };

    let accumulator = RsaAccumulator::new_test();

    // Verify at Level 3 - should fail due to empty proof
    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level3(&proof, &binding, &empty_stark, &accumulator, issuer_pk);

    assert!(
        !result.valid,
        "Empty STARK proof should fail Level 3 verification"
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("STARK")),
        "Should mention STARK in failure reason"
    );
}

#[test]
fn test_verification_level3_invalid_public_inputs_fails() {
    use icn_zkp::{RsaAccumulator, StarkProof};

    let keybundle = test_keybundle();
    let issuer_pk = [1u8; 32];

    // Generate ephemeral proof and binding
    let (proof, binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::Citizenship {
            country_code: [b'U', b'S'],
        },
        Duration::from_secs(3600),
        vec![Channel::Ble],
    );

    // Create STARK proof with all-zero public inputs (invalid)
    let invalid_stark = StarkProof {
        proof_bytes: vec![0u8; 100],   // Non-empty but garbage
        public_inputs_hash: [0u8; 32], // All zeros = invalid
        version: 1,
        simulated: false,
    };

    let accumulator = RsaAccumulator::new_test();

    // Verify at Level 3 - should fail due to invalid public inputs
    let verifier = EphemeralVerifier::new();
    let result = verifier.verify_level3(&proof, &binding, &invalid_stark, &accumulator, issuer_pk);

    assert!(
        !result.valid,
        "Invalid public inputs should fail Level 3 verification"
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("public inputs")),
        "Should mention public inputs in failure reason"
    );
}

#[test]
fn test_stark_proof_capability_check() {
    use icn_zkp::{proof_capability, ProofCapability};

    let capability = proof_capability();

    // icn-gateway depends on icn-zkp with stark feature enabled,
    // so we should always have STARK capability
    assert_eq!(
        capability,
        ProofCapability::Stark,
        "icn-gateway should have STARK capability via icn-zkp dependency"
    );
    assert!(capability.is_secure(), "STARK should be secure");
    assert!(capability.can_prove(), "Should be able to prove with STARK");

    println!(
        "Proof capability: {:?} - {}",
        capability,
        capability.description()
    );
}

#[test]
fn test_full_sdis_l3_flow_with_stark() {
    use icn_zkp::{
        proof_capability, AgeAttestation, ProofContext, RsaAccumulator, ZkProver, ZkVerifier,
    };

    // Skip if no proof capability
    if !proof_capability().can_prove() {
        eprintln!("Skipping full L3 flow test: no proof capability");
        return;
    }

    // === SETUP ===
    let keybundle = test_keybundle();
    let issuer_pk = [1u8; 32];

    // === PROVER SIDE ===

    // 1. Generate ephemeral proof for presentation
    let (ephemeral_proof, _binding) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![Channel::Nfc, Channel::Ble],
    );

    // 2. Generate STARK proof of age attribute
    let prover = ZkProver::new();
    let context = ProofContext::new(None);

    let current_days = (context.timestamp / 86400) as u32;
    let attestation = AgeAttestation {
        birthdate_days: current_days - (30 * 365), // 30 years old
        signature: vec![0u8; 64],
    };

    let stark_proof = prover
        .prove_age(21, &attestation, issuer_pk, &context)
        .expect("Age proof should succeed for 30 year old");

    // 3. Setup accumulator (simulating revocation registry)
    let mut accumulator = RsaAccumulator::new_test();
    // Add some revoked credentials (not ours)
    accumulator.add(&[0xDE, 0xAD].repeat(16).try_into().unwrap());

    // === VERIFIER SIDE ===

    // 1. Receive the QR code, decode it
    let qr_bytes = encode_for_qr(&ephemeral_proof).expect("QR encoding should work");
    let decoded_proof = decode_from_qr(&qr_bytes).expect("QR decoding should work");

    // 2. Setup verifier
    let verifier = EphemeralVerifier::new();
    verifier.add_trusted_issuer(issuer_pk);

    // 3. Level 1: QR scan verification
    let l1_result = verifier.verify_level1(&decoded_proof);
    assert!(l1_result.valid, "L1 should pass");
    assert_eq!(l1_result.level, 1);

    // 4. Level 2: With binding (received via NFC/BLE)
    // Note: We need a fresh proof since L1 consumed the nonce
    let (ephemeral_proof2, binding2) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![Channel::Nfc],
    );
    let l2_result = verifier.verify_level2(&ephemeral_proof2, &binding2);
    assert!(l2_result.valid, "L2 should pass");
    assert_eq!(l2_result.level, 2);

    // 5. Level 3: Full STARK verification
    // Note: Need fresh proof again
    let (ephemeral_proof3, binding3) = generate_proof_and_binding(
        &keybundle,
        ProofType::AgeAtLeast { threshold: 21 },
        Duration::from_secs(3600),
        vec![Channel::Http],
    );
    let l3_result = verifier.verify_level3(
        &ephemeral_proof3,
        &binding3,
        &stark_proof,
        &accumulator,
        issuer_pk,
    );
    assert!(l3_result.valid, "L3 should pass: {:?}", l3_result.warnings);
    assert_eq!(l3_result.level, 3);

    // === VERIFY STARK PROOF DIRECTLY ===
    let mut zk_verifier = ZkVerifier::new();
    let verification = zk_verifier
        .verify_age(21, &stark_proof, issuer_pk, &context)
        .expect("Direct STARK verification should not error");

    assert!(
        verification.valid,
        "Direct STARK verification should pass: {:?}",
        verification.warnings
    );

    println!("Full SDIS L3 flow completed successfully!");
    println!("  - L1 (QR): PASS");
    println!("  - L2 (Binding): PASS");
    println!("  - L3 (STARK): PASS");
    println!("  - Proof capability: {}", proof_capability().description());
}
