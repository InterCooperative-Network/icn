//! State snapshot for graceful restarts
//!
//! This crate provides serializable state types for persisting runtime state
//! across daemon restarts. It is intentionally dependency-free (except serde)
//! to avoid circular dependencies.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use icn_snapshot::{StateSnapshot, save_snapshot, load_snapshot};
//!
//! // Before shutdown
//! let mut snapshot = StateSnapshot::new();
//! snapshot.gossip_state = Some(gossip_actor.export_state());
//! snapshot.network_state = Some(network_actor.export_state());
//! save_snapshot(&snapshot, "/path/to/data")?;
//!
//! // On startup
//! if let Some(snapshot) = load_snapshot("/path/to/data")? {
//!     if let Some(gossip_state) = snapshot.gossip_state {
//!         gossip_actor.restore_state(gossip_state)?;
//!     }
//!     if let Some(network_state) = snapshot.network_state {
//!         network_actor.restore_state(network_state)?;
//!     }
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Filename for state snapshot
const SNAPSHOT_FILENAME: &str = "state.snapshot";

/// Complete state snapshot for graceful restart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Version of the snapshot format (for future migrations)
    pub version: u32,

    /// Timestamp when snapshot was created (Unix seconds)
    pub created_at: u64,

    /// Gossip actor state
    pub gossip_state: Option<GossipState>,

    /// Network actor state
    pub network_state: Option<NetworkState>,
}

impl StateSnapshot {
    /// Create a new snapshot with current timestamp
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            version: 1,
            created_at: now,
            gossip_state: None,
            network_state: None,
        }
    }
}

impl Default for StateSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Gossip actor state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipState {
    /// Vector clock state (peer DID string -> clock value)
    pub vector_clock: HashMap<String, u64>,

    /// Topic subscriptions (topic name -> list of subscriber DID strings)
    pub subscriptions: HashMap<String, Vec<String>>,

    /// Topic metadata (topic name -> TopicMetadata)
    pub topics: HashMap<String, TopicMetadata>,
}

/// Serializable topic metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMetadata {
    pub name: String,
    pub access_control: String,  // Serialized AccessControl enum
    pub max_entries: usize,
    pub scope: String,  // Serialized Scope enum
}

/// Network actor state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkState {
    /// Peer X25519 public keys for end-to-end encryption
    /// Map of DID string -> 32-byte X25519 public key
    pub peer_x25519_keys: HashMap<String, [u8; 32]>,

    /// Known peer addresses (DID string -> last known SocketAddr string)
    /// Note: These may be stale after restart, but can help with reconnection
    pub peer_addresses: HashMap<String, String>,
}

/// Save state snapshot to disk
///
/// Snapshot is saved to `{data_dir}/state.snapshot` as JSON
pub fn save_snapshot(snapshot: &StateSnapshot, data_dir: impl AsRef<Path>) -> Result<()> {
    let path = snapshot_path(data_dir);

    // Serialize to JSON
    let json = serde_json::to_vec_pretty(snapshot)
        .context("Failed to serialize snapshot")?;

    // Write atomically using temp file + rename
    let temp_path = path.with_extension("snapshot.tmp");
    std::fs::write(&temp_path, json)
        .context("Failed to write snapshot")?;
    std::fs::rename(&temp_path, &path)
        .context("Failed to rename snapshot")?;

    Ok(())
}

/// Load state snapshot from disk
///
/// Returns None if snapshot doesn't exist, Some(snapshot) if it exists and is valid
pub fn load_snapshot(data_dir: impl AsRef<Path>) -> Result<Option<StateSnapshot>> {
    let path = snapshot_path(data_dir);

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read(&path)
        .context("Failed to read snapshot")?;

    let snapshot: StateSnapshot = serde_json::from_slice(&json)
        .context("Failed to deserialize snapshot")?;

    Ok(Some(snapshot))
}

/// Delete state snapshot
///
/// Used after successful restore or when explicitly clearing state
pub fn delete_snapshot(data_dir: impl AsRef<Path>) -> Result<()> {
    let path = snapshot_path(data_dir);

    if path.exists() {
        std::fs::remove_file(&path)
            .context("Failed to delete snapshot")?;
    }

    Ok(())
}

/// Get the snapshot file path
fn snapshot_path(data_dir: impl AsRef<Path>) -> PathBuf {
    data_dir.as_ref().join(SNAPSHOT_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_save_and_load_snapshot() {
        let temp = std::env::temp_dir().join("icn-snapshot-test");
        std::fs::create_dir_all(&temp).unwrap();

        let mut snapshot = StateSnapshot::new();
        snapshot.gossip_state = Some(GossipState {
            vector_clock: [("did:icn:alice".to_string(), 42)]
                .iter()
                .cloned()
                .collect(),
            subscriptions: [("topic:test".to_string(), vec!["did:icn:bob".to_string()])]
                .iter()
                .cloned()
                .collect(),
            topics: HashMap::new(),
        });

        // Save
        save_snapshot(&snapshot, &temp).unwrap();

        // Load
        let loaded = load_snapshot(&temp).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(
            loaded.gossip_state.as_ref().unwrap().vector_clock.get("did:icn:alice"),
            Some(&42)
        );
        assert_eq!(
            loaded.gossip_state.as_ref().unwrap().subscriptions.get("topic:test").unwrap().len(),
            1
        );

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_load_nonexistent_snapshot() {
        let temp = std::env::temp_dir().join("icn-snapshot-nonexistent");
        let loaded = load_snapshot(&temp).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_delete_snapshot() {
        let temp = std::env::temp_dir().join("icn-snapshot-delete");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        save_snapshot(&snapshot, &temp).unwrap();
        assert!(snapshot_path(&temp).exists());

        delete_snapshot(&temp).unwrap();
        assert!(!snapshot_path(&temp).exists());

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_network_state() {
        let temp = std::env::temp_dir().join("icn-snapshot-network");
        std::fs::create_dir_all(&temp).unwrap();

        let mut snapshot = StateSnapshot::new();
        snapshot.network_state = Some(NetworkState {
            peer_x25519_keys: [(
                "did:icn:alice".to_string(),
                [1u8; 32],
            )]
            .iter()
            .cloned()
            .collect(),
            peer_addresses: [(
                "did:icn:alice".to_string(),
                "127.0.0.1:5000".to_string(),
            )]
            .iter()
            .cloned()
            .collect(),
        });

        save_snapshot(&snapshot, &temp).unwrap();
        let loaded = load_snapshot(&temp).unwrap().unwrap();

        let net_state = loaded.network_state.unwrap();
        assert_eq!(net_state.peer_x25519_keys.get("did:icn:alice"), Some(&[1u8; 32]));
        assert_eq!(
            net_state.peer_addresses.get("did:icn:alice"),
            Some(&"127.0.0.1:5000".to_string())
        );

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
