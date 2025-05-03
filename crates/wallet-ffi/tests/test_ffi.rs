#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use wallet_ffi::{
        WalletApi, WalletConfig, DataValidator, WalletError,
        IdentityInfo, IdentityDetails, ActionStatus
    };
    use wallet_agent::governance::TrustBundle;
    use wallet_core::dag::DagNode;
    use std::time::SystemTime;

    #[test]
    fn test_create_wallet() {
        // Test creating a wallet with default config
        let wallet = WalletApi::new(None);
        assert!(wallet.is_ok());
    }

    #[test]
    fn test_custom_wallet_config() {
        // Test creating a wallet with custom config
        let config = WalletConfig {
            storage_path: "./target/tmp-test-wallet".to_string(),
            federation_urls: vec!["http://test-federation".to_string()],
            sync_interval_seconds: 60,
            auto_sync_on_startup: false,
        };
        
        let wallet = WalletApi::new(Some(config));
        assert!(wallet.is_ok());
    }

    #[test]
    fn test_identity_management() {
        // Create wallet
        let wallet = WalletApi::new(None).unwrap();
        
        // Test identity creation with metadata
        let mut metadata = HashMap::new();
        metadata.insert("displayName".to_string(), "Test User".to_string());
        metadata.insert("organization".to_string(), "Test Org".to_string());
        
        let id = wallet.create_identity("user".to_string(), metadata);
        assert!(id.is_ok());
        
        // Test listing identities
        let identities = wallet.list_identities();
        assert!(identities.is_ok());
        
        let identities = identities.unwrap();
        assert!(!identities.is_empty());
        
        // At least one identity should be present (we just created one)
        let created_id = id.unwrap();
        assert!(identities.iter().any(|i| i.id == created_id));
        
        // Test getting identity details
        let details = wallet.get_identity(created_id.clone());
        assert!(details.is_ok());
        
        let details = details.unwrap();
        assert_eq!(details.id, created_id);
        assert_eq!(details.display_name, "Test User");
    }

    #[test]
    fn test_action_management() {
        // Create wallet
        let wallet = WalletApi::new(None).unwrap();
        
        // Create an identity to use as creator
        let mut metadata = HashMap::new();
        metadata.insert("displayName".to_string(), "Action Test User".to_string());
        let creator_id = wallet.create_identity("action_test".to_string(), metadata).unwrap();
        
        // Test queueing an action
        let mut action_payload = HashMap::new();
        action_payload.insert("target".to_string(), "test_target".to_string());
        action_payload.insert("operation".to_string(), "test_operation".to_string());
        
        let action_id = wallet.queue_action(
            creator_id.clone(),
            "test_action".to_string(),
            action_payload
        );
        
        assert!(action_id.is_ok());
        let action_id = action_id.unwrap();
        
        // Test getting action status
        let status = wallet.get_action_status(action_id.clone());
        assert!(status.is_ok());
        
        let status = status.unwrap();
        assert_eq!(status.id, action_id);
        assert_eq!(status.creator_id, creator_id);
        assert_eq!(status.action_type, "test_action");
        assert_eq!(status.status, ActionStatus::Pending);
        
        // Test listing actions
        let actions = wallet.list_actions(None);
        assert!(actions.is_ok());
        assert!(!actions.unwrap().is_empty());
    }

    #[test]
    fn test_trust_bundle_validation() {
        use wallet_core::identity::IdentityWallet;
        
        // Create an identity for testing
        let identity = IdentityWallet::new("test", Some("Test Validator")).unwrap();
        
        // Create validator
        let validator = DataValidator::new(identity);
        
        // Valid trust bundle
        let valid_bundle = TrustBundle {
            id: "test-bundle-1".to_string(),
            epoch: 1,
            threshold: 2,
            guardians: vec![
                "did:icn:guardian1".to_string(),
                "did:icn:guardian2".to_string(),
                "did:icn:guardian3".to_string(),
            ],
            active: true,
            created_at: SystemTime::now(),
            expires_at: None,
            links: HashMap::new(),
            signatures: HashMap::new(),
            metadata: HashMap::new(),
        };
        
        // Test valid bundle
        let result = validator.validate_trust_bundle(&valid_bundle);
        assert!(result.is_ok());
        
        // Invalid bundle - future timestamp
        let mut future_bundle = valid_bundle.clone();
        future_bundle.created_at = SystemTime::now() + std::time::Duration::from_secs(3600); // 1 hour in future
        let result = validator.validate_trust_bundle(&future_bundle);
        assert!(result.is_err());
        
        // Invalid bundle - bad threshold
        let mut bad_threshold_bundle = valid_bundle.clone();
        bad_threshold_bundle.threshold = 5; // More than # of guardians
        let result = validator.validate_trust_bundle(&bad_threshold_bundle);
        assert!(result.is_err());
        
        // Invalid bundle - expired
        let mut expired_bundle = valid_bundle.clone();
        expired_bundle.expires_at = Some(SystemTime::now() - std::time::Duration::from_secs(3600)); // 1 hour ago
        let result = validator.validate_trust_bundle(&expired_bundle);
        assert!(result.is_err());
    }

    #[test]
    fn test_dag_node_validation() {
        use wallet_core::identity::IdentityWallet;
        
        // Create an identity for testing
        let identity = IdentityWallet::new("test", Some("Test Validator")).unwrap();
        
        // Create validator
        let validator = DataValidator::new(identity);
        
        // Valid DAG node
        let valid_node = DagNode {
            cid: "bafyreihpcgxa6wjz2cl3mfpxssjcm54chzoj66xtnxekyxuio5h5tsuxsy".to_string(),
            parents: vec!["bafyreia4k7k7qpx52pe6je6zymkmufetmdllycnvhg2bopjadhdvw2a3m4".to_string()],
            epoch: 1,
            creator: "did:icn:creator1".to_string(),
            timestamp: SystemTime::now(),
            content_type: "test".to_string(),
            content: serde_json::json!({"test": "data"}),
            signatures: vec!["signature1".to_string()],
        };
        
        // Test valid node
        let result = validator.validate_dag_node(&valid_node, None);
        assert!(result.is_ok());
        
        // Test valid node with expected CID
        let result = validator.validate_dag_node(&valid_node, 
            Some("bafyreihpcgxa6wjz2cl3mfpxssjcm54chzoj66xtnxekyxuio5h5tsuxsy"));
        assert!(result.is_ok());
        
        // Test CID mismatch
        let result = validator.validate_dag_node(&valid_node, Some("wrong-cid"));
        assert!(result.is_err());
        
        // Invalid node - missing CID
        let mut no_cid_node = valid_node.clone();
        no_cid_node.cid = "".to_string();
        let result = validator.validate_dag_node(&no_cid_node, None);
        assert!(result.is_err());
        
        // Invalid node - future timestamp
        let mut future_node = valid_node.clone();
        future_node.timestamp = SystemTime::now() + std::time::Duration::from_secs(3600); // 1 hour in future
        let result = validator.validate_dag_node(&future_node, None);
        assert!(result.is_err());
        
        // Invalid node - empty parent CID
        let mut bad_parent_node = valid_node.clone();
        bad_parent_node.parents = vec!["".to_string()];
        let result = validator.validate_dag_node(&bad_parent_node, None);
        assert!(result.is_err());
        
        // Invalid node - no signatures
        let mut no_sig_node = valid_node.clone();
        no_sig_node.signatures = vec![];
        let result = validator.validate_dag_node(&no_sig_node, None);
        assert!(result.is_err());
        
        // Invalid node - no creator
        let mut no_creator_node = valid_node.clone();
        no_creator_node.creator = "".to_string();
        let result = validator.validate_dag_node(&no_creator_node, None);
        assert!(result.is_err());
    }
} 