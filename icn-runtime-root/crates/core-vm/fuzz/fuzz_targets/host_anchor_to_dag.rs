#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use icn_core_vm::{
    ConcreteHostEnvironment, ResourceType, ResourceAuthorization, VMContext, IdentityContext
};
use icn_identity::{IdentityId, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

// Define a fuzzable input for DAG anchoring tests
#[derive(Arbitrary, Debug)]
struct DagAnchorInput {
    // Key to anchor (truncate if too long)
    #[arbitrary(with = |u: &mut arbitrary::Unstructured| {
        let mut key = String::new();
        for _ in 0..u.int_in_range(1..=64)? {
            key.push(u.choose(b"abcdefghijklmnopqrstuvwxyz0123456789-_:")? as char);
        }
        Ok(key)
    })]
    key: String,
    
    // Value to anchor (truncate if too long)
    #[arbitrary(with = |u: &mut arbitrary::Unstructured| {
        let len = u.int_in_range(1..=1024)?;
        let mut bytes = vec![0u8; len];
        u.fill_buffer(&mut bytes)?;
        Ok(bytes)
    })]
    value: Vec<u8>,
    
    // Should this operation succeed or fail due to resource limits?
    should_succeed: bool,
}

// Setup a test environment for executing the host function
fn setup_test_env(input: &DagAnchorInput) -> (ConcreteHostEnvironment, Runtime) {
    // Create in-memory storage
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // Create test identity
    let keypair = KeyPair::new(vec![1, 2, 3], vec![4, 5, 6]);
    let identity_id = IdentityId::new("did:icn:fuzz:dag_anchor");
    
    // Create authorizations - ensure resource limits match expected outcome
    let mut authorizations = Vec::new();
    
    // Always authorize some compute
    authorizations.push(ResourceAuthorization {
        resource_type: ResourceType::Compute,
        limit: 10000,
    });
    
    // Add storage authorization based on should_succeed
    let storage_limit = if input.should_succeed {
        // Enough storage for the operation
        (input.key.len() + input.value.len() + 1000) as u64
    } else {
        // Not enough storage
        10 // Very small limit that should fail
    };
    
    authorizations.push(ResourceAuthorization {
        resource_type: ResourceType::Storage,
        limit: storage_limit,
    });
    
    // Create VM context
    let identity_context = Arc::new(IdentityContext::new(
        keypair,
        identity_id.to_string(),
    ));
    
    let vm_context = VMContext::new(
        identity_context.clone(),
        authorizations,
    );
    
    // Create host environment
    let env = ConcreteHostEnvironment::new_with_storage(vm_context, storage);
    
    // Create tokio runtime for async operations
    let rt = Runtime::new().expect("Failed to create tokio runtime");
    
    (env, rt)
}

fuzz_target!(|input: DagAnchorInput| {
    // Set up environment and runtime
    let (env, rt) = setup_test_env(&input);
    
    // Use the runtime to execute the async anchor operation
    rt.block_on(async {
        // Call function being fuzzed
        let result = env.anchor_to_dag(&input.key, input.value.clone()).await;
        
        // Verify behavior matches expected outcome
        if input.should_succeed {
            assert!(result.is_ok(), "Expected success but got error: {:?}", result.err());
            
            // Verify the data was actually stored
            let cid = result.unwrap();
            let stored_data = env.get_node(&input.key, cid.to_bytes()).await;
            assert!(stored_data.is_ok(), "Failed to retrieve anchored data");
            
            if let Ok(Some(data)) = stored_data {
                // Data should be retrievable and match what we stored
                assert!(!data.is_empty(), "Retrieved data is empty");
            } else {
                panic!("Expected to retrieve data but got None");
            }
        } else {
            // Should fail due to resource limits
            assert!(result.is_err(), "Expected failure but got success");
            let err = result.err().unwrap();
            let err_str = format!("{:?}", err);
            // The error should be about resource limits
            assert!(err_str.contains("Resource") || err_str.contains("resource") || 
                   err_str.contains("limit") || err_str.contains("Limit"),
                   "Error doesn't mention resources: {}", err_str);
        }
    });
}); 