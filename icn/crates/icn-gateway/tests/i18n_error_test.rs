//! Tests for i18n integration in Gateway error messages

#![allow(clippy::unwrap_used)] // Tests can use unwrap

use actix_web::ResponseError;
use icn_gateway::error::GatewayError;

// Note: rust-i18n is initialized in icn-gateway/src/lib.rs
// The macro expansion makes translations available globally

#[tokio::test]
async fn test_error_messages_use_i18n() {
    // Test AuthenticationFailed error uses i18n
    let err = GatewayError::AuthenticationFailed("invalid token".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should contain translated message
    assert!(json["error"].as_str().unwrap().contains("Authentication failed"));
    assert!(json["error"].as_str().unwrap().contains("invalid token"));
    assert_eq!(json["code"], "AUTHENTICATION_FAILED");
}

#[tokio::test]
async fn test_authorization_failed_translation() {
    let err = GatewayError::AuthorizationFailed("insufficient permissions".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json["error"].as_str().unwrap().contains("Authorization failed"));
    assert!(json["error"].as_str().unwrap().contains("insufficient permissions"));
    assert_eq!(json["code"], "AUTHORIZATION_FAILED");
}

#[tokio::test]
async fn test_not_found_translation() {
    let err = GatewayError::NotFound("user:123".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json["error"].as_str().unwrap().contains("Resource not found"));
    assert!(json["error"].as_str().unwrap().contains("user:123"));
    assert_eq!(json["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_rate_limit_translation() {
    let err = GatewayError::RateLimitExceeded("did:icn:abc123".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json["error"].as_str().unwrap().contains("Rate limit exceeded"));
    assert!(json["error"].as_str().unwrap().contains("did:icn:abc123"));
    assert_eq!(json["code"], "RATE_LIMIT_EXCEEDED");
}

#[tokio::test]
async fn test_internal_error_sanitization() {
    // Internal errors should NOT expose implementation details
    let err = GatewayError::InternalError("database connection failed: localhost:5432".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should show generic message, NOT the internal details
    assert_eq!(json["error"], "Internal server error");
    assert!(!json["error"].as_str().unwrap().contains("database"));
    assert!(!json["error"].as_str().unwrap().contains("localhost"));
    // Internal errors don't have error codes
    assert!(json.get("code").is_none());
}

#[tokio::test]
async fn test_service_unavailable_translation() {
    let err = GatewayError::ServiceUnavailable("maintenance".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["error"], "Service temporarily unavailable");
    assert_eq!(json["code"], "SERVICE_UNAVAILABLE");
}

#[tokio::test]
async fn test_bad_request_translation() {
    let err = GatewayError::BadRequest("missing field: amount".to_string());
    let response = err.error_response();
    let body = actix_web::body::to_bytes(response.into_body())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json["error"].as_str().unwrap().contains("Invalid request"));
    assert!(json["error"].as_str().unwrap().contains("missing field: amount"));
    assert_eq!(json["code"], "BAD_REQUEST");
}

#[test]
fn test_error_code_stability() {
    // Error codes must remain stable for SDK clients
    // Only the human-readable messages should change with locale
    assert_eq!(
        GatewayError::AuthenticationFailed("test".to_string()).error_code(),
        Some("AUTHENTICATION_FAILED")
    );
    assert_eq!(
        GatewayError::NotFound("test".to_string()).error_code(),
        Some("NOT_FOUND")
    );
    assert_eq!(
        GatewayError::RateLimitExceeded("test".to_string()).error_code(),
        Some("RATE_LIMIT_EXCEEDED")
    );
    // Internal errors don't expose codes
    assert_eq!(
        GatewayError::InternalError("test".to_string()).error_code(),
        None
    );
}
