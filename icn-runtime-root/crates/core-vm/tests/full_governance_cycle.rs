/*!
 * Integration test for a complete governance cycle
 * 
 * This test demonstrates a full cooperative governance cycle:
 * 1. Creating a CCL proposal with economic actions
 * 2. Executing the proposal with token metering
 * 3. Verifying the DAG anchors and issued credentials
 */

use icn_core_vm::{
    execute_wasm, VmError, VmExecutionResult, VMContext, ResourceType, ResourceAuthorization,
    ConcreteHostEnvironment, IdentityScope, VerifiableCredential, ExecutionReceiptSubject
};
use icn_ccl_compiler::{CclCompiler, CompilationOptions};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use libipld::Cid;

mod common {
    // Mock modules
    pub mod storage {
        use std::collections::HashMap;
        use std::sync::{Arc, RwLock};
        use async_trait::async_trait;
        use icn_storage::{StorageManager, StorageError};
        
        /// Mock storage manager for testing
        #[derive(Default, Clone)]
        pub struct MockStorageManager {
            dag_content: Arc<RwLock<HashMap<String, Vec<u8>>>>,
        }
        
        impl MockStorageManager {
            pub fn new() -> Self {
                Self {
                    dag_content: Arc::new(RwLock::new(HashMap::new())),
                }
            }
            
            /// Get the DAG content 
            pub fn get_dag_content(&self) -> HashMap<String, Vec<u8>> {
                self.dag_content.read().unwrap().clone()
            }
        }
        
        #[async_trait]
        impl StorageManager for MockStorageManager {
            async fn store_dag_node(&self, key: &str, data: Vec<u8>) -> Result<String, StorageError> {
                let cid = format!("bafybei{}", hex::encode(&data[0..16]));
                self.dag_content.write().unwrap().insert(key.to_string(), data);
                Ok(cid)
            }
            
            async fn get_dag_node(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
                let content = self.dag_content.read().unwrap();
                Ok(content.get(key).cloned())
            }
        }
    }
    
    pub mod identity {
        use std::collections::HashMap;
        use std::sync::{Arc, RwLock};
        use async_trait::async_trait;
        use icn_identity::{IdentityManager, IdentityError};
        
        /// Mock identity manager for testing
        #[derive(Default, Clone)]
        pub struct MockIdentityManager {
            identities: Arc<RwLock<HashMap<String, String>>>,
        }
        
        impl MockIdentityManager {
            pub fn new() -> Self {
                Self {
                    identities: Arc::new(RwLock::new(HashMap::new())),
                }
            }
            
            /// Register a new identity
            pub fn register_identity(&self, did: &str, name: &str) {
                self.identities.write().unwrap().insert(did.to_string(), name.to_string());
            }
        }
        
        #[async_trait]
        impl IdentityManager for MockIdentityManager {
            async fn resolve_did(&self, did: &str) -> Result<Option<String>, IdentityError> {
                let identities = self.identities.read().unwrap();
                Ok(identities.get(did).cloned())
            }
            
            async fn verify_identity(&self, did: &str) -> Result<bool, IdentityError> {
                let identities = self.identities.read().unwrap();
                Ok(identities.contains_key(did))
            }
        }
    }
}

use common::storage::MockStorageManager;
use common::identity::MockIdentityManager;

/// Create a test cooperative proposal
fn create_test_cooperative_proposal() -> JsonValue {
    json!({
        "type": "cooperative_proposal",
        "id": "proposal-123",
        "title": "Resource allocation and transfer",
        "description": "Allocate energy tokens and transfer to a member",
        "author": "did:icn:coop:treasury",
        "actions": [
            {
                "type": "perform_metered_action",
                "resource_type": "energy",
                "amount": 10
            },
            {
                "type": "transfer_resource",
                "from": "did:icn:coop:treasury",
                "to": "did:icn:member:alice",
                "amount": 5
            },
            {
                "type": "anchor_data",
                "key": "status",
                "value": "50% complete"
            }
        ]
    })
}

/// Create a VM context with appropriate scopes
fn create_cooperative_vm_context() -> VMContext {
    let mut context = VMContext::default();
    
    // Set up identity scopes
    context.identity_scopes.insert("did:icn:coop:treasury".to_string(), IdentityScope::Cooperative);
    context.identity_scopes.insert("did:icn:member:alice".to_string(), IdentityScope::Member);
    
    // Set up resource authorizations
    let mut auth = ResourceAuthorization::default();
    auth.set_energy(100); // Allow up to 100 energy tokens
    context.resource_authorizations.insert("energy".to_string(), auth);
    
    context
}

/// Set up a host environment for testing
fn setup_test_environment() -> ConcreteHostEnvironment {
    let storage_manager = Arc::new(MockStorageManager::new());
    let identity_manager = Arc::new(MockIdentityManager::new());
    
    // Register identities
    identity_manager.register_identity("did:icn:coop:treasury", "Cooperative Treasury");
    identity_manager.register_identity("did:icn:member:alice", "Alice (Member)");
    
    let vm_context = create_cooperative_vm_context();
    
    ConcreteHostEnvironment::new(
        vm_context,
        storage_manager,
        identity_manager,
        Some("did:icn:federation:test".to_string()),
    )
}

#[tokio::test]
async fn test_full_governance_cycle() {
    // === 1. SETUP ===
    let compiler = CclCompiler::new();
    let proposal = create_test_cooperative_proposal();
    let host_env = setup_test_environment();
    
    // === 2. COMPILE PROPOSAL ===
    let compilation_result = compiler.compile_proposal(&proposal, CompilationOptions::default())
        .expect("Failed to compile CCL proposal");
    
    let wasm_bytes = compilation_result.wasm_module;
    println!("Successfully compiled proposal to WASM ({} bytes)", wasm_bytes.len());
    
    // === 3. EXECUTE PROPOSAL ===
    let result = execute_wasm(
        &wasm_bytes,
        Some(create_cooperative_vm_context()),
        &host_env,
        Some("proposal-123"),
        Some("did:icn:federation:test"),
    ).await.expect("Failed to execute WASM");
    
    // === 4. VERIFY EXECUTION RESULT ===
    assert_eq!(result.code, 0, "Execution should succeed with code 0");
    
    // Check resource usage
    let energy_usage = result.resource_usage.get(&ResourceType::Compute).cloned().unwrap_or(0);
    println!("Energy usage: {}", energy_usage);
    assert!(energy_usage > 0, "Should have consumed some energy");
    
    // === 5. VERIFY DAG ANCHORS ===
    let storage_manager = host_env.storage_manager().unwrap();
    let mock_storage = storage_manager.downcast_ref::<MockStorageManager>().unwrap();
    let dag_content = mock_storage.get_dag_content();
    
    // Verify data anchor
    let status_data = dag_content.get("status").cloned();
    assert!(status_data.is_some(), "Should have anchored status data to DAG");
    let status_value = String::from_utf8(status_data.unwrap()).unwrap();
    assert_eq!(status_value, "50% complete", "Status value should match");
    
    // Verify credential anchor
    let credential_key = format!("credential:execution_receipt:proposal-123");
    let credential_data = dag_content.get(&credential_key).cloned();
    assert!(credential_data.is_some(), "Should have anchored execution receipt credential");
    
    // === 6. VERIFY CREDENTIAL ===
    let credential_json = String::from_utf8(credential_data.unwrap()).unwrap();
    let credential: VerifiableCredential<ExecutionReceiptSubject> = 
        serde_json::from_str(&credential_json).expect("Failed to parse credential JSON");
    
    assert_eq!(credential.credential_subject.proposal_id, "proposal-123", 
        "Credential should reference the correct proposal");
    assert_eq!(credential.credential_subject.outcome, "Success", 
        "Execution outcome should be Success");
    
    // Check resource usage in credential
    let energy_value = credential.credential_subject.resource_usage.get("Compute").cloned();
    assert!(energy_value.is_some(), "Credential should track compute resource usage");
    
    println!("✅ Full governance cycle test passed successfully");
} 