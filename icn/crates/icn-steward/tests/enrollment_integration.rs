//! Integration tests for the enrollment flow
//!
//! Tests multi-steward enrollment scenarios including:
//! - Threshold PRF computation across multiple stewards
//! - VUI uniqueness verification
//! - Gossip message coordination
//! - Client-server enrollment protocol

use icn_crypto_pq::threshold::{PepperShare, PrfPartial, ShareCommitment};
use icn_identity::KeyPair;
use icn_steward::ceremony::enrollment::ShareContribution;
use icn_steward::gossip::{EnrollmentMessage, StewardMessage, VuiSyncMessage};
use icn_steward::vui_registry::VuiRegistry;
use icn_steward::{EnrollmentClient, EnrollmentConfig, EnrollmentRequest, EnrollmentService};

/// Create a test DID
fn test_did() -> icn_identity::Did {
    let keypair = KeyPair::generate().unwrap();
    keypair.did().clone()
}

/// Create pepper shares for a set of stewards
fn create_pepper_shares(n: usize) -> Vec<PepperShare> {
    (1..=n)
        .map(|i| {
            let mut seed = [0u8; 32];
            seed[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            PepperShare::new(i as u8, seed)
        })
        .collect()
}

#[test]
fn test_single_steward_enrollment() {
    // Setup single steward
    let did = test_did();
    let mut service = EnrollmentService::new(
        did,
        EnrollmentConfig {
            threshold: 1,
            total_stewards: 1,
            ..Default::default()
        },
    );

    let share = PepperShare::new(1, [42u8; 32]);
    service.set_pepper_share(share.clone());

    // Client initiates enrollment
    let id_data_hash = [99u8; 32];
    let mut client = EnrollmentClient::new(id_data_hash);
    let request = client.create_request([1u8; 8]);

    // Verify request commitment
    assert!(request.verify_commitment(&id_data_hash));

    // Start ceremony
    let response = service.start_ceremony(request).unwrap();
    assert!(!response.stewards.is_empty());

    // Compute PRF partial
    let partial = service
        .compute_prf_partial(&response.ceremony_id, &id_data_hash)
        .unwrap();

    // Client collects partial
    client.add_partial(partial.clone());
    assert!(client.has_enough_partials(1));

    // Add contribution to ceremony
    let contribution = ShareContribution {
        steward_did: service.steward_did().clone(),
        encrypted_share: vec![1, 2, 3],
        share_commitment: service.share_commitment().unwrap().commitment,
        signature: vec![4, 5, 6],
    };
    service
        .add_contribution(&response.ceremony_id, contribution)
        .unwrap();

    // Complete VUI computation on server
    let server_vui = service
        .complete_vui_computation(&response.ceremony_id, &[partial])
        .unwrap();

    // Client computes VUI
    let client_vui = client.compute_vui().unwrap();
    assert_eq!(server_vui, client_vui);

    // Verify uniqueness
    let is_unique = service.verify_uniqueness(&response.ceremony_id).unwrap();
    assert!(is_unique);

    // Issue token
    let token_request = client.create_blinded_request(response.ceremony_id).unwrap();
    let result = service
        .issue_blinded_signature(&response.ceremony_id, &token_request.blinded_message)
        .unwrap();

    assert_eq!(result.vui_hash, server_vui);
}

#[test]
fn test_three_of_five_threshold_prf_combination() {
    // This test verifies that threshold PRF partials from different stewards
    // can be combined to produce the same VUI. In production, ceremonies
    // are synchronized via gossip; here we test the cryptographic properties.

    // Setup 5 independent pepper shares (simulating 5 stewards)
    let shares = create_pepper_shares(5);
    let threshold = 3;

    // Client initiates enrollment
    let id_data_hash = [42u8; 32];
    let mut client = EnrollmentClient::new(id_data_hash);
    let _request = client.create_request([1u8; 8]);

    // Collect PRF partials from threshold number of stewards
    // In production, these would come from different steward nodes
    for share in shares.iter().take(threshold) {
        let partial = PrfPartial::compute(share, &id_data_hash);
        client.add_partial(partial);
    }

    assert!(client.has_enough_partials(threshold));

    // Client computes VUI from partials
    let vui = client.compute_vui().unwrap();

    // Verify determinism: same partials produce same VUI
    let mut client2 = EnrollmentClient::new(id_data_hash);
    for share in shares.iter().take(threshold) {
        let partial = PrfPartial::compute(share, &id_data_hash);
        client2.add_partial(partial);
    }
    let vui2 = client2.compute_vui().unwrap();
    assert_eq!(vui, vui2);

    // Using different subset of stewards produces different VUI
    // (since we're using different pepper shares)
    let mut client3 = EnrollmentClient::new(id_data_hash);
    for share in shares.iter().skip(2).take(threshold) {
        let partial = PrfPartial::compute(share, &id_data_hash);
        client3.add_partial(partial);
    }
    let vui3 = client3.compute_vui().unwrap();
    // Different shares combination produces different VUI
    assert_ne!(vui, vui3);
}

#[test]
fn test_multi_steward_enrollment_simulation() {
    // Simulates multi-steward enrollment using a coordinator pattern
    // In production, ceremonies would be synchronized via gossip

    let threshold = 3;
    let shares = create_pepper_shares(5);

    // Client setup
    let id_data_hash = [42u8; 32];
    let mut client = EnrollmentClient::new(id_data_hash);
    let request = client.create_request([1u8; 8]);

    // Coordinator steward starts ceremony
    let coordinator_did = test_did();
    let mut coordinator = EnrollmentService::new(
        coordinator_did.clone(),
        EnrollmentConfig {
            threshold: threshold as u32,
            total_stewards: 5,
            ..Default::default()
        },
    );
    coordinator.set_pepper_share(shares[0].clone());

    let response = coordinator.start_ceremony(request).unwrap();
    let ceremony_id = response.ceremony_id;

    // Collect PRF partials (simulating gossip delivery)
    let mut partials = Vec::new();
    for (i, share) in shares.iter().enumerate().take(threshold) {
        let partial = PrfPartial::compute(share, &id_data_hash);
        partials.push(partial.clone());
        client.add_partial(partial);

        // Add contribution to coordinator's ceremony
        let contribution = ShareContribution {
            steward_did: if i == 0 {
                coordinator_did.clone()
            } else {
                test_did() // Simulated steward DID
            },
            encrypted_share: vec![i as u8],
            share_commitment: ShareCommitment::commit(share).commitment,
            signature: vec![],
        };
        coordinator
            .add_contribution(&ceremony_id, contribution)
            .unwrap();
    }

    // Client computes VUI
    let client_vui = client.compute_vui().unwrap();

    // Coordinator completes VUI computation
    let server_vui = coordinator
        .complete_vui_computation(&ceremony_id, &partials)
        .unwrap();

    // VUIs match
    assert_eq!(client_vui, server_vui);

    // Verify uniqueness
    let is_unique = coordinator.verify_uniqueness(&ceremony_id).unwrap();
    assert!(is_unique);

    // Issue token
    let token_request = client.create_blinded_request(ceremony_id).unwrap();
    let result = coordinator
        .issue_blinded_signature(&ceremony_id, &token_request.blinded_message)
        .unwrap();

    assert_eq!(result.vui_hash, server_vui);

    // VUI is now registered
    let stats = coordinator.stats();
    assert_eq!(stats.registered_vuis, 1);
}

#[test]
fn test_duplicate_enrollment_rejection() {
    // Setup steward
    let did = test_did();
    let mut service = EnrollmentService::new(
        did,
        EnrollmentConfig {
            threshold: 1,
            total_stewards: 1,
            ..Default::default()
        },
    );

    let share = PepperShare::new(1, [42u8; 32]);
    service.set_pepper_share(share);

    // First enrollment
    let id_data_hash = [99u8; 32];
    let mut client1 = EnrollmentClient::new(id_data_hash);
    let request1 = client1.create_request([1u8; 8]);

    let response1 = service.start_ceremony(request1).unwrap();
    let partial1 = service
        .compute_prf_partial(&response1.ceremony_id, &id_data_hash)
        .unwrap();

    let contribution1 = ShareContribution {
        steward_did: service.steward_did().clone(),
        encrypted_share: vec![1],
        share_commitment: service.share_commitment().unwrap().commitment,
        signature: vec![],
    };
    service
        .add_contribution(&response1.ceremony_id, contribution1)
        .unwrap();

    service
        .complete_vui_computation(&response1.ceremony_id, std::slice::from_ref(&partial1))
        .unwrap();

    let is_unique1 = service.verify_uniqueness(&response1.ceremony_id).unwrap();
    assert!(is_unique1);

    // Complete first enrollment
    client1.add_partial(partial1);
    client1.compute_vui().unwrap();
    let token_request1 = client1
        .create_blinded_request(response1.ceremony_id)
        .unwrap();
    service
        .issue_blinded_signature(&response1.ceremony_id, &token_request1.blinded_message)
        .unwrap();

    // Second enrollment with SAME identity data should be rejected
    let mut client2 = EnrollmentClient::new(id_data_hash); // Same id_data_hash!
    let request2 = client2.create_request([2u8; 8]); // Different pathway

    let response2 = service.start_ceremony(request2).unwrap();
    let partial2 = service
        .compute_prf_partial(&response2.ceremony_id, &id_data_hash)
        .unwrap();

    let contribution2 = ShareContribution {
        steward_did: service.steward_did().clone(),
        encrypted_share: vec![2],
        share_commitment: service.share_commitment().unwrap().commitment,
        signature: vec![],
    };
    service
        .add_contribution(&response2.ceremony_id, contribution2)
        .unwrap();

    service
        .complete_vui_computation(&response2.ceremony_id, &[partial2])
        .unwrap();

    // This should fail - VUI already registered
    let is_unique2 = service.verify_uniqueness(&response2.ceremony_id).unwrap();
    assert!(!is_unique2);
}

#[test]
fn test_different_users_can_enroll() {
    // Setup steward
    let did = test_did();
    let mut service = EnrollmentService::new(
        did,
        EnrollmentConfig {
            threshold: 1,
            total_stewards: 1,
            ..Default::default()
        },
    );

    let share = PepperShare::new(1, [42u8; 32]);
    service.set_pepper_share(share);

    // First user enrolls
    let id_data_hash1 = [11u8; 32];
    let mut client1 = EnrollmentClient::new(id_data_hash1);
    let request1 = client1.create_request([1u8; 8]);

    let response1 = service.start_ceremony(request1).unwrap();
    let partial1 = service
        .compute_prf_partial(&response1.ceremony_id, &id_data_hash1)
        .unwrap();

    let contribution1 = ShareContribution {
        steward_did: service.steward_did().clone(),
        encrypted_share: vec![1],
        share_commitment: service.share_commitment().unwrap().commitment,
        signature: vec![],
    };
    service
        .add_contribution(&response1.ceremony_id, contribution1)
        .unwrap();

    service
        .complete_vui_computation(&response1.ceremony_id, std::slice::from_ref(&partial1))
        .unwrap();
    service.verify_uniqueness(&response1.ceremony_id).unwrap();

    client1.add_partial(partial1);
    client1.compute_vui().unwrap();
    let token_request1 = client1
        .create_blinded_request(response1.ceremony_id)
        .unwrap();
    service
        .issue_blinded_signature(&response1.ceremony_id, &token_request1.blinded_message)
        .unwrap();

    // Second user (different identity) should succeed
    let id_data_hash2 = [22u8; 32]; // Different identity
    let mut client2 = EnrollmentClient::new(id_data_hash2);
    let request2 = client2.create_request([1u8; 8]);

    let response2 = service.start_ceremony(request2).unwrap();
    let partial2 = service
        .compute_prf_partial(&response2.ceremony_id, &id_data_hash2)
        .unwrap();

    let contribution2 = ShareContribution {
        steward_did: service.steward_did().clone(),
        encrypted_share: vec![2],
        share_commitment: service.share_commitment().unwrap().commitment,
        signature: vec![],
    };
    service
        .add_contribution(&response2.ceremony_id, contribution2)
        .unwrap();

    service
        .complete_vui_computation(&response2.ceremony_id, std::slice::from_ref(&partial2))
        .unwrap();

    // Should succeed - different VUI
    let is_unique2 = service.verify_uniqueness(&response2.ceremony_id).unwrap();
    assert!(is_unique2);

    client2.add_partial(partial2);
    client2.compute_vui().unwrap();
    let token_request2 = client2
        .create_blinded_request(response2.ceremony_id)
        .unwrap();
    service
        .issue_blinded_signature(&response2.ceremony_id, &token_request2.blinded_message)
        .unwrap();

    // Both VUIs registered
    let stats = service.stats();
    assert_eq!(stats.registered_vuis, 2);
}

#[test]
fn test_enrollment_stats_tracking() {
    let did = test_did();
    let mut service = EnrollmentService::new(
        did,
        EnrollmentConfig {
            threshold: 1,
            total_stewards: 1,
            ..Default::default()
        },
    );

    let share = PepperShare::new(1, [42u8; 32]);
    service.set_pepper_share(share);

    // Initial stats
    let stats = service.stats();
    assert_eq!(stats.active_ceremonies, 0);
    assert_eq!(stats.registered_vuis, 0);

    // Start ceremony
    let id_data_hash = [42u8; 32];
    let request = EnrollmentRequest::new(&id_data_hash, [1u8; 8]);
    let _response = service.start_ceremony(&request).unwrap();

    let stats = service.stats();
    assert_eq!(stats.active_ceremonies, 1);
}

#[test]
fn test_vui_commitment_verification() {
    let id_data_hash = [42u8; 32];
    let pathway_hash = [1u8; 8];

    let request1 = EnrollmentRequest::new(&id_data_hash, pathway_hash);
    let request2 = EnrollmentRequest::new(&id_data_hash, pathway_hash);

    // Same id_data_hash produces different commitments due to random salt
    assert_ne!(request1.vui_commitment, request2.vui_commitment);

    // But both verify correctly
    assert!(request1.verify_commitment(&id_data_hash));
    assert!(request2.verify_commitment(&id_data_hash));

    // Wrong data fails
    assert!(!request1.verify_commitment(&[0u8; 32]));
}

#[test]
fn test_prf_partial_consistency() {
    // Same pepper share + same id_data should produce same partial
    let share = PepperShare::new(1, [42u8; 32]);
    let id_data_hash = [99u8; 32];

    let partial1 = PrfPartial::compute(&share, &id_data_hash);
    let partial2 = PrfPartial::compute(&share, &id_data_hash);

    assert_eq!(partial1.value, partial2.value);
    assert_eq!(partial1.index, partial2.index);
}

#[test]
fn test_different_pepper_shares_different_partials() {
    let share1 = PepperShare::new(1, [1u8; 32]);
    let share2 = PepperShare::new(2, [2u8; 32]);
    let id_data_hash = [99u8; 32];

    let partial1 = PrfPartial::compute(&share1, &id_data_hash);
    let partial2 = PrfPartial::compute(&share2, &id_data_hash);

    // Different shares produce different partials
    assert_ne!(partial1.value, partial2.value);
    assert_ne!(partial1.index, partial2.index);
}

#[test]
fn test_enrollment_gossip_message_serialization() {
    let steward_did = test_did();

    // ParticipationRequest
    let msg = EnrollmentMessage::ParticipationRequest {
        ceremony_id: [1u8; 32],
        steward_did: steward_did.clone(),
        vui_commitment: [2u8; 32],
        pathway_hash: [3u8; 8],
        threshold: 3,
        timestamp: 12345,
        signature: vec![4, 5, 6],
    };

    let serialized = serde_json::to_string(&msg).unwrap();
    let deserialized: EnrollmentMessage = serde_json::from_str(&serialized).unwrap();

    if let EnrollmentMessage::ParticipationRequest {
        ceremony_id,
        threshold,
        ..
    } = deserialized
    {
        assert_eq!(ceremony_id, [1u8; 32]);
        assert_eq!(threshold, 3);
    } else {
        panic!("Wrong variant");
    }
}

#[test]
fn test_vui_sync_message_serialization() {
    let did = test_did();

    let msg = VuiSyncMessage::NewVui {
        from_did: did,
        vui_hash: [42u8; 32],
        timestamp: 12345,
        signature: vec![1, 2, 3],
    };

    let wrapped = StewardMessage::VuiSync(msg.clone());
    let serialized = serde_json::to_string(&wrapped).unwrap();
    let deserialized: StewardMessage = serde_json::from_str(&serialized).unwrap();

    if let StewardMessage::VuiSync(VuiSyncMessage::NewVui { vui_hash, .. }) = deserialized {
        assert_eq!(vui_hash, [42u8; 32]);
    } else {
        panic!("Wrong variant");
    }
}

#[test]
fn test_client_blinded_request_creation() {
    let id_data_hash = [42u8; 32];
    let mut client = EnrollmentClient::new(id_data_hash);
    client.create_request([1u8; 8]);

    // Add enough partials
    let share = PepperShare::new(1, [99u8; 32]);
    client.add_partial(PrfPartial::compute(&share, &id_data_hash));

    // Compute VUI
    client.compute_vui().unwrap();

    // Create blinded request
    let ceremony_id = [0u8; 32];
    let request = client.create_blinded_request(ceremony_id).unwrap();

    assert_eq!(request.ceremony_id, ceremony_id);
    assert!(!request.blinded_message.is_empty());
    assert_eq!(request.vui_hash, client.vui().unwrap());
}

#[test]
fn test_enrollment_without_pepper_share_fails() {
    let did = test_did();
    let mut service = EnrollmentService::new(
        did,
        EnrollmentConfig {
            threshold: 1,
            total_stewards: 1,
            ..Default::default()
        },
    );

    // Don't set pepper share

    let id_data_hash = [42u8; 32];
    let request = EnrollmentRequest::new(&id_data_hash, [1u8; 8]);
    let response = service.start_ceremony(&request).unwrap();

    // Should fail - no pepper share
    let result = service.compute_prf_partial(&response.ceremony_id, &id_data_hash);
    assert!(result.is_err());
}

#[test]
fn test_client_without_partials_fails() {
    let id_data_hash = [42u8; 32];
    let mut client = EnrollmentClient::new(id_data_hash);
    client.create_request([1u8; 8]);

    // Don't add any partials
    assert!(!client.has_enough_partials(1));

    // Should fail
    let result = client.compute_vui();
    assert!(result.is_err());
}

#[test]
fn test_share_commitment_verification() {
    let share = PepperShare::new(1, [42u8; 32]);
    let commitment = ShareCommitment::commit(&share);

    // Commitment should be deterministic
    let commitment2 = ShareCommitment::commit(&share);
    assert_eq!(commitment.commitment, commitment2.commitment);

    // Different share has different commitment
    let share2 = PepperShare::new(2, [43u8; 32]);
    let commitment3 = ShareCommitment::commit(&share2);
    assert_ne!(commitment.commitment, commitment3.commitment);
}

#[test]
fn test_vui_registry_basic_operations() {
    let mut registry = VuiRegistry::with_defaults();
    let steward_did = test_did();

    let vui_hash = [42u8; 32];

    // Initially not registered
    let check = registry.check_local(&vui_hash);
    assert!(matches!(
        check,
        icn_steward::vui_registry::LocalCheckResult::NotRegistered
    ));

    // Register
    let result = registry.register(vui_hash, steward_did.clone(), steward_did.clone());
    assert!(result.is_ok());

    // Now registered
    let check = registry.check_local(&vui_hash);
    assert!(matches!(
        check,
        icn_steward::vui_registry::LocalCheckResult::Registered(_)
    ));
}
