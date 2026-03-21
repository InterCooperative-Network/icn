//! Genesis bundle schema for reproducible devnet bootstrap.
//!
//! A genesis bundle is a sealed, versioned document that describes the initial state
//! of an ICN network: founding member DIDs, seed peer addresses, an optional initial
//! cooperative, and optional pre-loaded CCL contracts.
//!
//! ## Reproducibility
//!
//! The bundle includes a `hash` field (SHA-256 of the canonical JSON without the hash
//! field itself). Nodes that load the same genesis bundle are guaranteed to start with
//! identical initial conditions. Use `GenesisBundle::compute_hash()` to verify integrity.
//!
//! ## Versioning
//!
//! `schema_version` is incremented when the bundle format changes in a backward-incompatible
//! way. Nodes reject bundles with unknown schema versions. The current version is 1.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Current schema version for genesis bundles.
pub const GENESIS_SCHEMA_VERSION: u32 = 1;

/// Genesis bundle — the sealed initial-state document for an ICN network.
///
/// Write with [`GenesisBundle::to_file`], read with [`GenesisBundle::from_file`].
/// Verify integrity with [`GenesisBundle::verify_hash`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenesisBundle {
    /// Schema version. Nodes reject bundles with unknown versions.
    pub schema_version: u32,

    /// Human-readable network identifier (e.g. "devnet-0", "acme-coop-mainnet").
    pub network_id: String,

    /// Unix timestamp (seconds) when this bundle was created.
    pub created_at: u64,

    /// DIDs of nodes that are founding members of this network.
    /// These peers are pre-trusted at genesis — no web-of-trust proofs required.
    pub initial_dids: Vec<String>,

    /// Bootstrap peer addresses in multiaddr or `host:port` format.
    /// Nodes will attempt to connect to all seed peers on startup.
    pub seed_peers: Vec<String>,

    /// Optional initial cooperative definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_coop: Option<InitialCoop>,

    /// Optional CCL contracts pre-loaded at genesis.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_contracts: Vec<InitialContract>,

    /// SHA-256 hex digest of the canonical bundle (without this field).
    ///
    /// Computed by serializing the bundle to canonical JSON (sorted keys,
    /// no whitespace) with `hash` set to `None`, then SHA-256 hashing the bytes.
    /// Set by [`GenesisBundle::seal`] and verified by [`GenesisBundle::verify_hash`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

/// Initial cooperative specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitialCoop {
    /// Cooperative name (human-readable).
    pub name: String,

    /// DID of the cooperative entity.
    pub did: String,

    /// DIDs of founding members.
    pub founding_members: Vec<String>,
}

/// A CCL contract pre-loaded at genesis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitialContract {
    /// Contract identifier (used as the CCL document ID).
    pub name: String,

    /// CCL source text.
    pub ccl_source: String,
}

impl GenesisBundle {
    /// Create a minimal genesis bundle for a single node.
    ///
    /// This is the default used by `icnd --init` when no bundle file is provided.
    /// It records the node's DID and config address so the bundle can be extended
    /// when additional nodes join.
    pub fn new_single_node(network_id: impl Into<String>, node_did: impl Into<String>) -> Self {
        Self {
            schema_version: GENESIS_SCHEMA_VERSION,
            network_id: network_id.into(),
            created_at: icn_time::current_timestamp_secs(),
            initial_dids: vec![node_did.into()],
            seed_peers: Vec::new(),
            initial_coop: None,
            initial_contracts: Vec::new(),
            hash: None,
        }
    }

    /// Compute the canonical SHA-256 hash of this bundle.
    ///
    /// The hash is computed over the bundle serialized to canonical JSON (sorted
    /// keys, no whitespace) with the `hash` field set to `None`. This means the
    /// hash covers all content except the hash field itself.
    pub fn compute_hash(&self) -> Result<String> {
        let mut hashable = self.clone();
        hashable.hash = None;

        let canonical = canonical_json(&hashable)?;
        let digest = sha256_hex(canonical.as_bytes());
        Ok(digest)
    }

    /// Seal the bundle: compute and store the hash.
    ///
    /// Returns `self` with `hash` set. Call this before writing to disk.
    pub fn seal(mut self) -> Result<Self> {
        let h = self.compute_hash()?;
        self.hash = Some(h);
        Ok(self)
    }

    /// Verify the bundle's stored hash matches the computed hash.
    ///
    /// Returns `Ok(())` if the hash is valid or absent (unsigned bundles are accepted).
    /// Returns `Err` if the stored hash does not match.
    pub fn verify_hash(&self) -> Result<()> {
        let Some(stored) = &self.hash else {
            return Ok(()); // unsigned bundle — allowed
        };
        let computed = self.compute_hash()?;
        anyhow::ensure!(
            *stored == computed,
            "genesis bundle hash mismatch: stored={stored}, computed={computed}"
        );
        Ok(())
    }

    /// Verify the schema version is supported.
    pub fn verify_version(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == GENESIS_SCHEMA_VERSION,
            "unsupported genesis bundle schema version {} (supported: {})",
            self.schema_version,
            GENESIS_SCHEMA_VERSION,
        );
        Ok(())
    }

    /// Write the bundle as pretty-printed JSON to a file.
    pub fn to_file(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize genesis bundle")?;
        std::fs::write(path, json)
            .with_context(|| format!("failed to write genesis bundle to {}", path.display()))
    }

    /// Load a genesis bundle from a JSON file and verify its hash and version.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read genesis bundle from {}", path.display()))?;
        let bundle: Self =
            serde_json::from_str(&content).context("failed to parse genesis bundle JSON")?;
        bundle.verify_version()?;
        bundle.verify_hash()?;
        Ok(bundle)
    }
}

/// Serialize a value to canonical JSON (sorted keys, no whitespace).
///
/// Uses a `BTreeMap` round-trip to ensure deterministic key ordering.
fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    // Serialize to serde_json::Value, then into a BTreeMap for sorted keys,
    // then back to a compact string.
    let v: serde_json::Value =
        serde_json::to_value(value).context("canonical_json: serialization failed")?;
    let sorted = sort_value(v);
    serde_json::to_string(&sorted).context("canonical_json: re-serialization failed")
}

/// Recursively sort JSON object keys for canonical encoding.
fn sort_value(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<String, serde_json::Value> =
                map.into_iter().map(|(k, v)| (k, sort_value(v))).collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(sort_value).collect())
        }
        other => other,
    }
}

/// Compute the SHA-256 hex digest of bytes using the ring crate.
fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write as _;
    // Use a simple SHA-256 implementation via sha2 (already in workspace deps via icn-crypto-pq)
    // We use the raw bytes approach compatible with what's available.
    // Fallback: use icn_crypto_pq or a direct dependency.
    // For now, implement using the sha2 crate that is already available in the workspace.
    let digest = sha2_256(data);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// SHA-256 using the `sha2` crate.
fn sha2_256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_bundle_roundtrip() {
        let bundle = GenesisBundle::new_single_node("devnet-0", "did:icn:test123");
        let sealed = bundle.seal().unwrap();

        assert!(sealed.hash.is_some());
        sealed.verify_hash().unwrap();
        sealed.verify_version().unwrap();
    }

    #[test]
    fn test_genesis_bundle_hash_tamper_detection() {
        let bundle = GenesisBundle::new_single_node("devnet-0", "did:icn:test123");
        let mut sealed = bundle.seal().unwrap();

        // Tamper with a field
        sealed.network_id = "devnet-EVIL".to_string();

        assert!(
            sealed.verify_hash().is_err(),
            "tampered bundle should fail hash check"
        );
    }

    #[test]
    fn test_genesis_bundle_file_roundtrip() {
        let bundle = GenesisBundle {
            schema_version: GENESIS_SCHEMA_VERSION,
            network_id: "test-net".to_string(),
            created_at: 1000000,
            initial_dids: vec!["did:icn:alice".to_string(), "did:icn:bob".to_string()],
            seed_peers: vec!["127.0.0.1:9000".to_string()],
            initial_coop: Some(InitialCoop {
                name: "Test Coop".to_string(),
                did: "did:icn:testcoop".to_string(),
                founding_members: vec!["did:icn:alice".to_string()],
            }),
            initial_contracts: vec![InitialContract {
                name: "basic-charter".to_string(),
                ccl_source: "// charter".to_string(),
            }],
            hash: None,
        };
        let sealed = bundle.seal().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("genesis.json");
        sealed.to_file(&path).unwrap();

        let loaded = GenesisBundle::from_file(&path).unwrap();
        assert_eq!(loaded, sealed);
    }

    #[test]
    fn test_canonical_json_deterministic() {
        let b1 = GenesisBundle::new_single_node("net-1", "did:icn:a");
        let b2 = GenesisBundle::new_single_node("net-1", "did:icn:a");
        // Same content but different instances should produce identical canonical JSON
        let j1 = canonical_json(&b1).unwrap();
        let j2 = canonical_json(&b2).unwrap();
        assert_eq!(j1, j2);
    }

    #[test]
    fn test_schema_version_check() {
        let mut bundle = GenesisBundle::new_single_node("net", "did:icn:x")
            .seal()
            .unwrap();
        bundle.schema_version = 99;
        // Bypass hash (no need to re-seal for this test)
        bundle.hash = None;
        assert!(bundle.verify_version().is_err());
    }
}
