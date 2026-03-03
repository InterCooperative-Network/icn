//! Flow C convenience routes.
//!
//! Exposes alternate route shapes expected by Flow C demo clients while
//! delegating to the same governance/treasury logic.

use std::sync::Arc;

use actix_web::{get, post, web, HttpRequest, HttpResponse};

use crate::api::treasury::{do_get_treasury_position, do_propose_spend, SpendRequest};
use crate::error::{GatewayError, Result};
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::governance_mgr::GovernanceManager;
use crate::middleware::{get_claims, require_scope};
use crate::models::CastVoteRequest;
use crate::notifications::NotificationService;
use crate::treasury_mgr::GatewayTreasuryManager;
use icn_identity::Did;

/// POST /v1/coops/{coop_id}/proposals - Propose treasury spend.
#[post("/{coop_id}/proposals")]
pub async fn propose_spend_alias(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SpendRequest>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    governance_mgr: web::Data<Arc<GovernanceManager>>,
) -> Result<HttpResponse> {
    do_propose_spend(req, path, body, treasury_mgr, governance_mgr).await
}

/// GET /v1/coops/{coop_id}/treasury/position - Treasury position read.
#[get("/{coop_id}/treasury/position")]
pub async fn get_treasury_position_alias(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    do_get_treasury_position(req, path, treasury_mgr).await
}

/// POST /v1/proposals/{id}/vote - Cast vote alias.
#[post("/{id}/vote")]
pub async fn cast_vote_alias(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    notification_service: web::Data<Arc<NotificationService>>,
    id: web::Path<String>,
    req: web::Json<CastVoteRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let voter_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    crate::validation::validate_vote_comment(&req.comment)?;

    let proposal_id_str = id.into_inner();

    gov_mgr
        .cast_vote_str(
            proposal_id_str.clone(),
            voter_did.clone(),
            &req.choice,
            req.comment.clone(),
        )
        .await
        .map_err(|e| GatewayError::BadRequest(e.to_string()))?;

    let proposal = gov_mgr
        .get_proposal_str(&proposal_id_str)
        .await?
        .ok_or_else(|| {
            GatewayError::InternalError("Vote cast succeeded but proposal not found".to_string())
        })?;

    let vote_event = GatewayEvent::GovernanceVoteCast {
        proposal_id: proposal_id_str.clone(),
        domain_id: proposal.domain_id.0.clone(),
        voter: voter_did.to_string(),
        choice: req.choice.clone(),
    };

    event_broadcaster
        .broadcast(&proposal.domain_id.0, vote_event.clone())
        .await;

    let notif_service = notification_service.into_inner();
    tokio::spawn(async move {
        use crate::notification_listener::handle_event_for_notifications;
        handle_event_for_notifications(&vote_event, &notif_service).await;
    });

    Ok(HttpResponse::Ok().json(proposal))
}

/// GET /v1/proposals/{id}/proof - Proof read alias.
#[get("/{id}/proof")]
pub async fn get_proof_alias(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let proposal_id_str = id.into_inner();

    let proof = gov_mgr
        .get_proof_str(&proposal_id_str)
        .await?
        .ok_or_else(|| {
            GatewayError::NotFound(format!("No proof found for proposal: {proposal_id_str}"))
        })?;

    if !proof.verify_receipt() {
        tracing::warn!(
            "Invalid proof binding for proposal {} — not serving",
            proposal_id_str
        );
        return Err(GatewayError::NotFound(format!(
            "No valid proof found for proposal: {proposal_id_str}"
        )));
    }

    if proof.attestations.is_empty() {
        tracing::warn!(
            "Proof for proposal {} has no attestations — not serving",
            proposal_id_str
        );
        return Err(GatewayError::NotFound(format!(
            "No valid proof found for proposal: {proposal_id_str}"
        )));
    }

    Ok(HttpResponse::Ok().json(proof))
}

/// Configure /v1/coops Flow C alias routes.
pub fn configure_coops(cfg: &mut web::ServiceConfig) {
    cfg.service(propose_spend_alias)
        .service(get_treasury_position_alias);
}

/// Configure /v1/proposals Flow C alias routes.
pub fn configure_proposals(cfg: &mut web::ServiceConfig) {
    cfg.service(cast_vote_alias).service(get_proof_alias);
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::StatusCode, test, App, HttpMessage};

    use crate::auth::TokenClaims;
    use crate::governance_mgr::GovernanceManager;
    use crate::notifications::{NotificationService, NotificationStore};

    fn test_claims(coop_id: &str, scopes: Vec<&str>) -> TokenClaims {
        TokenClaims {
            sub: "did:icn:test-user".to_string(),
            iat: 1000000000,
            coop_id: coop_id.to_string(),
            scopes: scopes.into_iter().map(|s| s.to_string()).collect(),
            exp: 9999999999,
        }
    }

    fn notification_service() -> Arc<NotificationService> {
        let store = Arc::new(NotificationStore::new(
            sled::Config::new()
                .temporary(true)
                .open()
                .expect("temp sled"),
        ));
        Arc::new(NotificationService::new(store, None))
    }

    #[actix_web::test]
    async fn coops_alias_routes_are_registered() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(Arc::new(GatewayTreasuryManager::new())))
                .app_data(web::Data::new(Arc::new(GovernanceManager::new())))
                .service(web::scope("/coops").configure(configure_coops)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/coops/test-coop/treasury/position")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status(), StatusCode::NOT_FOUND);

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/proposals")
            .set_json(serde_json::json!({
                "amount": 5,
                "recipient": "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t",
                "memo": "flow-c alias",
                "unit": "credits"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn proposals_alias_routes_are_registered() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(Arc::new(GovernanceManager::new())))
                .app_data(web::Data::new(Arc::new(EventBroadcaster::new())))
                .app_data(web::Data::new(notification_service()))
                .service(web::scope("/proposals").configure(configure_proposals)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/proposals/p-1/vote")
            .set_json(serde_json::json!({ "choice": "for", "comment": null }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status(), StatusCode::NOT_FOUND);

        let req = test::TestRequest::get()
            .uri("/proposals/p-1/proof")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn cross_coop_access_is_enforced_on_coops_alias() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(Arc::new(GatewayTreasuryManager::new())))
                .app_data(web::Data::new(Arc::new(GovernanceManager::new())))
                .service(web::scope("/coops").configure(configure_coops)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/coops/other-coop/treasury/position")
            .to_request();
        req.extensions_mut()
            .insert(test_claims("test-coop", vec!["treasury:read"]));

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
