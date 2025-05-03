use axum::{
    response::{Response, IntoResponse},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::fmt;
use thiserror::Error;
use wallet_core::error::WalletError as CoreError;
use wallet_agent::error::AgentError;
use wallet_sync::error::SyncError;

/// API Result type
pub type ApiResult<T> = Result<T, ApiError>;

/// API Error types
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Store error: {0}")]
    StoreError(String),
    
    #[error("Wallet error: {0}")]
    WalletError(String),
    
    #[error("Sync error: {0}")]
    SyncError(String),
    
    #[error("Internal server error: {0}")]
    InternalError(String),
    
    #[error("Core error: {0}")]
    CoreError(#[from] CoreError),
    
    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),
    
    #[error("Sync adapter error: {0}")]
    SyncAdapterError(#[from] SyncError),
}

/// Convert ApiError to HTTP response
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            ApiError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::AuthError(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::StoreError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::WalletError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::SyncError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::CoreError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::AgentError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::SyncAdapterError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        
        let body = Json(json!({
            "error": {
                "message": error_message,
                "code": status.as_u16()
            }
        }));
        
        (status, body).into_response()
    }
} 