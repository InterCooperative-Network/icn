//! App Manifest Schema
//!
//! Defines the YAML manifest format for ICN apps.
//!
//! # Example Manifest
//!
//! ```yaml
//! name: trust
//! version: 1.0.0
//! publisher: did:icn:icn-foundation
//!
//! capabilities_required:
//!   - state:logs:append:self
//!   - state:kv:read:self
//!   - comms:subscribe:trust:*
//!
//! capabilities_provided:
//!   - oracle:trust:*
//!
//! state:
//!   logs:
//!     - name: attestations
//!       ordering: causal
//!   kv:
//!     - name: scores
//!
//! compute:
//!   reducers:
//!     - name: attestation_reducer
//!       event_type: trust:attestation
//!   services:
//!     - name: score_query
//!       request_type: trust:query
//!
//! oracle:
//!   domain: trust
//! ```

use icn_kernel_api::bootstrap::CapabilityRequest;
use icn_kernel_api::types::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// App manifest defining an app's requirements and capabilities.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// App name (unique identifier within publisher namespace)
    pub name: String,
    /// Semantic version
    pub version: String,
    /// Publisher DID
    pub publisher: Did,
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// Capabilities this app requires to function
    #[serde(default)]
    pub capabilities_required: Vec<String>,
    /// Capabilities this app provides to others
    #[serde(default)]
    pub capabilities_provided: Vec<String>,
    /// State configuration
    #[serde(default)]
    pub state: StateConfig,
    /// Compute handlers
    #[serde(default)]
    pub compute: ComputeConfig,
    /// Oracle configuration (if app provides a PolicyOracle)
    #[serde(default)]
    pub oracle: Option<OracleConfig>,
    /// App-specific metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Manifest {
    /// Load a manifest from a YAML file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ManifestError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| ManifestError::Io {
            path: path.as_ref().display().to_string(),
            source: e,
        })?;
        Self::parse(&content)
    }

    /// Parse a manifest from YAML string.
    pub fn parse(yaml: &str) -> Result<Self, ManifestError> {
        let manifest: Manifest =
            serde_yaml::from_str(yaml).map_err(|e| ManifestError::Parse(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest.
    pub fn validate(&self) -> Result<(), ManifestError> {
        // Name must not be empty
        if self.name.is_empty() {
            return Err(ManifestError::Validation(
                "App name cannot be empty".to_string(),
            ));
        }

        // Name must be valid identifier
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ManifestError::Validation(format!(
                "App name '{}' contains invalid characters",
                self.name
            )));
        }

        // Version must be valid semver
        semver::Version::parse(&self.version).map_err(|e| {
            ManifestError::Validation(format!("Invalid semver version '{}': {e}", self.version))
        })?;

        // Publisher must be a DID
        if !self.publisher.starts_with("did:") {
            return Err(ManifestError::Validation(format!(
                "Publisher '{}' must be a DID",
                self.publisher
            )));
        }

        // Validate capability request syntax
        for cap in &self.capabilities_required {
            if CapabilityRequest::parse(cap).is_none() {
                return Err(ManifestError::Validation(format!(
                    "Invalid capability request syntax: '{}'",
                    cap
                )));
            }
        }

        // Validate state config
        self.state.validate()?;

        // Validate compute config
        self.compute.validate()?;

        Ok(())
    }

    /// Get the app's namespace.
    ///
    /// Format: `/{publisher}/{name}`
    pub fn namespace(&self) -> String {
        format!("/{}/{}", self.publisher, self.name)
    }

    /// Get the app's unique ID.
    ///
    /// Format: `{publisher}:{name}:{version}`
    pub fn app_id(&self) -> String {
        format!("{}:{}:{}", self.publisher, self.name, self.version)
    }

    /// Check if this app provides a PolicyOracle.
    pub fn provides_oracle(&self) -> bool {
        self.oracle.is_some()
    }

    /// Parse capability requests.
    pub fn parsed_capability_requests(&self) -> Vec<CapabilityRequest> {
        self.capabilities_required
            .iter()
            .filter_map(|s| CapabilityRequest::parse(s))
            .collect()
    }
}

/// State configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StateConfig {
    /// Log stores
    #[serde(default)]
    pub logs: Vec<LogConfig>,
    /// Key-value stores
    #[serde(default)]
    pub kv: Vec<KvConfig>,
    /// Blob stores
    #[serde(default)]
    pub blobs: Vec<BlobConfig>,
}

impl StateConfig {
    fn validate(&self) -> Result<(), ManifestError> {
        // Check for duplicate names
        let mut names = std::collections::HashSet::new();

        for log in &self.logs {
            if !names.insert(&log.name) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate state name: '{}'",
                    log.name
                )));
            }
            log.validate()?;
        }

        for kv in &self.kv {
            if !names.insert(&kv.name) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate state name: '{}'",
                    kv.name
                )));
            }
        }

        for blob in &self.blobs {
            if !names.insert(&blob.name) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate state name: '{}'",
                    blob.name
                )));
            }
        }

        Ok(())
    }
}

/// Log store configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogConfig {
    /// Store name
    pub name: String,
    /// Ordering guarantee
    #[serde(default)]
    pub ordering: LogOrdering,
}

impl LogConfig {
    fn validate(&self) -> Result<(), ManifestError> {
        if self.name.is_empty() {
            return Err(ManifestError::Validation(
                "Log name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Log ordering guarantee.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogOrdering {
    /// Strict total ordering
    #[default]
    Total,
    /// Causal ordering (happens-before)
    Causal,
    /// Concurrent (CRDTs resolve conflicts)
    Concurrent,
}

/// Key-value store configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KvConfig {
    /// Store name
    pub name: String,
}

/// Blob store configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobConfig {
    /// Store name
    pub name: String,
    /// Maximum blob size in bytes
    #[serde(default)]
    pub max_size: Option<u64>,
}

/// Compute configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ComputeConfig {
    /// Reducers (pure, deterministic event handlers)
    #[serde(default)]
    pub reducers: Vec<ReducerConfig>,
    /// Services (async request handlers)
    #[serde(default)]
    pub services: Vec<ServiceConfig>,
}

impl ComputeConfig {
    fn validate(&self) -> Result<(), ManifestError> {
        // Check for duplicate handler names
        let mut names = std::collections::HashSet::new();

        for reducer in &self.reducers {
            if !names.insert(&reducer.name) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate handler name: '{}'",
                    reducer.name
                )));
            }
            reducer.validate()?;
        }

        for service in &self.services {
            if !names.insert(&service.name) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate handler name: '{}'",
                    service.name
                )));
            }
            service.validate()?;
        }

        Ok(())
    }
}

/// Reducer configuration.
///
/// Reducers are pure, deterministic functions that process events
/// and update state. They run synchronously with no async runtime access.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReducerConfig {
    /// Handler name
    pub name: String,
    /// Event type this reducer handles (e.g., "trust:attestation")
    pub event_type: String,
}

impl ReducerConfig {
    fn validate(&self) -> Result<(), ManifestError> {
        if self.name.is_empty() {
            return Err(ManifestError::Validation(
                "Reducer name cannot be empty".to_string(),
            ));
        }
        if self.event_type.is_empty() {
            return Err(ManifestError::Validation(format!(
                "Reducer '{}' must have an event_type",
                self.name
            )));
        }
        Ok(())
    }
}

/// Service configuration.
///
/// Services are async request handlers that can call external systems
/// but must emit events (not mutate state directly).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Handler name
    pub name: String,
    /// Request type this service handles (e.g., "trust:query")
    pub request_type: String,
}

impl ServiceConfig {
    fn validate(&self) -> Result<(), ManifestError> {
        if self.name.is_empty() {
            return Err(ManifestError::Validation(
                "Service name cannot be empty".to_string(),
            ));
        }
        if self.request_type.is_empty() {
            return Err(ManifestError::Validation(format!(
                "Service '{}' must have a request_type",
                self.name
            )));
        }
        Ok(())
    }
}

/// Oracle configuration (for apps that provide PolicyOracle).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OracleConfig {
    /// Domain this oracle handles (e.g., "trust", "governance")
    pub domain: String,
}

/// Manifest parsing/validation errors.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// IO error reading manifest file
    #[error("Failed to read manifest from '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// YAML parse error
    #[error("Failed to parse manifest: {0}")]
    Parse(String),
    /// Validation error
    #[error("Manifest validation failed: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECHO_MANIFEST: &str = r#"
name: echo
version: 0.1.0
publisher: did:icn:test

capabilities_required:
  - state:kv:write:self
  - state:kv:read:self

state:
  kv:
    - name: echoes

compute:
  reducers:
    - name: echo_reducer
      event_type: echo:message
  services:
    - name: echo_query
      request_type: echo:query
"#;

    const TRUST_MANIFEST: &str = r#"
name: trust
version: 1.0.0
publisher: did:icn:icn-foundation

capabilities_required:
  - state:logs:append:self
  - state:kv:read:self
  - state:kv:write:self
  - comms:subscribe:trust:*

capabilities_provided:
  - oracle:trust:*

state:
  logs:
    - name: attestations
      ordering: causal
  kv:
    - name: scores
    - name: graph

compute:
  reducers:
    - name: attestation_reducer
      event_type: trust:attestation
  services:
    - name: score_query
      request_type: trust:query

oracle:
  domain: trust
"#;

    #[test]
    fn test_parse_echo_manifest() {
        let manifest = Manifest::parse(ECHO_MANIFEST).unwrap();

        assert_eq!(manifest.name, "echo");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.publisher, "did:icn:test");
        assert_eq!(manifest.capabilities_required.len(), 2);
        assert_eq!(manifest.state.kv.len(), 1);
        assert_eq!(manifest.compute.reducers.len(), 1);
        assert_eq!(manifest.compute.services.len(), 1);
        assert!(!manifest.provides_oracle());
    }

    #[test]
    fn test_parse_trust_manifest() {
        let manifest = Manifest::parse(TRUST_MANIFEST).unwrap();

        assert_eq!(manifest.name, "trust");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.publisher, "did:icn:icn-foundation");
        assert_eq!(manifest.capabilities_required.len(), 4);
        assert_eq!(manifest.capabilities_provided.len(), 1);
        assert_eq!(manifest.state.logs.len(), 1);
        assert_eq!(manifest.state.logs[0].ordering, LogOrdering::Causal);
        assert_eq!(manifest.state.kv.len(), 2);
        assert!(manifest.provides_oracle());
        assert_eq!(manifest.oracle.as_ref().unwrap().domain, "trust");
    }

    #[test]
    fn test_manifest_namespace() {
        let manifest = Manifest::parse(ECHO_MANIFEST).unwrap();
        assert_eq!(manifest.namespace(), "/did:icn:test/echo");
    }

    #[test]
    fn test_manifest_app_id() {
        let manifest = Manifest::parse(ECHO_MANIFEST).unwrap();
        assert_eq!(manifest.app_id(), "did:icn:test:echo:0.1.0");
    }

    #[test]
    fn test_invalid_manifest_empty_name() {
        let yaml = r#"
name: ""
version: 1.0.0
publisher: did:icn:test
"#;
        let result = Manifest::parse(yaml);
        assert!(matches!(result, Err(ManifestError::Validation(_))));
    }

    #[test]
    fn test_invalid_manifest_bad_publisher() {
        let yaml = r#"
name: test
version: 1.0.0
publisher: not-a-did
"#;
        let result = Manifest::parse(yaml);
        assert!(matches!(result, Err(ManifestError::Validation(_))));
    }

    #[test]
    fn test_invalid_manifest_duplicate_state() {
        let yaml = r#"
name: test
version: 1.0.0
publisher: did:icn:test
state:
  kv:
    - name: data
    - name: data
"#;
        let result = Manifest::parse(yaml);
        assert!(matches!(result, Err(ManifestError::Validation(_))));
    }

    #[test]
    fn test_invalid_manifest_reducer_no_event() {
        let yaml = r#"
name: test
version: 1.0.0
publisher: did:icn:test
compute:
  reducers:
    - name: my_reducer
      event_type: ""
"#;
        let result = Manifest::parse(yaml);
        assert!(matches!(result, Err(ManifestError::Validation(_))));
    }

    #[test]
    fn test_parsed_capability_requests() {
        let manifest = Manifest::parse(ECHO_MANIFEST).unwrap();
        let reqs = manifest.parsed_capability_requests();

        // "state:kv:write:self" -> resource="state:kv:write", action="self"
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].resource, "state:kv:write");
        assert_eq!(reqs[0].action, "self");
    }

    #[test]
    fn test_log_ordering_default() {
        let yaml = r#"
name: test
version: 1.0.0
publisher: did:icn:test
state:
  logs:
    - name: events
"#;
        let manifest = Manifest::parse(yaml).unwrap();
        assert_eq!(manifest.state.logs[0].ordering, LogOrdering::Total);
    }
}
