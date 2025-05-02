use crate::{CclCompiler, CompilationOptions};
use icn_governance_kernel::config::GovernanceConfig;
use icn_identity::IdentityScope;
use serde_json::Value as JsonValue;

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
    let result = compiler.validate_dsl_for_template(&ccl_config, &dsl_input);
    assert!(result.is_ok(), "Valid DSL input should pass validation");

    // Test invalid input (missing required field)
    let invalid_input = serde_json::json!({
        "action": "propose_membership",
        // Missing "applicant_did"
        "name": "John Doe",
        "reason": "Wants to join the cooperative"
    });
    let result = compiler.validate_dsl_for_template(&ccl_config, &invalid_input);
    assert!(result.is_err(), "Invalid DSL input should fail validation");
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
    let compiler = CclCompiler::new();
    let ccl_config = create_test_ccl_config();
    let dsl_input = create_test_dsl_input();

    let result = compiler.compile_to_wasm(&ccl_config, &dsl_input, None);
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