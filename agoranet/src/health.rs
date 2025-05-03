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
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Overall status of the service: "ok" or "degraded"
    pub status: String,
    
    /// Database connection status
    pub database_connection: bool,
    
    /// Runtime client status (if enabled)
    pub runtime_client: Option<bool>,
    
    /// Federation service status (if enabled)
    pub federation: Option<bool>,
    
    /// API version
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub database_healthy: bool,
    pub threads_count: i64,
    pub messages_count: i64,
    pub dag_nodes_count: i64,
    pub federation_sync: bool,
    pub dag_anchoring: bool,
}

/// Check the health of the API and its dependencies
async fn health_check(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mut health = HealthResponse {
        status: "ok".to_string(),
        database_connection: false,
        runtime_client: None,
        federation: None,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    
    // Check database connectivity
    let db_result = sqlx::query("SELECT 1").execute(state.db()).await;
    health.database_connection = db_result.is_ok();
    
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
    if !health.database_connection || 
       health.runtime_client == Some(false) || 
       health.federation == Some(false) {
        health.status = "degraded".to_string();
        return (StatusCode::SERVICE_UNAVAILABLE, Json(health));
    }
    
    (StatusCode::OK, Json(health))
}

pub async fn check_health(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthResponse>, StatusCode> {
    // Check database connection
    let db_connection = sqlx::query("SELECT 1")
        .fetch_one(state.db_pool.as_ref())
        .await
        .is_ok();
    
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        database_connection: db_connection,
        runtime_client: None,
        federation: None,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

pub async fn check_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, StatusCode> {
    // Check database connection
    let db_connection = sqlx::query("SELECT 1")
        .fetch_one(state.db_pool.as_ref())
        .await
        .is_ok();
    
    // Get counts if database is connected
    let (threads_count, messages_count, dag_nodes_count) = if db_connection {
        // Note: we're using try_query here to handle the case where tables don't exist yet
        let threads = match sqlx::query!("SELECT COUNT(*) as count FROM threads")
            .fetch_one(state.db_pool.as_ref())
            .await {
                Ok(r) => r.count.unwrap_or(0),
                Err(_) => 0,
            };
            
        let messages = match sqlx::query!("SELECT COUNT(*) as count FROM messages")
            .fetch_one(state.db_pool.as_ref())
            .await {
                Ok(r) => r.count.unwrap_or(0),
                Err(_) => 0,
            };
            
        let dag_nodes = match sqlx::query!("SELECT COUNT(*) as count FROM dag_nodes")
            .fetch_one(state.db_pool.as_ref())
            .await {
                Ok(r) => r.count.unwrap_or(0),
                Err(_) => 0,
            };
            
        (threads, messages, dag_nodes) 
    } else {
        (0, 0, 0)
    };
    
    Ok(Json(StatusResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database_healthy: db_connection,
        threads_count,
        messages_count,
        dag_nodes_count,
        federation_sync: true, // Replace with config value
        dag_anchoring: true,   // Replace with config value
    }))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health_check))
} 