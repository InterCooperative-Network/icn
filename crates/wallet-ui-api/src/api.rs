use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use crate::state::{AppState, SharedState};
use crate::handlers;

pub struct WalletAPI {
    state: SharedState,
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
            // Identity routes
            .route("/api/did/list", get(handlers::list_identities))
            .route("/api/did/:id", get(handlers::get_identity))
            .route("/api/did/create", post(handlers::create_identity))
            .route("/api/did/activate/:id", post(handlers::set_active_identity))
            
            // Proposal routes
            .route("/api/proposal/sign", post(handlers::sign_proposal))
            .route("/api/actions/:action_type", get(handlers::list_actions))
            
            // Credential routes
            .route("/api/vc/verify", post(handlers::verify_credential))
            
            // Sync routes
            .route("/api/sync/dag", post(handlers::sync_dag))
            .route("/api/sync/trust-bundles", post(handlers::sync_trust_bundles))
            
            // Trust Bundle routes
            .route("/api/bundles", get(handlers::list_trust_bundles))
            .route("/api/guardian/status", get(handlers::check_guardian_status))
            .route("/api/proposals/:proposal_id/receipt", post(handlers::create_execution_receipt))
            
            // Governance routes
            .route("/api/governance/appeal/:mandate_id", post(handlers::appeal_mandate))
            
            // AgoraNet integration routes
            .route("/api/agoranet/threads", get(handlers::get_threads))
            .route("/api/agoranet/threads/:thread_id", get(handlers::get_thread))
            .route("/api/agoranet/threads/:thread_id/credential-links", get(handlers::get_credential_links))
            .route("/api/agoranet/credential-link", post(handlers::link_credential))
            .route("/api/agoranet/proposals/:proposal_id/notify", post(handlers::notify_proposal_event))
            
            .layer(cors)
            .with_state(self.state.clone());
            
        println!("Wallet API server starting on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;
            
        Ok(())
    }
} 