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

/// Default number of snapshots to keep (reserved for future automatic cleanup)
#[allow(dead_code)]
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
    pub access_control: String, // Serialized AccessControl enum
    pub max_entries: usize,
    pub scope: String, // Serialized Scope enum
}

/// Serializable peer connection info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConnectionInfo {
    /// Peer's DID (string format)
    pub did: String,

    /// Negotiated protocol version for this connection
    pub negotiated_version: u32,

    /// Peer's announced capabilities (bitflags as u64)
    pub peer_capabilities: u64,

    /// Peer's software version string
    pub peer_software: String,

    /// X25519 public key for end-to-end encryption
    pub x25519_key: [u8; 32],
}

/// Network actor state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkState {
    /// Peer connection info (DID string -> PeerConnectionInfo)
    /// This is the modern format that includes version, capabilities, and X25519 keys
    #[serde(default)]
    pub peer_connections: HashMap<String, PeerConnectionInfo>,

    /// Legacy: Peer X25519 public keys for end-to-end encryption
    /// Map of DID string -> 32-byte X25519 public key
    /// Deprecated in favor of peer_connections, kept for backward compatibility
    #[serde(default)]
    pub peer_x25519_keys: HashMap<String, [u8; 32]>,

    /// Known peer addresses (DID string -> last known SocketAddr string)
    /// Note: These may be stale after restart, but can help with reconnection
    #[serde(default)]
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
    let json = serde_json::to_vec_pretty(snapshot).context("Failed to serialize snapshot")?;

    // Compute SHA256 checksum
    let checksum = compute_checksum(&json);

    // Write atomically using temp file + rename
    let temp_path = path.with_extension("snapshot.tmp");
    let temp_checksum_path = checksum_path.with_extension("sha256.tmp");

    std::fs::write(&temp_path, &json).context("Failed to write snapshot")?;
    std::fs::write(&temp_checksum_path, &checksum).context("Failed to write checksum")?;

    std::fs::rename(&temp_path, &path).context("Failed to rename snapshot")?;
    std::fs::rename(&temp_checksum_path, &checksum_path).context("Failed to rename checksum")?;

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

    let json = std::fs::read(&path).context("Failed to read snapshot")?;

    // Verify checksum if it exists
    if checksum_path.exists() {
        let expected_checksum =
            std::fs::read_to_string(&checksum_path).context("Failed to read checksum")?;
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

    let snapshot: StateSnapshot =
        serde_json::from_slice(&json).context("Failed to deserialize snapshot")?;

    Ok(Some(snapshot))
}

/// Delete state snapshot and checksum
///
/// Used after successful restore or when explicitly clearing state
pub fn delete_snapshot(data_dir: impl AsRef<Path>) -> Result<()> {
    let path = snapshot_path(&data_dir);
    let checksum_path = checksum_path(&data_dir);

    if path.exists() {
        std::fs::remove_file(&path).context("Failed to delete snapshot")?;
    }

    if checksum_path.exists() {
        std::fs::remove_file(&checksum_path).context("Failed to delete checksum")?;
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

    let json = std::fs::read(&path).context("Failed to read snapshot")?;
    let expected_checksum =
        std::fs::read_to_string(&checksum_path).context("Failed to read checksum")?;
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
        let checksum_path = data_dir.join(format!("{filename}.sha256"));

        // Delete snapshot file
        if snapshot_path.exists() {
            std::fs::remove_file(&snapshot_path)
                .context(format!("Failed to delete snapshot: {filename}"))?;
            deleted_count += 1;
        }

        // Delete associated checksum file
        if checksum_path.exists() {
            std::fs::remove_file(&checksum_path)
                .context(format!("Failed to delete checksum: {filename}.sha256"))?;
        }
    }

    Ok(deleted_count)
}

/// Save a timestamped backup snapshot in addition to the primary snapshot
///
/// Creates state.snapshot.{unix_timestamp} for archival purposes
pub fn save_timestamped_snapshot(
    snapshot: &StateSnapshot,
    data_dir: impl AsRef<Path>,
) -> Result<()> {
    let data_dir = data_dir.as_ref();
    let timestamp = snapshot.created_at;
    let timestamped_filename = format!("state.snapshot.{timestamp}");
    let timestamped_path = data_dir.join(&timestamped_filename);
    let timestamped_checksum_path = data_dir.join(format!("{timestamped_filename}.sha256"));

    // Serialize to JSON
    let json = serde_json::to_vec_pretty(snapshot).context("Failed to serialize snapshot")?;

    // Compute SHA256 checksum
    let checksum = compute_checksum(&json);

    // Write timestamped snapshot
    std::fs::write(&timestamped_path, &json).context("Failed to write timestamped snapshot")?;
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
            loaded
                .gossip_state
                .as_ref()
                .unwrap()
                .vector_clock
                .get("did:icn:alice"),
            Some(&42)
        );
        assert_eq!(
            loaded
                .gossip_state
                .as_ref()
                .unwrap()
                .subscriptions
                .get("topic:test")
                .unwrap()
                .len(),
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
            peer_connections: HashMap::new(),
            peer_x25519_keys: [("did:icn:alice".to_string(), [1u8; 32])]
                .iter()
                .cloned()
                .collect(),
            peer_addresses: [("did:icn:alice".to_string(), "127.0.0.1:5000".to_string())]
                .iter()
                .cloned()
                .collect(),
        });

        save_snapshot(&snapshot, &temp).unwrap();
        let loaded = load_snapshot(&temp).unwrap().unwrap();

        let net_state = loaded.network_state.unwrap();
        assert_eq!(
            net_state.peer_x25519_keys.get("did:icn:alice"),
            Some(&[1u8; 32])
        );
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
        assert_eq!(
            checksum.trim().len(),
            64,
            "SHA256 should be 64 hex characters"
        );
        assert!(checksum
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c.is_whitespace()));

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
        assert!(
            verify_result.is_err(),
            "Verification should fail for corrupted snapshot"
        );
        assert!(verify_result
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"));

        // Load should fail
        let load_result = load_snapshot(&temp);
        assert!(
            load_result.is_err(),
            "Load should fail for corrupted snapshot"
        );
        assert!(load_result
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"));

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
        assert!(verify_result
            .unwrap_err()
            .to_string()
            .contains("Checksum file does not exist"));

        // But load should still succeed (backward compatibility)
        let loaded = load_snapshot(&temp).unwrap();
        assert!(
            loaded.is_some(),
            "Legacy snapshots without checksums should still load"
        );

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
        let expected_filename = format!("state.snapshot.{timestamp}");
        let expected_path = temp.join(&expected_filename);
        let expected_checksum_path = temp.join(format!("{expected_filename}.sha256"));

        assert!(expected_path.exists(), "Timestamped snapshot should exist");
        assert!(
            expected_checksum_path.exists(),
            "Timestamped checksum should exist"
        );

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

    // ========== Fuzzing / Property-Based Tests ==========

    use proptest::prelude::*;

    // Strategy for generating arbitrary StateSnapshot
    prop_compose! {
        fn arb_gossip_state()(
            vector_clock in prop::collection::hash_map(
                "[a-z]{3,10}",  // Random DID-like strings
                0u64..1000,     // Clock values
                0..50           // Up to 50 entries
            ),
            subscriptions in prop::collection::hash_map(
                "[a-z]{3,10}",  // Topic names
                prop::collection::vec("[a-z]{3,10}", 0..10),  // Subscriber DIDs
                0..50
            ),
            topics in prop::collection::hash_map(
                "[a-z]{3,10}",  // Topic names
                (
                    "[a-z]{3,10}",  // name
                    "(Public|Private|TrustGated)",  // access_control
                    0usize..10000,   // max_entries
                    "(Global|Regional|Local)",  // scope
                ),
                0..50
            ),
        ) -> GossipState {
            GossipState {
                vector_clock,
                subscriptions,
                topics: topics.into_iter().map(|(k, (name, ac, max, scope))| {
                    (k, TopicMetadata {
                        name,
                        access_control: ac,
                        max_entries: max,
                        scope,
                    })
                }).collect(),
            }
        }
    }

    prop_compose! {
        fn arb_network_state()(
            peer_x25519_keys in prop::collection::hash_map(
                "[a-z]{3,10}",  // DID-like strings
                prop::array::uniform32(0u8..),  // Random 32-byte keys
                0..100
            ),
            peer_addresses in prop::collection::hash_map(
                "[a-z]{3,10}",
                "127\\.0\\.0\\.1:[0-9]{4,5}",  // IP:port
                0..100
            ),
        ) -> NetworkState {
            NetworkState {
                peer_connections: HashMap::new(),
                peer_x25519_keys,
                peer_addresses,
            }
        }
    }

    prop_compose! {
        fn arb_state_snapshot()(
            version in 1u32..10,
            created_at in 0u64..2000000000,
            has_gossip in prop::bool::ANY,
            has_network in prop::bool::ANY,
            gossip_state in arb_gossip_state(),
            network_state in arb_network_state(),
        ) -> StateSnapshot {
            StateSnapshot {
                version,
                created_at,
                gossip_state: if has_gossip { Some(gossip_state) } else { None },
                network_state: if has_network { Some(network_state) } else { None },
            }
        }
    }

    proptest! {
        #[test]
        fn fuzz_serialize_deserialize_roundtrip(snapshot in arb_state_snapshot()) {
            // Property: Any StateSnapshot should serialize and deserialize correctly
            let json = serde_json::to_vec(&snapshot).unwrap();
            let deserialized: StateSnapshot = serde_json::from_slice(&json).unwrap();

            // Verify version and timestamp
            assert_eq!(snapshot.version, deserialized.version);
            assert_eq!(snapshot.created_at, deserialized.created_at);

            // Verify gossip state if present
            if let Some(ref gossip) = snapshot.gossip_state {
                let deser_gossip = deserialized.gossip_state.as_ref().unwrap();
                assert_eq!(gossip.vector_clock.len(), deser_gossip.vector_clock.len());
                assert_eq!(gossip.subscriptions.len(), deser_gossip.subscriptions.len());
                assert_eq!(gossip.topics.len(), deser_gossip.topics.len());
            } else {
                assert!(deserialized.gossip_state.is_none());
            }

            // Verify network state if present
            if let Some(ref network) = snapshot.network_state {
                let deser_network = deserialized.network_state.as_ref().unwrap();
                assert_eq!(network.peer_x25519_keys.len(), deser_network.peer_x25519_keys.len());
                assert_eq!(network.peer_addresses.len(), deser_network.peer_addresses.len());
            } else {
                assert!(deserialized.network_state.is_none());
            }
        }

        #[test]
        fn fuzz_save_load_roundtrip(snapshot in arb_state_snapshot()) {
            // Property: Any StateSnapshot should save and load correctly with checksum
            let temp = std::env::temp_dir().join(format!("icn-fuzz-{}", snapshot.created_at));
            std::fs::create_dir_all(&temp).unwrap();

            // Save
            save_snapshot(&snapshot, &temp).unwrap();

            // Verify snapshot file exists
            assert!(snapshot_path(&temp).exists());
            assert!(checksum_path(&temp).exists());

            // Verify checksum is valid
            verify_snapshot(&temp).unwrap();

            // Load and compare
            let loaded = load_snapshot(&temp).unwrap().unwrap();
            assert_eq!(snapshot.version, loaded.version);
            assert_eq!(snapshot.created_at, loaded.created_at);

            // Cleanup
            std::fs::remove_dir_all(&temp).unwrap_or(());
        }

        #[test]
        fn fuzz_malformed_json_fails_gracefully(
            random_bytes in prop::collection::vec(any::<u8>(), 0..1000)
        ) {
            // Property: Random bytes should fail to deserialize (not panic)
            let result: Result<StateSnapshot, _> = serde_json::from_slice(&random_bytes);
            // Should fail, not panic
            if result.is_ok() {
                // Very unlikely, but if it succeeds, ensure it's valid
                let _ = result.unwrap();
            }
        }

        #[test]
        fn fuzz_corrupted_snapshot_detected(snapshot in arb_state_snapshot()) {
            // Property: Any corruption should be detected by checksum
            let temp = std::env::temp_dir().join(format!("icn-fuzz-corrupt-{}", snapshot.created_at));
            std::fs::create_dir_all(&temp).unwrap();

            save_snapshot(&snapshot, &temp).unwrap();

            // Read and corrupt the snapshot
            let snapshot_path = snapshot_path(&temp);
            let mut content = std::fs::read(&snapshot_path).unwrap();

            // Flip some bits (if there's content)
            if !content.is_empty() {
                for i in (0..content.len()).step_by(content.len() / 10 + 1).take(5) {
                    content[i] ^= 0xFF;  // Flip all bits
                }
                std::fs::write(&snapshot_path, content).unwrap();

                // Verification should fail
                let result = verify_snapshot(&temp);
                assert!(result.is_err(), "Corrupted snapshot should fail verification");

                // Load should also fail
                let load_result = load_snapshot(&temp);
                assert!(load_result.is_err(), "Corrupted snapshot should fail to load");
            }

            // Cleanup
            std::fs::remove_dir_all(&temp).unwrap_or(());
        }

        #[test]
        fn fuzz_large_collections_handled(
            large_clock_size in 0usize..500,
            large_sub_size in 0usize..500,
        ) {
            // Property: Large collections should be handled without panic
            let mut gossip = GossipState {
                vector_clock: HashMap::new(),
                subscriptions: HashMap::new(),
                topics: HashMap::new(),
            };

            // Create large vector clock
            for i in 0..large_clock_size {
                gossip.vector_clock.insert(format!("did:icn:peer{i}"), i as u64);
            }

            // Create large subscriptions
            for i in 0..large_sub_size {
                let subs: Vec<String> = (0..10).map(|j| format!("sub{j}")).collect();
                gossip.subscriptions.insert(format!("topic{i}"), subs);
            }

            let mut snapshot = StateSnapshot::new();
            snapshot.gossip_state = Some(gossip);

            // Should serialize without panic
            let json = serde_json::to_vec(&snapshot).unwrap();

            // Should deserialize without panic
            let _deserialized: StateSnapshot = serde_json::from_slice(&json).unwrap();
        }

        #[test]
        fn fuzz_extreme_values(
            version in any::<u32>(),
            created_at in any::<u64>(),
        ) {
            // Property: Extreme u32/u64 values should be handled
            let mut snapshot = StateSnapshot::new();
            snapshot.version = version;
            snapshot.created_at = created_at;

            // Should serialize
            let json = serde_json::to_vec(&snapshot).unwrap();

            // Should deserialize with same values
            let deserialized: StateSnapshot = serde_json::from_slice(&json).unwrap();
            assert_eq!(snapshot.version, deserialized.version);
            assert_eq!(snapshot.created_at, deserialized.created_at);
        }
    }

    #[test]
    fn test_empty_collections() {
        // Edge case: Empty collections should work
        let mut snapshot = StateSnapshot::new();
        snapshot.gossip_state = Some(GossipState {
            vector_clock: HashMap::new(),
            subscriptions: HashMap::new(),
            topics: HashMap::new(),
        });
        snapshot.network_state = Some(NetworkState {
            peer_connections: HashMap::new(),
            peer_x25519_keys: HashMap::new(),
            peer_addresses: HashMap::new(),
        });

        let temp = std::env::temp_dir().join("icn-snapshot-empty-colls");
        std::fs::create_dir_all(&temp).unwrap();

        save_snapshot(&snapshot, &temp).unwrap();
        let loaded = load_snapshot(&temp).unwrap().unwrap();

        assert!(loaded.gossip_state.is_some());
        assert!(loaded.network_state.is_some());
        assert_eq!(loaded.gossip_state.unwrap().vector_clock.len(), 0);
        assert_eq!(loaded.network_state.unwrap().peer_x25519_keys.len(), 0);

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_invalid_utf8_in_json() {
        // Edge case: Invalid UTF-8 should fail gracefully
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
        let result: Result<StateSnapshot, _> = serde_json::from_slice(&invalid_utf8);
        assert!(result.is_err(), "Invalid UTF-8 should fail deserialization");
    }

    #[test]
    fn test_truncated_json() {
        // Edge case: Truncated JSON should fail gracefully
        let snapshot = StateSnapshot::new();
        let json = serde_json::to_string(&snapshot).unwrap();

        // Take only first half
        let truncated = &json[..json.len() / 2];
        let result: Result<StateSnapshot, _> = serde_json::from_str(truncated);
        assert!(
            result.is_err(),
            "Truncated JSON should fail deserialization"
        );
    }

    #[test]
    fn test_malicious_checksum() {
        // Security: Attacker modifies checksum to match corrupted data
        let temp = std::env::temp_dir().join("icn-snapshot-malicious");
        std::fs::create_dir_all(&temp).unwrap();

        let snapshot = StateSnapshot::new();
        save_snapshot(&snapshot, &temp).unwrap();

        // Corrupt both snapshot and checksum
        let snapshot_path = snapshot_path(&temp);
        let checksum_path = checksum_path(&temp);

        let mut content = std::fs::read(&snapshot_path).unwrap();
        content.extend_from_slice(b"MALICIOUS");
        std::fs::write(&snapshot_path, &content).unwrap();

        // Compute new checksum for corrupted data
        let malicious_checksum = super::compute_checksum(&content);
        std::fs::write(&checksum_path, malicious_checksum).unwrap();

        // Checksum will match, but JSON will be invalid
        verify_snapshot(&temp).unwrap(); // Checksum matches

        // But deserialization should fail (invalid JSON)
        let load_result = load_snapshot(&temp);
        assert!(
            load_result.is_err(),
            "Malicious payload should fail deserialization"
        );

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn test_very_long_strings() {
        // Edge case: Very long DID strings and topics
        let mut gossip = GossipState {
            vector_clock: HashMap::new(),
            subscriptions: HashMap::new(),
            topics: HashMap::new(),
        };

        let long_did = "a".repeat(10000);
        let long_topic = "topic".to_string() + &"x".repeat(5000);

        gossip.vector_clock.insert(long_did.clone(), 42);
        gossip.subscriptions.insert(long_topic, vec![long_did]);

        let mut snapshot = StateSnapshot::new();
        snapshot.gossip_state = Some(gossip);

        // Should serialize without panic
        let json = serde_json::to_vec(&snapshot).unwrap();

        // Should deserialize without panic
        let deserialized: StateSnapshot = serde_json::from_slice(&json).unwrap();
        assert!(deserialized.gossip_state.is_some());
    }
}
