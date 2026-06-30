//! Error type for appliance manifest operations.

/// Errors from serializing, parsing, or hashing for an [`crate::ApplianceManifest`].
#[derive(Debug, thiserror::Error)]
pub enum ApplianceError {
    /// JSON serialization or deserialization failed.
    #[error("appliance manifest JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Reading a build artifact for hashing failed.
    #[error("appliance artifact I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The manifest's `manifest_version` is not the supported schema version.
    #[error("unsupported appliance manifest_version {found} (this build supports {expected})")]
    UnsupportedVersion { found: u32, expected: u32 },
}
