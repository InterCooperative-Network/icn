//! Integration tests for service endpoint query API (Issue #936)
//!
//! Tests the new GET /api/v1/services endpoint with filtering and sorting.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App};
use icn_gateway::api::services;
use icn_gateway::service_discovery_mgr::ServiceDiscoveryManager;
use icn_kernel_api::naming::{EndpointType, ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::{Endpoint, Signature};
use std::sync::Arc;

/// Helper to create a test service endpoint
fn make_endpoint(
    id: &str,
    provider: &str,
    scope: ScopeLevel,
    trust: f64,
    ttl: u64,
) -> ServiceEndpoint {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    ServiceEndpoint {
        service_id: id.to_string(),
        provider: provider.to_string(),
        endpoint_type: EndpointType::Http,
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        endpoints: vec![Endpoint::new("https", "example.com", 8080)],
        addresses: vec![],
        capabilities: vec!["read".to_string()],
        trust_threshold: trust,
        scope_visibility: scope,
        cell_id: None,
        ttl_secs: ttl,
        signature: Signature::new(vec![0; 64]),
        created_at: now,
        updated_at: now,
    }
}

#[actix_web::test]
async fn test_query_services_empty_result() {
    // Create empty manager
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query with no services registered
    let req = test::TestRequest::get()
        .uri("/services?scope=org")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 0);
    assert_eq!(body["endpoints"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_query_services_scope_filtering() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Register services at different scopes
    mgr.announce(make_endpoint(
        "local-svc",
        "did:icn:alice",
        ScopeLevel::Local,
        0.5,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "cell-svc",
        "did:icn:bob",
        ScopeLevel::Cell,
        0.5,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "org-svc",
        "did:icn:charlie",
        ScopeLevel::Org,
        0.5,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "fed-svc",
        "did:icn:dave",
        ScopeLevel::Federation,
        0.5,
        3600,
    ))
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query at Cell scope - should return Local and Cell, but not Org or Federation
    let req = test::TestRequest::get()
        .uri("/services?scope=cell")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 2);
    let endpoints = body["endpoints"].as_array().unwrap();
    let service_ids: Vec<&str> = endpoints
        .iter()
        .map(|e| e["service_id"].as_str().unwrap())
        .collect();
    assert!(service_ids.contains(&"local-svc"));
    assert!(service_ids.contains(&"cell-svc"));
    assert!(!service_ids.contains(&"org-svc"));
    assert!(!service_ids.contains(&"fed-svc"));

    // Query at Org scope - should include Local, Cell, and Org
    let req = test::TestRequest::get()
        .uri("/services?scope=org")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 3);
}

#[actix_web::test]
async fn test_query_services_trust_threshold_filter() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Register services with different trust thresholds
    mgr.announce(make_endpoint(
        "low-trust",
        "did:icn:alice",
        ScopeLevel::Org,
        0.2,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "med-trust",
        "did:icn:bob",
        ScopeLevel::Org,
        0.5,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "high-trust",
        "did:icn:charlie",
        ScopeLevel::Org,
        0.8,
        3600,
    ))
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query with min_trust=0.5 - should return med and high only
    let req = test::TestRequest::get()
        .uri("/services?scope=org&min_trust=0.5")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 2);
    let service_ids: Vec<&str> = body["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["service_id"].as_str().unwrap())
        .collect();
    assert!(service_ids.contains(&"med-trust"));
    assert!(service_ids.contains(&"high-trust"));
    assert!(!service_ids.contains(&"low-trust"));

    // Query with min_trust=0.8 - should return only high
    let req = test::TestRequest::get()
        .uri("/services?scope=org&min_trust=0.8")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(
        body["endpoints"][0]["service_id"].as_str().unwrap(),
        "high-trust"
    );
}

#[actix_web::test]
async fn test_query_services_stale_excluded() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Register a stale endpoint (TTL = 1 sec, updated_at in the past)
    let mut stale_ep = make_endpoint("stale-svc", "did:icn:alice", ScopeLevel::Org, 0.5, 1);
    stale_ep.updated_at = 1000; // Very old timestamp
    mgr.announce(stale_ep).await.unwrap();

    // Register a fresh endpoint
    mgr.announce(make_endpoint(
        "fresh-svc",
        "did:icn:bob",
        ScopeLevel::Org,
        0.5,
        3600,
    ))
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query should exclude stale endpoint
    let req = test::TestRequest::get()
        .uri("/services?scope=org")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(
        body["endpoints"][0]["service_id"].as_str().unwrap(),
        "fresh-svc"
    );
}

#[actix_web::test]
async fn test_query_services_sorting() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Register services with different scopes and trust levels
    mgr.announce(make_endpoint(
        "org-high",
        "did:icn:alice",
        ScopeLevel::Org,
        0.9,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "org-low",
        "did:icn:bob",
        ScopeLevel::Org,
        0.3,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "local-high",
        "did:icn:charlie",
        ScopeLevel::Local,
        0.9,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "local-low",
        "did:icn:dave",
        ScopeLevel::Local,
        0.3,
        3600,
    ))
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query at Org scope - should sort by scope (Local first), then by trust
    let req = test::TestRequest::get()
        .uri("/services?scope=org")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 4);

    let service_ids: Vec<&str> = body["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["service_id"].as_str().unwrap())
        .collect();

    // Expected order: Local scope first (sorted by trust desc), then Org scope (sorted by trust desc)
    assert_eq!(service_ids[0], "local-high"); // Local, trust 0.9
    assert_eq!(service_ids[1], "local-low"); // Local, trust 0.3
    assert_eq!(service_ids[2], "org-high"); // Org, trust 0.9
    assert_eq!(service_ids[3], "org-low"); // Org, trust 0.3
}

#[actix_web::test]
async fn test_query_services_by_service_id() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Register multiple services
    mgr.announce(make_endpoint(
        "svc-1",
        "did:icn:alice",
        ScopeLevel::Org,
        0.5,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "svc-2",
        "did:icn:bob",
        ScopeLevel::Org,
        0.5,
        3600,
    ))
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query for specific service_id
    let req = test::TestRequest::get()
        .uri("/services?service_id=svc-1&scope=org")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(
        body["endpoints"][0]["service_id"].as_str().unwrap(),
        "svc-1"
    );
}

#[actix_web::test]
async fn test_query_services_combined_filters() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Register services with various attributes
    mgr.announce(make_endpoint(
        "match-all",
        "did:icn:alice",
        ScopeLevel::Cell,
        0.7,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "wrong-scope",
        "did:icn:bob",
        ScopeLevel::Federation,
        0.7,
        3600,
    ))
    .await
    .unwrap();
    mgr.announce(make_endpoint(
        "low-trust",
        "did:icn:charlie",
        ScopeLevel::Cell,
        0.3,
        3600,
    ))
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(mgr.clone()))
            .service(web::scope("/services").configure(services::configure)),
    )
    .await;

    // Query with scope=cell and min_trust=0.5
    let req = test::TestRequest::get()
        .uri("/services?scope=cell&min_trust=0.5")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["count"], 1);
    assert_eq!(
        body["endpoints"][0]["service_id"].as_str().unwrap(),
        "match-all"
    );
}
