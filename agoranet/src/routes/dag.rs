use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router, Extension,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;
use crate::dag::types::{AnchorRequest, ThreadAnchorRequest, MessageAnchorRequest};
use crate::auth::AuthUser;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/anchor/thread", post(anchor_thread))
        .route("/anchor/message", post(anchor_message))
}

async fn anchor_thread(
    State(state): State<Arc<AppState>>,
    Extension(AuthUser(user)): Extension<AuthUser>,
    Json(request): Json<ThreadAnchorRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Verify the user is authorized to anchor this thread
    if request.signer_did != user.did {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match state.services.dag_service.anchor_thread(request).await {
        Ok(response) => Ok(Json(serde_json::json!({
            "dag_ref": response.dag_ref,
            "content_hash": response.content_hash,
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn anchor_message(
    State(state): State<Arc<AppState>>,
    Extension(AuthUser(user)): Extension<AuthUser>,
    Json(request): Json<MessageAnchorRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Verify the user is authorized to anchor this message
    if request.signer_did != user.did {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match state.services.dag_service.anchor_message(request).await {
        Ok(response) => Ok(Json(serde_json::json!({
            "dag_ref": response.dag_ref,
            "content_hash": response.content_hash,
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
} 