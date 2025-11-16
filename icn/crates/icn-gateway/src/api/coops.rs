//! Cooperative namespace API endpoints

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::coop::{CoopManager, MemberRole};
use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::middleware::require_scope;
use crate::models::{AddMemberRequest, CreateCoopRequest, UpdateRoleRequest, UpdateSettingsRequest};
use icn_obs::metrics::gateway;

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn parse_role(role_str: &str) -> Result<MemberRole> {
    match role_str.to_lowercase().as_str() {
        "owner" => Ok(MemberRole::Owner),
        "admin" => Ok(MemberRole::Admin),
        "member" => Ok(MemberRole::Member),
        _ => Err(crate::error::GatewayError::BadRequest(
            format!("Invalid role: {role_str}")
        )),
    }
}

/// POST /coops - Create a new cooperative
#[post("")]
pub async fn create_coop(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    req: web::Json<CreateCoopRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract owner DID from authenticated token
    use crate::middleware::get_claims;
    let claims = get_claims(&http_req)
        .ok_or_else(|| crate::error::GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let owner: icn_identity::Did = claims.sub.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    coop_mgr.create_coop(
        req.id.clone(),
        req.name.clone(),
        owner,
        timestamp(),
    )?;

    // Track cooperative creation
    gateway::coops_created_inc();

    let coop = coop_mgr.get_coop(&req.id)?;
    Ok(HttpResponse::Created().json(coop))
}

/// GET /coops/:id - Get cooperative info
#[get("/{id}")]
pub async fn get_coop(
    req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "coop:read")?;

    let coop = coop_mgr.get_coop(&id)?;
    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/settings - Update cooperative settings
#[put("/{id}/settings")]
pub async fn update_settings(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<UpdateSettingsRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    let mut coop = coop_mgr.get_coop(&id)?;

    if let Some(gov) = &req.governance_model {
        coop.settings.governance_model = gov.clone();
    }
    if let Some(policy) = &req.credit_policy {
        coop.settings.credit_policy = policy.clone();
    }
    if let Some(currency) = &req.currency {
        coop.settings.currency = currency.clone();
    }

    coop_mgr.update_coop(&id, coop.clone())?;

    // Broadcast settings updated event
    let event = GatewayEvent::SettingsUpdated {
        coop_id: id.to_string(),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id = id.into_inner();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// DELETE /coops/:id - Delete cooperative
#[delete("/{id}")]
pub async fn delete_coop(
    req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "coop:admin")?;

    let coop_id = id.into_inner();

    coop_mgr.delete_coop(&coop_id)?;

    // Track cooperative deletion
    gateway::coops_deleted_inc();

    // Clean up any WebSocket subscribers for this deleted cooperative
    let broadcaster_clone = broadcaster.clone();
    let coop_id_clone = coop_id.clone();
    tokio::spawn(async move {
        broadcaster_clone.cleanup(&coop_id_clone).await;
    });

    Ok(HttpResponse::NoContent().finish())
}

/// POST /coops/:id/members - Add a member to cooperative
#[post("/{id}/members")]
pub async fn add_member(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<AddMemberRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    let mut coop = coop_mgr.get_coop(&id)?;

    let did: icn_identity::Did = req.did.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let role = parse_role(&req.role)?;

    coop.add_member(did.clone(), role.clone(), timestamp())?;
    coop_mgr.update_coop(&id, coop.clone())?;

    // Track member addition
    gateway::members_added_inc();

    // Broadcast member added event
    let event = GatewayEvent::MemberAdded {
        coop_id: id.to_string(),
        did: did.to_string(),
        role: format!("{role:?}"),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id = id.into_inner();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// DELETE /coops/:id/members/:did - Remove a member from cooperative
#[delete("/{id}/members/{did}")]
pub async fn remove_member(
    req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "coop:admin")?;

    let (coop_id, did_str) = path.into_inner();
    let mut coop = coop_mgr.get_coop(&coop_id)?;

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    coop.remove_member(&did)?;
    coop_mgr.update_coop(&coop_id, coop.clone())?;

    // Track member removal
    gateway::members_removed_inc();

    // Broadcast member removed event
    let event = GatewayEvent::MemberRemoved {
        coop_id: coop_id.clone(),
        did: did_str.clone(),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id_clone = coop_id.clone();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id_clone, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/members/:did/role - Update member role
#[put("/{id}/members/{did}/role")]
pub async fn update_member_role(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateRoleRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    let (coop_id, did_str) = path.into_inner();
    let mut coop = coop_mgr.get_coop(&coop_id)?;

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let new_role = parse_role(&req.role)?;

    coop.update_role(&did, new_role.clone())?;
    coop_mgr.update_coop(&coop_id, coop.clone())?;

    // Broadcast role updated event
    let event = GatewayEvent::RoleUpdated {
        coop_id: coop_id.clone(),
        did: did_str.clone(),
        new_role: format!("{new_role:?}"),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id_clone = coop_id.clone();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id_clone, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App, HttpMessage};
    use crate::auth::TokenClaims;
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_create_and_get_coop() {
        let coop_mgr = Arc::new(CoopManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .service(
                    web::scope("/coops")
                        .service(create_coop)
                        .service(get_coop)
                )
        ).await;

        // Create coop with authorization
        let req_body = CreateCoopRequest {
            id: "test-coop".to_string(),
            name: "Test Cooperative".to_string(),
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Get coop with authorization
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::get()
            .uri("/coops/test-coop")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_add_remove_member() {
        let coop_mgr = Arc::new(CoopManager::new());
        let broadcaster = Arc::new(EventBroadcaster::new());
        let owner = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        // Create coop directly
        coop_mgr.create_coop(
            "test-coop".to_string(),
            "Test".to_string(),
            owner.did().clone(),
            timestamp(),
        ).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .app_data(web::Data::new(broadcaster.clone()))
                .service(
                    web::scope("/coops")
                        .service(add_member)
                        .service(remove_member)
                )
        ).await;

        // Add member with authorization
        let req_body = AddMemberRequest {
            did: member.did().to_string(),
            role: "member".to_string(),
        };

        let claims = TokenClaims {
            sub: owner.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:admin".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/members")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Remove member with authorization
        let uri = format!("/coops/test-coop/members/{}", member.did());
        let req = test::TestRequest::delete()
            .uri(&uri)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_authorization_scope_check() {
        let coop_mgr = Arc::new(CoopManager::new());
        let broadcaster = Arc::new(EventBroadcaster::new());
        let owner = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        // Create coop directly
        coop_mgr.create_coop(
            "test-coop".to_string(),
            "Test".to_string(),
            owner.did().clone(),
            timestamp(),
        ).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .app_data(web::Data::new(broadcaster.clone()))
                .service(
                    web::scope("/coops")
                        .service(add_member)
                        .service(update_settings)
                )
        ).await;

        // Try to add member with only "coop:read" scope (should fail)
        let req_body = AddMemberRequest {
            did: member.did().to_string(),
            role: "member".to_string(),
        };

        let claims = TokenClaims {
            sub: owner.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:read".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/members")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Try to update settings with "coop:read" scope (should fail)
        let req_body = UpdateSettingsRequest {
            governance_model: Some("consensus".to_string()),
            credit_policy: None,
            currency: None,
        };

        let claims = TokenClaims {
            sub: owner.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:read".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::put()
            .uri("/coops/test-coop/settings")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_create_coop_uses_authenticated_did() {
        let coop_mgr = Arc::new(CoopManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .service(
                    web::scope("/coops")
                        .service(create_coop)
                )
        ).await;

        // Create coop with Alice's token
        let req_body = CreateCoopRequest {
            id: "alice-coop".to_string(),
            name: "Alice's Cooperative".to_string(),
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "alice-coop".to_string(),
            scopes: vec!["coop:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Verify that Alice is the owner
        let coop = coop_mgr.get_coop(&"alice-coop".to_string()).unwrap();
        assert_eq!(coop.members.len(), 1, "Coop should have exactly one member (the owner)");

        let alice_member = coop.members.iter()
            .find(|m| m.did == *alice.did())
            .expect("Alice should be a member");
        assert_eq!(alice_member.role, MemberRole::Owner, "Alice should be the owner");
    }
}
