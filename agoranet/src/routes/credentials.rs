use axum::{
    Json, 
    Router, 
    routing::{post, get},
    extract::State,
    http::StatusCode,
    middleware,
};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use crate::storage::CredentialLinkRepository;
use crate::types::credential::CredentialLinkResponse;
use crate::auth::{did_auth_middleware, DidAuth};
use crate::state::AppState;

// Define public request and response types
#[derive(Deserialize, Debug)]
pub struct CredentialLinkRequest {
    pub thread_id: String,
    pub credential_cid: String,
    pub signer_did: String,
}

// Define the route handlers
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/credential-link", post(link_credential))
        .route("/api/threads/credential-links", get(list_links))
        .route("/api/threads/:id/credential-links", get(list_links_for_thread))
        .layer(middleware::from_fn_with_state::<AppState, _>(did_auth_middleware))
}

// Create a credential link (requires authentication)
async fn link_credential(
    State(state): State<AppState>,
    auth: DidAuth,
    Json(mut req): Json<CredentialLinkRequest>
) -> Result<Json<CredentialLinkResponse>, StatusCode> {
    // Use authenticated DID as the signer
    req.signer_did = auth.0;
    
    let link_repo = CredentialLinkRepository::new(state.db_pool.clone());
    
    match link_repo.create_credential_link(&req).await {
        Ok(link) => {
            // Announce the new credential link to the federation if enabled
            if let Some(federation) = state.federation() {
                if let Err(e) = federation.announce_credential_link(
                    &link.thread_id.to_string(), 
                    &link.id.to_string()
                ).await {
                    tracing::warn!("Failed to announce credential link: {}", e);
                    // Don't return an error, as the link was created successfully
                }
            }
            
            Ok(Json(link.into()))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// List all credential links
async fn list_links(
    State(state): State<AppState>
) -> Result<Json<Vec<CredentialLinkResponse>>, StatusCode> {
    let link_repo = CredentialLinkRepository::new(state.db_pool);
    
    match link_repo.list_credential_links().await {
        Ok(links) => {
            let responses: Vec<CredentialLinkResponse> = links.into_iter()
                .map(|link| link.into())
                .collect();
            Ok(Json(responses))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// List credential links for a specific thread
async fn list_links_for_thread(
    State(state): State<AppState>,
    axum::extract::Path(thread_id): axum::extract::Path<String>,
) -> Result<Json<Vec<CredentialLinkResponse>>, StatusCode> {
    let link_repo = CredentialLinkRepository::new(state.db_pool);
    
    let thread_id = Uuid::parse_str(&thread_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    match link_repo.get_links_for_thread(thread_id).await {
        Ok(links) => {
            let responses: Vec<CredentialLinkResponse> = links.into_iter()
                .map(|link| link.into())
                .collect();
            Ok(Json(responses))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
} 