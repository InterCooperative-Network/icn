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

    /// The manifest declares no built binaries.
    #[error("appliance manifest has no built_binaries (no binary attestations is not valid)")]
    EmptyBinaries,

    /// The posture flags are internally contradictory.
    #[error("appliance manifest posture contradiction: {detail}")]
    PostureContradiction { detail: String },

    /// A recorded artifact hash did not match the file on disk.
    #[error("appliance artifact hash mismatch for {path}: manifest={expected}, actual={found}")]
    HashMismatch {
        path: String,
        expected: String,
        found: String,
    },

    /// A recorded artifact could not be read for re-hashing.
    #[error("appliance artifact missing or unreadable at {path}: {detail}")]
    MissingArtifact { path: String, detail: String },
}
