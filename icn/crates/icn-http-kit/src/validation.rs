//! Generic input validation helpers — no domain imports, pure functions.

use crate::error::ApiError;

/// Validate that a string ID is non-empty and contains only safe characters.
///
/// Allowed: alphanumerics, hyphens, underscores, dots, colons.
/// This is a baseline check; domain crates should add domain-specific rules.
pub fn validate_id(value: &str, field: &str) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} must not be empty")));
    }

    if value.len() > 256 {
        return Err(ApiError::BadRequest(format!(
            "{field} must not exceed 256 characters"
        )));
    }

    let ok = value
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));

    if !ok {
        return Err(ApiError::BadRequest(format!(
            "{field} contains invalid characters (allowed: alphanumeric, -, _, ., :)"
        )));
    }

    Ok(())
}

/// Validate a human-readable name: non-empty, printable, max length.
pub fn validate_name(value: &str, field: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("{field} must not be empty")));
    }

    if value.len() > 512 {
        return Err(ApiError::BadRequest(format!(
            "{field} must not exceed 512 characters"
        )));
    }

    Ok(())
}

/// Validate a non-negative integer count/amount.
pub fn validate_positive(value: u64, field: &str) -> Result<(), ApiError> {
    if value == 0 {
        return Err(ApiError::BadRequest(format!("{field} must be positive")));
    }

    Ok(())
}
