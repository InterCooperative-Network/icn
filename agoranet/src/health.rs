use axum::{
    Router,
    routing::get,
    extract::State,
    response::{IntoResponse, Json},
    http::StatusCode,
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use sqlx::Executor;
use utoipa::ToSchema;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Overall status of the service: "ok" or "degraded"
    status: String,
    
    /// Database connection status
    database: bool,
    
    /// Runtime client status (if enabled)
    runtime_client: Option<bool>,
    
    /// Federation service status (if enabled)
    federation: Option<bool>,
    
    /// API version
    version: String,
}

/// Check the health of the API and its dependencies
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service health status", body = HealthResponse),
        (status = 503, description = "Service unhealthy", body = HealthResponse)
    )
)]
async fn health_check(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mut health = HealthResponse {
        status: "ok".to_string(),
        database: false,
        runtime_client: None,
        federation: None,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    
    // Check database connectivity
    let db_result = sqlx::query("SELECT 1").execute(state.db()).await;
    health.database = db_result.is_ok();
    
    // Check federation status if enabled
    if let Some(federation) = state.federation() {
        health.federation = Some(federation.is_running());
    }
    
    // Check runtime client status if enabled
    let runtime_enabled = std::env::var("ENABLE_RUNTIME_CLIENT")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);
        
    if runtime_enabled {
        // We don't have direct access to runtime client status here,
        // so we just report that it's configured
        health.runtime_client = Some(true);
    }
    
    // Set overall status
    if !health.database || 
       health.runtime_client == Some(false) || 
       health.federation == Some(false) {
        health.status = "degraded".to_string();
        return (StatusCode::SERVICE_UNAVAILABLE, Json(health));
    }
    
    (StatusCode::OK, Json(health))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health_check))
} 