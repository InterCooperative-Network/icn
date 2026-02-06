//! Adversarial auth boundary tests for service discovery (B4, Issue #1082).
//!
//! Every test in this file demonstrates a specific attack that the service
//! discovery auth boundary must prevent. There are NO happy-path tests here.
//! Each test has an `/// Attack:` doc-comment explaining the threat model.
//!
//! These tests exercise the gossip-level signing, verification, and validation
//! functions that form the cryptographic security boundary for service discovery.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::service_discovery::{
    sign_service_endpoint, sign_service_response, validate_service_response,
    verify_service_endpoint, verify_service_response, ServiceDiscoveryMessage,
};
use icn_identity::KeyPair;
use icn_kernel_api::naming::{ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::{Endpoint, Signature};

// ============================================================================
// Helpers
// ============================================================================

fn make_keypair() -> (ed25519_dalek::SigningKey, icn_identity::Did) {
    let kp = KeyPair::generate().unwrap();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes());
    let did = kp.did().clone();
    (signing_key, did)
}

fn make_endpoint(did: &icn_identity::Did, scope: ScopeLevel) -> ServiceEndpoint {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    ServiceEndpoint {
        service_id: "svc-test".to_string(),
        provider: did.to_string(),
        endpoint_type: icn_kernel_api::naming::EndpointType::Http,
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        endpoints: vec![Endpoint::new("https", "node.example.com", 8443)],
        addresses: vec![],
        capabilities: vec!["read".to_string()],
        trust_threshold: 0.1,
        scope_visibility: scope,
        cell_id: None,
        ttl_secs: 3600,
        signature: Signature::new(vec![0; 64]),
        created_at: now,
        updated_at: now,
    }
}

fn future_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

fn past_timestamp() -> u64 {
    1_000_000 // ~1970, always in the past
}

fn make_signed_response(
    query_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    did: &icn_identity::Did,
    endpoints: Vec<ServiceEndpoint>,
    scope: ScopeLevel,
    expires_at: u64,
) -> ServiceDiscoveryMessage {
    let mut response = ServiceDiscoveryMessage::Response {
        query_id: query_id.to_string(),
        endpoints,
        responder: did.clone(),
        signature: vec![0; 64],
        expires_at,
        scope,
    };
    sign_service_response(&mut response, signing_key).expect("signing should succeed");
    response
}

// ============================================================================
// Test 1: Cross-scope query rejected
// ============================================================================

/// Attack: A malicious peer responds to a Cooperative (Org) scope query with
/// Federation-scope results, leaking service endpoints that should not be
/// visible at the queried scope. The `validate_service_response` function must
/// reject responses whose scope exceeds the query scope.
#[test]
fn cross_scope_response_rejected() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Federation);

    // Response claims Federation scope
    let response = make_signed_response(
        "q-scope-test",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Federation,
        future_timestamp(),
    );

    // Query only asked for Org scope (narrower than Federation)
    let valid = validate_service_response(&response, &ScopeLevel::Org);
    assert!(
        !valid,
        "Response with scope (Federation) exceeding query scope (Org) must be rejected"
    );

    // Also verify narrower scope direction: Commons query accepts Federation response
    let valid_wider = validate_service_response(&response, &ScopeLevel::Commons);
    assert!(
        valid_wider,
        "Sanity check: Federation response within Commons query scope should pass"
    );
}

// ============================================================================
// Test 2: Response from unauthorized DID ignored
// ============================================================================

/// Attack: An attacker possesses a valid Ed25519 keypair and signs a response
/// claiming to be from a victim's DID. The signature is technically valid for
/// the attacker's own key but does not match the responder DID embedded in the
/// response. Signature verification must bind the response to the claimed
/// responder identity, preventing impersonation.
#[test]
fn response_signed_by_wrong_key_rejected() {
    let (attacker_key, _attacker_did) = make_keypair();
    let (_victim_key, victim_did) = make_keypair();
    let ep = make_endpoint(&victim_did, ScopeLevel::Org);

    // Attacker signs the response but sets responder to victim's DID
    let mut response = ServiceDiscoveryMessage::Response {
        query_id: "q-impersonation".to_string(),
        endpoints: vec![ep],
        responder: victim_did.clone(),
        signature: vec![0; 64],
        expires_at: future_timestamp(),
        scope: ScopeLevel::Org,
    };
    sign_service_response(&mut response, &attacker_key)
        .expect("signing with attacker key should succeed mechanically");

    // Verification checks the signature against the responder DID's public key
    // (victim's key), not the attacker's key -- must fail
    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Response signed by wrong key (impersonation) must fail signature verification"
    );

    // Full validation must also reject (it calls verify internally)
    let valid = validate_service_response(&response, &ScopeLevel::Federation);
    assert!(!valid, "Impersonated response must fail full validation");
}

// ============================================================================
// Test 3: Replayed responses rejected (TTL)
// ============================================================================

/// Attack: An attacker captures a legitimately signed response and replays it
/// after its TTL has expired, hoping the receiver will accept stale data that
/// may no longer reflect the current service topology. The `expires_at`
/// timestamp must be checked against the current system time.
#[test]
fn expired_response_replay_rejected() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Org);

    // Set expires_at far in the past (simulating a recorded and replayed response)
    let response = make_signed_response(
        "q-replay",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Org,
        past_timestamp(),
    );

    // Signature is cryptographically valid (it was legitimately signed)
    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: signature should be cryptographically valid (it was properly signed)"
    );

    // But validation rejects it because TTL expired
    let valid = validate_service_response(&response, &ScopeLevel::Federation);
    assert!(
        !valid,
        "Expired response (TTL in the past) must be rejected even if signature is valid"
    );
}

// ============================================================================
// Test 4: Unsigned announce doesn't pollute registry
// ============================================================================

/// Attack: An attacker injects a fabricated Announce message with an invalid
/// (zeroed-out) signature, hoping to register a fake service endpoint. The
/// `verify_service_endpoint` function (called by gossip handlers before
/// storing) must reject endpoints with invalid signatures.
#[test]
fn unsigned_announce_signature_rejected() {
    let (_signing_key, did) = make_keypair();

    // Create an endpoint but do NOT sign it (signature remains zeroed out)
    let ep = make_endpoint(&did, ScopeLevel::Org);
    assert_eq!(
        ep.signature.as_bytes(),
        &[0u8; 64],
        "Precondition: endpoint should have zeroed signature"
    );

    // verify_service_endpoint must reject the unsigned endpoint
    let result = verify_service_endpoint(&ep);
    assert!(
        result.is_err(),
        "Unsigned endpoint (zeroed signature) must fail verification"
    );
}

/// Attack: An attacker generates a random 64-byte signature (not all zeros)
/// and attaches it to a fabricated endpoint, hoping to bypass simple
/// zero-check guards. Full Ed25519 verification must catch this.
#[test]
fn random_signature_announce_rejected() {
    let (_signing_key, did) = make_keypair();
    let mut ep = make_endpoint(&did, ScopeLevel::Org);

    // Set a random but invalid 64-byte signature
    ep.signature = Signature::new(vec![0xAB; 64]);

    let result = verify_service_endpoint(&ep);
    assert!(
        result.is_err(),
        "Endpoint with random (non-Ed25519) signature must fail verification"
    );
}

// ============================================================================
// Test 5: Tampered response body rejected
// ============================================================================

/// Attack: An attacker intercepts a validly signed response in transit and
/// modifies the endpoint list (replacing legitimate endpoints with
/// attacker-controlled ones). The signature must be invalidated by any
/// modification to the signed payload fields.
#[test]
fn tampered_response_endpoints_rejected() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Org);

    let mut response = make_signed_response(
        "q-tamper",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Org,
        future_timestamp(),
    );

    // Verify the response is valid before tampering
    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: response should be valid before tampering"
    );

    // Tamper: modify the endpoint addresses (attacker replaces endpoints)
    if let ServiceDiscoveryMessage::Response {
        ref mut endpoints, ..
    } = response
    {
        endpoints[0].endpoints = vec![Endpoint::new("https", "evil.attacker.com", 666)];
    }

    // Signature must now fail because endpoints hash changed
    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Response with tampered endpoint addresses must fail signature verification"
    );
}

/// Attack: An attacker modifies the scope field of a signed response after
/// signing, attempting to escalate a narrowly-scoped response to a wider
/// scope. The scope is part of the signed payload and tampering must break
/// the signature.
#[test]
fn tampered_response_scope_rejected() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Local);

    let mut response = make_signed_response(
        "q-scope-tamper",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Local,
        future_timestamp(),
    );

    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: response should be valid before tampering"
    );

    // Tamper: escalate scope from Local to Commons
    if let ServiceDiscoveryMessage::Response { ref mut scope, .. } = response {
        *scope = ScopeLevel::Commons;
    }

    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Response with tampered scope (Local->Commons) must fail signature verification"
    );
}

/// Attack: An attacker modifies the expires_at field of a signed response to
/// extend its lifetime, keeping stale service data alive longer than intended.
/// The expires_at value is part of the signed payload.
#[test]
fn tampered_response_expiry_rejected() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Org);

    let mut response = make_signed_response(
        "q-expiry-tamper",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Org,
        future_timestamp(),
    );

    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: response should be valid before tampering"
    );

    // Tamper: extend the expiry by 1 year to keep stale data alive
    if let ServiceDiscoveryMessage::Response {
        ref mut expires_at, ..
    } = response
    {
        *expires_at += 365 * 24 * 3600;
    }

    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Response with tampered expires_at must fail signature verification"
    );
}

// ============================================================================
// Test 6: Empty/malformed query ID ignored
// ============================================================================

/// Attack: An attacker crafts a signed response with an empty query_id,
/// attempting to match against pending queries or cause undefined behavior
/// in the routing logic. While signature validation still passes (empty
/// query_id is a valid string), the query_id is part of the signed payload,
/// so the attacker cannot change it post-signing to match a real query_id
/// without invalidating the signature.
#[test]
fn empty_query_id_cannot_be_retargeted() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Org);

    // Sign a response with empty query_id
    let mut response = make_signed_response(
        "",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Org,
        future_timestamp(),
    );

    // The signature is valid for the empty query_id
    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: empty query_id response has valid signature"
    );

    // Attacker tries to retarget this response to a real query_id
    if let ServiceDiscoveryMessage::Response {
        ref mut query_id, ..
    } = response
    {
        *query_id = "q-real-pending-query".to_string();
    }

    // Signature must break because query_id is part of the signed payload
    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Retargeting a response by changing query_id must break the signature"
    );
}

// ============================================================================
// Test 7: Response with wrong query_id dropped
// ============================================================================

/// Attack: An attacker observes a pending query_id and attempts to construct
/// a signed response for a different query_id, then modify it to match the
/// target. Since the query_id is included in the canonical signing payload,
/// changing it after signing invalidates the signature, preventing
/// cross-query response injection.
#[test]
fn query_id_swap_breaks_signature() {
    let (signing_key, did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Org);

    // Sign a response for query "q-original"
    let mut response = make_signed_response(
        "q-original",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Org,
        future_timestamp(),
    );

    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: response valid for original query_id"
    );

    // Attacker swaps query_id to inject into a different pending query
    if let ServiceDiscoveryMessage::Response {
        ref mut query_id, ..
    } = response
    {
        *query_id = "q-target-victim".to_string();
    }

    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Swapping query_id must break the signature (query_id is in signed payload)"
    );

    // Full validation must also reject
    let valid = validate_service_response(&response, &ScopeLevel::Federation);
    assert!(
        !valid,
        "Response with swapped query_id must fail full validation"
    );
}

// ============================================================================
// Test 8: Expired query ignored
// ============================================================================

/// Attack: An attacker replays a previously valid query message after its
/// `expires_at` timestamp has passed, attempting to trick peers into wasting
/// resources computing and signing responses. The query expiry must be
/// checked before processing. This test verifies the expiry field semantics
/// that handlers rely on for rejection.
#[test]
fn expired_query_detectable() {
    let (_, requester_did) = make_keypair();

    // Build an expired query (expires_at in the past)
    let query = ServiceDiscoveryMessage::Query {
        requester: requester_did,
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        max_scope: ScopeLevel::Org,
        required_capabilities: vec![],
        query_id: "q-expired".to_string(),
        expires_at: past_timestamp(),
    };

    // Extract and verify the expiry check that handlers must perform
    if let ServiceDiscoveryMessage::Query { expires_at, .. } = &query {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert!(
            now > *expires_at,
            "Expired query must have expires_at < current time"
        );
    } else {
        panic!("Expected Query variant");
    }

    // Verify the message survives serialization roundtrip (attacker can't
    // hide the expiry by encoding tricks)
    let encoded = icn_encoding::encode(&query).expect("encode");
    let decoded: ServiceDiscoveryMessage = icn_encoding::decode(&encoded).expect("decode");

    if let ServiceDiscoveryMessage::Query { expires_at, .. } = &decoded {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            now > *expires_at,
            "Expired query must remain expired after deserialization (no encoding bypass)"
        );
    } else {
        panic!("Expected Query variant after roundtrip");
    }
}

/// Attack: An attacker crafts a query with `expires_at` set to `u64::MAX`,
/// attempting to create a query that never expires and consumes peer resources
/// indefinitely. While the protocol does not enforce a max TTL at the gossip
/// layer, this test verifies that the expiry field is properly serialized
/// and can be checked by handlers (which should enforce reasonable TTL limits).
#[test]
fn query_with_max_expiry_is_inspectable() {
    let (_, requester_did) = make_keypair();

    let query = ServiceDiscoveryMessage::Query {
        requester: requester_did,
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        max_scope: ScopeLevel::Org,
        required_capabilities: vec![],
        query_id: "q-forever".to_string(),
        expires_at: u64::MAX,
    };

    // Verify the field survives roundtrip and can be inspected for policy
    let encoded = icn_encoding::encode(&query).expect("encode");
    let decoded: ServiceDiscoveryMessage = icn_encoding::decode(&encoded).expect("decode");

    if let ServiceDiscoveryMessage::Query { expires_at, .. } = &decoded {
        assert_eq!(
            *expires_at,
            u64::MAX,
            "Max expiry must survive serialization for handler-level policy enforcement"
        );
        // Handlers should detect unreasonable TTLs like this and reject
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ttl_secs = expires_at.saturating_sub(now);
        assert!(
            ttl_secs > 86400 * 365,
            "Unreasonable TTL (> 1 year) should be detectable by handlers"
        );
    } else {
        panic!("Expected Query variant after roundtrip");
    }
}

/// Attack: An attacker modifies the responder DID field of a signed response
/// to attribute it to a trusted node, attempting to gain elevated trust for
/// the response. The responder DID is part of the signed payload and
/// tampering must break the signature.
#[test]
fn tampered_responder_did_rejected() {
    let (signing_key, did) = make_keypair();
    let (_, trusted_did) = make_keypair();
    let ep = make_endpoint(&did, ScopeLevel::Org);

    let mut response = make_signed_response(
        "q-did-tamper",
        &signing_key,
        &did,
        vec![ep],
        ScopeLevel::Org,
        future_timestamp(),
    );

    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: response valid with original responder DID"
    );

    // Tamper: replace responder with a more trusted DID
    if let ServiceDiscoveryMessage::Response {
        ref mut responder, ..
    } = response
    {
        *responder = trusted_did;
    }

    // Signature must fail: responder DID is in the signed payload, AND the
    // public key used for verification is extracted from the responder DID,
    // so verification will use the wrong key
    let result = verify_service_response(&response);
    assert!(
        result.is_err(),
        "Changing the responder DID must break signature verification"
    );
}

/// Attack: An attacker creates a service endpoint signed with their key but
/// sets the provider DID to a different (more trusted) identity. The
/// signature verification must use the public key from the provider DID,
/// not from an external source, preventing provider impersonation.
#[test]
fn endpoint_provider_impersonation_rejected() {
    let (attacker_key, _attacker_did) = make_keypair();
    let (_victim_key, victim_did) = make_keypair();

    // Create endpoint claiming to be from the victim
    let mut ep = make_endpoint(&victim_did, ScopeLevel::Org);

    // Sign with attacker's key
    sign_service_endpoint(&mut ep, &attacker_key);

    // Verification extracts the public key from ep.provider (victim_did)
    // and checks the signature (which was made with attacker_key) -- must fail
    let result = verify_service_endpoint(&ep);
    assert!(
        result.is_err(),
        "Endpoint signed by attacker but claiming victim provider DID must fail verification"
    );
}

/// Attack: An attacker creates a response with an empty endpoints list,
/// attempting to waste bandwidth or cause downstream panics in consumers
/// that assume at least one endpoint exists. The `validate_service_response`
/// function must reject responses with no endpoints.
#[test]
fn empty_endpoints_response_rejected() {
    let (signing_key, did) = make_keypair();

    // Sign a response with zero endpoints
    let response = make_signed_response(
        "q-empty-eps",
        &signing_key,
        &did,
        vec![], // no endpoints
        ScopeLevel::Org,
        future_timestamp(),
    );

    // Signature is valid (empty list is a valid payload)
    assert!(
        verify_service_response(&response).is_ok(),
        "Precondition: signature valid even with empty endpoints"
    );

    // But validation rejects it (empty endpoints are useless/suspicious)
    let valid = validate_service_response(&response, &ScopeLevel::Federation);
    assert!(
        !valid,
        "Response with empty endpoints must be rejected by validation"
    );
}
