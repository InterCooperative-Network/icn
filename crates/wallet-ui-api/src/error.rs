use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_agent::error::AgentError;
use wallet_sync::error::SyncError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    #[error("Core wallet error: {0}")]
    CoreError(#[from] CoreError),
    
    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),
    
    #[error("Sync error: {0}")]
    SyncError(#[from] SyncError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(ref message) => (StatusCode::NOT_FOUND, message.clone()),
            ApiError::InvalidRequest(ref message) => (StatusCode::BAD_REQUEST, message.clone()),
            ApiError::AuthError(ref message) => (StatusCode::UNAUTHORIZED, message.clone()),
            ApiError::InternalError(ref message) => (StatusCode::INTERNAL_SERVER_ERROR, message.clone()),
            ApiError::CoreError(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::AgentError(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::SyncError(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        
        let body = Json(serde_json::json!({
            "error": message
        }));
        
        (status, body).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>; 