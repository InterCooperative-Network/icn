use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use crate::state::AppState;
use crate::handlers;
use wallet_core::store::LocalWalletStore;

pub struct WalletAPI {
    state: Arc<AppState<S>>,
}

impl WalletAPI {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        let state = Arc::new(AppState::new(data_dir));
        Self { state }
    }
    
    pub fn with_agoranet_url(mut self, url: &str) -> Self {
        let state = Arc::new(AppState::new(&self.state.data_dir).with_agoranet_url(url));
        self.state = state;
        self
    }
    
    pub async fn run(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
            
        let app = Router::new()
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
            
            .layer(cors)
            .with_state(self.state.clone());
            
        println!("Wallet API server starting on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;
            
        Ok(())
    }
}

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