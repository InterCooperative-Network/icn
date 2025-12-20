//! Integration tests for the recovery flow
//!
//! Tests multi-steward recovery scenarios including:
//! - Evidence verification and attestation collection
//! - Multi-steward consensus for recovery
//! - Revocation record tracking
//! - Double-recovery prevention
//! - Gossip message coordination

use icn_identity::KeyPair;
use icn_steward::ceremony::recovery::RecoveryAttestation;
use icn_steward::gossip::{RecoveryMessage, StewardMessage};
use icn_steward::{
    RecoveryClient, RecoveryConfig, RecoveryEvidence, RecoveryRequest, RecoveryService,
};

/// Create a test DID
fn test_did() -> icn_identity::Did {
    let keypair = KeyPair::generate().unwrap();
    keypair.did().clone()
}

#[test]
fn test_single_steward_recovery() {
    // Setup single steward
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did.clone(),
        RecoveryConfig {
            threshold: 1,
            total_stewards: 1,
            ..Default::default()
        },
    );

    // Client initiates recovery
    let old_did = test_did();
    let new_did = test_did();
    let mut client = RecoveryClient::new(old_did.clone(), new_did.clone());

    // Generate evidence
    let vui = [42u8; 32];
    let anchor_commitment = [1u8; 32];
    client.generate_evidence(&vui, anchor_commitment);

    // Create and submit request
    let request = client.create_request().unwrap();
    let response = service.start_ceremony(request).unwrap();
    client.set_ceremony_id(response.ceremony_id);

    assert_eq!(response.threshold, 1);

    // Add attestation
    let attestation = RecoveryAttestation::support(steward_did.clone(), vec![1, 2, 3]);
    service
        .add_attestation(&response.ceremony_id, attestation)
        .unwrap();

    // Verify and complete
    assert!(service.can_proceed(&response.ceremony_id));
    service.verify_and_bind(&response.ceremony_id).unwrap();
    let result = service.complete_recovery(&response.ceremony_id).unwrap();

    client.set_result(result.clone());

    // Verify completion
    assert!(client.is_complete());
    assert_eq!(result.old_did, old_did);
    assert_eq!(result.new_did, new_did);
    assert!(service.is_revoked(&old_did));
    assert_eq!(service.get_replacement_did(&old_did), Some(&new_did));
}

#[test]
fn test_three_of_five_threshold_recovery() {
    // Setup with 3-of-5 threshold
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 3,
            total_stewards: 5,
            ..Default::default()
        },
    );

    // Client initiates recovery
    let old_did = test_did();
    let new_did = test_did();
    let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence);
    let response = service.start_ceremony(request).unwrap();

    // Add attestations from 3 stewards
    for i in 0..3 {
        let steward = test_did();
        let attestation = RecoveryAttestation::support(steward, vec![i as u8]);
        service
            .add_attestation(&response.ceremony_id, attestation)
            .unwrap();
    }

    // Should have enough support
    assert!(service.can_proceed(&response.ceremony_id));

    // Complete recovery
    service.verify_and_bind(&response.ceremony_id).unwrap();
    let result = service.complete_recovery(&response.ceremony_id).unwrap();

    assert_eq!(result.attesters.len(), 3);
    assert!(service.is_revoked(&old_did));
}

#[test]
fn test_recovery_requires_threshold() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 3,
            total_stewards: 5,
            ..Default::default()
        },
    );

    let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request = RecoveryRequest::new(test_did(), test_did(), evidence);
    let response = service.start_ceremony(request).unwrap();

    // Add only 2 attestations (below threshold)
    for i in 0..2 {
        let attestation = RecoveryAttestation::support(test_did(), vec![i as u8]);
        service
            .add_attestation(&response.ceremony_id, attestation)
            .unwrap();
    }

    // Should NOT have enough support
    assert!(!service.can_proceed(&response.ceremony_id));

    // Attempting to proceed should fail
    let result = service.verify_and_bind(&response.ceremony_id);
    assert!(result.is_err());
}

#[test]
fn test_mixed_attestations() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 3,
            total_stewards: 5,
            ..Default::default()
        },
    );

    let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request = RecoveryRequest::new(test_did(), test_did(), evidence);
    let response = service.start_ceremony(request).unwrap();

    // Add 2 supporting, 1 rejecting, 1 supporting
    service
        .add_attestation(
            &response.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![1]),
        )
        .unwrap();
    service
        .add_attestation(
            &response.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![2]),
        )
        .unwrap();
    service
        .add_attestation(
            &response.ceremony_id,
            RecoveryAttestation::reject(test_did(), "Suspicious".to_string(), vec![3]),
        )
        .unwrap();
    service
        .add_attestation(
            &response.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![4]),
        )
        .unwrap();

    // 3 supporting attestations - should be able to proceed
    assert!(service.can_proceed(&response.ceremony_id));
}

#[test]
fn test_double_recovery_prevention() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 1,
            ..Default::default()
        },
    );

    let old_did = test_did();
    let new_did = test_did();

    // First recovery succeeds
    let evidence1 = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request1 = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence1);
    let response1 = service.start_ceremony(request1).unwrap();

    service
        .add_attestation(
            &response1.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![]),
        )
        .unwrap();
    service.verify_and_bind(&response1.ceremony_id).unwrap();
    service.complete_recovery(&response1.ceremony_id).unwrap();

    assert!(service.is_revoked(&old_did));

    // Second recovery attempt for same DID should fail
    let evidence2 = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request2 = RecoveryRequest::new(old_did.clone(), test_did(), evidence2);
    let result = service.start_ceremony(request2);

    assert!(result.is_err());
}

#[test]
fn test_recovery_evidence_verification() {
    let vui = [42u8; 32];
    let anchor_commitment = [1u8; 32];

    let evidence = RecoveryEvidence::new(&vui, anchor_commitment);

    // Should verify with correct anchor
    assert!(evidence.verify(&anchor_commitment));

    // Should fail with wrong anchor
    assert!(!evidence.verify(&[2u8; 32]));

    // Evidence hash should be deterministic
    let evidence2 = RecoveryEvidence::new(&vui, anchor_commitment);
    assert_eq!(evidence.vui_hash, evidence2.vui_hash);
}

#[test]
fn test_revocation_chain() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 1,
            ..Default::default()
        },
    );

    let original_did = test_did();
    let new_did_1 = test_did();
    let new_did_2 = test_did();

    // First recovery: original -> new_did_1
    let evidence1 = RecoveryEvidence::new(&[1u8; 32], [1u8; 32]);
    let request1 = RecoveryRequest::new(original_did.clone(), new_did_1.clone(), evidence1);
    let response1 = service.start_ceremony(request1).unwrap();
    service
        .add_attestation(
            &response1.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![]),
        )
        .unwrap();
    service.verify_and_bind(&response1.ceremony_id).unwrap();
    service.complete_recovery(&response1.ceremony_id).unwrap();

    // Second recovery: new_did_1 -> new_did_2
    let evidence2 = RecoveryEvidence::new(&[2u8; 32], [2u8; 32]);
    let request2 = RecoveryRequest::new(new_did_1.clone(), new_did_2.clone(), evidence2);
    let response2 = service.start_ceremony(request2).unwrap();
    service
        .add_attestation(
            &response2.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![]),
        )
        .unwrap();
    service.verify_and_bind(&response2.ceremony_id).unwrap();
    service.complete_recovery(&response2.ceremony_id).unwrap();

    // Both old DIDs should be revoked
    assert!(service.is_revoked(&original_did));
    assert!(service.is_revoked(&new_did_1));
    assert!(!service.is_revoked(&new_did_2)); // Current active DID

    // Revocation chain should be trackable
    assert_eq!(service.get_replacement_did(&original_did), Some(&new_did_1));
    assert_eq!(service.get_replacement_did(&new_did_1), Some(&new_did_2));
}

#[test]
fn test_recovery_stats_tracking() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 1,
            ..Default::default()
        },
    );

    // Initial stats
    let stats = service.stats();
    assert_eq!(stats.active_ceremonies, 0);
    assert_eq!(stats.revoked_dids, 0);

    // Start ceremony
    let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request = RecoveryRequest::new(test_did(), test_did(), evidence);
    let response = service.start_ceremony(request).unwrap();

    let stats = service.stats();
    assert_eq!(stats.active_ceremonies, 1);

    // Complete it
    service
        .add_attestation(
            &response.ceremony_id,
            RecoveryAttestation::support(test_did(), vec![]),
        )
        .unwrap();
    service.verify_and_bind(&response.ceremony_id).unwrap();
    service.complete_recovery(&response.ceremony_id).unwrap();

    let stats = service.stats();
    assert_eq!(stats.active_ceremonies, 0);
    assert_eq!(stats.successful_recoveries, 1);
    assert_eq!(stats.revoked_dids, 1);
}

#[test]
fn test_duplicate_attestation_rejected() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 2,
            ..Default::default()
        },
    );

    let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request = RecoveryRequest::new(test_did(), test_did(), evidence);
    let response = service.start_ceremony(request).unwrap();

    // Same steward tries to attest twice
    let attesting_steward = test_did();
    let attestation1 = RecoveryAttestation::support(attesting_steward.clone(), vec![1]);
    let attestation2 = RecoveryAttestation::support(attesting_steward, vec![2]);

    service
        .add_attestation(&response.ceremony_id, attestation1)
        .unwrap();
    let result = service.add_attestation(&response.ceremony_id, attestation2);

    assert!(result.is_err());
}

#[test]
fn test_recovery_client_workflow() {
    let old_did = test_did();
    let new_did = test_did();
    let mut client = RecoveryClient::new(old_did.clone(), new_did.clone());

    // Generate evidence
    let vui = [42u8; 32];
    let anchor_commitment = [1u8; 32];
    let evidence = client.generate_evidence(&vui, anchor_commitment);

    assert!(evidence.verify(&anchor_commitment));

    // Create request
    let request = client.create_request().unwrap();
    assert_eq!(request.old_did, old_did);
    assert_eq!(request.new_did, new_did);

    // Check state
    assert!(!client.is_complete());
    assert!(client.result().is_none());
}

#[test]
fn test_recovery_gossip_message_serialization() {
    let old_did = test_did();
    let new_did = test_did();
    let initiator_did = test_did();

    // RecoveryRequest
    let msg = RecoveryMessage::RecoveryRequest {
        ceremony_id: [1u8; 32],
        old_did: old_did.clone(),
        new_did: new_did.clone(),
        initiator_did: initiator_did.clone(),
        evidence_hash: [2u8; 32],
        timestamp: 12345,
        signature: Vec::new(),
    };

    let serialized = serde_json::to_string(&msg).unwrap();
    let deserialized: RecoveryMessage = serde_json::from_str(&serialized).unwrap();

    if let RecoveryMessage::RecoveryRequest {
        ceremony_id,
        evidence_hash,
        ..
    } = deserialized
    {
        assert_eq!(ceremony_id, [1u8; 32]);
        assert_eq!(evidence_hash, [2u8; 32]);
    } else {
        panic!("Wrong variant");
    }
}

#[test]
fn test_recovery_complete_message() {
    let old_did = test_did();
    let new_did = test_did();
    let attester1 = test_did();
    let attester2 = test_did();

    let msg = RecoveryMessage::RecoveryComplete {
        ceremony_id: [1u8; 32],
        old_did: old_did.clone(),
        new_did: new_did.clone(),
        attesters: vec![attester1.clone(), attester2.clone()],
        timestamp: 12345,
        signature: vec![1, 2, 3, 4],
    };

    let wrapped = StewardMessage::Recovery(msg);
    let serialized = serde_json::to_string(&wrapped).unwrap();
    let deserialized: StewardMessage = serde_json::from_str(&serialized).unwrap();

    if let StewardMessage::Recovery(RecoveryMessage::RecoveryComplete { attesters, .. }) =
        deserialized
    {
        assert_eq!(attesters.len(), 2);
    } else {
        panic!("Wrong variant");
    }
}

#[test]
fn test_recovery_attestation_message() {
    let steward_did = test_did();

    // Supporting attestation
    let msg1 = RecoveryMessage::RecoveryAttestation {
        ceremony_id: [1u8; 32],
        steward_did: steward_did.clone(),
        supports: true,
        reason: None,
        timestamp: 12345,
        signature: vec![1, 2, 3],
    };

    let serialized1 = serde_json::to_string(&msg1).unwrap();
    let deserialized1: RecoveryMessage = serde_json::from_str(&serialized1).unwrap();

    if let RecoveryMessage::RecoveryAttestation {
        supports, reason, ..
    } = deserialized1
    {
        assert!(supports);
        assert!(reason.is_none());
    } else {
        panic!("Wrong variant");
    }

    // Rejecting attestation
    let msg2 = RecoveryMessage::RecoveryAttestation {
        ceremony_id: [1u8; 32],
        steward_did,
        supports: false,
        reason: Some("Evidence not convincing".to_string()),
        timestamp: 12345,
        signature: vec![4, 5, 6],
    };

    let serialized2 = serde_json::to_string(&msg2).unwrap();
    let deserialized2: RecoveryMessage = serde_json::from_str(&serialized2).unwrap();

    if let RecoveryMessage::RecoveryAttestation {
        supports, reason, ..
    } = deserialized2
    {
        assert!(!supports);
        assert_eq!(reason, Some("Evidence not convincing".to_string()));
    } else {
        panic!("Wrong variant");
    }
}

#[test]
fn test_concurrent_recovery_limit() {
    let steward_did = test_did();
    let mut service = RecoveryService::new(
        steward_did,
        RecoveryConfig {
            threshold: 1,
            max_concurrent_recoveries: 2, // Very low limit for testing
            ..Default::default()
        },
    );

    // Start 2 ceremonies (at limit)
    for _ in 0..2 {
        let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request = RecoveryRequest::new(test_did(), test_did(), evidence);
        service.start_ceremony(request).unwrap();
    }

    // Third should fail
    let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
    let request = RecoveryRequest::new(test_did(), test_did(), evidence);
    let result = service.start_ceremony(request);

    assert!(result.is_err());
}
