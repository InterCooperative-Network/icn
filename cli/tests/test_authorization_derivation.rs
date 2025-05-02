// Integration test for authorization derivation from CCL configs

use icn_governance_kernel::CclInterpreter;
use icn_identity::IdentityScope;
use icn_economics::ResourceType;
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;
use std::path::Path;

// Import the derive_authorizations function 
use icn_covm::derive_authorizations;

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
    
    // Call the derivation function
    let (resource_types, authorizations) = derive_authorizations(
        &governance_config,
        caller_did,
        IdentityScope::Cooperative,
        timestamp,
        true // verbose
    );
    
    // Verify the derived authorizations
    
    // Should have at least basic compute authorization 
    assert!(resource_types.contains(&ResourceType::Compute));
    
    // Should have storage due to economic_model section
    assert!(resource_types.contains(&ResourceType::Storage));
    
    // Should have network bandwidth due to dispute_resolution section
    assert!(resource_types.contains(&ResourceType::NetworkBandwidth));
    
    // Should have custom memory resource due to working_groups section
    assert!(resource_types.iter().any(|rt| matches!(rt, ResourceType::Custom { identifier } if identifier == "Memory")));
    
    // Should have labor hours resources due to compensation_policy
    assert!(resource_types.iter().any(|rt| matches!(rt, ResourceType::LaborHours { skill } if skill == "programming")));
    assert!(resource_types.iter().any(|rt| matches!(rt, ResourceType::LaborHours { skill } if skill == "design")));
    assert!(resource_types.iter().any(|rt| matches!(rt, ResourceType::LaborHours { skill } if skill == "documentation")));
    
    // Should have community credit resource due to working_groups budget
    assert!(resource_types.iter().any(|rt| matches!(rt, ResourceType::CommunityCredit { community_did } if community_did == caller_did)));
    
    // Print the authorizations for inspection
    println!("Derived resource types for test_coop_bylaws.ccl:");
    for rt in &resource_types {
        println!("  {:?}", rt);
    }
    
    println!("Derived {} authorizations for test_coop_bylaws.ccl", authorizations.len());
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
    
    // Call the derivation function
    let (resource_types, authorizations) = derive_authorizations(
        &governance_config,
        caller_did,
        IdentityScope::Community,
        timestamp,
        true // verbose
    );
    
    // Verify the derived authorizations
    
    // Should have at least basic compute authorization 
    assert!(resource_types.contains(&ResourceType::Compute));
    
    // Should have network bandwidth due to dispute_resolution section
    assert!(resource_types.contains(&ResourceType::NetworkBandwidth));
    
    // Should have storage due to the fallback minimal resources
    assert!(resource_types.contains(&ResourceType::Storage));
    
    // Should NOT have labor hours resources (no compensation_policy)
    assert!(!resource_types.iter().any(|rt| matches!(rt, ResourceType::LaborHours { .. })));
    
    // Should NOT have community credit resource (no working_groups budget)
    assert!(!resource_types.iter().any(|rt| matches!(rt, ResourceType::CommunityCredit { .. })));
    
    // Print the authorizations for inspection
    println!("Derived resource types for test_community_charter.ccl:");
    for rt in &resource_types {
        println!("  {:?}", rt);
    }
    
    println!("Derived {} authorizations for test_community_charter.ccl", authorizations.len());
} 