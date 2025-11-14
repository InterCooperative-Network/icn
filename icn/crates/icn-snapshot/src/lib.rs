//! State snapshot for graceful restarts
//!
//! This crate provides serializable state types for persisting runtime state
//! across daemon restarts. It is intentionally dependency-free (except serde)
//! to avoid circular dependencies.
//!
//! ## Usage
//!
//! ```rust,ignore
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
//! ```

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Filename for state snapshot
const SNAPSHOT_FILENAME: &str = "state.snapshot";

/// Filename for snapshot checksum
const CHECKSUM_FILENAME: &str = "state.snapshot.sha256";

/// Default number of snapshots to keep
const DEFAULT_SNAPSHOT_RETENTION: usize = 3;

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

/// Save state snapshot to disk with SHA256 checksum
///
/// Snapshot is saved to `{data_dir}/state.snapshot` as JSON
/// Checksum is saved to `{data_dir}/state.snapshot.sha256`
pub fn save_snapshot(snapshot: &StateSnapshot, data_dir: impl AsRef<Path>) -> Result<()> {
    let path = snapshot_path(&data_dir);
    let checksum_path = checksum_path(&data_dir);

    // Serialize to JSON
    let json = serde_json::to_vec_pretty(snapshot)
        .context("Failed to serialize snapshot")?;

    // Compute SHA256 checksum
    let checksum = compute_checksum(&json);

    // Write atomically using temp file + rename
    let temp_path = path.with_extension("snapshot.tmp");
    let temp_checksum_path = checksum_path.with_extension("sha256.tmp");

    std::fs::write(&temp_path, &json)
        .context("Failed to write snapshot")?;
    std::fs::write(&temp_checksum_path, &checksum)
        .context("Failed to write checksum")?;

    std::fs::rename(&temp_path, &path)
        .context("Failed to rename snapshot")?;
    std::fs::rename(&temp_checksum_path, &checksum_path)
        .context("Failed to rename checksum")?;

    Ok(())
}

/// Load state snapshot from disk with checksum verification
///
/// Returns None if snapshot doesn't exist, Some(snapshot) if it exists and is valid
/// Returns error if snapshot is corrupted (checksum mismatch)
pub fn load_snapshot(data_dir: impl AsRef<Path>) -> Result<Option<StateSnapshot>> {
    let path = snapshot_path(&data_dir);
    let checksum_path = checksum_path(&data_dir);

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read(&path)
        .context("Failed to read snapshot")?;

    // Verify checksum if it exists
    if checksum_path.exists() {
        let expected_checksum = std::fs::read_to_string(&checksum_path)
            .context("Failed to read checksum")?;
        let actual_checksum = compute_checksum(&json);

        if expected_checksum.trim() != actual_checksum {
            return Err(anyhow!(
                "Snapshot checksum mismatch! Expected: {}, Actual: {}. Snapshot may be corrupted.",
                expected_checksum.trim(),
                actual_checksum
            ));
        }
    } else {
        // Checksum file doesn't exist - this is OK for legacy snapshots
        // but we should log a warning (caller can do this)
    }

    let snapshot: StateSnapshot = serde_json::from_slice(&json)
        .context("Failed to deserialize snapshot")?;

    Ok(Some(snapshot))
}

/// Delete state snapshot and checksum
///
/// Used after successful restore or when explicitly clearing state
pub fn delete_snapshot(data_dir: impl AsRef<Path>) -> Result<()> {
    let path = snapshot_path(&data_dir);
    let checksum_path = checksum_path(&data_dir);

    if path.exists() {
        std::fs::remove_file(&path)
            .context("Failed to delete snapshot")?;
    }

    if checksum_path.exists() {
        std::fs::remove_file(&checksum_path)
            .context("Failed to delete checksum")?;
    }

    Ok(())
}

/// Verify snapshot checksum without loading
///
/// Returns Ok(()) if checksum is valid, Err if corrupted or checksum missing
pub fn verify_snapshot(data_dir: impl AsRef<Path>) -> Result<()> {
    let path = snapshot_path(&data_dir);
    let checksum_path = checksum_path(&data_dir);

    if !path.exists() {
        return Err(anyhow!("Snapshot does not exist"));
    }

    if !checksum_path.exists() {
        return Err(anyhow!("Checksum file does not exist (legacy snapshot?)"));
    }

    let json = std::fs::read(&path)
        .context("Failed to read snapshot")?;
    let expected_checksum = std::fs::read_to_string(&checksum_path)
        .context("Failed to read checksum")?;
    let actual_checksum = compute_checksum(&json);

    if expected_checksum.trim() != actual_checksum {
        return Err(anyhow!(
            "Snapshot checksum mismatch! Expected: {}, Actual: {}",
            expected_checksum.trim(),
            actual_checksum
        ));
    }

    Ok(())
}

/// List all timestamped snapshots in data directory
///
/// Returns (filename, timestamp, size_bytes) sorted by timestamp (newest first)
pub fn list_snapshots(data_dir: impl AsRef<Path>) -> Result<Vec<(String, u64, u64)>> {
    let data_dir = data_dir.as_ref();

    if !data_dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();

    for entry in std::fs::read_dir(data_dir).context("Failed to read data directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Match pattern: state.snapshot.{timestamp}
        if filename_str.starts_with("state.snapshot.") && !filename_str.ends_with(".sha256") {
            if let Some(timestamp_str) = filename_str.strip_prefix("state.snapshot.") {
                if let Ok(timestamp) = timestamp_str.parse::<u64>() {
                    let metadata = entry.metadata().context("Failed to read file metadata")?;
                    snapshots.push((filename_str.to_string(), timestamp, metadata.len()));
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    snapshots.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(snapshots)
}

/// Clean up old timestamped snapshots, keeping only the most recent N
///
/// This does NOT delete the primary snapshot (state.snapshot)
pub fn cleanup_old_snapshots(data_dir: impl AsRef<Path>, keep_count: usize) -> Result<usize> {
    let data_dir = data_dir.as_ref();
    let snapshots = list_snapshots(data_dir)?;

    if snapshots.len() <= keep_count {
        return Ok(0);
    }

    let mut deleted_count = 0;

    // Delete all snapshots beyond the keep_count
    for (filename, _, _) in snapshots.iter().skip(keep_count) {
        let snapshot_path = data_dir.join(filename);
        let checksum_path = data_dir.join(format!("{}.sha256", filename));

        // Delete snapshot file
        if snapshot_path.exists() {
            std::fs::remove_file(&snapshot_path)
                .context(format!("Failed to delete snapshot: {}", filename))?;
            deleted_count += 1;
        }

        // Delete associated checksum file
        if checksum_path.exists() {
            std::fs::remove_file(&checksum_path)
                .context(format!("Failed to delete checksum: {}.sha256", filename))?;
        }
    }

    Ok(deleted_count)
}

/// Save a timestamped backup snapshot in addition to the primary snapshot
///
/// Creates state.snapshot.{unix_timestamp} for archival purposes
pub fn save_timestamped_snapshot(snapshot: &StateSnapshot, data_dir: impl AsRef<Path>) -> Result<()> {
    let data_dir = data_dir.as_ref();
    let timestamp = snapshot.created_at;
    let timestamped_filename = format!("state.snapshot.{}", timestamp);
    let timestamped_path = data_dir.join(&timestamped_filename);
    let timestamped_checksum_path = data_dir.join(format!("{}.sha256", timestamped_filename));

    // Serialize to JSON
    let json = serde_json::to_vec_pretty(snapshot)
        .context("Failed to serialize snapshot")?;

    // Compute SHA256 checksum
    let checksum = compute_checksum(&json);

    // Write timestamped snapshot
    std::fs::write(&timestamped_path, &json)
        .context("Failed to write timestamped snapshot")?;
    std::fs::write(&timestamped_checksum_path, &checksum)
        .context("Failed to write timestamped checksum")?;

    Ok(())
}

/// Compute SHA256 checksum of data
fn compute_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Get the snapshot file path
fn snapshot_path(data_dir: impl AsRef<Path>) -> PathBuf {
    data_dir.as_ref().join(SNAPSHOT_FILENAME)
}

/// Get the checksum file path
fn checksum_path(data_dir: impl AsRef<Path>) -> PathBuf {
    data_dir.as_ref().join(CHECKSUM_FILENAME)
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

    #[test]
    fn test_checksum_creation() {
        let temp = std::env::temp_dir().join("icn-snapshot-checksum");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        save_snapshot(&snapshot, &temp).unwrap();

        // Verify checksum file was created
        let checksum_path = checksum_path(&temp);
        assert!(checksum_path.exists(), "Checksum file should be created");

        // Verify checksum is valid hex SHA256 (64 characters)
        let checksum = std::fs::read_to_string(&checksum_path).unwrap();
        assert_eq!(checksum.trim().len(), 64, "SHA256 should be 64 hex characters");
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit() || c.is_whitespace()));

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_checksum_verification_success() {
        let temp = std::env::temp_dir().join("icn-snapshot-verify-success");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        save_snapshot(&snapshot, &temp).unwrap();

        // Verify should succeed
        verify_snapshot(&temp).unwrap();

        // Load should succeed
        let loaded = load_snapshot(&temp).unwrap();
        assert!(loaded.is_some());

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_checksum_corruption_detected() {
        let temp = std::env::temp_dir().join("icn-snapshot-corrupted");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        save_snapshot(&snapshot, &temp).unwrap();

        // Corrupt the snapshot by modifying its content
        let snapshot_path = snapshot_path(&temp);
        let mut content = std::fs::read_to_string(&snapshot_path).unwrap();
        content.push_str("CORRUPTED");
        std::fs::write(&snapshot_path, content).unwrap();

        // Verify should fail
        let verify_result = verify_snapshot(&temp);
        assert!(verify_result.is_err(), "Verification should fail for corrupted snapshot");
        assert!(verify_result.unwrap_err().to_string().contains("checksum mismatch"));

        // Load should fail
        let load_result = load_snapshot(&temp);
        assert!(load_result.is_err(), "Load should fail for corrupted snapshot");
        assert!(load_result.unwrap_err().to_string().contains("checksum mismatch"));

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_legacy_snapshot_without_checksum() {
        let temp = std::env::temp_dir().join("icn-snapshot-legacy");
        std::fs::create_dir_all(&temp).unwrap();

        // Create snapshot manually without checksum
        let snapshot = StateSnapshot::new();
        let json = serde_json::to_vec_pretty(&snapshot).unwrap();
        std::fs::write(snapshot_path(&temp), json).unwrap();

        // Verify should fail (checksum missing)
        let verify_result = verify_snapshot(&temp);
        assert!(verify_result.is_err());
        assert!(verify_result.unwrap_err().to_string().contains("Checksum file does not exist"));

        // But load should still succeed (backward compatibility)
        let loaded = load_snapshot(&temp).unwrap();
        assert!(loaded.is_some(), "Legacy snapshots without checksums should still load");

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_delete_removes_checksum() {
        let temp = std::env::temp_dir().join("icn-snapshot-delete-checksum");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        save_snapshot(&snapshot, &temp).unwrap();

        // Both files should exist
        assert!(snapshot_path(&temp).exists());
        assert!(checksum_path(&temp).exists());

        // Delete should remove both
        delete_snapshot(&temp).unwrap();
        assert!(!snapshot_path(&temp).exists());
        assert!(!checksum_path(&temp).exists());

        // Cleanup
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_list_snapshots_empty() {
        let temp = std::env::temp_dir().join("icn-snapshot-list-empty");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshots = list_snapshots(&temp).unwrap();
        assert_eq!(snapshots.len(), 0);

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_save_timestamped_snapshot() {
        let temp = std::env::temp_dir().join("icn-snapshot-timestamped");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        let timestamp = snapshot.created_at;

        save_timestamped_snapshot(&snapshot, &temp).unwrap();

        // Verify timestamped snapshot was created
        let expected_filename = format!("state.snapshot.{}", timestamp);
        let expected_path = temp.join(&expected_filename);
        let expected_checksum_path = temp.join(format!("{}.sha256", expected_filename));

        assert!(expected_path.exists(), "Timestamped snapshot should exist");
        assert!(expected_checksum_path.exists(), "Timestamped checksum should exist");

        // Verify it appears in list
        let snapshots = list_snapshots(&temp).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].0, expected_filename);
        assert_eq!(snapshots[0].1, timestamp);

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_cleanup_old_snapshots() {
        let temp = std::env::temp_dir().join("icn-snapshot-cleanup");
        std::fs::create_dir_all(&temp).unwrap();

        // Create 5 timestamped snapshots with different timestamps
        for i in 0..5 {
            let mut snapshot = StateSnapshot::new();
            snapshot.created_at = 1000 + i; // Increasing timestamps
            save_timestamped_snapshot(&snapshot, &temp).unwrap();
        }

        // Verify all 5 exist
        let snapshots = list_snapshots(&temp).unwrap();
        assert_eq!(snapshots.len(), 5, "Should have 5 snapshots");

        // Keep only 3 most recent
        let deleted = cleanup_old_snapshots(&temp, 3).unwrap();
        assert_eq!(deleted, 2, "Should delete 2 old snapshots");

        // Verify only 3 remain
        let snapshots = list_snapshots(&temp).unwrap();
        assert_eq!(snapshots.len(), 3, "Should have 3 snapshots remaining");

        // Verify the kept snapshots are the newest ones (1004, 1003, 1002)
        assert_eq!(snapshots[0].1, 1004);
        assert_eq!(snapshots[1].1, 1003);
        assert_eq!(snapshots[2].1, 1002);

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_cleanup_no_deletion_when_under_limit() {
        let temp = std::env::temp_dir().join("icn-snapshot-cleanup-under");
        std::fs::create_dir_all(&temp).unwrap();

        // Create 2 snapshots
        for i in 0..2 {
            let mut snapshot = StateSnapshot::new();
            snapshot.created_at = 1000 + i;
            save_timestamped_snapshot(&snapshot, &temp).unwrap();
        }

        // Try to keep 3 (we only have 2)
        let deleted = cleanup_old_snapshots(&temp, 3).unwrap();
        assert_eq!(deleted, 0, "Should delete nothing when under limit");

        // Verify both still exist
        let snapshots = list_snapshots(&temp).unwrap();
        assert_eq!(snapshots.len(), 2);

        std::fs::remove_dir_all(&temp).unwrap();
    }
}
