//! Cooperative namespace API endpoints

use actix_web::{delete, get, post, put, web, HttpResponse};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::coop::{CoopManager, MemberRole};
use crate::error::Result;
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
#[post("/coops")]
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
#[get("/coops/{id}")]
pub async fn get_coop(
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    let coop = coop_mgr.get_coop(&id)?;
    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/settings - Update cooperative settings
#[put("/coops/{id}/settings")]
pub async fn update_settings(
    coop_mgr: web::Data<Arc<CoopManager>>,
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
    Ok(HttpResponse::Ok().json(coop))
}

/// DELETE /coops/:id - Delete cooperative
#[delete("/coops/{id}")]
pub async fn delete_coop(
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    coop_mgr.delete_coop(&id)?;
    Ok(HttpResponse::NoContent().finish())
}

/// POST /coops/:id/members - Add a member to cooperative
#[post("/coops/{id}/members")]
pub async fn add_member(
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
    req: web::Json<AddMemberRequest>,
) -> Result<HttpResponse> {
    let mut coop = coop_mgr.get_coop(&id)?;

    let did = req.did.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    let role = parse_role(&req.role)?;

    coop.add_member(did, role, timestamp())?;
    coop_mgr.update_coop(&id, coop.clone())?;

    Ok(HttpResponse::Ok().json(coop))
}

/// DELETE /coops/:id/members/:did - Remove a member from cooperative
#[delete("/coops/{id}/members/{did}")]
pub async fn remove_member(
    coop_mgr: web::Data<Arc<CoopManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();
    let mut coop = coop_mgr.get_coop(&coop_id)?;

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    coop.remove_member(&did)?;
    coop_mgr.update_coop(&coop_id, coop.clone())?;

    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/members/:did/role - Update member role
#[put("/coops/{id}/members/{did}/role")]
pub async fn update_member_role(
    coop_mgr: web::Data<Arc<CoopManager>>,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateRoleRequest>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();
    let mut coop = coop_mgr.get_coop(&coop_id)?;

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    let new_role = parse_role(&req.role)?;

    coop.update_role(&did, new_role)?;
    coop_mgr.update_coop(&coop_id, coop.clone())?;

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
                .service(create_coop)
                .service(get_coop)
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
                .service(add_member)
                .service(remove_member)
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
