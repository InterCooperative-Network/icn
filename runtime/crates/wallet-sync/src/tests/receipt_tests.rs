use crate::federation::{
    FederationSyncClient, FederationSyncClientConfig, FederationEndpoint,
    MemoryCredentialStore, VerifiableCredential, ExportFormat, verify_execution_receipt
};
use crate::export::{export_receipts_to_file, import_receipts_from_file};
use chrono::{DateTime, Utc};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use std::path::PathBuf;
use mockito::{mock, server_url};
use tempfile::tempdir;
use serde_json::json;

#[tokio::test]
async fn test_fetch_and_export_receipts() {
    // Create mock server
    let mock_server = mock("GET", "/dag/receipts?scope=cooperative")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[
            {
                "@context": ["https://www.w3.org/2018/credentials/v1"],
                "id": "urn:uuid:123e4567-e89b-12d3-a456-426614174000",
                "type": ["VerifiableCredential", "ExecutionReceipt"],
                "issuer": "did:icn:federation1",
                "issuanceDate": "2023-01-01T12:00:00Z",
                "credential_subject": {
                    "id": "did:icn:user1",
                    "proposal_id": "prop-123",
                    "outcome": "Success",
                    "resource_usage": {"Compute": 1000, "Storage": 500},
                    "dag_anchor": "bafybeih123456",
                    "federation_scope": "cooperative",
                    "execution_timestamp": "2023-01-01T12:00:00Z"
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
                    "proposal_id": "prop-124",
                    "outcome": "Failure",
                    "resource_usage": {"Compute": 2000, "Storage": 1000},
                    "dag_anchor": "bafybeih234567",
                    "federation_scope": "cooperative",
                    "execution_timestamp": "2023-01-02T12:00:00Z"
                }
            }
        ]"#)
        .create();
    
    // Set up mock client
    let store = Arc::new(MemoryCredentialStore::new());
    let mock_url = server_url();
    
    let config = FederationSyncClientConfig {
        endpoints: vec![
            FederationEndpoint {
                federation_id: "federation1".to_string(),
                base_url: mock_url,
                last_sync: None,
                auth_token: None,
            }
        ],
        sync_interval: Some(Duration::from_secs(60)),
        verify_credentials: true,
        notify_on_sync: false,
    };
    
    let client = FederationSyncClient::new(store, config);
    
    // Fetch receipts
    let receipts = client.fetch_execution_receipts("federation1", "cooperative", None).await.unwrap();
    
    // Verify we got the expected number of receipts
    assert_eq!(receipts.len(), 2, "Should have received 2 receipts");
    
    // Check first receipt contents
    assert_eq!(receipts[0].id, "urn:uuid:123e4567-e89b-12d3-a456-426614174000");
    assert_eq!(receipts[0].issuer, "did:icn:federation1");
    
    assert!(verify_execution_receipt(&receipts[0]), "First receipt should verify");
    assert!(verify_execution_receipt(&receipts[1]), "Second receipt should verify");
    
    // Test JSON export
    let temp_dir = tempdir().unwrap();
    let json_path = temp_dir.path().join("receipts.json");
    
    export_receipts_to_file(&receipts, ExportFormat::Json, &json_path).unwrap();
    
    // Verify file exists
    assert!(json_path.exists(), "JSON file should exist");
    
    // Test CSV export
    let csv_path = temp_dir.path().join("receipts.csv");
    export_receipts_to_file(&receipts, ExportFormat::Csv, &csv_path).unwrap();
    
    // Verify file exists
    assert!(csv_path.exists(), "CSV file should exist");
    
    // Test signed bundle export
    let bundle_path = temp_dir.path().join("receipts_bundle.json");
    export_receipts_to_file(&receipts, ExportFormat::SignedBundle, &bundle_path).unwrap();
    
    // Verify file exists
    assert!(bundle_path.exists(), "Bundle file should exist");
    
    // Test import from JSON
    let imported_receipts = import_receipts_from_file(&json_path, true).unwrap();
    assert_eq!(imported_receipts.len(), 2, "Should have imported 2 receipts from JSON");
    
    // Ensure mock server received the expected request
    mock_server.assert();
}

#[tokio::test]
async fn test_fetch_receipts_with_timestamp() {
    // Create mock server for timestamp-filtered request
    let mock_server = mock("GET", "/dag/receipts?scope=cooperative&since=1672574400")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[
            {
                "@context": ["https://www.w3.org/2018/credentials/v1"],
                "id": "urn:uuid:223e4567-e89b-12d3-a456-426614174000",
                "type": ["VerifiableCredential", "ExecutionReceipt"],
                "issuer": "did:icn:federation1",
                "issuanceDate": "2023-01-02T12:00:00Z",
                "credential_subject": {
                    "id": "did:icn:user1",
                    "proposal_id": "prop-124",
                    "outcome": "Success",
                    "resource_usage": {"Compute": 2000, "Storage": 1000},
                    "dag_anchor": "bafybeih234567",
                    "federation_scope": "cooperative",
                    "execution_timestamp": "2023-01-02T12:00:00Z"
                }
            }
        ]"#)
        .create();
    
    // Set up mock client
    let store = Arc::new(MemoryCredentialStore::new());
    let mock_url = server_url();
    
    let config = FederationSyncClientConfig {
        endpoints: vec![
            FederationEndpoint {
                federation_id: "federation1".to_string(),
                base_url: mock_url,
                last_sync: None,
                auth_token: None,
            }
        ],
        sync_interval: Some(Duration::from_secs(60)),
        verify_credentials: true,
        notify_on_sync: false,
    };
    
    let client = FederationSyncClient::new(store, config);
    
    // January 1, 2023 in Unix timestamp
    let timestamp = 1672574400;
    
    // Fetch receipts with timestamp filter
    let receipts = client.fetch_execution_receipts("federation1", "cooperative", Some(timestamp)).await.unwrap();
    
    // Verify we got the expected number of receipts
    assert_eq!(receipts.len(), 1, "Should have received 1 receipt after the timestamp filter");
    
    // Check receipt contents
    assert_eq!(receipts[0].id, "urn:uuid:223e4567-e89b-12d3-a456-426614174000");
    
    // Ensure mock server received the expected request
    mock_server.assert();
} 