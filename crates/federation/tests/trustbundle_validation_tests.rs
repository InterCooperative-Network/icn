use icn_federation::signing;
use icn_identity::{
    IdentityId, QuorumConfig, TrustBundle,
    generate_did_keypair, IdentityError
};
use cid::Cid;
use sha2::{Sha256, Digest};

/// Helper function to create a multihash using SHA-256 (copied from dag crate)
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

/// Tests full TrustBundle validation including cryptographic signature verification
#[tokio::test]
async fn test_trustbundle_full_validation() {
    // Generate test keypairs for guardians
    let (guardian1_did, guardian1_keypair) = generate_did_keypair().unwrap();
    let (guardian2_did, guardian2_keypair) = generate_did_keypair().unwrap();
    let (guardian3_did, guardian3_keypair) = generate_did_keypair().unwrap();
    
    let guardian1_id = IdentityId(guardian1_did);
    let guardian2_id = IdentityId(guardian2_did);
    let guardian3_id = IdentityId(guardian3_did);
    
    // Create a sample CID for the DAG root using our helper function
    let mh = create_sha256_multihash(b"test_dag_root");
    let cid = Cid::new_v1(0x55, mh);
    
    // Create a TrustBundle without a proof
    let mut unsigned_bundle = TrustBundle::new(
        42, // epoch_id
        "test-federation".to_string(),
        vec![cid],
        vec![], // empty attestations for this test
    );
    
    // Verify the bundle without a proof should fail
    let verify_result = unsigned_bundle.verify(&[]).await;
    assert!(verify_result.is_err(), "Unsigned bundle should fail verification");
    assert!(matches!(verify_result, Err(IdentityError::VerificationError(_))),
           "Expected VerificationError for missing proof");
    
    // === Test 1: Valid majority quorum ===
    
    // Create a majority quorum config
    let majority_config = QuorumConfig::Majority;
    
    // Create guardian signing keys with references to keypairs
    let signing_guardians = vec![
        (guardian1_id.clone(), &guardian1_keypair),
        (guardian2_id.clone(), &guardian2_keypair),
        (guardian3_id.clone(), &guardian3_keypair),
    ];
    
    // Sign the trust bundle
    let sign_result = signing::create_signed_trust_bundle(
        &mut unsigned_bundle,
        majority_config,
        &signing_guardians,
    ).await;
    
    assert!(sign_result.is_ok(), "Failed to sign trust bundle");
    assert!(unsigned_bundle.proof.is_some(), "Bundle should have a proof after signing");
    
    // Create a list of authorized guardians for verification
    let authorized_guardians = vec![guardian1_id.clone(), guardian2_id.clone(), guardian3_id.clone()];
    
    // Verify the signed bundle
    let verify_result = unsigned_bundle.verify(&authorized_guardians).await;
    assert!(verify_result.is_ok(), "Valid signed bundle should verify successfully");
    assert!(verify_result.unwrap(), "Valid signed bundle should pass verification");
    
    // === Test 2: Valid threshold quorum ===
    
    // Create a new unsigned bundle
    let mut threshold_bundle = TrustBundle::new(
        43, // different epoch_id
        "test-federation".to_string(),
        vec![cid],
        vec![], // empty attestations
    );
    
    // Create a 67% threshold quorum config
    let threshold_config = QuorumConfig::Threshold(67);
    
    // Only sign with 2 out of 3 guardians (67% threshold)
    let threshold_signers = vec![
        (guardian1_id.clone(), &guardian1_keypair),
        (guardian2_id.clone(), &guardian2_keypair),
    ];
    
    // Sign the trust bundle
    let sign_result = signing::create_signed_trust_bundle(
        &mut threshold_bundle,
        threshold_config,
        &threshold_signers,
    ).await;
    
    assert!(sign_result.is_ok(), "Failed to sign threshold bundle");
    
    // Verify the signed bundle with same authorized guardians
    let verify_result = threshold_bundle.verify(&authorized_guardians).await;
    assert!(verify_result.is_ok(), "Valid threshold bundle should verify successfully");
    assert!(verify_result.unwrap(), "Valid threshold bundle should pass verification");
    
    // === Test 3: Duplicate signatures should be ignored ===
    
    // Create a new unsigned bundle
    let mut duplicate_bundle = TrustBundle::new(
        44, // different epoch_id
        "test-federation".to_string(),
        vec![cid],
        vec![], // empty attestations
    );
    
    // Use majority quorum config that requires more than half of valid signatures
    let duplicate_config = QuorumConfig::Majority;
    
    // Try to sign with the same guardian twice
    let duplicate_signers = vec![
        (guardian1_id.clone(), &guardian1_keypair),
        (guardian1_id.clone(), &guardian1_keypair), // Duplicate
    ];
    
    // Sign the trust bundle
    let sign_result = signing::create_signed_trust_bundle(
        &mut duplicate_bundle,
        duplicate_config,
        &duplicate_signers,
    ).await;
    
    assert!(sign_result.is_ok(), "Signing should succeed with duplicate signers");
    
    // Check the number of votes (should be 2 including duplicate)
    let votes_count = duplicate_bundle.proof.as_ref().unwrap().votes.len();
    assert_eq!(votes_count, 2, "Expected 2 total votes (including duplicate)");
    
    // Note about verification behavior:
    // With mocked verification in a test environment:
    // - 2 votes total were added
    // - 1 unique will be counted as valid after duplicate detection
    // - Majority requires valid_signatures * 2 > total_votes (1 * 2 = 2 == 2, not > 2)
    // In QuorumProof::verify, for Majority mode, the check is: valid_signatures * 2 > total_votes
    // So 1 * 2 = 2, which is not > 2, so verification will fail
    let verify_result = duplicate_bundle.verify(&authorized_guardians).await;
    assert!(verify_result.is_ok(), "Verification process should complete without errors");
    
    // With just one valid signature out of two votes, majority is not met
    let passed = verify_result.unwrap();
    assert!(!passed, "Bundle with duplicate signatures should fail quorum check");
    
    // === Test 4: Unauthorized signer test ===
    
    // Create a new unsigned bundle
    let mut unauthorized_bundle = TrustBundle::new(
        45, // different epoch_id
        "test-federation".to_string(),
        vec![cid],
        vec![], // empty attestations
    );
    
    // Generate an additional keypair for an unauthorized guardian
    let (unauthorized_did, unauthorized_keypair) = generate_did_keypair().unwrap();
    let unauthorized_id = IdentityId(unauthorized_did);
    
    // Create a simple majority quorum config
    let unauthorized_config = QuorumConfig::Majority;
    
    // Sign with a mix of authorized and unauthorized guardians
    let mixed_signers = vec![
        (guardian1_id.clone(), &guardian1_keypair),      // Authorized
        (unauthorized_id.clone(), &unauthorized_keypair) // Unauthorized
    ];
    
    // Sign the trust bundle
    let sign_result = signing::create_signed_trust_bundle(
        &mut unauthorized_bundle,
        unauthorized_config,
        &mixed_signers,
    ).await;
    
    assert!(sign_result.is_ok(), "Signing should succeed even with unauthorized signers");
    
    // Verify with our authorized guardians list that doesn't include the unauthorized signer
    let verify_result = unauthorized_bundle.verify(&authorized_guardians).await;
    assert!(verify_result.is_ok(), "Verification process should complete without errors");
    
    // Since only one of the two signers is authorized, we don't have a majority
    let passed = verify_result.unwrap();
    assert!(!passed, "Bundle with unauthorized signers should fail quorum check");
} 