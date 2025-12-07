//! Federation Crate Integration Tests
//!
//! These tests validate the full federation stack:
//! 1. Cross-cooperative registry operations
//! 2. Trust bridging with attestations
//! 3. Clearing agreements and settlement
//! 4. Federated DID resolution
//! 5. Federation gossip protocol

use ed25519_dalek::SigningKey;
use icn_federation::types::{CooperativeInfo, CurrencyInfo, Vouch};
use icn_federation::{
    AttestationStore, BilateralClearingAgreement, ClearingManager, CooperativeRegistry,
    EvidenceSummary, FederatedDidResolver, FederatedTrustAttestation, FederationPolicy,
    SettlementInterval, TrustContext,
};
use icn_identity::{Did, KeyPair};
use icn_store::{SledStore, Store};
use rand::rngs::OsRng;
use std::sync::Arc;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

// === Test Helpers ===

fn test_did() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

fn create_test_store() -> Arc<dyn Store> {
    Arc::new(SledStore::temporary().unwrap())
}

fn create_coop_info(id: &str, name: &str, policy: FederationPolicy) -> CooperativeInfo {
    CooperativeInfo::new(id.to_string(), name.to_string(), test_did(), policy)
}

// === Cross-Cooperative Registry Integration Tests ===

#[test]
fn test_multi_coop_federation_workflow() {
    // Scenario: Three cooperatives joining a federation with vouching policy

    // Create the anchor cooperative (establishes federation)
    let store1 = create_test_store();
    let anchor_coop = create_coop_info("anchor-coop", "Anchor Cooperative", FederationPolicy::Open);
    let anchor_registry = CooperativeRegistry::new(store1, anchor_coop).unwrap();

    // Anchor coop has open policy - register food coop
    let food_coop = create_coop_info("food-coop", "Food Cooperative", FederationPolicy::Open);
    anchor_registry.register(food_coop.clone()).unwrap();

    // Now set vouched policy requiring 1 vouch using update_own_info
    let mut own_info = anchor_registry.own_coop_info();
    own_info.federation_policy = FederationPolicy::Vouched { min_vouches: 1 };
    anchor_registry.update_own_info(own_info).unwrap();

    // New coop tries to join - should need vouches
    let new_coop = create_coop_info("new-coop", "New Cooperative", FederationPolicy::Open);
    let result = anchor_registry.register(new_coop.clone());
    assert!(result.is_err()); // Should fail without vouches

    // Food coop vouches for new coop
    let vouch = Vouch::new(
        "food-coop".to_string(),
        test_did(),
        "new-coop".to_string(),
        0.9,
    );
    anchor_registry.add_vouch(&vouch).unwrap();

    // Now new coop should be able to join
    anchor_registry.register(new_coop).unwrap();

    // Verify all three coops are federated
    let federated_coops = anchor_registry.list().unwrap();
    assert_eq!(federated_coops.len(), 2); // food-coop and new-coop (not ourselves)
}

#[test]
fn test_registry_persistence_across_restarts() {
    let store = create_test_store();

    // First "session" - create registry and add cooperatives
    {
        let own_info = create_coop_info("main-coop", "Main Cooperative", FederationPolicy::Open);
        let registry = CooperativeRegistry::new(store.clone(), own_info).unwrap();

        // Register multiple coops
        for i in 1..=5 {
            let coop = create_coop_info(
                &format!("coop-{i}"),
                &format!("Cooperative {i}"),
                FederationPolicy::Open,
            );
            registry.register(coop).unwrap();
        }

        assert_eq!(registry.count(), 5);
    }

    // Second "session" - new registry instance should load existing data
    {
        let own_info = create_coop_info("main-coop", "Main Cooperative", FederationPolicy::Open);
        let registry = CooperativeRegistry::new(store.clone(), own_info).unwrap();

        // All cooperatives should be loaded from persistent storage
        assert_eq!(registry.count(), 5);

        // Verify specific cooperative exists
        let coop3 = registry.get("coop-3").unwrap();
        assert!(coop3.is_some());
        assert_eq!(coop3.unwrap().name, "Cooperative 3");
    }
}

#[test]
fn test_find_coops_by_capability_and_currency() {
    let store = create_test_store();
    let own_info = create_coop_info("test-coop", "Test Cooperative", FederationPolicy::Open);
    let registry = CooperativeRegistry::new(store, own_info).unwrap();

    // Register coops with different capabilities
    let compute_coop = create_coop_info(
        "compute-coop",
        "Compute Cooperative",
        FederationPolicy::Open,
    )
    .with_capability("compute")
    .with_capability("storage");

    let clearing_coop = create_coop_info(
        "clearing-coop",
        "Clearing Cooperative",
        FederationPolicy::Open,
    )
    .with_capability("clearing")
    .with_currency(CurrencyInfo::new("hours", "Hours", 2));

    let full_coop = create_coop_info(
        "full-coop",
        "Full Service Cooperative",
        FederationPolicy::Open,
    )
    .with_capability("compute")
    .with_capability("clearing")
    .with_currency(CurrencyInfo::new("hours", "Hours", 2))
    .with_currency(CurrencyInfo::new("credits", "Credits", 2));

    registry.register(compute_coop).unwrap();
    registry.register(clearing_coop).unwrap();
    registry.register(full_coop).unwrap();

    // Find by capability
    let compute_coops = registry.find_by_capability("compute");
    assert_eq!(compute_coops.len(), 2); // compute-coop and full-coop

    let clearing_coops = registry.find_by_capability("clearing");
    assert_eq!(clearing_coops.len(), 2); // clearing-coop and full-coop

    // Find by currency
    let hours_coops = registry.find_by_currency("hours");
    assert_eq!(hours_coops.len(), 2); // clearing-coop and full-coop

    let credits_coops = registry.find_by_currency("credits");
    assert_eq!(credits_coops.len(), 1); // only full-coop
}

#[test]
fn test_stale_cooperative_detection() {
    let store = create_test_store();
    let own_info = create_coop_info("test-coop", "Test Cooperative", FederationPolicy::Open);
    let registry = CooperativeRegistry::new(store, own_info).unwrap();

    // Register coop with old last_seen
    let mut old_coop = create_coop_info("old-coop", "Old Cooperative", FederationPolicy::Open);
    old_coop.last_seen = 1000; // Very old timestamp

    // Need to set policy to open temporarily for this test
    registry.register(old_coop).unwrap();

    // Register coop with recent last_seen
    let recent_coop = create_coop_info("recent-coop", "Recent Cooperative", FederationPolicy::Open);
    registry.register(recent_coop).unwrap();

    // Find stale coops (not seen in last 30 days)
    let max_age = 30 * 24 * 60 * 60; // 30 days in seconds
    let stale = registry.find_stale(max_age);

    // old-coop should be stale, recent-coop should not
    assert!(stale.contains(&"old-coop".to_string()));
    assert!(!stale.contains(&"recent-coop".to_string()));
}

// === Trust Bridging Integration Tests ===

#[test]
fn test_attestation_signing_and_verification() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    // Create attestation with evidence
    let mut attestation = FederatedTrustAttestation::new(
        "food-coop".to_string(),
        test_did(),
        test_did(), // member being attested
        0.85,
        TrustContext::Economic,
        30 * 24 * 60 * 60, // 30 days expiry
    )
    .with_evidence(
        EvidenceSummary::new("transactions", "Total successful transactions").with_count(150),
    )
    .with_evidence(EvidenceSummary::new("disputes", "Open disputes").with_count(0))
    .with_evidence(
        EvidenceSummary::new("membership_days", "Days as active member").with_count(365),
    );

    // Sign the attestation
    attestation.sign(&signing_key).unwrap();
    assert!(!attestation.signature.is_empty());
    assert!(attestation.is_valid());

    // Verify signature
    let verified = attestation.verify(&verifying_key).unwrap();
    assert!(verified);

    // Tamper with attestation - verification should fail
    let mut tampered = attestation.clone();
    tampered.trust_score = 0.5; // Changed!
    let tampered_verified = tampered.verify(&verifying_key).unwrap();
    assert!(!tampered_verified);
}

#[test]
fn test_attestation_store_operations() {
    let store = create_test_store();
    let attestation_store = AttestationStore::new(store);

    let signing_key = SigningKey::generate(&mut OsRng);
    let source_coop_did = test_did();
    let member_did = test_did();

    // Create and sign attestation
    let mut attestation = FederatedTrustAttestation::new(
        "food-coop".to_string(),
        source_coop_did.clone(),
        member_did.clone(),
        0.9,
        TrustContext::General,
        30 * 24 * 60 * 60,
    );
    attestation.sign(&signing_key).unwrap();

    // Store attestation
    attestation_store
        .store_attestation(attestation.clone())
        .unwrap();

    // Retrieve by member
    let retrieved = attestation_store.get_attestations_for(&member_did).unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].trust_score, 0.9);

    // Retrieve by source cooperative
    let by_coop = attestation_store
        .get_attestations_from("food-coop")
        .unwrap();
    assert_eq!(by_coop.len(), 1);

    // Store another attestation for same member from different coop
    let mut attestation2 = FederatedTrustAttestation::new(
        "tech-coop".to_string(),
        test_did(),
        member_did.clone(),
        0.8,
        TrustContext::Economic,
        30 * 24 * 60 * 60,
    );
    attestation2
        .sign(&SigningKey::generate(&mut OsRng))
        .unwrap();
    attestation_store.store_attestation(attestation2).unwrap();

    // Member should now have 2 attestations
    let all_attestations = attestation_store.get_attestations_for(&member_did).unwrap();
    assert_eq!(all_attestations.len(), 2);
}

#[test]
fn test_attestation_expiry() {
    // Create an attestation with a non-zero TTL first
    let mut attestation = FederatedTrustAttestation::new(
        "food-coop".to_string(),
        test_did(),
        test_did(),
        0.85,
        TrustContext::Economic,
        1, // 1 second TTL
    );

    // Manually set expires_at to a past timestamp to ensure it's expired
    attestation.expires_at = 1; // Unix epoch + 1 second (definitely in the past)

    // Should be expired
    assert!(attestation.is_expired());
    assert!(!attestation.is_valid()); // Not valid because expired

    // Even with signature, expired attestation is not valid
    let signing_key = SigningKey::generate(&mut OsRng);
    attestation.sign(&signing_key).unwrap();
    assert!(!attestation.is_valid());
}

// === Clearing Agreement Integration Tests ===

#[test]
fn test_clearing_agreement_creation_and_rates() {
    let coop_a_did = test_did();
    let coop_b_did = test_did();

    let agreement = BilateralClearingAgreement::new(
        "agreement-001".to_string(),
        "food-coop".to_string(),
        coop_a_did.clone(),
        "tech-coop".to_string(),
        coop_b_did.clone(),
    )
    .with_rate("hours", "credits", 10.0) // 1 hour = 10 credits
    .with_rate("credits", "hours", 0.1) // 10 credits = 1 hour
    .with_interval(SettlementInterval::Weekly)
    .with_max_imbalance(5000);

    // Verify rates
    assert_eq!(agreement.get_rate("hours", "credits"), Some(10.0));
    assert_eq!(agreement.get_rate("credits", "hours"), Some(0.1));
    assert_eq!(agreement.get_rate("unknown", "currency"), None);

    // Not fully signed yet
    assert!(!agreement.is_fully_signed());
    assert!(!agreement.has_signed(&coop_a_did));
}

#[test]
fn test_clearing_manager_agreement_lifecycle() {
    let store = create_test_store();
    let manager = ClearingManager::new(store, "food-coop".to_string()).unwrap();

    let coop_a_did = test_did();
    let coop_b_did = test_did();

    // Create and add agreement
    let agreement = BilateralClearingAgreement::new(
        "agreement-001".to_string(),
        "food-coop".to_string(),
        coop_a_did,
        "tech-coop".to_string(),
        coop_b_did,
    )
    .with_rate("hours", "credits", 10.0)
    .with_max_imbalance(1000);

    let agreement_id = manager.create_agreement(agreement).unwrap();
    assert_eq!(agreement_id, "agreement-001");

    // Verify agreement exists
    let retrieved = manager.get_agreement(&agreement_id).unwrap();
    assert!(retrieved.is_some());

    // Check initial position
    let position = manager.calculate_position(&agreement_id).unwrap();
    assert_eq!(position.net_position(), 0);

    // List agreements
    let agreements = manager.list_agreements();
    assert_eq!(agreements.len(), 1);

    // Trigger settlement (even with zero balance)
    let report = manager.trigger_settlement(&agreement_id).unwrap();
    assert_eq!(report.agreement_id, "agreement-001");
    assert_eq!(report.net_settlement, 0);
}

#[test]
fn test_settlement_interval_durations() {
    assert_eq!(SettlementInterval::Daily.seconds(), Some(24 * 60 * 60));
    assert_eq!(SettlementInterval::Weekly.seconds(), Some(7 * 24 * 60 * 60));
    assert_eq!(
        SettlementInterval::Monthly.seconds(),
        Some(30 * 24 * 60 * 60)
    );
    assert_eq!(SettlementInterval::Manual.seconds(), None);
}

// === Federated DID Resolution Integration Tests ===

#[tokio::test]
async fn test_federated_did_resolution_workflow() {
    // Start mock server for partner gateway
    let mock_server = MockServer::start().await;

    let store = create_test_store();
    let own_info = create_coop_info("my-coop", "My Cooperative", FederationPolicy::Open);
    let registry = Arc::new(CooperativeRegistry::new(store, own_info).unwrap());

    // Create partner DID before setting up mock (so we can use it in response)
    let partner_did = test_did();
    let partner_did_str = partner_did.to_string();

    // Set up mock response for partner gateway
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/identity/resolve/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "did": partner_did_str,
            "valid": true,
            "coop_id": "partner-coop",
            "has_attestations": false
        })))
        .mount(&mock_server)
        .await;

    // Register partner cooperative with mock server as gateway
    let partner = create_coop_info(
        "partner-coop",
        "Partner Cooperative",
        FederationPolicy::Open,
    )
    .with_gateway(mock_server.uri());
    registry.register(partner).unwrap();

    let mut resolver = FederatedDidResolver::new(registry);
    resolver.set_own_coop_id("my-coop".to_string());

    // Resolve local DID (non-federated format)
    let local_did = test_did();
    let local_result = resolver.resolve(&local_did.to_string()).await.unwrap();
    assert_eq!(local_result.coop_id, Some("my-coop".to_string()));
    assert!(local_result.resolved_via.is_none());

    // Resolve own coop's federated DID
    let own_federated = FederatedDidResolver::construct_federated_did(&local_did, "my-coop");
    let own_result = resolver.resolve(&own_federated).await.unwrap();
    assert_eq!(own_result.coop_id, Some("my-coop".to_string()));
    assert!(own_result.resolved_via.is_none());

    // Resolve partner coop's federated DID (via mock gateway)
    let partner_federated =
        FederatedDidResolver::construct_federated_did(&partner_did, "partner-coop");
    let partner_result = resolver.resolve(&partner_federated).await.unwrap();
    assert_eq!(partner_result.coop_id, Some("partner-coop".to_string()));
    assert!(partner_result.resolved_via.is_some());

    // Unknown coop should fail
    let unknown_federated =
        FederatedDidResolver::construct_federated_did(&test_did(), "unknown-coop");
    let unknown_result = resolver.resolve(&unknown_federated).await;
    assert!(unknown_result.is_err());
}

#[tokio::test]
async fn test_did_resolution_caching() {
    let store = create_test_store();
    let own_info = create_coop_info("test-coop", "Test Cooperative", FederationPolicy::Open);
    let registry = Arc::new(CooperativeRegistry::new(store, own_info).unwrap());

    let resolver = FederatedDidResolver::new(registry);
    let did = test_did();
    let did_str = did.to_string();

    // Initial resolution - cache miss
    assert_eq!(resolver.cache_size(), 0);
    let _ = resolver.resolve(&did_str).await.unwrap();
    assert_eq!(resolver.cache_size(), 1);

    // Second resolution - cache hit
    let _ = resolver.resolve(&did_str).await.unwrap();
    assert_eq!(resolver.cache_size(), 1); // Still 1, used cache

    // Resolve different DID
    let did2 = test_did();
    let _ = resolver.resolve(&did2.to_string()).await.unwrap();
    assert_eq!(resolver.cache_size(), 2);

    // Clear cache
    resolver.clear_cache();
    assert_eq!(resolver.cache_size(), 0);
}

#[test]
fn test_federated_did_parsing() {
    // Valid federated DID
    let federated = "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    assert!(FederatedDidResolver::is_federated_did(federated));

    let (coop_id, key) = FederatedDidResolver::parse_federated_did(federated).unwrap();
    assert_eq!(coop_id, "food-coop");
    assert_eq!(key, "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");

    // Standard (non-federated) DID
    let standard = "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    assert!(!FederatedDidResolver::is_federated_did(standard));
    assert!(FederatedDidResolver::parse_federated_did(standard).is_none());

    // Invalid formats
    assert!(!FederatedDidResolver::is_federated_did("not-a-did"));
    assert!(!FederatedDidResolver::is_federated_did("did:other:method"));
}

// === Combined Federation Workflow Tests ===

#[tokio::test]
async fn test_full_federation_onboarding_workflow() {
    // Scenario: New cooperative joins federation, establishes trust, sets up clearing

    // Start mock server for new coop's gateway
    let mock_server = MockServer::start().await;

    // 1. Create federation anchor
    let anchor_store = create_test_store();
    let anchor_info = create_coop_info("anchor", "Anchor Coop", FederationPolicy::Open);
    let anchor_registry =
        Arc::new(CooperativeRegistry::new(anchor_store.clone(), anchor_info).unwrap());

    // 2. New coop applies to join
    let new_coop_did = test_did();
    let new_coop_info = create_coop_info("new-coop", "New Cooperative", FederationPolicy::Open)
        .with_gateway(mock_server.uri())
        .with_currency(CurrencyInfo::new("hours", "Hours", 2));

    anchor_registry.register(new_coop_info).unwrap();
    assert!(anchor_registry.is_federated("new-coop"));

    // 3. Anchor creates trust attestation for new coop's representative
    let anchor_key = SigningKey::generate(&mut OsRng);
    let mut attestation = FederatedTrustAttestation::new(
        "anchor".to_string(),
        test_did(),
        new_coop_did.clone(),
        0.7, // Initial trust score
        TrustContext::General,
        90 * 24 * 60 * 60, // 90 days
    )
    .with_evidence(EvidenceSummary::new(
        "verification",
        "Identity verified through documentation",
    ));
    attestation.sign(&anchor_key).unwrap();

    let attestation_store = AttestationStore::new(anchor_store.clone());
    attestation_store.store_attestation(attestation).unwrap();

    // 4. Setup clearing agreement
    let clearing_manager =
        ClearingManager::new(anchor_store.clone(), "anchor".to_string()).unwrap();
    let agreement = BilateralClearingAgreement::new(
        "anchor-new-clearing".to_string(),
        "anchor".to_string(),
        test_did(),
        "new-coop".to_string(),
        new_coop_did,
    )
    .with_rate("hours", "hours", 1.0) // Same currency
    .with_interval(SettlementInterval::Weekly);

    clearing_manager.create_agreement(agreement).unwrap();

    // 5. Verify complete setup
    // Create member DID and set up mock response before resolution
    let member_did = test_did();
    let member_did_str = member_did.to_string();

    Mock::given(method("GET"))
        .and(path_regex(r"/v1/identity/resolve/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "did": member_did_str,
            "valid": true,
            "coop_id": "new-coop",
            "has_attestations": false
        })))
        .mount(&mock_server)
        .await;

    let mut resolver = FederatedDidResolver::new(anchor_registry.clone());
    resolver.set_own_coop_id("anchor".to_string());

    // Can resolve new coop's DIDs
    let federated_did = FederatedDidResolver::construct_federated_did(&member_did, "new-coop");
    let resolution = resolver.resolve(&federated_did).await.unwrap();
    assert_eq!(resolution.coop_id, Some("new-coop".to_string()));

    // Attestations exist
    let member_attestations = attestation_store.get_attestations_from("anchor").unwrap();
    assert_eq!(member_attestations.len(), 1);

    // Clearing agreement active
    let agreements = clearing_manager.list_agreements();
    assert_eq!(agreements.len(), 1);
}

#[test]
fn test_vouch_chain_federation() {
    // Scenario: Cooperative C joins via vouches from A and B (who are already federated)

    let store = create_test_store();
    let own_info = create_coop_info(
        "federation-hub",
        "Federation Hub",
        FederationPolicy::Vouched { min_vouches: 2 },
    );
    let registry = CooperativeRegistry::new(store, own_info).unwrap();

    // First, register initial partners with open policy
    let mut own_info_open = registry.own_coop_info();
    own_info_open.federation_policy = FederationPolicy::Open;
    registry.update_own_info(own_info_open).unwrap();

    let coop_a = create_coop_info("coop-a", "Cooperative A", FederationPolicy::Open);
    let coop_b = create_coop_info("coop-b", "Cooperative B", FederationPolicy::Open);
    registry.register(coop_a).unwrap();
    registry.register(coop_b).unwrap();

    // Restore vouched policy
    let mut own_info_vouched = registry.own_coop_info();
    own_info_vouched.federation_policy = FederationPolicy::Vouched { min_vouches: 2 };
    registry.update_own_info(own_info_vouched).unwrap();

    // Cooperative C wants to join
    let coop_c = create_coop_info("coop-c", "Cooperative C", FederationPolicy::Open);

    // C can't join without vouches
    let result = registry.register(coop_c.clone());
    assert!(result.is_err());

    // A vouches for C
    let vouch_a = Vouch::new("coop-a".to_string(), test_did(), "coop-c".to_string(), 0.8);
    registry.add_vouch(&vouch_a).unwrap();

    // Still can't join - need 2 vouches
    let result = registry.register(coop_c.clone());
    assert!(result.is_err());

    // B vouches for C
    let vouch_b = Vouch::new("coop-b".to_string(), test_did(), "coop-c".to_string(), 0.9);
    registry.add_vouch(&vouch_b).unwrap();

    // Now C can join
    registry.register(coop_c).unwrap();
    assert!(registry.is_federated("coop-c"));

    // Verify vouch count
    let vouches = registry.get_vouches("coop-c").unwrap();
    assert_eq!(vouches.len(), 2);
}

#[test]
fn test_trust_context_variations() {
    // Test all trust contexts
    let contexts = [
        (TrustContext::Economic, "economic"),
        (TrustContext::Social, "social"),
        (TrustContext::Governance, "governance"),
        (TrustContext::General, "general"),
    ];

    for (context, expected_str) in contexts {
        assert_eq!(context.as_str(), expected_str);

        let attestation = FederatedTrustAttestation::new(
            "test-coop".to_string(),
            test_did(),
            test_did(),
            0.8,
            context,
            86400,
        );

        assert_eq!(attestation.trust_context, context);
    }
}

#[test]
fn test_cooperative_info_builder_pattern() {
    let coop = CooperativeInfo::new(
        "full-coop".to_string(),
        "Full Featured Cooperative".to_string(),
        test_did(),
        FederationPolicy::Open,
    )
    .with_gateway("https://gateway.example.com:8080".to_string())
    .with_gateway("https://backup.example.com:8080".to_string())
    .with_capability("compute")
    .with_capability("storage")
    .with_capability("clearing")
    .with_currency(CurrencyInfo::new("hours", "Community Hours", 2))
    .with_currency(CurrencyInfo::new("credits", "Service Credits", 4));

    assert_eq!(coop.gateway_endpoints.len(), 2);
    assert_eq!(coop.capabilities.len(), 3);
    assert_eq!(coop.currencies.len(), 2);
    assert!(coop.has_capability("compute"));
    assert!(coop.has_capability("clearing"));
    assert!(!coop.has_capability("unknown"));
}

// === Additional Edge Case Tests ===

#[test]
fn test_attestation_removal() {
    let store = create_test_store();
    let attestation_store = AttestationStore::new(store);

    let member_did = test_did();

    // Create and store attestation
    let attestation = FederatedTrustAttestation::new(
        "food-coop".to_string(),
        test_did(),
        member_did.clone(),
        0.85,
        TrustContext::Economic,
        30 * 24 * 60 * 60,
    );
    attestation_store.store_attestation(attestation).unwrap();

    // Verify it exists
    let retrieved = attestation_store.get_attestations_for(&member_did).unwrap();
    assert_eq!(retrieved.len(), 1);

    // Remove attestation
    attestation_store
        .remove_attestation(&member_did, "food-coop")
        .unwrap();

    // Verify it's gone
    let after_removal = attestation_store.get_attestations_for(&member_did).unwrap();
    assert_eq!(after_removal.len(), 0);
}

#[test]
fn test_clearing_manager_duplicate_agreement_error() {
    let store = create_test_store();
    let manager = ClearingManager::new(store, "food-coop".to_string()).unwrap();

    let agreement = BilateralClearingAgreement::new(
        "duplicate-test".to_string(),
        "food-coop".to_string(),
        test_did(),
        "tech-coop".to_string(),
        test_did(),
    );

    // First creation should succeed
    manager.create_agreement(agreement.clone()).unwrap();

    // Second creation should fail
    let result = manager.create_agreement(agreement);
    assert!(result.is_err());
}

#[test]
fn test_registry_cannot_register_self() {
    let store = create_test_store();
    let own_info = create_coop_info("my-coop", "My Cooperative", FederationPolicy::Open);
    let registry = CooperativeRegistry::new(store, own_info).unwrap();

    // Try to register ourselves - should fail
    let self_info = create_coop_info("my-coop", "My Cooperative", FederationPolicy::Open);
    let result = registry.register(self_info);
    assert!(result.is_err());
}

#[test]
fn test_registry_update_last_seen() {
    let store = create_test_store();
    let own_info = create_coop_info("test-coop", "Test Cooperative", FederationPolicy::Open);
    let registry = CooperativeRegistry::new(store, own_info).unwrap();

    // Register a partner
    let partner = create_coop_info(
        "partner-coop",
        "Partner Cooperative",
        FederationPolicy::Open,
    );
    registry.register(partner).unwrap();

    // Update last_seen
    let new_timestamp = 9999999999u64;
    registry
        .update_last_seen("partner-coop", new_timestamp)
        .unwrap();

    // Verify update
    let retrieved = registry.get("partner-coop").unwrap().unwrap();
    assert_eq!(retrieved.last_seen, new_timestamp);

    // Updating non-existent coop should fail
    let result = registry.update_last_seen("non-existent", new_timestamp);
    assert!(result.is_err());
}

#[test]
fn test_valid_attestations_filter() {
    let store = create_test_store();
    let attestation_store = AttestationStore::new(store);

    let member_did = test_did();

    // Create a valid (non-expired) attestation
    let valid_att = FederatedTrustAttestation::new(
        "food-coop".to_string(),
        test_did(),
        member_did.clone(),
        0.85,
        TrustContext::Economic,
        30 * 24 * 60 * 60, // 30 days - will be valid
    );
    attestation_store.store_attestation(valid_att).unwrap();

    // Create an expired attestation by manually setting expires_at to past
    let mut expired_att = FederatedTrustAttestation::new(
        "tech-coop".to_string(),
        test_did(),
        member_did.clone(),
        0.9,
        TrustContext::Social,
        1, // Create with some TTL first
    );
    expired_att.expires_at = 1; // Set to past (Unix epoch + 1 second)
    attestation_store.store_attestation(expired_att).unwrap();

    // Get all attestations - should be 2
    let all = attestation_store.get_attestations_for(&member_did).unwrap();
    assert_eq!(all.len(), 2);

    // Get valid attestations only - should be 1
    let valid = attestation_store
        .get_valid_attestations_for(&member_did)
        .unwrap();
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].source_coop_id, "food-coop");
}
