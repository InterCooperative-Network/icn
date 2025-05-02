use icn_core_vm::{
    ConcreteHostEnvironment, HostEnvironment, IdentityContext,
    ResourceType, ResourceAuthorization, ExecutionResult, VMContext
};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use std::sync::Arc;
use std::collections::HashMap;

/// Simple test module with exported functions
const TEST_WASM_BYTES: &[u8] = include_bytes!("fixtures/test_module.wasm");

// Helper function to create test identity context
fn create_test_identity_context() -> Arc<IdentityContext> {
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    // Create identity context with DID
    let identity_context = IdentityContext::new(keypair, "did:icn:test");
    
    Arc::new(identity_context)
}

// Helper function to create resource authorizations
fn create_test_authorizations() -> Vec<ResourceAuthorization> {
    vec![
        ResourceAuthorization::new(
            ResourceType::Compute, 
            1_000_000,  // 1M compute units
            None,
            "Test computation allowance".to_string()
        ),
        ResourceAuthorization::new(
            ResourceType::Storage, 
            50_000,     // 50K storage units
            None,
            "Test storage allowance".to_string()
        ),
        ResourceAuthorization::new(
            ResourceType::Network, 
            10_000,     // 10K network units
            None,
            "Test network allowance".to_string()
        ),
    ]
}

#[test]
fn test_vm_context_initialization() {
    // Create identity context
    let identity_ctx = create_test_identity_context();
    
    // Create resource authorizations
    let authorizations = create_test_authorizations();
    
    // Create VM context
    let vm_context = VMContext::new(
        identity_ctx.clone(),
        authorizations.clone(),
    );
    
    // Verify context is properly initialized
    assert_eq!(vm_context.caller_did(), "did:icn:test");
    assert_eq!(vm_context.resource_authorizations().len(), 3);
    
    // Check resource limits
    let compute_auth = vm_context.resource_authorizations().iter()
        .find(|auth| auth.resource_type == ResourceType::Compute)
        .expect("Compute authorization should exist");
    
    assert_eq!(compute_auth.limit, 1_000_000);
}

#[test]
fn test_host_environment_construction() {
    // Create identity context and VM context
    let identity_ctx = create_test_identity_context();
    let authorizations = create_test_authorizations();
    let vm_context = VMContext::new(identity_ctx.clone(), authorizations);
    
    // Create host environment
    let host_env = ConcreteHostEnvironment::new(vm_context);
    
    // Verify initial resource consumption is zero
    assert_eq!(host_env.get_compute_consumed(), 0);
    assert_eq!(host_env.get_storage_consumed(), 0);
    assert_eq!(host_env.get_network_consumed(), 0);
    
    // Test cloning the host environment
    let cloned_env = host_env.clone();
    assert_eq!(cloned_env.get_compute_consumed(), 0);
}

#[test]
fn test_wasm_execution_and_resource_tracking() {
    // Create identity context and VM context
    let identity_ctx = create_test_identity_context();
    let authorizations = create_test_authorizations();
    let vm_context = VMContext::new(identity_ctx.clone(), authorizations);
    
    // Create host environment with resource tracking
    let mut host_env = ConcreteHostEnvironment::new(vm_context);
    
    // Record some resource usage (simulating what would happen during execution)
    host_env.record_compute_usage(50_000).unwrap(); // 50K compute units
    host_env.record_storage_usage(1_000).unwrap();  // 1K storage units
    
    // Check resource consumption is tracked
    assert_eq!(host_env.get_compute_consumed(), 50_000);
    assert_eq!(host_env.get_storage_consumed(), 1_000);
    
    // Attempt to exceed resource limits
    let result = host_env.record_compute_usage(2_000_000); // 2M compute units (over limit)
    assert!(result.is_err(), "Should reject resource usage exceeding limits");
}

#[test]
#[cfg(feature = "wasmtime")] // Only run if wasmtime feature is enabled
fn test_wasm_module_execution() {
    use icn_core_vm::execute_wasm;
    
    // Create identity context and VM context
    let identity_ctx = create_test_identity_context();
    let authorizations = create_test_authorizations();
    let vm_context = VMContext::new(identity_ctx.clone(), authorizations);
    
    // Define exported function name to call
    let function_name = "test_function";
    
    // Execute the WASM module
    let result = execute_wasm(
        TEST_WASM_BYTES,
        function_name,
        &[],  // No parameters
        vm_context
    );
    
    assert!(result.is_ok(), "WASM execution should succeed");
    
    let exec_result = result.unwrap();
    
    // Verify execution succeeded
    assert!(exec_result.is_success());
    
    // Verify resource consumption is recorded
    assert!(exec_result.resources_consumed.compute > 0, "Should record compute consumption");
}

#[test]
fn test_host_environment_context_access() {
    // Create identity context and VM context
    let identity_ctx = create_test_identity_context();
    let authorizations = create_test_authorizations();
    let vm_context = VMContext::new(identity_ctx.clone(), authorizations);
    
    // Create host environment
    let host_env = ConcreteHostEnvironment::new(vm_context);
    
    // Verify context access
    assert_eq!(host_env.caller_did(), "did:icn:test");
    
    // Test DID scope access
    assert_eq!(host_env.caller_scope(), IdentityScope::Personal);
}

#[test]
fn test_derive_authorizations() {
    use icn_execution_tools::derive_authorizations;
    use icn_governance_kernel::config::ProposalTemplate;
    
    // Create a simple proposal template
    let mut template = ProposalTemplate::new(
        "Test Template".to_string(),
        "Test proposal template".to_string(),
    );
    
    // Set expected resource requirements
    template.set_estimated_compute(500_000);
    template.set_estimated_storage(2_000);
    template.set_estimated_network(1_000);
    
    // Derive authorizations
    let authorizations = derive_authorizations(&template);
    
    // Verify the right authorizations are created
    assert!(authorizations.len() >= 3, "Should have at least 3 resource types");
    
    // Check compute authorization
    let compute_auth = authorizations.iter()
        .find(|auth| auth.resource_type == ResourceType::Compute)
        .expect("Compute authorization should exist");
    
    // The resource amount should be at least what was estimated
    assert!(compute_auth.limit >= 500_000, "Compute limit should be at least estimated value");
    
    // Check storage authorization
    let storage_auth = authorizations.iter()
        .find(|auth| auth.resource_type == ResourceType::Storage)
        .expect("Storage authorization should exist");
    
    assert!(storage_auth.limit >= 2_000, "Storage limit should be at least estimated value");
} 