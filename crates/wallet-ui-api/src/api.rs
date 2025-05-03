use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use crate::state::AppState;
use crate::handlers;
use wallet_core::store::LocalWalletStore;

/// Create the API router with the application state
pub fn create_api_router<S: LocalWalletStore + 'static>(
    state: Arc<AppState<S>>,
) -> Router {
    Router::new()
        // Health check endpoint
        .route("/health", get(handlers::health_check))
        
        // Identity routes
        .route("/did/list", get(handlers::list_identities::<S>))
        .route("/did/:id", get(handlers::get_identity::<S>))
        .route("/did/create", post(handlers::create_identity::<S>))
        
        // Credential routes
        .route("/vc/issue/:issuer_did", post(handlers::create_credential::<S>))
        .route("/vc/verify", post(handlers::verify_credential::<S>))
        
        // Action queue routes
        .route("/actions/queue", post(handlers::queue_action::<S>))
        
        // Sync routes
        .route("/sync/dag", post(handlers::sync_dag::<S>))
        
        .with_state(state)
} 