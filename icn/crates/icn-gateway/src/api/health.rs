//! Health check endpoint

use actix_web::{get, web, HttpResponse};
use crate::models::{ComponentHealth, HealthResponse};
use crate::coop::CoopManager;
use std::collections::HashMap;
use std::sync::Arc;

/// GET /health - Basic health check endpoint
#[get("/health")]
pub async fn health() -> HttpResponse {
    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        checks: None,
    };
    HttpResponse::Ok().json(response)
}

/// GET /health/detailed - Detailed health check with component status
#[get("/health/detailed")]
pub async fn health_detailed(
    coop_manager: web::Data<Arc<CoopManager>>,
) -> HttpResponse {
    let mut checks = HashMap::new();

    // Check cooperative manager
    let coop_health = match coop_manager.list_all_coop_ids() {
        Ok(coops) => ComponentHealth {
            status: "ok".to_string(),
            details: Some(format!("{} cooperatives active", coops.len())),
        },
        Err(e) => ComponentHealth {
            status: "error".to_string(),
            details: Some(format!("Failed to list cooperatives: {}", e)),
        },
    };
    checks.insert("cooperative_manager".to_string(), coop_health);

    // Check system resources
    checks.insert("system".to_string(), ComponentHealth {
        status: "ok".to_string(),
        details: Some("System resources available".to_string()),
    });

    // Determine overall status
    let overall_status = if checks.values().any(|c| c.status == "error") {
        "unhealthy"
    } else if checks.values().any(|c| c.status == "degraded") {
        "degraded"
    } else {
        "healthy"
    };

    let response = HealthResponse {
        status: overall_status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        checks: Some(checks),
    };

    HttpResponse::Ok().json(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_health_endpoint() {
        let app = test::init_service(App::new().service(health)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }
}
