//! Error types for ICN Gateway

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use icn_kernel_api::IcnError as KernelError;
use icn_ledger::fx::FxError;
use rust_i18n::t;

/// Gateway result type
pub type Result<T> = std::result::Result<T, GatewayError>;

/// Gateway error types
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Rate limit exceeded for DID: {0}")]
    RateLimitExceeded(String),

    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("ICN substrate error: {0}")]
    SubstrateError(#[from] anyhow::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FX error: {0}")]
    Fx(#[from] FxError),

    #[error("Kernel error: {0}")]
    Kernel(#[from] KernelError),
}

impl GatewayError {
    /// Get the error code for structured error responses
    ///
    /// Returns a machine-readable error code that SDK clients can use for
    /// programmatic error handling.
    pub fn error_code(&self) -> Option<&'static str> {
        match self {
            GatewayError::AuthenticationFailed(_) => Some("AUTHENTICATION_FAILED"),
            GatewayError::AuthorizationFailed(_) => Some("AUTHORIZATION_FAILED"),
            GatewayError::Forbidden(_) => Some("FORBIDDEN"),
            GatewayError::NotFound(_) => Some("NOT_FOUND"),
            GatewayError::BadRequest(_) => Some("BAD_REQUEST"),
            GatewayError::RateLimitExceeded(_) => Some("RATE_LIMIT_EXCEEDED"),
            GatewayError::BudgetExceeded(_) => Some("BUDGET_EXCEEDED"),
            GatewayError::Conflict(_) => Some("CONFLICT"),
            GatewayError::ServiceUnavailable(_) => Some("SERVICE_UNAVAILABLE"),
            GatewayError::Fx(fx_err) => Some(fx_err.error_code()),
            GatewayError::Kernel(kernel_err) => Some(kernel_err.code.as_str()),
            // Internal errors don't expose codes
            GatewayError::InternalError(_) => None,
            GatewayError::SubstrateError(_) => None,
            GatewayError::IoError(_) => None,
        }
    }
}

impl ResponseError for GatewayError {
    fn status_code(&self) -> StatusCode {
        match self {
            GatewayError::AuthenticationFailed(_) => StatusCode::UNAUTHORIZED,
            GatewayError::AuthorizationFailed(_) => StatusCode::FORBIDDEN,
            GatewayError::Forbidden(_) => StatusCode::FORBIDDEN,
            GatewayError::NotFound(_) => StatusCode::NOT_FOUND,
            GatewayError::BadRequest(_) => StatusCode::BAD_REQUEST,
            GatewayError::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            GatewayError::BudgetExceeded(_) => StatusCode::FORBIDDEN,
            GatewayError::Conflict(_) => StatusCode::CONFLICT,
            GatewayError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            GatewayError::SubstrateError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Fx(fx_err) => {
                if fx_err.is_client_error() {
                    StatusCode::BAD_REQUEST
                } else if fx_err.is_not_found() {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            }
            GatewayError::Kernel(kernel_err) => {
                StatusCode::from_u16(kernel_err.http_status())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    /// Generate HTTP error response with i18n support.
    ///
    /// Locale is determined by rust-i18n (defaults to "en").
    /// Falls back to English if the requested locale is unavailable
    /// or if a translation key is missing.
    ///
    /// # Security
    /// Internal errors (InternalError, SubstrateError, IoError) are
    /// sanitized to prevent information leakage. Full details are logged
    /// but only generic messages are returned to clients.
    fn error_response(&self) -> HttpResponse {
        // Sanitize error messages to prevent information leakage
        // Internal errors should not expose implementation details to clients
        // Use i18n translations for user-facing messages
        let error_message = match self {
            // User-facing errors - safe to expose with i18n
            GatewayError::AuthenticationFailed(reason) => {
                format!("{}: {}", t!("gateway.errors.auth_failed_prefix"), reason)
            }
            GatewayError::AuthorizationFailed(reason) => {
                format!("{}: {}", t!("gateway.errors.authz_failed_prefix"), reason)
            }
            GatewayError::Forbidden(reason) => {
                format!("{}: {}", t!("gateway.errors.forbidden_prefix"), reason)
            }
            GatewayError::NotFound(resource) => {
                format!("{}: {}", t!("gateway.errors.not_found_prefix"), resource)
            }
            GatewayError::BadRequest(reason) => {
                format!("{}: {}", t!("gateway.errors.bad_request_prefix"), reason)
            }
            GatewayError::RateLimitExceeded(did) => {
                format!(
                    "{}: {}",
                    t!("gateway.errors.rate_limit_exceeded_prefix"),
                    did
                )
            }
            GatewayError::BudgetExceeded(reason) => {
                format!(
                    "{}: {}",
                    t!("gateway.errors.budget_exceeded_prefix"),
                    reason
                )
            }
            GatewayError::Conflict(reason) => {
                format!("{}: {}", t!("gateway.errors.conflict_prefix"), reason)
            }
            GatewayError::ServiceUnavailable(reason) => {
                // Log the reason for debugging but return generic message to avoid
                // exposing potentially sensitive service state information
                tracing::warn!("Service unavailable: {}", reason);
                t!("gateway.errors.service_unavailable").to_string()
            }

            // FX errors - user-facing with structured codes
            GatewayError::Fx(fx_err) => fx_err.to_string(),

            // Kernel errors - protocol-level errors with stable codes
            GatewayError::Kernel(kernel_err) => kernel_err.message.clone(),

            // Internal errors - sanitize to prevent information leakage
            // Log the full error for debugging but return generic message to client
            GatewayError::InternalError(details) => {
                tracing::error!("Internal error: {}", details);
                t!("gateway.errors.internal").to_string()
            }
            GatewayError::SubstrateError(err) => {
                tracing::error!("Substrate error: {:?}", err);
                t!("gateway.errors.internal").to_string()
            }
            GatewayError::IoError(err) => {
                tracing::error!("I/O error: {:?}", err);
                t!("gateway.errors.internal").to_string()
            }
        };

        // Build response with optional error code
        let mut response = serde_json::json!({
            "error": error_message,
        });

        if let Some(code) = self.error_code() {
            response["code"] = serde_json::Value::String(code.to_string());
        }

        HttpResponse::build(self.status_code()).json(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::{ErrCode, IcnError as KernelError};

    #[test]
    fn gateway_kernel_error_maps_to_http() {
        let kernel_err = KernelError::new(ErrCode::Unauthorized, "missing DID");
        let gateway_err = GatewayError::from(kernel_err);

        assert_eq!(gateway_err.status_code(), actix_web::http::StatusCode::UNAUTHORIZED);
        assert_eq!(gateway_err.error_code(), Some("unauthorized"));
    }

    #[test]
    fn gateway_kernel_error_http_mapping() {
        let test_cases = vec![
            (ErrCode::Unauthorized, 401),
            (ErrCode::InvalidSignature, 401),
            (ErrCode::Forbidden, 403),
            (ErrCode::NotFound, 404),
            (ErrCode::Replay, 409),
            (ErrCode::Expired, 410),
            (ErrCode::Oversize, 413),
            (ErrCode::RateLimited, 429),
            (ErrCode::Busy, 503),
            (ErrCode::InvalidRequest, 400),
        ];

        for (code, expected_status) in test_cases {
            let kernel_err = KernelError::new(code, "test message");
            let gateway_err = GatewayError::from(kernel_err);
            assert_eq!(
                gateway_err.status_code().as_u16(),
                expected_status,
                "HTTP status mismatch for {:?}",
                code
            );
        }
    }
}
