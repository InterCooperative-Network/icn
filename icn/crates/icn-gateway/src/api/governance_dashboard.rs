//! Governance Dashboard API
//!
//! Aggregate governance statistics and activity for domain dashboards.
//!
//! The DTO types (GovernanceDashboard, AmendmentsBreakdown, AppealsBreakdown,
//! GovernanceActivityEvent) are defined in crate::models and re-exported here
//! for backward compatibility with openapi.rs and other consumers.

use actix_web::{get, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::commons_mgr::CommonsManager;
use crate::error::Result;
use crate::middleware::require_scope;

// Re-export dashboard DTOs from models so openapi.rs keeps working.
pub use crate::models::GovernanceActivityEvent as ActivityEvent;
pub use crate::models::{AmendmentsBreakdown, AppealsBreakdown, GovernanceDashboard};

/// GET /governance/{charter_id}/dashboard - Governance dashboard
#[get("/governance/{charter_id}/dashboard")]
pub async fn get_governance_dashboard(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    charter_id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let charter_id_str = charter_id.into_inner();

    let dashboard = commons_mgr
        .build_governance_dashboard(&charter_id_str)
        .await?;

    Ok(HttpResponse::Ok().json(dashboard))
}

/// Configure governance dashboard routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_governance_dashboard);
}
