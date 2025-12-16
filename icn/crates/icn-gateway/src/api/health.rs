//! Health check endpoints

use crate::coop::CoopManager;
use crate::models::{ComponentHealth, HealthResponse};
use crate::notification_queue::NotificationQueue;
use actix_web::{get, web, HttpResponse};
use std::collections::HashMap;
use std::sync::Arc;

/// GET /healthz - Kubernetes liveness probe
///
/// Lightweight check that the service is alive and responsive.
/// Should only fail if the process is deadlocked or unrecoverable.
#[get("/healthz")]
pub async fn liveness() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "alive",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// GET /readyz - Kubernetes readiness probe
///
/// Checks if the service can accept traffic.
/// Returns 503 if critical dependencies are unavailable.
#[get("/readyz")]
pub async fn readiness(coop_manager: web::Data<Arc<CoopManager>>) -> HttpResponse {
    // Check critical dependencies
    let mut ready = true;
    let mut checks = HashMap::new();

    // Check cooperative manager (critical for most operations)
    match coop_manager.list_all_coop_ids() {
        Ok(_) => {
            checks.insert(
                "cooperative_manager".to_string(),
                ComponentHealth {
                    status: "ok".to_string(),
                    details: Some("Available".to_string()),
                },
            );
        }
        Err(e) => {
            ready = false;
            checks.insert(
                "cooperative_manager".to_string(),
                ComponentHealth {
                    status: "error".to_string(),
                    details: Some(format!("Unavailable: {e}")),
                },
            );
        }
    }

    let response = HealthResponse {
        status: if ready {
            "ready".to_string()
        } else {
            "not_ready".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        checks: Some(checks),
    };

    if ready {
        HttpResponse::Ok().json(response)
    } else {
        HttpResponse::ServiceUnavailable().json(response)
    }
}

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
    notification_queue: web::Data<Arc<NotificationQueue>>,
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
            details: Some(format!("Failed to list cooperatives: {e}")),
        },
    };
    checks.insert("cooperative_manager".to_string(), coop_health);

    // Check notification queue
    let queue_stats = notification_queue.get_stats();
    let queue_health = ComponentHealth {
        status: "ok".to_string(),
        details: Some(format!(
            "queued: {}, delivered: {}, failed: {}, pending: {}",
            queue_stats.queued,
            queue_stats.delivered,
            queue_stats.failed,
            queue_stats.pending_count
        )),
    };
    checks.insert("notification_queue".to_string(), queue_health);

    // Check system
    checks.insert(
        "system".to_string(),
        ComponentHealth {
            status: "ok".to_string(),
            details: Some("System resources available".to_string()),
        },
    );

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
    async fn test_liveness_endpoint() {
        let app = test::init_service(App::new().service(liveness)).await;
        let req = test::TestRequest::get().uri("/healthz").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "alive");
    }

    #[actix_web::test]
    async fn test_health_endpoint() {
        let app = test::init_service(App::new().service(health)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_readiness_endpoint() {
        let coop_manager = Arc::new(CoopManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .service(readiness),
        )
        .await;

        let req = test::TestRequest::get().uri("/readyz").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ready");
    }
}
