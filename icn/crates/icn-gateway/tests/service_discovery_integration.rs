#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for service discovery API
//!
//! Tests the announce, discover, and withdraw endpoints with focus on:
//! - Timestamp validation (bounds checking)
//! - Response format (includes endpoint_type, addresses, updated_at)
//! - Ed25519 signature verification

use actix_web::{test, App};
use icn_gateway::api::services;
use icn_gateway::service_discovery_mgr::ServiceDiscoveryManager;
use std::sync::Arc;

/// Helper to create a valid Ed25519 signature for testing
fn create_test_signature() -> String {
    // For testing, we use a dummy signature
    // In production, this would be computed from the signing payload
    hex::encode(vec![0u8; 64])
}

#[actix_web::test]
async fn test_announce_rejects_far_future_timestamps() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());
    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(mgr.clone()))
            .service(actix_web::web::scope("/api/services").configure(services::configure)),
    )
    .await;

    // Get current time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create request with timestamp 2 hours in the future (should be rejected)
    let far_future = now + 7200; // 2 hours

    let payload = serde_json::json!({
        "service_id": "test-svc-1",
        "provider": "did:icn:test",
        "endpoint_type": "http",
        "service_type": "ledger",
        "service_version": "1.0",
        "endpoints": [
            {
                "protocol": "https",
                "host": "example.com",
                "port": 8080
            }
        ],
        "addresses": ["/ip4/127.0.0.1/tcp/8080"],
        "capabilities": ["read"],
        "trust_threshold": 0.1,
        "scope_visibility": "org",
        "ttl_secs": 3600,
        "created_at": far_future,
        "updated_at": far_future,
        "signature": create_test_signature()
    });

    let req = test::TestRequest::post()
        .uri("/api/services/announce")
        .set_json(&payload)
        .to_request();

    let resp = test::call_service(&app, req).await;

    // Should reject with 400 Bad Request
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::BAD_REQUEST,
        "Should reject timestamps too far in future"
    );

    let body = test::read_body(resp).await;
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("too far in the future") || body_str.contains("Timestamps"),
        "Error message should mention future timestamps: {}",
        body_str
    );
}

#[actix_web::test]
async fn test_announce_rejects_updated_at_before_created_at() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());
    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(mgr.clone()))
            .service(actix_web::web::scope("/api/services").configure(services::configure)),
    )
    .await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create request with updated_at before created_at (should be rejected)
    let payload = serde_json::json!({
        "service_id": "test-svc-2",
        "provider": "did:icn:test",
        "endpoint_type": "grpc",
        "service_type": "governance",
        "service_version": "2.0",
        "endpoints": [
            {
                "protocol": "grpc",
                "host": "localhost",
                "port": 50051
            }
        ],
        "addresses": [],
        "capabilities": ["propose", "vote"],
        "trust_threshold": 0.5,
        "scope_visibility": "federation",
        "ttl_secs": 7200,
        "created_at": now,
        "updated_at": now - 100, // 100 seconds before created_at
        "signature": create_test_signature()
    });

    let req = test::TestRequest::post()
        .uri("/api/services/announce")
        .set_json(&payload)
        .to_request();

    let resp = test::call_service(&app, req).await;

    // Should reject with 400 Bad Request
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::BAD_REQUEST,
        "Should reject updated_at before created_at"
    );

    let body = test::read_body(resp).await;
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("updated_at") && body_str.contains("created_at"),
        "Error message should mention timestamp ordering: {}",
        body_str
    );
}

#[actix_web::test]
async fn test_announce_accepts_valid_timestamps() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());
    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(mgr.clone()))
            .service(actix_web::web::scope("/api/services").configure(services::configure)),
    )
    .await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create request with valid timestamps (within 1 hour window)
    let payload = serde_json::json!({
        "service_id": "test-svc-3",
        "provider": "did:icn:test",
        "endpoint_type": "websocket",
        "service_type": "compute",
        "service_version": "1.2",
        "endpoints": [
            {
                "protocol": "wss",
                "host": "compute.example.com",
                "port": 9000,
                "path": "/ws"
            }
        ],
        "addresses": ["/dns/compute.example.com/tcp/9000/wss"],
        "capabilities": ["execute", "stream"],
        "trust_threshold": 0.7,
        "scope_visibility": "cell",
        "cell_id": hex::encode([1u8; 32]),
        "ttl_secs": 1800,
        "created_at": now - 10,
        "updated_at": now,
        "signature": create_test_signature()
    });

    let req = test::TestRequest::post()
        .uri("/api/services/announce")
        .set_json(&payload)
        .to_request();

    let resp = test::call_service(&app, req).await;

    // Note: Will fail signature verification but should pass timestamp validation
    // We expect 400 for bad signature, not for timestamp issues
    if resp.status() == actix_web::http::StatusCode::BAD_REQUEST {
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Should fail on signature, not timestamp
        assert!(
            body_str.contains("signature") || body_str.contains("Invalid endpoint signature"),
            "Should fail on signature verification, not timestamp: {}",
            body_str
        );
        assert!(
            !body_str.contains("timestamp") && !body_str.contains("future"),
            "Should not complain about timestamps: {}",
            body_str
        );
    }
}

#[actix_web::test]
async fn test_discover_response_includes_new_fields() {
    // This test validates that the ServiceEndpointResponse includes
    // endpoint_type, addresses, and updated_at fields

    let mgr = Arc::new(ServiceDiscoveryManager::new());

    // Manually register a valid endpoint (bypassing signature verification)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let endpoint = icn_kernel_api::naming::ServiceEndpoint {
        service_id: "test-discovery-1".to_string(),
        provider: "did:icn:alice".to_string(),
        endpoint_type: icn_kernel_api::naming::EndpointType::Quic,
        service_type: icn_kernel_api::naming::ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        endpoints: vec![icn_kernel_api::types::Endpoint::new(
            "quic",
            "ledger.coop.local",
            4433,
        )],
        addresses: vec![
            "/ip4/10.0.0.5/udp/4433/quic".to_string(),
            "/dns/ledger.coop.local/udp/4433/quic".to_string(),
        ],
        capabilities: vec!["read".to_string(), "write".to_string(), "audit".to_string()],
        trust_threshold: 0.3,
        scope_visibility: icn_kernel_api::scope::ScopeLevel::Org,
        cell_id: None,
        ttl_secs: 3600,
        signature: icn_kernel_api::types::Signature::new(vec![0; 64]),
        created_at: now - 500,
        updated_at: now - 50,
    };

    mgr.announce(endpoint).await.unwrap();

    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(mgr.clone()))
            .service(actix_web::web::scope("/api/services").configure(services::configure)),
    )
    .await;

    // Query the specific service
    let req = test::TestRequest::get()
        .uri("/api/services/test-discovery-1")
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::OK,
        "Should successfully retrieve the service"
    );

    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify new fields are present
    assert_eq!(
        json["endpoint_type"].as_str().unwrap(),
        "quic",
        "Response should include endpoint_type field"
    );

    assert!(
        json["addresses"].is_array(),
        "Response should include addresses array"
    );
    assert_eq!(
        json["addresses"].as_array().unwrap().len(),
        2,
        "Should have 2 addresses"
    );
    assert_eq!(
        json["addresses"][0].as_str().unwrap(),
        "/ip4/10.0.0.5/udp/4433/quic"
    );
    assert_eq!(
        json["addresses"][1].as_str().unwrap(),
        "/dns/ledger.coop.local/udp/4433/quic"
    );

    assert!(
        json["updated_at"].is_u64() || json["updated_at"].is_number(),
        "Response should include updated_at field"
    );
    assert_eq!(
        json["updated_at"].as_u64().unwrap(),
        now - 50,
        "updated_at should match the endpoint value"
    );

    // Verify existing fields still work
    assert_eq!(json["service_id"].as_str().unwrap(), "test-discovery-1");
    assert_eq!(json["provider"].as_str().unwrap(), "did:icn:alice");
    assert_eq!(json["created_at"].as_u64().unwrap(), now - 500);
    assert_eq!(json["capabilities"].as_array().unwrap().len(), 3);
}

#[actix_web::test]
async fn test_discover_all_includes_new_fields() {
    let mgr = Arc::new(ServiceDiscoveryManager::new());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Register multiple endpoints with different endpoint types
    for (i, endpoint_type) in [
        icn_kernel_api::naming::EndpointType::Http,
        icn_kernel_api::naming::EndpointType::Grpc,
        icn_kernel_api::naming::EndpointType::WebSocket,
    ]
    .iter()
    .enumerate()
    {
        let endpoint = icn_kernel_api::naming::ServiceEndpoint {
            service_id: format!("multi-svc-{}", i),
            provider: format!("did:icn:node{}", i),
            endpoint_type: endpoint_type.clone(),
            service_type: icn_kernel_api::naming::ServiceType {
                name: "multi-service".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![],
            addresses: vec![format!("/ip4/10.0.0.{}/tcp/8080", i + 1)],
            capabilities: vec![],
            trust_threshold: 0.0,
            scope_visibility: icn_kernel_api::scope::ScopeLevel::Commons,
            cell_id: None,
            ttl_secs: 3600,
            signature: icn_kernel_api::types::Signature::new(vec![0; 64]),
            created_at: now,
            updated_at: now + (i as u64 * 10),
        };
        mgr.announce(endpoint).await.unwrap();
    }

    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(mgr.clone()))
            .service(actix_web::web::scope("/api/services").configure(services::configure)),
    )
    .await;

    // Query all services
    let req = test::TestRequest::get()
        .uri("/api/services/discover?scope=commons")
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["count"].as_u64().unwrap(), 3);

    let endpoints = json["endpoints"].as_array().unwrap();

    // Verify each endpoint has the new fields
    for endpoint in endpoints {
        assert!(
            endpoint["endpoint_type"].is_string(),
            "Each endpoint should have endpoint_type"
        );
        assert!(
            endpoint["addresses"].is_array(),
            "Each endpoint should have addresses array"
        );
        assert!(
            endpoint["updated_at"].is_number(),
            "Each endpoint should have updated_at"
        );
    }

    // Verify different endpoint types are represented
    let endpoint_types: Vec<&str> = endpoints
        .iter()
        .map(|e| e["endpoint_type"].as_str().unwrap())
        .collect();

    assert!(endpoint_types.contains(&"http"));
    assert!(endpoint_types.contains(&"grpc"));
    assert!(endpoint_types.contains(&"websocket"));
}
