//! Integration tests for gateway scope validation security fixes
//!
//! These tests validate that the scope allowlist properly prevents
//! privilege escalation via arbitrary scope requests.

use icn_gateway::validation;

#[test]
fn test_scope_allowlist_validation() {
    // Valid scopes should pass
    assert!(validation::validate_scopes(&["ledger:read".to_string()]).is_ok());
    assert!(validation::validate_scopes(&["ledger:write".to_string()]).is_ok());
    assert!(validation::validate_scopes(&["coop:admin".to_string()]).is_ok());
    assert!(validation::validate_scopes(&["gov:read".to_string()]).is_ok());
    assert!(validation::validate_scopes(&["payments:write".to_string()]).is_ok());

    // Multiple valid scopes
    assert!(validation::validate_scopes(&[
        "ledger:read".to_string(),
        "coop:write".to_string(),
        "gov:read".to_string(),
    ])
    .is_ok());

    // Invalid scopes should fail
    assert!(validation::validate_scopes(&["admin".to_string()]).is_err());
    assert!(validation::validate_scopes(&["superuser".to_string()]).is_err());
    assert!(validation::validate_scopes(&["ledger:admin".to_string()]).is_err());
    assert!(validation::validate_scopes(&["*".to_string()]).is_err());
    assert!(validation::validate_scopes(&["root".to_string()]).is_err());

    // Mix of valid and invalid should fail
    assert!(validation::validate_scopes(
        &["ledger:read".to_string(), "invalid:scope".to_string(),]
    )
    .is_err());
}

#[test]
fn test_scope_allowlist_prevents_privilege_escalation() {
    // Attempt various privilege escalation patterns
    let attack_scopes = vec![
        "admin",
        "root",
        "superuser",
        "ledger:*",
        "*:read",
        "system:admin",
        "database:write",
        "config:modify",
        "auth:bypass",
    ];

    for scope in attack_scopes {
        let result = validation::validate_scopes(&[scope.to_string()]);
        assert!(result.is_err(), "Attack scope '{scope}' should be rejected");
    }
}

#[test]
fn test_scope_count_limit() {
    // Max scopes (20) should work
    let max_scopes: Vec<String> = (0..20).map(|_| "ledger:read".to_string()).collect();
    assert!(validation::validate_scopes(&max_scopes).is_ok());

    // Over limit should fail
    let too_many_scopes: Vec<String> = (0..21).map(|_| "ledger:read".to_string()).collect();
    assert!(validation::validate_scopes(&too_many_scopes).is_err());
}

#[test]
fn test_all_allowed_scopes_are_valid() {
    // Verify all scopes in ALLOWED_SCOPES actually pass validation
    let all_allowed_scopes = validation::ALLOWED_SCOPES
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    assert!(
        validation::validate_scopes(&all_allowed_scopes).is_ok(),
        "All ALLOWED_SCOPES should be valid"
    );
}

#[test]
fn test_case_sensitive_scope_validation() {
    // Scope validation should be case-sensitive
    assert!(validation::validate_scopes(&["ledger:read".to_string()]).is_ok());
    assert!(validation::validate_scopes(&["LEDGER:READ".to_string()]).is_err());
    assert!(validation::validate_scopes(&["Ledger:Read".to_string()]).is_err());
}

#[test]
fn test_empty_scopes_list() {
    // Empty scopes list should be valid (user has no permissions)
    assert!(validation::validate_scopes(&[]).is_ok());
}

#[test]
fn test_scope_injection_attempts() {
    // Test various injection patterns
    let injection_attempts = vec![
        "ledger:read; admin",
        "ledger:read\nadmin",
        "ledger:read,admin",
        "ledger:read|admin",
        "../admin",
        "ledger:read\0admin",
        "$(admin)",
        "${admin}",
    ];

    for attempt in injection_attempts {
        let result = validation::validate_scopes(&[attempt.to_string()]);
        assert!(
            result.is_err(),
            "Injection attempt '{attempt}' should be rejected"
        );
    }
}

#[test]
fn test_wildcard_scope_rejection() {
    // Wildcards should not be allowed
    assert!(validation::validate_scopes(&["*".to_string()]).is_err());
    assert!(validation::validate_scopes(&["ledger:*".to_string()]).is_err());
    assert!(validation::validate_scopes(&["*:read".to_string()]).is_err());
    assert!(validation::validate_scopes(&["*:*".to_string()]).is_err());
}

#[test]
fn test_scope_namespace_boundaries() {
    // Only exact matches should pass
    assert!(validation::validate_scopes(&["ledger:read".to_string()]).is_ok());
    assert!(validation::validate_scopes(&["ledger:readonly".to_string()]).is_err());
    assert!(validation::validate_scopes(&["ledger".to_string()]).is_err());
    assert!(validation::validate_scopes(&["ledger:".to_string()]).is_err());
    assert!(validation::validate_scopes(&[":read".to_string()]).is_err());
}

#[test]
fn test_duplicate_scopes() {
    // Duplicate scopes should be allowed (idempotent)
    let duplicate_scopes = vec![
        "ledger:read".to_string(),
        "ledger:read".to_string(),
        "coop:write".to_string(),
    ];
    assert!(validation::validate_scopes(&duplicate_scopes).is_ok());
}

#[test]
fn test_scope_validation_error_messages() {
    // Error messages should be informative
    let result = validation::validate_scopes(&["invalid_scope".to_string()]);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("invalid_scope") || err_msg.contains("Invalid scope"),
        "Error message should mention the invalid scope: {err_msg}"
    );
}
