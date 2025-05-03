use anyhow::Result;
use chrono::Utc;
use cid::Cid;
use icn_core_vm::{
    IdentityContext, ResourceAuthorization, ResourceType, VMContext, ExecutionResult,
};
use icn_dag::{DagNode, DagNodeBuilder, DagNodeMetadata, codec::DagCborCodec};
use icn_identity::{
    ConcreteIdentityManager, IdentityId, IdentityManager, IdentityScope, KeyPair, KeyStorage,
};
use icn_storage::{RocksDBStorageManager, StorageManager};
use libipld::Ipld;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

// Import CLI dependencies and functions to test
use icn_covm::{
    handle_execute_command, sign_node_data, create_identity_context, derive_core_vm_authorizations,
};

/// Test data for entity creation
struct TestData {
    parent_did: String,
    entity_type: String,
    genesis_payload: Ipld,
}

/// Test WASM binary that calls host_create_sub_dag
/// This is a pre-compiled WASM module that calls host_create_sub_dag with predefined parameters
/// To simplify the test, we're including it as a byte array instead of creating it dynamically
static TEST_WASM_BINARY: &[u8] = include_bytes!("../test_fixtures/create_entity.wasm");

/// Create a verification function similar to sign_node_data
async fn verify_node_signature(
    identity_manager: &Arc<dyn IdentityManager>,
    signer_did: &str, 
    node_data: &[u8],
    signature: &[u8],
) -> Result<bool> {
    // Retrieve the JWK (public key) for the signer DID
    let jwk_opt = identity_manager.get_key(signer_did).await?;
    
    let jwk = jwk_opt.ok_or_else(|| 
        anyhow::anyhow!("No key found for signer DID: {}", signer_did))?;
    
    // Convert JWK to KeyPair for verification
    let key_pair = icn_identity::KeyPair::try_from_jwk(&jwk)?;
    
    // Verify the signature
    let is_valid = key_pair.verify(node_data, signature)?;
    
    Ok(is_valid)
}

/// Set up the test environment
async fn setup_test_env() -> Result<(Arc<dyn IdentityManager>, Arc<dyn StorageManager>, TempDir, TestData)> {
    // Create temporary directory for storage
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().to_path_buf();
    
    // Initialize StorageManager with the temporary directory
    let storage_manager = Arc::new(RocksDBStorageManager::new(storage_path.clone()).await?);
    
    // Initialize IdentityManager (in-memory for simplicity)
    let identity_manager = Arc::new(ConcreteIdentityManager::new_in_memory());
    
    // Create a parent federation identity with a keypair
    let (parent_did, _) = identity_manager.generate_and_store_did_key().await?;
    
    // Create test data
    let test_data = TestData {
        parent_did,
        entity_type: "Cooperative".to_string(),
        genesis_payload: libipld::ipld!({
            "name": "Test Cooperative",
            "description": "A cooperative created for testing",
            "created_at": Utc::now().timestamp(),
        }),
    };
    
    Ok((identity_manager, storage_manager, temp_dir, test_data))
}

/// Test the entity creation and parent anchoring functionality
#[tokio::test]
async fn test_entity_creation_and_anchoring() -> Result<()> {
    // Set up the test environment
    let (identity_manager, storage_manager, _temp_dir, test_data) = setup_test_env().await?;
    
    // Save WASM binary to a temporary file
    let wasm_file = tempfile::NamedTempFile::new()?;
    std::fs::write(&wasm_file, TEST_WASM_BINARY)?;
    let wasm_path = wasm_file.path().to_str().unwrap().to_string();
    
    // Create a constitution file (simplified for testing)
    let constitution_file = tempfile::NamedTempFile::new()?;
    std::fs::write(&constitution_file, r#"
    governance {
        name: "Test Federation"
        description: "A federation for testing entity creation"
    }
    "#)?;
    let constitution_path = constitution_file.path().to_str().unwrap().to_string();
    
    // Execute the entity creation with CLI handle_execute_command function
    let result = handle_execute_command(
        wasm_path,                    // Proposal payload (WASM file)
        constitution_path,            // Constitution file
        test_data.parent_did.clone(), // Identity (parent federation DID)
        "federation".to_string(),     // Scope
        None,                         // Proposal ID (None for this test)
        true,                         // Verbose for debugging
    ).await;
    
    // Check that execution succeeded
    assert!(result.is_ok(), "Entity creation execution failed: {:?}", result.err());
    
    // Let's simulate direct execution to get the return values, since handle_execute_command
    // just prints the results rather than returning them
    
    // 1. Execute the WASM module directly
    // Create execution context
    let identity_ctx = create_identity_context(&test_data.parent_did);
    let core_vm_authorizations = vec![
        ResourceAuthorization::new(
            ResourceType::Compute, 1_000_000, None, "Test compute".to_string()
        ),
        ResourceAuthorization::new(
            ResourceType::Storage, 5_000_000, None, "Test storage".to_string()
        ),
        ResourceAuthorization::new(
            ResourceType::Network, 1_000, None, "Test network".to_string()
        ),
    ];
    
    let vm_context = VMContext::new(
        identity_ctx,
        core_vm_authorizations,
    );
    
    // Execute the WASM module directly
    let direct_result = icn_core_vm::execute_wasm(
        TEST_WASM_BINARY,
        "main",
        &[],
        vm_context,
        storage_manager.clone(),
        identity_manager.clone(),
        Some(test_data.parent_did.clone()),
    ).await?;
    
    // Check that execution succeeded
    assert!(direct_result.success, "Direct WASM execution failed: {:?}", direct_result.error);
    
    // Extract entity creation details
    assert!(direct_result.created_entity_did.is_some(), "No entity DID was returned");
    assert!(direct_result.created_entity_genesis_cid.is_some(), "No genesis CID was returned");
    
    let entity_did = direct_result.created_entity_did.unwrap();
    let genesis_cid = direct_result.created_entity_genesis_cid.unwrap();
    
    println!("Created entity DID: {}", entity_did);
    println!("Genesis CID: {}", genesis_cid);
    
    // 2. Verify that the genesis node exists
    let genesis_node_bytes = storage_manager.get_node_bytes(&entity_did, &genesis_cid).await?;
    assert!(genesis_node_bytes.is_some(), "Genesis node does not exist");
    
    // 3. Verify that the parent federation has an anchor node
    // Find all nodes in the parent federation's DAG
    let parent_nodes = storage_manager.get_all_nodes(&test_data.parent_did).await?;
    
    // There should be at least one node in the parent federation's DAG (the anchor node)
    assert!(!parent_nodes.is_empty(), "No nodes found in parent federation DAG");
    
    // The latest node should be the anchor node
    let (anchor_cid, anchor_node) = parent_nodes.last().unwrap();
    
    // 4. Verify the anchor node's content
    // Decode the node's payload
    let payload = match &anchor_node.payload {
        Ipld::Map(map) => map,
        _ => panic!("Anchor node payload is not a map"),
    };
    
    // Check that the payload contains the expected fields
    assert!(payload.contains_key("event"), "Anchor node payload missing 'event' field");
    assert!(payload.contains_key("entity_did"), "Anchor node payload missing 'entity_did' field");
    assert!(payload.contains_key("genesis_cid"), "Anchor node payload missing 'genesis_cid' field");
    
    // Check that the values match our expectations
    if let Ipld::String(event) = &payload["event"] {
        assert_eq!(event, "entity_created", "Unexpected event type in anchor node");
    } else {
        panic!("'event' field is not a string");
    }
    
    if let Ipld::String(anchor_entity_did) = &payload["entity_did"] {
        assert_eq!(anchor_entity_did, &entity_did, "Entity DID in anchor doesn't match created entity");
    } else {
        panic!("'entity_did' field is not a string");
    }
    
    if let Ipld::String(anchor_genesis_cid_str) = &payload["genesis_cid"] {
        assert!(anchor_genesis_cid_str.contains(&genesis_cid.to_string()), 
                "Genesis CID in anchor doesn't match created entity");
    } else {
        panic!("'genesis_cid' field is not a string");
    }
    
    // 5. Verify the anchor node's signature
    // Create a partial node without the signature for verification
    let unsigned_node = DagNodeBuilder::new()
        .issuer(anchor_node.issuer.clone())
        .payload(anchor_node.payload.clone())
        .metadata(anchor_node.metadata.clone().unwrap_or_default())
        .parents(anchor_node.parents.clone())
        .build_unsigned();
    
    // Encode the partial node to get bytes for verification
    let node_data_to_verify = DagCborCodec.encode(&unsigned_node)?;
    
    // Verify the signature
    let is_valid = verify_node_signature(
        &identity_manager,
        &test_data.parent_did,
        &node_data_to_verify,
        &anchor_node.signature,
    ).await?;
    
    assert!(is_valid, "Anchor node signature verification failed");
    
    Ok(())
} 