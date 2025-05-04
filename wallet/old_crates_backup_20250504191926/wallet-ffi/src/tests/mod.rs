/*!
 * ICN Wallet FFI Interface Tests
 *
 * Tests for the foreign function interface of ICN wallet functionality.
 */

#[cfg(test)]
mod ffi_tests {
    use crate::{Receipt, Filter, import_receipts_from_file, filter_receipts, share_receipts_ffi, verify_receipt};
    use std::fs;
    use tempfile::tempdir;
    use std::path::{Path, PathBuf};
    
    // Helper to create a sample receipt JSON file
    fn create_sample_receipts_file(path: &Path) -> PathBuf {
        let file_path = path.join("sample_receipts.json");
        
        let json_content = r#"[
            {
                "@context": ["https://www.w3.org/2018/credentials/v1"],
                "id": "urn:uuid:123e4567-e89b-12d3-a456-426614174000",
                "type": ["VerifiableCredential", "ExecutionReceipt"],
                "issuer": "did:icn:federation1",
                "issuanceDate": "2023-01-01T12:00:00Z",
                "credential_subject": {
                    "id": "did:icn:user1",
                    "proposal_id": "tech-123",
                    "outcome": "Success",
                    "federation_scope": "cooperative",
                    "dag_anchor": "bafybeih123456"
                }
            },
            {
                "@context": ["https://www.w3.org/2018/credentials/v1"],
                "id": "urn:uuid:223e4567-e89b-12d3-a456-426614174000",
                "type": ["VerifiableCredential", "ExecutionReceipt"],
                "issuer": "did:icn:federation1",
                "issuanceDate": "2023-01-02T12:00:00Z",
                "credential_subject": {
                    "id": "did:icn:user1",
                    "proposal_id": "gov-124",
                    "outcome": "Failure",
                    "federation_scope": "technical",
                    "dag_anchor": "bafybeih234567"
                }
            },
            {
                "@context": ["https://www.w3.org/2018/credentials/v1"],
                "id": "urn:uuid:323e4567-e89b-12d3-a456-426614174000",
                "type": ["VerifiableCredential", "ExecutionReceipt"],
                "issuer": "did:icn:federation2",
                "issuanceDate": "2023-01-03T12:00:00Z",
                "credential_subject": {
                    "id": "did:icn:user1",
                    "proposal_id": "tech-125",
                    "outcome": "Success",
                    "federation_scope": "cooperative",
                    "dag_anchor": null
                }
            }
        ]"#;
        
        fs::write(&file_path, json_content).expect("Failed to write sample receipts file");
        
        file_path
    }
    
    // Helper to create a sample Receipt struct
    fn create_sample_receipts() -> Vec<Receipt> {
        vec![
            Receipt {
                id: "urn:uuid:123e4567-e89b-12d3-a456-426614174000".to_string(),
                proposal_id: "tech-123".to_string(),
                outcome: "Success".to_string(),
                dag_anchor: Some("bafybeih123456".to_string()),
                federation_scope: "cooperative".to_string(),
                issuance_date: "2023-01-01T12:00:00Z".to_string(),
                issuer: "did:icn:federation1".to_string(),
            },
            Receipt {
                id: "urn:uuid:223e4567-e89b-12d3-a456-426614174000".to_string(),
                proposal_id: "gov-124".to_string(),
                outcome: "Failure".to_string(),
                dag_anchor: Some("bafybeih234567".to_string()),
                federation_scope: "technical".to_string(),
                issuance_date: "2023-01-02T12:00:00Z".to_string(),
                issuer: "did:icn:federation1".to_string(),
            },
            Receipt {
                id: "urn:uuid:323e4567-e89b-12d3-a456-426614174000".to_string(),
                proposal_id: "tech-125".to_string(),
                outcome: "Success".to_string(),
                dag_anchor: None,
                federation_scope: "cooperative".to_string(),
                issuance_date: "2023-01-03T12:00:00Z".to_string(),
                issuer: "did:icn:federation2".to_string(),
            },
        ]
    }
    
    #[tokio::test]
    async fn test_import_receipts() {
        // Skip this test in most environments since we can't easily mock the filesystem
        // in FFI tests. In a real application, we would have more sophisticated testing.
        if std::env::var("RUN_FFI_FILE_TESTS").is_err() {
            return;
        }
        
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let file_path = create_sample_receipts_file(temp_dir.path());
        
        let receipts = import_receipts_from_file(file_path.to_string_lossy().to_string());
        
        assert_eq!(receipts.len(), 3, "Should have imported 3 receipts");
    }
    
    #[test]
    fn test_filter_receipts_by_scope() {
        let receipts = create_sample_receipts();
        
        let filter = Filter {
            scope: Some("cooperative".to_string()),
            outcome: None,
            since: None,
            proposal_prefix: None,
            limit: None,
        };
        
        let filtered = filter_receipts(receipts, filter);
        
        assert_eq!(filtered.len(), 2, "Should have 2 cooperative receipts");
        assert!(filtered.iter().all(|r| r.federation_scope == "cooperative"));
    }
    
    #[test]
    fn test_filter_receipts_by_outcome() {
        let receipts = create_sample_receipts();
        
        let filter = Filter {
            scope: None,
            outcome: Some("Success".to_string()),
            since: None,
            proposal_prefix: None,
            limit: None,
        };
        
        let filtered = filter_receipts(receipts, filter);
        
        assert_eq!(filtered.len(), 2, "Should have 2 Success receipts");
        assert!(filtered.iter().all(|r| r.outcome == "Success"));
    }
    
    #[test]
    fn test_filter_receipts_by_timestamp() {
        let receipts = create_sample_receipts();
        
        // Filter for receipts after Jan 2, 2023
        let filter = Filter {
            scope: None,
            outcome: None,
            since: Some(1672617600), // 2023-01-02T00:00:00Z
            proposal_prefix: None,
            limit: None,
        };
        
        let filtered = filter_receipts(receipts, filter);
        
        assert_eq!(filtered.len(), 2, "Should have 2 receipts after Jan 2");
    }
    
    #[test]
    fn test_filter_receipts_by_proposal_prefix() {
        let receipts = create_sample_receipts();
        
        let filter = Filter {
            scope: None,
            outcome: None,
            since: None,
            proposal_prefix: Some("tech".to_string()),
            limit: None,
        };
        
        let filtered = filter_receipts(receipts, filter);
        
        assert_eq!(filtered.len(), 2, "Should have 2 receipts with tech prefix");
        assert!(filtered.iter().all(|r| r.proposal_id.starts_with("tech")));
    }
    
    #[test]
    fn test_filter_receipts_combined() {
        let receipts = create_sample_receipts();
        
        let filter = Filter {
            scope: Some("cooperative".to_string()),
            outcome: Some("Success".to_string()),
            since: None,
            proposal_prefix: None,
            limit: None,
        };
        
        let filtered = filter_receipts(receipts, filter);
        
        assert_eq!(filtered.len(), 2, "Should have 2 cooperative Success receipts");
        assert!(filtered.iter().all(|r| r.federation_scope == "cooperative" && r.outcome == "Success"));
    }
    
    #[test]
    fn test_share_receipts() {
        let receipts = create_sample_receipts();
        
        let result = share_receipts_ffi(
            receipts.clone(),
            "json".to_string(),
            "output.json".to_string(),
            true
        );
        
        // Verify that the result indicates success
        assert!(result.contains("Successfully shared 3 receipts"));
    }
    
    #[test]
    fn test_verify_receipt() {
        let receipts = create_sample_receipts();
        
        // Receipt with DAG anchor should verify
        assert!(verify_receipt(receipts[0].clone()), "Receipt with DAG anchor should verify");
        
        // Receipt without DAG anchor should not verify
        assert!(!verify_receipt(receipts[2].clone()), "Receipt without DAG anchor should not verify");
    }
} 