//! Error type for appliance manifest operations.

/// Errors from serializing or parsing an [`crate::ApplianceManifest`].
#[derive(Debug, thiserror::Error)]
pub enum ApplianceError {
    /// JSON serialization or deserialization failed.
    #[error("appliance manifest JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
