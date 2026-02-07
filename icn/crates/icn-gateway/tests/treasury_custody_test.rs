//! Treasury Key Custody Enforcement Tests (Issue #1087)
//!
//! These tests verify that:
//! 1. No gateway route exposes private key material (key export, secrets, etc.)
//! 2. Treasury operations go through governance-gated paths only
//! 3. The Cooperative struct never stores keypair data -- only the DID string
//!
//! Security invariant: Treasury keys live in the Age-encrypted keystore and are
//! only loaded by the governance executor during approved signing operations.
#![allow(clippy::unwrap_used, clippy::expect_used)]

/// Source code of the gateway server module, read at compile time for static analysis.
const SERVER_SOURCE: &str = include_str!("../src/server.rs");

/// Source code of the treasury API module.
const TREASURY_API_SOURCE: &str = include_str!("../src/api/treasury.rs");

/// Source code of the treasury manager module.
const TREASURY_MGR_SOURCE: &str = include_str!("../src/treasury_mgr.rs");

// ---------------------------------------------------------------------------
// 1. Route enumeration: no key-export routes exist in the gateway
// ---------------------------------------------------------------------------

/// Dangerous URL path segments that would indicate key material exposure.
const DANGEROUS_ROUTE_SEGMENTS: &[&str] = &[
    "/key/export",
    "/key_export",
    "/export_key",
    "/export-key",
    "/private_key",
    "/private-key",
    "/secret_key",
    "/secret-key",
    "/treasury/key",
    "/treasury_key",
    "/signing_key",
    "/signing-key",
    "/keypair",
    "/raw_key",
    "/raw-key",
];

#[test]
fn test_no_key_export_routes_in_gateway_server() {
    // Static analysis: scan the server source for any route registration that
    // could expose private key material.
    for segment in DANGEROUS_ROUTE_SEGMENTS {
        assert!(
            !SERVER_SOURCE.contains(segment),
            "SECURITY: Gateway server.rs contains dangerous route segment '{segment}'. \
             Treasury keys must NEVER be exposed via API routes."
        );
    }
}

#[test]
fn test_no_key_export_routes_in_treasury_api() {
    // The treasury API module must not have endpoints for exporting keys.
    for segment in DANGEROUS_ROUTE_SEGMENTS {
        assert!(
            !TREASURY_API_SOURCE.contains(segment),
            "SECURITY: Treasury API contains dangerous route segment '{segment}'. \
             Treasury keys must NEVER be exposed via API."
        );
    }
}

#[test]
fn test_treasury_api_has_no_keypair_fields_in_responses() {
    // Ensure the treasury API response types never include keypair/secret fields.
    // These are struct definitions that get serialized to JSON responses.
    let dangerous_field_patterns = [
        "secret_key",
        "private_key",
        "signing_key",
        "keypair",
        "key_pair",
        "secret_bytes",
    ];

    for pattern in &dangerous_field_patterns {
        assert!(
            !TREASURY_API_SOURCE.contains(pattern),
            "SECURITY: Treasury API source contains field pattern '{pattern}'. \
             Response types must never include key material."
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Treasury manager never stores or returns key material
// ---------------------------------------------------------------------------

#[test]
fn test_treasury_manager_has_no_keypair_storage() {
    // The GatewayTreasuryManager must not store any keypair data.
    let dangerous_patterns = [
        "KeyPair",
        "SigningKey",
        "secret_key",
        "private_key",
        "secret_bytes",
    ];

    for pattern in &dangerous_patterns {
        assert!(
            !TREASURY_MGR_SOURCE.contains(pattern),
            "SECURITY: Treasury manager contains '{pattern}'. \
             Treasury keys must only live in the Age-encrypted keystore, \
             never in the gateway's treasury manager."
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Treasury routes only expose read/write operations, not key access
// ---------------------------------------------------------------------------

#[test]
fn test_treasury_routes_are_governance_gated() {
    // The treasury configure() function should only register safe operations:
    // status, balance, budgets, spending-rules, audit, deposit.
    // It must NOT register routes for key export, key rotation, or signing.

    // Verify the known safe routes are registered
    assert!(
        TREASURY_API_SOURCE.contains("get_treasury_status"),
        "Expected get_treasury_status route"
    );
    assert!(
        TREASURY_API_SOURCE.contains("get_treasury_balance"),
        "Expected get_treasury_balance route"
    );
    assert!(
        TREASURY_API_SOURCE.contains("list_budgets"),
        "Expected list_budgets route"
    );
    assert!(
        TREASURY_API_SOURCE.contains("create_budget"),
        "Expected create_budget route"
    );
    assert!(
        TREASURY_API_SOURCE.contains("list_spending_rules"),
        "Expected list_spending_rules route"
    );
    assert!(
        TREASURY_API_SOURCE.contains("get_audit_trail"),
        "Expected get_audit_trail route"
    );
    assert!(
        TREASURY_API_SOURCE.contains("deposit_to_treasury"),
        "Expected deposit_to_treasury route"
    );

    // Verify no dangerous operations are registered
    let forbidden_route_names = [
        "export_key",
        "export_treasury_key",
        "get_private_key",
        "get_signing_key",
        "get_keypair",
        "rotate_key_ungovern",
    ];

    for name in &forbidden_route_names {
        assert!(
            !TREASURY_API_SOURCE.contains(name),
            "SECURITY: Treasury API contains forbidden route '{name}'"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Gateway server scope: treasury endpoints require auth and rate limiting
// ---------------------------------------------------------------------------

#[test]
fn test_treasury_scope_has_auth_middleware() {
    // The treasury scope in server.rs must have auth middleware wrapping it.
    // Find the treasury scope configuration and verify it has .wrap(auth.clone()).
    let treasury_scope_start = SERVER_SOURCE
        .find("web::scope(\"/treasury\")")
        .expect("Treasury scope must be registered in server.rs");

    // Get the text after the treasury scope start, up to the next .service( at same indent
    let scope_text = &SERVER_SOURCE[treasury_scope_start..];
    let scope_end = scope_text
        .find(".service(\n")
        .or_else(|| scope_text.find(".service(web::scope"))
        .unwrap_or(scope_text.len().min(500));
    let scope_block = &scope_text[..scope_end];

    assert!(
        scope_block.contains("auth.clone()"),
        "SECURITY: Treasury scope must have JWT auth middleware. Found: {}",
        scope_block
    );
    assert!(
        scope_block.contains("trust_rate_limit_middleware"),
        "SECURITY: Treasury scope must have trust-gated rate limiting. Found: {}",
        scope_block
    );
}

// ---------------------------------------------------------------------------
// 5. No route in the entire gateway exposes the patterns /key, /export, /secret
// ---------------------------------------------------------------------------

#[test]
fn test_no_dangerous_route_patterns_anywhere_in_server() {
    // Check that no web::scope or web::resource in the server registers a
    // dangerous pattern. We scan for actix route registration syntax.
    let route_pattern_indicators = [
        "scope(\"/key\")",
        "scope(\"/keys\")",
        "scope(\"/export\")",
        "scope(\"/secret\")",
        "scope(\"/private\")",
        "resource(\"/key\")",
        "resource(\"/keys\")",
        "resource(\"/export\")",
        "resource(\"/secret\")",
        "resource(\"/private\")",
    ];

    for pattern in &route_pattern_indicators {
        assert!(
            !SERVER_SOURCE.contains(pattern),
            "SECURITY: server.rs contains route pattern '{}' which could expose key material",
            pattern
        );
    }
}

// ---------------------------------------------------------------------------
// 6. Cooperative struct field audit
// ---------------------------------------------------------------------------

#[test]
fn test_cooperative_struct_has_no_key_fields() {
    // Read the icn-coop types source to verify no key material fields exist.
    let types_source = include_str!("../../icn-coop/src/types.rs");

    // The struct should have treasury_did (a String), but never a keypair field.
    assert!(
        types_source.contains("pub treasury_did: Option<String>"),
        "Cooperative should have treasury_did as Option<String>"
    );

    // Verify no KeyPair, SigningKey, or secret field exists in the struct definition.
    // We look specifically in the struct definition block.
    let struct_start = types_source
        .find("pub struct Cooperative {")
        .expect("Cooperative struct must exist");
    // Find the closing brace of the struct
    let struct_text = &types_source[struct_start..];
    let mut brace_depth = 0;
    let mut struct_end = struct_text.len();
    for (i, ch) in struct_text.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    struct_end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let struct_body = &struct_text[..struct_end];

    // These must NOT appear as fields in the Cooperative struct
    let forbidden_in_struct = [
        "KeyPair",
        "SigningKey",
        "secret_key",
        "private_key",
        "secret_bytes",
    ];

    for pattern in &forbidden_in_struct {
        assert!(
            !struct_body.contains(pattern),
            "SECURITY: Cooperative struct contains field with '{pattern}'. \
             Key material must NEVER be stored on the Cooperative struct."
        );
    }
}
