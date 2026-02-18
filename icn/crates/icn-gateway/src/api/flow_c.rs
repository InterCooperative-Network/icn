//! Flow C convenience routes.
//!
//! Exposes alternate route shapes expected by Flow C demo clients while
//! delegating to the same governance/treasury logic.

use std::sync::Arc;

use actix_web::{get, post, web, HttpRequest, HttpResponse};

use crate::api::governance::{do_cast_vote, do_get_proposal_proof};
use crate::api::treasury::{do_get_treasury_balance, do_propose_spend, SpendRequest};
use crate::error::Result;
use crate::events::EventBroadcaster;
use crate::governance_mgr::GovernanceManager;
use crate::models::CastVoteRequest;
use crate::notifications::NotificationService;
use crate::treasury_mgr::GatewayTreasuryManager;

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

/// GET /v1/coops/{coop_id}/treasury/balance - Treasury balance read.
#[get("/{coop_id}/treasury/balance")]
pub async fn get_treasury_balance_alias(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    do_get_treasury_balance(req, path, treasury_mgr).await
}

/// POST /v1/proposals/{id}/vote - Cast vote alias.
#[post("/{id}/vote")]
pub async fn cast_vote_alias(
    req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    notification_service: web::Data<Arc<NotificationService>>,
    id: web::Path<String>,
    body: web::Json<CastVoteRequest>,
) -> Result<HttpResponse> {
    do_cast_vote(
        req,
        gov_mgr,
        event_broadcaster,
        notification_service,
        id,
        body,
    )
    .await
}

/// GET /v1/proposals/{id}/proof - Proof read alias.
#[get("/{id}/proof")]
pub async fn get_proof_alias(
    req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    do_get_proposal_proof(req, gov_mgr, id).await
}

/// Configure /v1/coops Flow C alias routes.
pub fn configure_coops(cfg: &mut web::ServiceConfig) {
    cfg.service(propose_spend_alias)
        .service(get_treasury_balance_alias);
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
            .uri("/coops/test-coop/treasury/balance")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status(), StatusCode::NOT_FOUND);

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/proposals")
            .set_json(serde_json::json!({
                "amount": 5,
                "recipient": "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t",
                "memo": "flow-c alias",
                "currency": "credits"
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
            .uri("/coops/other-coop/treasury/balance")
            .to_request();
        req.extensions_mut()
            .insert(test_claims("test-coop", vec!["treasury:read"]));

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
