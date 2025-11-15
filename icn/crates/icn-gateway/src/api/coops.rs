//! Cooperative namespace API endpoints

use actix_web::{delete, get, post, put, web, HttpResponse};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::coop::{CoopManager, MemberRole};
use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::models::{AddMemberRequest, CreateCoopRequest, UpdateRoleRequest, UpdateSettingsRequest};

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
            format!("Invalid role: {}", role_str)
        )),
    }
}

/// POST /coops - Create a new cooperative
#[post("")]
pub async fn create_coop(
    coop_mgr: web::Data<Arc<CoopManager>>,
    req: web::Json<CreateCoopRequest>,
    // TODO: Extract owner DID from authenticated token
) -> Result<HttpResponse> {
    // For now, generate a placeholder owner DID - will be replaced with auth middleware
    use icn_identity::IdentityBundle;
    let bundle = IdentityBundle::generate()
        .map_err(|e| crate::error::GatewayError::InternalError(format!("{}", e)))?;
    let owner = bundle.did().clone();

    coop_mgr.create_coop(
        req.id.clone(),
        req.name.clone(),
        owner,
        timestamp(),
    )?;

    let coop = coop_mgr.get_coop(&req.id)?;
    Ok(HttpResponse::Created().json(coop))
}

/// GET /coops/:id - Get cooperative info
#[get("/{id}")]
pub async fn get_coop(
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    let coop = coop_mgr.get_coop(&id)?;
    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/settings - Update cooperative settings
#[put("/{id}/settings")]
pub async fn update_settings(
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<UpdateSettingsRequest>,
) -> Result<HttpResponse> {
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
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    coop_mgr.delete_coop(&id)?;
    Ok(HttpResponse::NoContent().finish())
}

/// POST /coops/:id/members - Add a member to cooperative
#[post("/{id}/members")]
pub async fn add_member(
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<AddMemberRequest>,
) -> Result<HttpResponse> {
    let mut coop = coop_mgr.get_coop(&id)?;

    let did: icn_identity::Did = req.did.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    let role = parse_role(&req.role)?;

    coop.add_member(did.clone(), role.clone(), timestamp())?;
    coop_mgr.update_coop(&id, coop.clone())?;

    // Broadcast member added event
    let event = GatewayEvent::MemberAdded {
        coop_id: id.to_string(),
        did: did.to_string(),
        role: format!("{:?}", role),
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
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();
    let mut coop = coop_mgr.get_coop(&coop_id)?;

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    coop.remove_member(&did)?;
    coop_mgr.update_coop(&coop_id, coop.clone())?;

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
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateRoleRequest>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();
    let mut coop = coop_mgr.get_coop(&coop_id)?;

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    let new_role = parse_role(&req.role)?;

    coop.update_role(&did, new_role.clone())?;
    coop_mgr.update_coop(&coop_id, coop.clone())?;

    // Broadcast role updated event
    let event = GatewayEvent::RoleUpdated {
        coop_id: coop_id.clone(),
        did: did_str.clone(),
        new_role: format!("{:?}", new_role),
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
    use actix_web::{test, App};
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_create_and_get_coop() {
        let coop_mgr = Arc::new(CoopManager::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .service(
                    web::scope("/coops")
                        .service(create_coop)
                        .service(get_coop)
                )
        ).await;

        // Create coop
        let req_body = CreateCoopRequest {
            id: "test-coop".to_string(),
            name: "Test Cooperative".to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/coops")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Get coop
        let req = test::TestRequest::get()
            .uri("/coops/test-coop")
            .to_request();

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

        // Add member
        let req_body = AddMemberRequest {
            did: member.did().to_string(),
            role: "member".to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/members")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Remove member
        let uri = format!("/coops/test-coop/members/{}", member.did());
        let req = test::TestRequest::delete()
            .uri(&uri)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
