#[cfg(test)]
mod trust_bundle_tests {
    use super::*;
    use crate::{TrustBundle, IdentityId, QuorumProof, VerifiableCredential};
    use std::time::{SystemTime, Duration};
    use async_trait::async_trait;
    use sha2::{Sha256, Digest};
    
    // Mock DAG store for testing
    struct MockDagStore {
        contains_results: std::collections::HashMap<cid::Cid, bool>,
    }
    
    #[async_trait]
    impl DagStore for MockDagStore {
        async fn contains(&self, cid: &cid::Cid) -> Result<bool, String> {
            Ok(self.contains_results.get(cid).copied().unwrap_or(false))
        }
        
        async fn get(&self, _cid: &cid::Cid) -> Result<Option<Vec<u8>>, String> {
            Ok(None) // Not needed for these tests
        }
        
        async fn put(&self, _data: &[u8]) -> Result<cid::Cid, String> {
            Err("Not implemented".to_string()) // Not needed for these tests
        }
    }
    
    // Create a test CID
    fn create_test_cid(data: &[u8]) -> cid::Cid {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        
        let mh = multihash::Multihash::wrap(0x12, &hash).unwrap();
        cid::Cid::new_v1(0x55, mh) // Raw format
    }
    
    // Helper to create a basic TrustBundle for testing
    fn create_test_bundle(epoch_id: u64) -> TrustBundle {
        let dag_root = create_test_cid(b"test dag root");
        
        TrustBundle {
            epoch_id,
            federation_id: "test-federation".to_string(),
            dag_roots: vec![dag_root],
            attestations: Vec::new(),
            proof: None,
        }
    }
    
    // Helper to create a mock quorum proof
    fn create_mock_proof(signers: Vec<String>) -> QuorumProof {
        let mut votes = std::collections::HashMap::new();
        for signer in signers {
            votes.insert(signer, vec![0u8; 64]); // Mock signature
        }
        
        QuorumProof {
            signatures: votes,
            threshold: QuorumConfig::Majority,
        }
    }
    
    #[tokio::test]
    async fn test_outdated_epoch_bundle() {
        // Create a bundle with epoch 10
        let mut bundle = create_test_bundle(10);
        
        // Add a valid proof
        let authorized_guardians = vec![
            "did:icn:guardian1".to_string(), 
            "did:icn:guardian2".to_string()
        ];
        bundle.proof = Some(create_mock_proof(authorized_guardians.clone()));
        
        // Verify with current epoch 15 (should fail - outdated)
        let current_epoch = 15;
        let current_time = SystemTime::now();
        let result = bundle.verify(&authorized_guardians, current_epoch, current_time).await;
        
        assert!(result.is_err(), "Outdated bundle should fail verification");
        if let Err(e) = result {
            assert!(e.to_string().contains("older than current epoch"), 
                    "Error should mention outdated epoch");
        }
    }
    
    #[tokio::test]
    async fn test_duplicate_signers() {
        // Create a bundle
        let mut bundle = create_test_bundle(10);
        
        // Add a proof with duplicate signers
        let authorized_guardians = vec![
            "did:icn:guardian1".to_string(), 
            "did:icn:guardian2".to_string()
        ];
        let duplicate_signers = vec![
            "did:icn:guardian1".to_string(), 
            "did:icn:guardian1".to_string() // Duplicate!
        ];
        bundle.proof = Some(create_mock_proof(duplicate_signers));
        
        // Verify (should fail - duplicate signers)
        let current_epoch = 5; // Lower than bundle epoch
        let current_time = SystemTime::now();
        let result = bundle.verify(&authorized_guardians, current_epoch, current_time).await;
        
        assert!(result.is_err(), "Bundle with duplicate signers should fail verification");
        if let Err(e) = result {
            assert!(e.to_string().contains("duplicate signer"), 
                    "Error should mention duplicate signer");
        }
    }
    
    #[tokio::test]
    async fn test_unauthorized_signers() {
        // Create a bundle
        let mut bundle = create_test_bundle(10);
        
        // Add a proof with unauthorized signers
        let authorized_guardians = vec![
            "did:icn:guardian1".to_string(), 
            "did:icn:guardian2".to_string()
        ];
        let unauthorized_signers = vec![
            "did:icn:guardian1".to_string(), 
            "did:icn:unauthorized".to_string() // Not authorized!
        ];
        bundle.proof = Some(create_mock_proof(unauthorized_signers));
        
        // Verify (should fail - unauthorized signer)
        let current_epoch = 5; // Lower than bundle epoch
        let current_time = SystemTime::now();
        let result = bundle.verify(&authorized_guardians, current_epoch, current_time).await;
        
        assert!(result.is_err(), "Bundle with unauthorized signers should fail verification");
        if let Err(e) = result {
            assert!(e.to_string().contains("not an authorized guardian"), 
                    "Error should mention unauthorized guardian");
        }
    }
    
    #[tokio::test]
    async fn test_dag_anchor_verification() {
        // Create a bundle
        let bundle = create_test_bundle(10);
        
        // Create mock DAG store where the DAG root exists
        let mut contains_results = std::collections::HashMap::new();
        contains_results.insert(bundle.dag_roots[0], true);
        let dag_store = MockDagStore { contains_results };
        
        // Verify DAG anchor (should succeed)
        let result = bundle.verify_dag_anchor(&dag_store).await;
        assert!(result.is_ok(), "DAG anchor verification should succeed");
        assert!(result.unwrap(), "DAG anchor verification should return true");
        
        // Now test with a missing DAG root
        let mut contains_results = std::collections::HashMap::new();
        contains_results.insert(bundle.dag_roots[0], false);
        let dag_store = MockDagStore { contains_results };
        
        // Verify DAG anchor (should return false but not error)
        let result = bundle.verify_dag_anchor(&dag_store).await;
        assert!(result.is_ok(), "DAG anchor verification should not error");
        assert!(!result.unwrap(), "DAG anchor verification should return false");
    }
} 