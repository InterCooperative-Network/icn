//! The wire-stable appliance build manifest.

use serde::{Deserialize, Serialize};

use crate::error::ApplianceError;

/// Current schema version of [`ApplianceManifest`]. Bump only on a
/// schema-breaking change to the field set.
pub const MANIFEST_VERSION: u32 = 1;

/// A single binary installed into the appliance image, with the content hash
/// of the artifact that was built and copied in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryRecord {
    /// Absolute path of the binary inside the image (e.g. `/usr/local/bin/icnd`).
    pub path: String,
    /// Repo-relative source the binary was built from (e.g.
    /// `icn/target/release/icnd`).
    pub source: String,
    /// Lowercase hex SHA-256 of the built binary.
    pub sha256: String,
}

/// Self-describing record of what an ICN appliance image build produced.
///
/// This is the wire-stable contract emitted by the appliance build path. It is
/// intentionally flat and free of host-protocol semantics: it describes a build
/// artifact and its provenance, nothing more. `#[serde(deny_unknown_fields)]`
/// makes emitter/consumer drift a hard error rather than a silent mismatch.
///
/// Field names serialize as `snake_case` (serde default) to stay byte-compatible
/// with the manifest `deploy/appliance/build-image.sh` already emits and the
/// `appliance.manifest.example.yaml` template. This is a build-record artifact,
/// not a gateway API boundary, so the camelCase JSON convention does not apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplianceManifest {
    /// Schema version; see [`MANIFEST_VERSION`].
    pub manifest_version: u32,
    /// Stable, operator-meaningful identifier for the appliance.
    pub appliance_id: String,
    /// Version label for this image build.
    pub version: String,
    /// Target architecture (e.g. `amd64`).
    pub arch: String,
    /// Emitted image format (e.g. `qcow2`, `raw`).
    pub image_format: String,
    /// Host-local path of the built image (build-host context, not portable).
    pub image_path: String,
    /// Lowercase hex SHA-256 of the built image.
    pub image_sha256: String,
    /// Host-local path of the base OS image the build layered on.
    pub base_image_path: String,
    /// Lowercase hex SHA-256 of the base OS image, or `unset` if not verified.
    pub base_image_sha256: String,
    /// Git commit the build was produced from.
    pub git_commit: String,
    /// Build time, RFC 3339 UTC.
    pub build_timestamp_utc: String,
    /// Binaries installed into the image, with content hashes.
    pub built_binaries: Vec<BinaryRecord>,
    /// True when the image is a non-production dev artifact.
    pub non_production: bool,
    /// True when the image is a signed release artifact.
    pub signed: bool,
    /// True when the image is immutable (A-B updates, rollback).
    pub immutable: bool,
    /// True when the DEV/DEMO profile payload was installed.
    pub demo_profile: bool,
}

impl ApplianceManifest {
    /// Serialize to pretty JSON.
    pub fn to_json_pretty(&self) -> Result<String, ApplianceError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse from a JSON string, rejecting unknown fields.
    pub fn from_json_str(s: &str) -> Result<Self, ApplianceError> {
        Ok(serde_json::from_str(s)?)
    }
}
