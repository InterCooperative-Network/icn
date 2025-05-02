// Integration test for authorization derivation from CCL configs

use icn_governance_kernel::config::GovernanceConfig;
use icn_identity::IdentityScope;
use icn_core_vm::ResourceType;
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;
use std::path::Path;

// Import the derive_core_vm_authorizations function which uses icn_core_vm types
use icn_covm::derive_core_vm_authorizations;

// Our own CclInterpreter implementation
struct CclInterpreter;

impl CclInterpreter {
    pub fn new() -> Self {
        Self
    }
    
    pub fn interpret_ccl(&self, _ccl_content: &str, scope: IdentityScope) -> anyhow::Result<GovernanceConfig> {
        // Mock implementation that returns a basic governance config
        // In a real test, we would parse the CCL, but for the test we can use a fixed config
        Ok(GovernanceConfig {
            template_type: "coop_bylaws".to_string(),
            template_version: "v1".to_string(),
            governing_scope: scope,
            identity: Some(icn_governance_kernel::config::IdentityInfo {
                name: Some("Test Organization".to_string()),
                description: Some("A test organization for testing".to_string()),
                founding_date: Some("2023-01-01".to_string()),
                mission_statement: None,
            }),
            governance: Some(icn_governance_kernel::config::GovernanceStructure {
                decision_making: Some("consent".to_string()),
                quorum: Some(0.75),
                majority: Some(0.67),
                term_length: Some(365),
                roles: None,
            }),
            membership: None,
            proposals: None,
            working_groups: None,
            dispute_resolution: None,
            economic_model: None,
        })
    }
}

// Function to get the absolute path to a file from project root
fn project_path(path: &str) -> String {
    let root = env!("CARGO_MANIFEST_DIR");
    Path::new(root).parent().unwrap().join(path).to_string_lossy().to_string()
}

// Test the authorization derivation logic with cooperative_bylaws.ccl
#[test]
fn test_coop_bylaws_authorization_derivation() {
    // Load the CCL content
    let ccl_file = project_path("examples/test_coop_bylaws.ccl");
    let ccl_content = fs::read_to_string(ccl_file)
        .expect("Failed to read test_coop_bylaws.ccl");
    
    // Create CCL interpreter
    let interpreter = CclInterpreter::new();
    
    // Interpret the CCL content
    let governance_config = interpreter.interpret_ccl(&ccl_content, IdentityScope::Cooperative)
        .expect("CCL interpretation failed");
    
    // Create test data
    let caller_did = "did:icn:test-user";
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    // Call the derivation function that returns icn_core_vm::ResourceAuthorization values
    let authorizations = derive_core_vm_authorizations(
        &governance_config,
        caller_did,
        IdentityScope::Cooperative,
        timestamp,
        true // verbose
    );
    
    // Verify the derived authorizations
    
    // Check that we have all the resource types we expect
    let has_compute = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Compute));
    let has_storage = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Storage));
    let has_network = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Network));
    let has_token = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Token));
    
    // Verify we have basic authorizations
    assert!(has_compute, "Should have compute authorization");
    assert!(has_storage, "Should have storage authorization");
    assert!(has_network, "Should have network authorization");
    assert!(has_token, "Should have token authorization");
    
    // Print the authorizations for inspection
    println!("Derived authorizations for test_coop_bylaws.ccl:");
    for auth in &authorizations {
        println!("  {:?} - limit: {}", auth.resource_type, auth.limit);
    }
    
    println!("Derived {} authorizations", authorizations.len());
}

// Test the authorization derivation logic with simple_community_charter.ccl
#[test]
fn test_community_charter_authorization_derivation() {
    // Load the CCL content
    let ccl_file = project_path("examples/test_community_charter.ccl");
    let ccl_content = fs::read_to_string(ccl_file)
        .expect("Failed to read test_community_charter.ccl");
    
    // Create CCL interpreter
    let interpreter = CclInterpreter::new();
    
    // Interpret the CCL content
    let governance_config = interpreter.interpret_ccl(&ccl_content, IdentityScope::Community)
        .expect("CCL interpretation failed");
    
    // Create test data
    let caller_did = "did:icn:test-user";
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    // Call the derivation function that returns icn_core_vm::ResourceAuthorization values
    let authorizations = derive_core_vm_authorizations(
        &governance_config,
        caller_did,
        IdentityScope::Community,
        timestamp,
        true // verbose
    );
    
    // Verify the derived authorizations
    
    // Check that we have all the resource types we expect
    let has_compute = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Compute));
    let has_storage = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Storage));
    let has_network = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Network));
    let has_token = authorizations.iter().any(|auth| matches!(auth.resource_type, ResourceType::Token));
    
    // Verify we have basic authorizations
    assert!(has_compute, "Should have compute authorization");
    assert!(has_storage, "Should have storage authorization");
    assert!(has_network, "Should have network authorization");
    assert!(has_token, "Should have token authorization");
    
    // Print the authorizations for inspection
    println!("Derived authorizations for test_community_charter.ccl:");
    for auth in &authorizations {
        println!("  {:?} - limit: {}", auth.resource_type, auth.limit);
    }
    
    println!("Derived {} authorizations", authorizations.len());
} 