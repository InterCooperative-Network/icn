//! Identity API endpoints
//!
//! Public API for DID resolution and verification, enabling federated identity
//! lookups across cooperatives.

use actix_web::{get, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::sync::Arc;

use crate::error::{GatewayError, Result};
use crate::federation_mgr::FederationManager;
use icn_identity::Did;

// ============================================================================
// Response Models
// ============================================================================

/// Response for DID resolution
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DidResolutionResponse {
    /// The resolved DID string
    pub did: String,

    /// Whether the DID is valid and parseable
    pub valid: bool,

    /// The cooperative ID this DID belongs to (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coop_id: Option<String>,

    /// Whether the DID has attestations in this cooperative
    pub has_attestations: bool,

    /// Number of valid attestations (if authenticated request)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation_count: Option<usize>,
}

/// Query parameters for DID resolution
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveQuery {
    /// Include attestation details (requires additional lookup)
    #[serde(default)]
    pub include_attestations: bool,
}

// ============================================================================
// Public Endpoints (No Auth Required)
// ============================================================================

/// GET /identity/resolve/{did} - Resolve and verify a DID
///
/// This is a PUBLIC endpoint that allows remote cooperatives to verify
/// that a DID is valid and recognized by this cooperative.
///
/// Returns:
/// - 200: DID is valid (may or may not have attestations)
/// - 400: DID format is invalid
#[get("/resolve/{did}")]
pub async fn resolve_did(
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
    query: web::Query<ResolveQuery>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();

    // Validate DID format
    let did: Did = match did_str.parse() {
        Ok(d) => d,
        Err(e) => {
            return Err(GatewayError::BadRequest(format!("Invalid DID format: {e}")));
        }
    };

    // Get own cooperative info if available
    let own_coop_id = fed_mgr.get_own_info().await.ok().map(|info| info.coop_id);

    // Check for attestations if requested
    let (has_attestations, attestation_count) = if query.include_attestations {
        match fed_mgr.get_attestations_for(&did).await {
            Ok(attestations) => (!attestations.is_empty(), Some(attestations.len())),
            Err(_) => (false, Some(0)),
        }
    } else {
        // Quick check - just verify DID is valid, don't look up attestations
        (false, None)
    };

    Ok(HttpResponse::Ok().json(DidResolutionResponse {
        did: did.to_string(),
        valid: true,
        coop_id: own_coop_id,
        has_attestations,
        attestation_count,
    }))
}

/// GET /identity/health - Simple health check for identity service
#[get("/health")]
pub async fn identity_health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "service": "identity"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[actix_web::test]
    async fn test_identity_health() {
        let fed_mgr = Arc::new(FederationManager::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr))
                .service(web::scope("/identity").service(identity_health)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/identity/health")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "identity");
    }

    #[actix_web::test]
    async fn test_resolve_valid_did() {
        let fed_mgr = Arc::new(FederationManager::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr))
                .service(web::scope("/identity").service(resolve_did)),
        )
        .await;

        let did = test_did();
        let req = test::TestRequest::get()
            .uri(&format!("/identity/resolve/{did}"))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: DidResolutionResponse = test::read_body_json(resp).await;
        assert!(body.valid);
        assert_eq!(body.did, did.to_string());
    }

    #[actix_web::test]
    async fn test_resolve_invalid_did() {
        let fed_mgr = Arc::new(FederationManager::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr))
                .service(web::scope("/identity").service(resolve_did)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/identity/resolve/not-a-valid-did")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_resolve_with_attestations_query() {
        let fed_mgr = Arc::new(FederationManager::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr))
                .service(web::scope("/identity").service(resolve_did)),
        )
        .await;

        let did = test_did();
        let req = test::TestRequest::get()
            .uri(&format!(
                "/identity/resolve/{did}?include_attestations=true"
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: DidResolutionResponse = test::read_body_json(resp).await;
        assert!(body.valid);
        assert!(!body.has_attestations); // No attestations stored
        assert_eq!(body.attestation_count, Some(0));
    }
}
