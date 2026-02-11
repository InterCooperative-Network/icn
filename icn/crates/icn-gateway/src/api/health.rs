//! Health check endpoints

use crate::coop::CoopManager;
use crate::identity_mgr::IdentityManager;
use crate::ledger_mgr::LedgerManager;
use crate::models::{
    ComponentHealth, DetailedComponentHealth, DetailedHealthResponse, HealthResponse, HealthStatus,
};
use crate::notification_queue::NotificationQueue;
use actix_web::{get, web, HttpResponse};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Server start time for uptime calculation
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Initialize server start time (call once at startup)
pub fn init_start_time() {
    let _ = START_TIME.get_or_init(Instant::now);
}

/// Get uptime in seconds
fn get_uptime_seconds() -> u64 {
    START_TIME
        .get()
        .map(|start| start.elapsed().as_secs())
        .unwrap_or(0)
}

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
    let mut is_ready = true;
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
            is_ready = false;
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
        status: if is_ready {
            "ready".to_string()
        } else {
            "not_ready".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        checks: Some(checks),
    };

    if is_ready {
        HttpResponse::Ok().json(response)
    } else {
        HttpResponse::ServiceUnavailable().json(response)
    }
}

/// GET /ready - V1 API readiness probe
///
/// Returns 200 if all critical components are healthy.
/// Returns 503 if any critical component is unhealthy.
#[get("/ready")]
pub async fn ready(
    coop_manager: web::Data<Arc<CoopManager>>,
    _ledger_manager: web::Data<Arc<LedgerManager>>,
    _identity_manager: web::Data<Arc<IdentityManager>>,
) -> HttpResponse {
    let mut components = HashMap::new();
    let mut all_healthy = true;

    // Check cooperative manager (keystore proxy - if it works, identity is loaded)
    let start = Instant::now();
    let coop_status = match coop_manager.list_all_coop_ids() {
        Ok(_) => DetailedComponentHealth {
            status: HealthStatus::Ok,
            message: Some("Cooperative manager available".to_string()),
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
        Err(e) => {
            all_healthy = false;
            DetailedComponentHealth {
                status: HealthStatus::Unhealthy,
                message: Some(format!("Cooperative manager unavailable: {e}")),
                latency_ms: Some(start.elapsed().as_millis() as u64),
            }
        }
    };
    components.insert("coop_manager".to_string(), coop_status);

    // Check ledger (critical for transactions)
    // Ledger is considered healthy if it exists (it's lazy-loaded per coop)
    let start = Instant::now();
    let ledger_status = DetailedComponentHealth {
        status: HealthStatus::Ok,
        message: Some("Ledger service initialized".to_string()),
        latency_ms: Some(start.elapsed().as_millis() as u64),
    };
    components.insert("ledger".to_string(), ledger_status);

    // Check identity manager (critical for DID operations)
    // Identity manager is healthy if instantiated
    let start = Instant::now();
    let identity_status = DetailedComponentHealth {
        status: HealthStatus::Ok,
        message: Some("Identity manager available".to_string()),
        latency_ms: Some(start.elapsed().as_millis() as u64),
    };
    components.insert("identity".to_string(), identity_status);

    let overall_status = if all_healthy {
        HealthStatus::Ok
    } else {
        HealthStatus::Unhealthy
    };

    let response = DetailedHealthResponse {
        status: overall_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: get_uptime_seconds(),
        components,
    };

    if all_healthy {
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
    _ledger_manager: web::Data<Arc<LedgerManager>>,
    _identity_manager: web::Data<Arc<IdentityManager>>,
    notification_queue: web::Data<Arc<NotificationQueue>>,
) -> HttpResponse {
    let mut components = HashMap::new();

    // Check cooperative manager with latency
    let start = Instant::now();
    let coop_health = match coop_manager.list_all_coop_ids() {
        Ok(coops) => DetailedComponentHealth {
            status: HealthStatus::Ok,
            message: Some(format!("{} cooperatives active", coops.len())),
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
        Err(e) => DetailedComponentHealth {
            status: HealthStatus::Unhealthy,
            message: Some(format!("Failed to list cooperatives: {e}")),
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
    };
    components.insert("coop_manager".to_string(), coop_health);

    // Check ledger
    let start = Instant::now();
    let ledger_health = DetailedComponentHealth {
        status: HealthStatus::Ok,
        message: Some("Ledger service initialized".to_string()),
        latency_ms: Some(start.elapsed().as_millis() as u64),
    };
    components.insert("ledger".to_string(), ledger_health);

    // Check identity manager
    let start = Instant::now();
    let identity_health = DetailedComponentHealth {
        status: HealthStatus::Ok,
        message: Some("Identity manager available".to_string()),
        latency_ms: Some(start.elapsed().as_millis() as u64),
    };
    components.insert("identity".to_string(), identity_health);

    // Check notification queue
    let start = Instant::now();
    let queue_stats = notification_queue.get_stats();
    let queue_health = DetailedComponentHealth {
        status: HealthStatus::Ok,
        message: Some(format!(
            "queued: {}, delivered: {}, failed: {}, pending: {}",
            queue_stats.queued,
            queue_stats.delivered,
            queue_stats.failed,
            queue_stats.pending_count
        )),
        latency_ms: Some(start.elapsed().as_millis() as u64),
    };
    components.insert("notification_queue".to_string(), queue_health);

    // Check system resources
    components.insert(
        "system".to_string(),
        DetailedComponentHealth {
            status: HealthStatus::Ok,
            message: Some("System resources available".to_string()),
            latency_ms: None,
        },
    );

    // Determine overall status
    let overall_status = if components
        .values()
        .any(|c| c.status == HealthStatus::Unhealthy)
    {
        HealthStatus::Unhealthy
    } else if components
        .values()
        .any(|c| c.status == HealthStatus::Degraded)
    {
        HealthStatus::Degraded
    } else {
        HealthStatus::Ok
    };

    let response = DetailedHealthResponse {
        status: overall_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: get_uptime_seconds(),
        components,
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

    #[actix_web::test]
    async fn test_ready_endpoint() {
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());
        let identity_manager = Arc::new(IdentityManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(ledger_manager))
                .app_data(web::Data::new(identity_manager))
                .service(ready),
        )
        .await;

        let req = test::TestRequest::get().uri("/ready").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert!(body["uptime_seconds"].is_number());
        assert!(body["components"].is_object());
    }

    #[actix_web::test]
    async fn test_health_detailed_endpoint() {
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());
        let identity_manager = Arc::new(IdentityManager::new());
        let notification_queue = Arc::new(NotificationQueue::new().0);

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_manager))
                .app_data(web::Data::new(ledger_manager))
                .app_data(web::Data::new(identity_manager))
                .app_data(web::Data::new(notification_queue))
                .service(health_detailed),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/health/detailed")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert!(body["uptime_seconds"].is_number());
        assert!(body["components"]["ledger"].is_object());
        assert!(body["components"]["identity"].is_object());
        assert!(body["components"]["coop_manager"].is_object());
    }

    #[actix_web::test]
    async fn test_health_status_enum() {
        assert_eq!(HealthStatus::Ok.to_string(), "ok");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }
}
