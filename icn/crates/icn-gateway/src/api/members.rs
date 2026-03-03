//! Member profile API endpoints

use actix_web::{get, put, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::commons_mgr::CommonsManager;
use crate::coop::{CoopManager, MemberRole};
use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::get_claims;
use crate::trust_mgr::TrustManager;
use icn_identity::Did;

/// Member profile response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MemberProfile {
    /// Member DID
    pub did: String,
    /// Display name (if available)
    pub name: Option<String>,
    /// Member role in the cooperative
    pub role: MemberRole,
    /// Timestamp when member joined
    pub joined_at: u64,
    /// Current position in the cooperative
    pub position: f64,
    /// Total number of transactions
    pub transaction_count: usize,
    /// Trust score from requester's perspective (None if unauthenticated or unavailable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_score: Option<f64>,
}

/// GET /v1/members/{coop_id}/{did} - Get member profile
///
/// Returns profile information for a member within a cooperative.
/// If authenticated, includes trust score from requester's perspective.
#[get("/members/{coop_id}/{did}")]
pub async fn get_member_profile(
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    coop_manager: web::Data<Arc<CoopManager>>,
    ledger_manager: web::Data<Arc<LedgerManager>>,
    trust_manager: web::Data<Arc<TrustManager>>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let (coop_id, did) = path.into_inner();

    // Parse DID
    let did_obj = did
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Get cooperative
    let coop = coop_manager.get_coop(&coop_id).await?;

    // Check if member exists and get role
    let member = coop
        .members
        .iter()
        .find(|m| m.did == did_obj)
        .ok_or_else(|| GatewayError::NotFound("Member not found in cooperative".to_string()))?;

    // Get position from ledger (using default unit "hours")
    let position = ledger_manager
        .get_position(&coop_id, &did_obj, "hours")
        .await
        .unwrap_or(0) as f64;

    // Get transaction count from history
    let history = ledger_manager
        .get_history(&coop_id, Some(&did_obj), 0, 1000)
        .await
        .unwrap_or_else(|_| Vec::new());
    let transaction_count = history.len();

    // Compute trust score if authenticated (from requester's perspective)
    let trust_score = if let Some(claims) = get_claims(&http_req) {
        if let Ok(requester_did) = claims.sub.parse::<Did>() {
            // Don't compute self-trust
            if requester_did != did_obj {
                Some(
                    trust_manager
                        .compute_trust_score_async(&requester_did, &did_obj)
                        .await,
                )
            } else {
                Some(1.0) // Self-trust is always 1.0
            }
        } else {
            None
        }
    } else {
        None
    };

    // Look up display name from CommonsHolderRecord
    let display_name = commons_manager
        .get_holder_by_did(&did_obj)
        .await
        .ok()
        .flatten()
        .and_then(|holder| holder.display_name);

    let profile = MemberProfile {
        did: did.clone(),
        name: display_name,
        role: member.role.clone(),
        joined_at: member.joined_at,
        position,
        transaction_count,
        trust_score,
    };

    Ok(HttpResponse::Ok().json(profile))
}

/// Request body for updating member profile
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateProfileRequest {
    /// Display name to set
    pub display_name: String,
}

/// PUT /v1/members/{coop_id}/{did}/profile - Update member profile
///
/// Self-service only: JWT subject must match the DID being edited.
#[put("/{coop_id}/{did}/profile")]
pub async fn update_member_profile(
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<UpdateProfileRequest>,
    coop_manager: web::Data<Arc<CoopManager>>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();

    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Self-service only: JWT sub must match the DID being edited
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;
    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::AuthenticationFailed(format!("Invalid caller DID: {e}")))?;
    if caller_did != did {
        return Err(GatewayError::Forbidden(
            "Can only update your own profile".to_string(),
        ));
    }

    // Verify member exists in coop
    let coop = coop_manager.get_coop(&coop_id).await?;
    if !coop.members.iter().any(|m| m.did == did) {
        return Err(GatewayError::NotFound(
            "Member not found in cooperative".to_string(),
        ));
    }

    // Validate display name
    let name = body.display_name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(GatewayError::BadRequest(
            "Display name must be 1-100 characters".to_string(),
        ));
    }

    commons_manager
        .update_display_name(&did, name.clone())
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to update display name: {e}")))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "did": did_str,
        "display_name": name
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use crate::coop::MemberRole;
    use actix_web::{test, App, HttpMessage};

    fn create_test_commons_manager() -> Arc<CommonsManager> {
        Arc::new(CommonsManager::new())
    }

    fn create_test_claims(did: &str, scopes: Vec<&str>) -> TokenClaims {
        TokenClaims {
            sub: did.to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            exp: 9999999999,
        }
    }

    #[actix_web::test]
    async fn test_get_member_profile() {
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());
        let commons_manager = create_test_commons_manager();

        // Create test coop with a real generated DID
        let coop_id = "test-coop";
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let did_str = did.to_string();
        let timestamp = icn_time::current_timestamp_secs();

        coop_manager
            .create_coop(
                coop_id.to_string(),
                "Test Coop".to_string(),
                did.clone(),
                timestamp,
            )
            .await
            .unwrap();

        let trust_manager = Arc::new(TrustManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(ledger_manager))
                .app_data(web::Data::new(trust_manager))
                .app_data(web::Data::new(commons_manager))
                .service(get_member_profile),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/members/{coop_id}/{did_str}"))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let profile: MemberProfile = test::read_body_json(resp).await;
        assert_eq!(profile.did, did_str);
        assert_eq!(profile.role, MemberRole::Steward);
        assert_eq!(profile.position, 0.0);
    }

    #[actix_web::test]
    async fn test_get_member_profile_not_found() {
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());
        let trust_manager = Arc::new(TrustManager::new());
        let commons_manager = create_test_commons_manager();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(ledger_manager))
                .app_data(web::Data::new(trust_manager))
                .app_data(web::Data::new(commons_manager))
                .service(get_member_profile),
        )
        .await;

        // Generate a valid DID for testing
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did_str = keypair.did().to_string();

        let req = test::TestRequest::get()
            .uri(&format!("/members/nonexistent/{did_str}"))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn test_update_member_profile_success() {
        let coop_manager = Arc::new(CoopManager::new());
        let commons_manager = create_test_commons_manager();

        let coop_id = "test-coop";
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let did_str = did.to_string();
        let timestamp = icn_time::current_timestamp_secs();

        coop_manager
            .create_coop(
                coop_id.to_string(),
                "Test".to_string(),
                did.clone(),
                timestamp,
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(commons_manager))
                .service(update_member_profile),
        )
        .await;

        let claims = create_test_claims(&did_str, vec!["coop:write"]);
        let req = test::TestRequest::put()
            .uri(&format!("/{coop_id}/{did_str}/profile"))
            .set_json(UpdateProfileRequest {
                display_name: "Alice".to_string(),
            })
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["display_name"], "Alice");
        assert_eq!(body["did"], did_str);
    }

    #[actix_web::test]
    async fn test_update_member_profile_forbidden_wrong_did() {
        let coop_manager = Arc::new(CoopManager::new());
        let commons_manager = create_test_commons_manager();

        let coop_id = "test-coop";
        let owner = icn_identity::KeyPair::generate().unwrap();
        let other = icn_identity::KeyPair::generate().unwrap();
        let timestamp = icn_time::current_timestamp_secs();

        coop_manager
            .create_coop(
                coop_id.to_string(),
                "Test".to_string(),
                owner.did().clone(),
                timestamp,
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(commons_manager))
                .service(update_member_profile),
        )
        .await;

        // Authenticated as `other` but trying to edit `owner`'s profile
        let claims = create_test_claims(&other.did().to_string(), vec!["coop:write"]);
        let req = test::TestRequest::put()
            .uri(&format!("/{coop_id}/{}/profile", owner.did()))
            .set_json(UpdateProfileRequest {
                display_name: "Hacked".to_string(),
            })
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_update_member_profile_bad_request_empty_name() {
        let coop_manager = Arc::new(CoopManager::new());
        let commons_manager = create_test_commons_manager();

        let coop_id = "test-coop";
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let did_str = did.to_string();
        let timestamp = icn_time::current_timestamp_secs();

        coop_manager
            .create_coop(
                coop_id.to_string(),
                "Test".to_string(),
                did.clone(),
                timestamp,
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(commons_manager))
                .service(update_member_profile),
        )
        .await;

        // Empty name (after trim)
        let claims = create_test_claims(&did_str, vec!["coop:write"]);
        let req = test::TestRequest::put()
            .uri(&format!("/{coop_id}/{did_str}/profile"))
            .set_json(UpdateProfileRequest {
                display_name: "   ".to_string(),
            })
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_update_member_profile_bad_request_too_long() {
        let coop_manager = Arc::new(CoopManager::new());
        let commons_manager = create_test_commons_manager();

        let coop_id = "test-coop";
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let did_str = did.to_string();
        let timestamp = icn_time::current_timestamp_secs();

        coop_manager
            .create_coop(
                coop_id.to_string(),
                "Test".to_string(),
                did.clone(),
                timestamp,
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(commons_manager))
                .service(update_member_profile),
        )
        .await;

        // Name exceeding 100 characters
        let claims = create_test_claims(&did_str, vec!["coop:write"]);
        let req = test::TestRequest::put()
            .uri(&format!("/{coop_id}/{did_str}/profile"))
            .set_json(UpdateProfileRequest {
                display_name: "A".repeat(101),
            })
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }
}
