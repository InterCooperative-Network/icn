//! Error types for ICN Gateway

use actix_web::{http::StatusCode, HttpResponse, ResponseError};

/// Gateway result type
pub type Result<T> = std::result::Result<T, GatewayError>;

/// Gateway error types
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Rate limit exceeded for DID: {0}")]
    RateLimitExceeded(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("ICN substrate error: {0}")]
    SubstrateError(#[from] anyhow::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

impl ResponseError for GatewayError {
    fn status_code(&self) -> StatusCode {
        match self {
            GatewayError::AuthenticationFailed(_) => StatusCode::UNAUTHORIZED,
            GatewayError::AuthorizationFailed(_) => StatusCode::FORBIDDEN,
            GatewayError::NotFound(_) => StatusCode::NOT_FOUND,
            GatewayError::BadRequest(_) => StatusCode::BAD_REQUEST,
            GatewayError::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            GatewayError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::SubstrateError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Sanitize error messages to prevent information leakage
        // Internal errors should not expose implementation details to clients
        let error_message = match self {
            // User-facing errors - safe to expose
            GatewayError::AuthenticationFailed(msg) => msg.clone(),
            GatewayError::AuthorizationFailed(msg) => msg.clone(),
            GatewayError::NotFound(msg) => msg.clone(),
            GatewayError::BadRequest(msg) => msg.clone(),
            GatewayError::RateLimitExceeded(msg) => format!("Rate limit exceeded for DID: {}", msg),

            // Internal errors - sanitize to prevent information leakage
            // Log the full error for debugging but return generic message to client
            GatewayError::InternalError(details) => {
                tracing::error!("Internal error: {}", details);
                "Internal server error".to_string()
            }
            GatewayError::SubstrateError(err) => {
                tracing::error!("Substrate error: {:?}", err);
                "Internal server error".to_string()
            }
            GatewayError::IoError(err) => {
                tracing::error!("I/O error: {:?}", err);
                "Internal server error".to_string()
            }
        };

        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "error": error_message,
        }))
    }
}
