//! Member profile API endpoints

use actix_web::{get, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::coop::{CoopManager, MemberRole};
use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::get_claims;
use crate::trust_mgr::TrustManager;
use icn_identity::Did;

/// Member profile response
#[derive(Debug, Serialize, Deserialize)]
pub struct MemberProfile {
    /// Member DID
    pub did: String,
    /// Display name (if available)
    pub name: Option<String>,
    /// Member role in the cooperative
    pub role: MemberRole,
    /// Timestamp when member joined
    pub joined_at: u64,
    /// Current balance in the cooperative
    pub balance: f64,
    /// Total number of transactions
    pub transaction_count: usize,
    /// Trust score (placeholder for future integration)
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
) -> Result<HttpResponse> {
    let (coop_id, did) = path.into_inner();

    // Parse DID
    let did_obj = did
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Get cooperative
    let coop = coop_manager.get_coop(&coop_id)?;

    // Check if member exists and get role
    let member = coop
        .members
        .iter()
        .find(|m| m.did == did_obj)
        .ok_or_else(|| GatewayError::NotFound("Member not found in cooperative".to_string()))?;

    // Get balance from ledger (using default currency "hours")
    let balance = ledger_manager
        .get_balance(&coop_id, &did_obj, "hours")
        .unwrap_or(0) as f64;

    // Get transaction count from history
    let history = ledger_manager
        .get_history(&coop_id, Some(&did_obj), 0, 1000)
        .unwrap_or_else(|_| Vec::new());
    let transaction_count = history.len();

    // Compute trust score if authenticated (from requester's perspective)
    let trust_score = if let Some(claims) = get_claims(&http_req) {
        if let Ok(requester_did) = claims.sub.parse::<Did>() {
            // Don't compute self-trust
            if requester_did != did_obj {
                Some(trust_manager.compute_trust_score(&requester_did, &did_obj))
            } else {
                Some(1.0) // Self-trust is always 1.0
            }
        } else {
            None
        }
    } else {
        None
    };

    let profile = MemberProfile {
        did: did.clone(),
        name: None, // TODO: Integrate with identity system for display names
        role: member.role.clone(),
        joined_at: member.joined_at,
        balance,
        transaction_count,
        trust_score,
    };

    Ok(HttpResponse::Ok().json(profile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coop::MemberRole;
    use actix_web::{test, App};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[actix_web::test]
    async fn test_get_member_profile() {
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());

        // Create test coop with a real generated DID
        let coop_id = "test-coop";
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let did_str = did.to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        coop_manager
            .create_coop(
                coop_id.to_string(),
                "Test Coop".to_string(),
                did.clone(),
                timestamp,
            )
            .unwrap();

        let trust_manager = Arc::new(TrustManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(ledger_manager))
                .app_data(web::Data::new(trust_manager))
                .service(get_member_profile),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/members/{}/{}", coop_id, did_str))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let profile: MemberProfile = test::read_body_json(resp).await;
        assert_eq!(profile.did, did_str);
        assert_eq!(profile.role, MemberRole::Steward);
        assert_eq!(profile.balance, 0.0);
    }

    #[actix_web::test]
    async fn test_get_member_profile_not_found() {
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());
        let trust_manager = Arc::new(TrustManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(ledger_manager))
                .app_data(web::Data::new(trust_manager))
                .service(get_member_profile),
        )
        .await;

        // Generate a valid DID for testing
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did_str = keypair.did().to_string();

        let req = test::TestRequest::get()
            .uri(&format!("/members/nonexistent/{}", did_str))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }
}
