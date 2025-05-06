// Integration tests for the CCL compiler
mod integration_test;

// Unit tests for the CCL compiler
mod unit_tests;

// Unit tests for specific compiler functionality can be added here later 

use super::*;
use icn_governance_kernel::config::GovernanceConfig;
use std::path::PathBuf;
use wasmparser::{Parser, Payload};

// Helper function to create a minimal test governance config
fn create_test_governance_config() -> GovernanceConfig {
    GovernanceConfig {
        template_type: "coop_bylaws".to_string(),
        template_version: "v1".to_string(),
        governing_scope: icn_identity::IdentityScope::Cooperative,
        identity: Some(icn_governance_kernel::config::IdentityInfo {
            name: Some("Test Cooperative".to_string()),
            description: Some("A test cooperative for CCL-to-WASM compilation".to_string()),
            founding_date: Some("2025-01-01".to_string()),
            mission_statement: Some("To test the ICN Runtime".to_string()),
        }),
        governance: None,
        membership: None,
        proposals: None,
        working_groups: None,
        dispute_resolution: None,
        economic_model: None,
    }
}

// Helper function to create a test DSL input for membership proposal
fn create_test_membership_dsl() -> serde_json::Value {
    serde_json::json!({
        "action": "propose_membership",
        "applicant_did": "did:icn:applicant123",
        "name": "Alice Johnson",
        "skills": ["software_development", "community_facilitation"],
        "reason": "I want to join this cooperative to contribute to its mission."
    })
}

// Helper function to check if a WASM module has valid sections
fn validate_wasm_module(wasm_bytes: &[u8]) -> bool {
    let mut has_type_section = false;
    let mut has_import_section = false;
    let mut has_export_section = false;
    let mut has_code_section = false;
    let mut has_metadata_section = false;
    
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        match payload {
            Ok(Payload::TypeSection(_)) => has_type_section = true,
            Ok(Payload::ImportSection(_)) => has_import_section = true,
            Ok(Payload::ExportSection(_)) => has_export_section = true,
            Ok(Payload::CodeSectionStart { .. }) => has_code_section = true,
            Ok(Payload::CustomSection(section)) => {
                if section.name() == "icn-metadata" {
                    has_metadata_section = true;
                }
            }
            _ => {}
        }
    }
    
    has_type_section && has_import_section && has_export_section && 
    has_code_section && has_metadata_section
}

#[test]
fn test_generate_wasm_module() {
    // Create test inputs
    let config = create_test_governance_config();
    let dsl = create_test_membership_dsl();
    
    // Create compilation options
    let options = CompilationOptions {
        include_debug_info: true,
        optimize: false,
        memory_limits: Some(MemoryLimits {
            min_pages: 1,
            max_pages: Some(10),
        }),
        additional_metadata: Some([
            ("test_key".to_string(), "test_value".to_string())
        ].into_iter().collect()),
        caller_did: Some("did:icn:test_caller".to_string()),
        execution_id: Some("test-execution-001".to_string()),
        schema_path: None,
        validate_schema: false,
    };
    
    // Create compiler and compile WASM
    let mut compiler = CclCompiler::new();
    let result = compiler.compile_to_wasm(&config, &dsl, Some(options));
    
    // Verify compilation succeeded
    assert!(result.is_ok(), "WASM compilation failed: {:?}", result.err());
    
    // Get the compiled WASM bytes
    let wasm_bytes = result.unwrap();
    
    // Verify the WASM is not empty
    assert!(!wasm_bytes.is_empty(), "Generated WASM is empty");
    
    // Validate WASM module structure
    assert!(validate_wasm_module(&wasm_bytes), "Invalid WASM module structure");
    
    // Print size of generated WASM for reference
    println!("Generated WASM size: {} bytes", wasm_bytes.len());
}

#[test]
fn test_wasm_with_different_actions() {
    let config = create_test_governance_config();
    let mut compiler = CclCompiler::new();
    
    // Create a basic compilation option to avoid validation errors
    let options = CompilationOptions {
        include_debug_info: false,
        optimize: false,
        validate_schema: false, // Turn off schema validation for this test
        memory_limits: None,
        additional_metadata: None,
        caller_did: None,
        execution_id: None,
        schema_path: None,
    };
    
    // Test with membership proposal
    let membership_dsl = create_test_membership_dsl();
    let membership_result = compiler.compile_to_wasm(&config, &membership_dsl, Some(options.clone()));
    assert!(membership_result.is_ok(), "Membership proposal compilation failed: {:?}", membership_result.err());
    
    // Test with budget proposal
    let budget_dsl = serde_json::json!({
        "action": "propose_budget",
        "amount": 5000,
        "category": "development",
        "title": "Web Infrastructure",
        "purpose": "Develop and deploy a new website"
    });
    let budget_result = compiler.compile_to_wasm(&config, &budget_dsl, Some(options.clone()));
    assert!(budget_result.is_ok(), "Budget proposal compilation failed: {:?}", budget_result.err());
    
    // Test with unknown action
    let unknown_dsl = serde_json::json!({
        "action": "unknown_action",
        "param1": "value1"
    });
    let unknown_result = compiler.compile_to_wasm(&config, &unknown_dsl, Some(options));
    assert!(unknown_result.is_ok(), "Unknown action compilation failed: {:?}", unknown_result.err());
}

#[test]
fn test_metadata_embedding() {
    // Create test inputs
    let config = create_test_governance_config();
    let dsl = create_test_membership_dsl();
    
    // Create compilation options with specific metadata
    let test_did = "did:icn:test_caller_123";
    let test_exec_id = "test-execution-123";
    
    let options = CompilationOptions {
        include_debug_info: true,
        optimize: false,
        additional_metadata: Some([
            ("custom_field".to_string(), "custom_value".to_string())
        ].into_iter().collect()),
        caller_did: Some(test_did.to_string()),
        execution_id: Some(test_exec_id.to_string()),
        schema_path: None,
        validate_schema: false,
        memory_limits: None,
    };
    
    // Create compiler and compile WASM
    let mut compiler = CclCompiler::new();
    let wasm_bytes = compiler.compile_to_wasm(&config, &dsl, Some(options)).unwrap();
    
    // Find and extract metadata from custom section
    let mut metadata_json = None;
    
    for payload in Parser::new(0).parse_all(&wasm_bytes) {
        if let Ok(Payload::CustomSection(section)) = payload {
            if section.name() == "icn-metadata" {
                metadata_json = Some(String::from_utf8_lossy(section.data()).to_string());
                break;
            }
        }
    }
    
    // Verify metadata was found
    assert!(metadata_json.is_some(), "Metadata custom section not found");
    
    // Parse the metadata JSON
    let metadata: MetadataInfo = serde_json::from_str(&metadata_json.unwrap()).unwrap();
    
    // Verify metadata fields
    assert_eq!(metadata.template_type, "coop_bylaws");
    assert_eq!(metadata.template_version, "v1");
    assert_eq!(metadata.action, "propose_membership");
    assert_eq!(metadata.caller_did, Some(test_did.to_string()));
    assert_eq!(metadata.execution_id, Some(test_exec_id.to_string()));
    assert!(metadata.additional_data.contains_key("custom_field"));
    assert_eq!(metadata.additional_data.get("custom_field").unwrap(), "custom_value");
} 