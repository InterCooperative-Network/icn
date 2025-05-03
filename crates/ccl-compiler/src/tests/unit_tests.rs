use crate::{CclCompiler, CompilationOptions, MetadataInfo};
use icn_governance_kernel::config::GovernanceConfig;
use icn_identity::IdentityScope;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

fn create_test_ccl_config() -> GovernanceConfig {
    GovernanceConfig {
        template_type: "coop_bylaws".to_string(),
        template_version: "v1".to_string(),
        governing_scope: IdentityScope::Cooperative,
        identity: Some(icn_governance_kernel::config::IdentityInfo {
            name: Some("Test Cooperative".to_string()),
            description: Some("A test cooperative for WASM compilation".to_string()),
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
    }
}

fn create_test_dsl_input() -> JsonValue {
    serde_json::json!({
        "action": "propose_membership",
        "applicant_did": "did:icn:test:applicant",
        "name": "John Doe",
        "reason": "Wants to join the cooperative"
    })
}

#[test]
fn test_validate_dsl_for_template() {
    let compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    let dsl_input = create_test_dsl_input();

    // Test valid input
    let result = compiler.validate_dsl_for_template(&ccl_config, &dsl_input, false);
    assert!(result.is_ok(), "Valid DSL input should pass validation");

    // Test invalid input (missing required field)
    let invalid_input = serde_json::json!({
        "action": "propose_membership",
        // Missing "applicant_did"
        "name": "John Doe",
        "reason": "Wants to join the cooperative"
    });
    let result = compiler.validate_dsl_for_template(&ccl_config, &invalid_input, false);
    assert!(result.is_err(), "Invalid DSL input should fail validation");

    // But with skip_strict_validation=true, it should pass
    let result = compiler.validate_dsl_for_template(&ccl_config, &invalid_input, true);
    assert!(result.is_ok(), "Expected validation to succeed with skip_strict_validation=true");
}

#[test]
fn test_generate_wasm_module() {
    let compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    let dsl_input = create_test_dsl_input();
    let options = CompilationOptions::default();

    let result = compiler.generate_wasm_module(&ccl_config, &dsl_input, &options);
    assert!(result.is_ok(), "WASM generation should succeed");

    let wasm_bytes = result.unwrap();
    // Check that we got a non-empty WASM module
    assert!(!wasm_bytes.is_empty(), "WASM module should not be empty");
    // Valid WASM modules start with the magic number \0asm
    assert_eq!(
        &wasm_bytes[0..4],
        &[0x00, 0x61, 0x73, 0x6d],
        "WASM module should start with \\0asm"
    );
}

#[test]
fn test_compile_to_wasm() {
    let mut compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    let dsl_input = create_test_dsl_input();

    // Disable schema validation for this test since we don't have actual schema files
    let options = CompilationOptions {
        validate_schema: false,
        ..CompilationOptions::default()
    };

    let result = compiler.compile_to_wasm(&ccl_config, &dsl_input, Some(options));
    assert!(result.is_ok(), "Compilation should succeed");

    let wasm_bytes = result.unwrap();
    // Check that we got a non-empty WASM module
    assert!(!wasm_bytes.is_empty(), "WASM module should not be empty");
    // Valid WASM modules start with the magic number \0asm
    assert_eq!(
        &wasm_bytes[0..4],
        &[0x00, 0x61, 0x73, 0x6d],
        "WASM module should start with \\0asm"
    );
}

#[test]
fn test_metadata_embedding() {
    let mut compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    let dsl_input = create_test_dsl_input();
    
    // Create options with custom metadata
    let mut additional_metadata = HashMap::new();
    additional_metadata.insert("test_key".to_string(), "test_value".to_string());
    
    let options = CompilationOptions {
        include_debug_info: true,
        optimize: true,
        memory_limits: None,
        additional_metadata: Some(additional_metadata),
        caller_did: Some("did:icn:test:caller123".to_string()),
        execution_id: Some("test-exec-001".to_string()),
        schema_path: None,
        validate_schema: false,
    };
    
    // Compile with metadata
    let result = compiler.compile_to_wasm(&ccl_config, &dsl_input, Some(options));
    assert!(result.is_ok(), "Compilation with metadata should succeed");
    
    let wasm_bytes = result.unwrap();
    
    // Verify that the WASM module contains a metadata section
    // This is a simple check for the "icn-metadata" string that would be in the custom section name
    let metadata_marker = "icn-metadata".as_bytes();
    let has_metadata = wasm_bytes.windows(metadata_marker.len())
        .any(|window| window == metadata_marker);
    
    assert!(has_metadata, "WASM module should contain a metadata section");
    
    // In a more robust test, we would actually parse the WASM and validate the custom section
    // That would require a WASM parsing library like wasmparser
}

#[test]
fn test_create_metadata() {
    let compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    let dsl_input = create_test_dsl_input();
    
    // Create options with test metadata
    let mut additional_metadata = HashMap::new();
    additional_metadata.insert("test_key".to_string(), "test_value".to_string());
    
    let options = CompilationOptions {
        include_debug_info: true,
        optimize: true,
        memory_limits: None,
        additional_metadata: Some(additional_metadata),
        caller_did: Some("did:icn:test:caller123".to_string()),
        execution_id: Some("test-exec-001".to_string()),
        schema_path: None,
        validate_schema: false,
    };
    
    // Create the metadata
    let metadata_result = compiler.create_metadata(&ccl_config, &dsl_input, &options);
    assert!(metadata_result.is_ok(), "Metadata creation should succeed");
    
    let metadata = metadata_result.unwrap();
    
    // Verify metadata fields
    assert_eq!(metadata.template_type, "coop_bylaws");
    assert_eq!(metadata.template_version, "v1");
    assert_eq!(metadata.action, "propose_membership");
    assert_eq!(metadata.caller_did, Some("did:icn:test:caller123".to_string()));
    assert_eq!(metadata.execution_id, Some("test-exec-001".to_string()));
    
    // Verify additional data
    assert!(metadata.additional_data.contains_key("test_key"));
    assert_eq!(metadata.additional_data.get("test_key").unwrap(), "test_value");
    
    // Verify DSL values are included
    assert!(metadata.additional_data.contains_key("dsl_name"));
    assert_eq!(metadata.additional_data.get("dsl_name").unwrap(), "John Doe");
}

#[test]
fn test_schema_validation_valid_input() {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    
    // Create a temporary directory for test schema
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let schema_path = temp_dir.path().join("propose_membership.schema.json");
    
    // Create a simple test schema for propose_membership
    let schema_content = r#"{
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["propose_membership"] },
            "applicant_did": { "type": "string" },
            "name": { "type": "string" },
            "reason": { "type": "string" }
        },
        "required": ["action", "applicant_did", "name", "reason"]
    }"#;
    
    // Write the schema to a temporary file
    fs::write(&schema_path, schema_content).expect("Failed to write schema file");
    
    let mut compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    
    // Create a valid membership proposal DSL
    let valid_dsl = serde_json::json!({
        "action": "propose_membership",
        "applicant_did": "did:icn:test:applicant",
        "name": "Jane Smith",
        "reason": "I want to contribute my skills to the cooperative"
    });
    
    // Configure compilation options with schema validation
    let options = CompilationOptions {
        include_debug_info: false,
        optimize: true,
        memory_limits: None,
        additional_metadata: None,
        caller_did: None,
        execution_id: None,
        schema_path: Some(schema_path),
        validate_schema: true,
    };
    
    // Compilation should succeed with valid input
    let result = compiler.compile_to_wasm(&ccl_config, &valid_dsl, Some(options));
    assert!(result.is_ok(), "Compilation with valid DSL should succeed");
}

#[test]
fn test_schema_validation_invalid_input() {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    
    // Create a temporary directory for test schema
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let schema_path = temp_dir.path().join("propose_membership.schema.json");
    
    // Create a simple test schema for propose_membership
    let schema_content = r#"{
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["propose_membership"] },
            "applicant_did": { "type": "string" },
            "name": { "type": "string" },
            "reason": { "type": "string" }
        },
        "required": ["action", "applicant_did", "name", "reason"]
    }"#;
    
    // Write the schema to a temporary file
    fs::write(&schema_path, schema_content).expect("Failed to write schema file");
    
    let mut compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    
    // Create an invalid membership proposal DSL (missing required name)
    let invalid_dsl = serde_json::json!({
        "action": "propose_membership",
        "applicant_did": "did:icn:test:applicant",
        // Missing "name" field
        "reason": "I want to contribute my skills to the cooperative"
    });
    
    // Configure compilation options with schema validation
    let options = CompilationOptions {
        include_debug_info: false,
        optimize: true,
        memory_limits: None,
        additional_metadata: None,
        caller_did: None,
        execution_id: None,
        schema_path: Some(schema_path),
        validate_schema: true,
    };
    
    // Compilation should fail with invalid input
    let result = compiler.compile_to_wasm(&ccl_config, &invalid_dsl, Some(options));
    assert!(result.is_err(), "Compilation with invalid DSL should fail");
    
    // Check that the error message contains useful information
    if let Err(err) = result {
        let err_string = err.to_string();
        println!("Validation error: {}", err_string);
        // The error should explain that a field is missing
        assert!(err_string.contains("name") || err_string.contains("Missing required property"), 
                "Error message should mention the missing field: {}", err_string);
    }
}

#[test]
fn test_schema_validation_with_custom_schema() {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    
    // Create a temporary directory for test schema
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let schema_path = temp_dir.path().join("test_schema.json");
    
    // Create a simple test schema
    let schema_content = r#"{
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["test_action"] },
            "test_field": { "type": "string" }
        },
        "required": ["action", "test_field"]
    }"#;
    
    // Write the schema to a temporary file
    fs::write(&schema_path, schema_content).expect("Failed to write schema file");
    
    let mut compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    
    // Valid DSL according to our custom schema
    let valid_dsl = serde_json::json!({
        "action": "test_action",
        "test_field": "test value"
    });
    
    // Invalid DSL according to our custom schema
    let invalid_dsl = serde_json::json!({
        "action": "test_action"
        // Missing required "test_field"
    });
    
    // Configure compilation options with custom schema
    let valid_options = CompilationOptions {
        include_debug_info: false,
        optimize: true,
        memory_limits: None,
        additional_metadata: None,
        caller_did: None,
        execution_id: None,
        schema_path: Some(schema_path.clone()),
        validate_schema: true,
    };
    
    // Test with valid input
    let result = compiler.compile_to_wasm(&ccl_config, &valid_dsl, Some(valid_options.clone()));
    assert!(result.is_ok(), "Compilation with valid DSL against custom schema should succeed");
    
    // Test with invalid input
    let invalid_options = CompilationOptions {
        schema_path: Some(schema_path),
        ..valid_options
    };
    
    let result = compiler.compile_to_wasm(&ccl_config, &invalid_dsl, Some(invalid_options));
    assert!(result.is_err(), "Compilation with invalid DSL against custom schema should fail");
    
    // Check that the error message contains useful information
    if let Err(err) = result {
        let err_string = err.to_string();
        println!("Validation error: {}", err_string);
        // The error should explain that test_field is missing
        assert!(err_string.contains("test_field") || err_string.contains("missing"), 
                "Error message should mention the missing field: {}", err_string);
    }
} 