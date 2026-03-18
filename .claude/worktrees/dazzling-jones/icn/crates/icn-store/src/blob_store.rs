//! Hybrid BlobStore: sled metadata + filesystem blob storage.
//!
//! Content-addressed storage using blake3 hashing. Metadata (size, namespace,
//! timestamps) lives in sled for fast lookups. Blob bytes live on the filesystem
//! for efficient large-blob handling and OS page cache utilization.
//!
//! # Layout
//!
//! ```text
//! <blob_dir>/
//!   <hash[0:2]>/
//!     <hash>.blob          # raw blob bytes
//! ```
//!
//! # Atomicity
//!
//! Writes use temp-file + rename for crash safety. The sled metadata write
//! is committed only after the blob file is durable on disk.
//!
//! # Integrity
//!
//! Every `get()` re-hashes the blob and compares against the stored hash.
//! Corruption is detected immediately and reported as `StateError::BlobIntegrity`.

use icn_kernel_api::{
    state::{BlobMetadata, BlobService, StateError},
    types::{Hash, Namespace},
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Default maximum blob size: 10 MB.
pub const DEFAULT_MAX_BLOB_SIZE: u64 = 10 * 1024 * 1024;

/// Sled key prefix for blob metadata.
const META_PREFIX: &[u8] = b"blob:meta:";

/// Serializable blob metadata stored in sled.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredBlobMeta {
    size: u64,
    namespace_org: String,
    namespace_app: String,
    namespace_sub: Option<String>,
    created_at: u64,
    content_type: Option<String>,
}

impl StoredBlobMeta {
    fn to_blob_metadata(&self) -> BlobMetadata {
        let mut ns = Namespace::new(&self.namespace_org, &self.namespace_app);
        if let Some(ref sub) = self.namespace_sub {
            ns = ns.with_sub(sub);
        }
        BlobMetadata {
            size: self.size,
            namespace: ns,
            created_at: self.created_at,
            content_type: self.content_type.clone(),
        }
    }
}

/// Content-addressed proof that a blob exists with verified ownership.
///
/// Produced by `HybridBlobStore::create_artifact_proof` after a blob is stored.
/// The proof is content-addressed: `metadata_hash` covers (blob_hash, owner, scope).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactProof {
    /// blake3 hash of the blob content
    pub blob_hash: Hash,
    /// blake3 hash of (blob_hash || owner_did || scope_id) — binds ownership
    pub metadata_hash: Hash,
    /// DID of the blob owner
    pub owner_did: String,
    /// Scope this blob belongs to (e.g., coop DID, federation ID)
    pub scope_id: String,
    /// Ed25519 signature over metadata_hash by owner_did
    pub signature: Vec<u8>,
}

impl ArtifactProof {
    /// Compute the metadata hash for an artifact proof.
    pub fn compute_metadata_hash(blob_hash: &Hash, owner_did: &str, scope_id: &str) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(blob_hash);
        hasher.update(owner_did.as_bytes());
        hasher.update(scope_id.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Verify that metadata_hash is consistent with the claimed fields.
    /// (Signature verification requires the owner's public key and is done externally.)
    pub fn verify_binding(&self) -> bool {
        let expected =
            Self::compute_metadata_hash(&self.blob_hash, &self.owner_did, &self.scope_id);
        self.metadata_hash == expected
    }
}

/// Hybrid blob store: sled for metadata, filesystem for blob data.
pub struct HybridBlobStore {
    /// Sled database for blob metadata.
    meta_db: sled::Db,
    /// Root directory for blob files.
    blob_dir: PathBuf,
    /// Maximum blob size in bytes.
    max_blob_size: u64,
}

impl HybridBlobStore {
    /// Open or create a hybrid blob store.
    ///
    /// - `meta_db_path`: Path to the sled database directory for metadata.
    /// - `blob_dir`: Root directory for blob file storage.
    /// - `max_blob_size`: Maximum blob size in bytes (default: 10 MB).
    pub fn open(
        meta_db_path: &Path,
        blob_dir: &Path,
        max_blob_size: Option<u64>,
    ) -> Result<Self, StateError> {
        let meta_db = sled::open(meta_db_path).map_err(|e| {
            StateError::StorageError(format!("failed to open blob metadata db: {e}"))
        })?;

        fs::create_dir_all(blob_dir).map_err(|e| {
            StateError::StorageError(format!("failed to create blob directory: {e}"))
        })?;

        Ok(Self {
            meta_db,
            blob_dir: blob_dir.to_path_buf(),
            max_blob_size: max_blob_size.unwrap_or(DEFAULT_MAX_BLOB_SIZE),
        })
    }

    /// Create a blob store backed by a temporary sled database.
    /// Useful for testing.
    pub fn temporary(blob_dir: &Path) -> Result<Self, StateError> {
        let meta_db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| StateError::StorageError(format!("failed to open temp db: {e}")))?;

        fs::create_dir_all(blob_dir).map_err(|e| {
            StateError::StorageError(format!("failed to create blob directory: {e}"))
        })?;

        Ok(Self {
            meta_db,
            blob_dir: blob_dir.to_path_buf(),
            max_blob_size: DEFAULT_MAX_BLOB_SIZE,
        })
    }

    /// Compute the filesystem path for a blob given its hash.
    fn blob_path(&self, hash: &Hash) -> PathBuf {
        let hex = hex::encode(hash);
        let prefix = &hex[..2];
        self.blob_dir.join(prefix).join(format!("{hex}.blob"))
    }

    /// Build the sled key for a blob's metadata.
    fn meta_key(hash: &Hash) -> Vec<u8> {
        let mut key = Vec::with_capacity(META_PREFIX.len() + 32);
        key.extend_from_slice(META_PREFIX);
        key.extend_from_slice(hash);
        key
    }

    /// Compute blake3 hash of data.
    fn hash(data: &[u8]) -> Hash {
        *blake3::hash(data).as_bytes()
    }

    /// Get the current unix timestamp.
    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Write blob data to disk atomically (temp + rename).
    fn write_blob_atomic(&self, hash: &Hash, data: &[u8]) -> Result<(), StateError> {
        let target = self.blob_path(hash);

        // Ensure parent directory exists
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                StateError::StorageError(format!("failed to create blob prefix dir: {e}"))
            })?;
        }

        // Write to temp file in the same directory (same filesystem for atomic rename)
        let temp_path = target.with_extension("tmp");
        let mut file = fs::File::create(&temp_path).map_err(|e| {
            StateError::StorageError(format!("failed to create temp blob file: {e}"))
        })?;
        file.write_all(data)
            .map_err(|e| StateError::StorageError(format!("failed to write blob data: {e}")))?;
        file.sync_all()
            .map_err(|e| StateError::StorageError(format!("failed to sync blob data: {e}")))?;
        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &target)
            .map_err(|e| StateError::StorageError(format!("failed to rename temp blob: {e}")))?;

        Ok(())
    }

    /// Store metadata in sled.
    fn write_meta(&self, hash: &Hash, meta: &StoredBlobMeta) -> Result<(), StateError> {
        let key = Self::meta_key(hash);
        let value = serde_json::to_vec(meta)
            .map_err(|e| StateError::Internal(format!("failed to serialize blob metadata: {e}")))?;
        self.meta_db
            .insert(key, value)
            .map_err(|e| StateError::StorageError(format!("failed to write blob metadata: {e}")))?;
        self.meta_db
            .flush()
            .map_err(|e| StateError::StorageError(format!("failed to flush metadata db: {e}")))?;
        Ok(())
    }

    /// Read metadata from sled.
    fn read_meta(&self, hash: &Hash) -> Result<StoredBlobMeta, StateError> {
        let key = Self::meta_key(hash);
        let value = self
            .meta_db
            .get(key)
            .map_err(|e| StateError::StorageError(format!("failed to read blob metadata: {e}")))?
            .ok_or(StateError::BlobNotFound)?;
        serde_json::from_slice(&value)
            .map_err(|e| StateError::Internal(format!("failed to deserialize blob metadata: {e}")))
    }

    /// Count stored blobs.
    pub fn count(&self) -> Result<usize, StateError> {
        let mut count = 0;
        for item in self.meta_db.scan_prefix(META_PREFIX) {
            let _ = item.map_err(|e| StateError::StorageError(format!("scan error: {e}")))?;
            count += 1;
        }
        Ok(count)
    }
}

impl BlobService for HybridBlobStore {
    fn put(&self, namespace: &Namespace, data: &[u8]) -> Result<Hash, StateError> {
        let size = data.len() as u64;

        // Enforce size limit
        if size > self.max_blob_size {
            return Err(StateError::BlobOversize {
                size,
                max: self.max_blob_size,
            });
        }

        // Compute content hash
        let hash = Self::hash(data);
        debug!(
            hash = hex::encode(hash),
            size,
            namespace = %namespace.to_path(),
            "storing blob"
        );

        // Check for duplicate (content-addressed: same data = same hash = no-op)
        if self.exists(&hash)? {
            debug!(
                hash = hex::encode(hash),
                "blob already exists, skipping write"
            );
            return Ok(hash);
        }

        // Write blob data to disk atomically
        self.write_blob_atomic(&hash, data)?;

        // Write metadata to sled
        let meta = StoredBlobMeta {
            size,
            namespace_org: namespace.org.clone(),
            namespace_app: namespace.app.clone(),
            namespace_sub: namespace.sub.clone(),
            created_at: Self::now_unix(),
            content_type: None,
        };
        self.write_meta(&hash, &meta)?;

        Ok(hash)
    }

    fn get(&self, hash: &Hash) -> Result<Vec<u8>, StateError> {
        // Verify metadata exists
        let _meta = self.read_meta(hash)?;

        // Read blob from disk
        let path = self.blob_path(hash);
        let data = fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                warn!(
                    hash = hex::encode(hash),
                    "blob metadata exists but file missing"
                );
                StateError::BlobNotFound
            } else {
                StateError::StorageError(format!("failed to read blob file: {e}"))
            }
        })?;

        // Verify integrity: re-hash and compare
        let actual_hash = Self::hash(&data);
        if &actual_hash != hash {
            warn!(
                expected = hex::encode(hash),
                actual = hex::encode(actual_hash),
                "blob integrity check failed"
            );
            return Err(StateError::BlobIntegrity {
                expected: hex::encode(hash),
                actual: hex::encode(actual_hash),
            });
        }

        Ok(data)
    }

    fn exists(&self, hash: &Hash) -> Result<bool, StateError> {
        let key = Self::meta_key(hash);
        self.meta_db
            .contains_key(key)
            .map_err(|e| StateError::StorageError(format!("failed to check blob existence: {e}")))
    }

    fn delete(&self, hash: &Hash) -> Result<(), StateError> {
        // Remove metadata
        let key = Self::meta_key(hash);
        self.meta_db.remove(key).map_err(|e| {
            StateError::StorageError(format!("failed to delete blob metadata: {e}"))
        })?;

        // Remove blob file (best-effort: metadata is source of truth)
        let path = self.blob_path(hash);
        if path.exists() {
            if let Err(e) = fs::remove_file(&path) {
                warn!(
                    hash = hex::encode(hash),
                    error = %e,
                    "failed to remove blob file (metadata already deleted)"
                );
            }
        }

        Ok(())
    }

    fn metadata(&self, hash: &Hash) -> Result<BlobMetadata, StateError> {
        self.read_meta(hash).map(|m| m.to_blob_metadata())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (HybridBlobStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let blob_dir = dir.path().join("blobs");
        let store = HybridBlobStore::temporary(&blob_dir).unwrap();
        (store, dir)
    }

    fn test_namespace() -> Namespace {
        Namespace::new("test-coop", "test-app")
    }

    #[test]
    fn blob_store_round_trip() {
        let (store, _dir) = test_store();
        let ns = test_namespace();
        let data = b"hello, cooperative world!";

        let hash = store.put(&ns, data).unwrap();
        assert_eq!(hash, HybridBlobStore::hash(data));

        let retrieved = store.get(&hash).unwrap();
        assert_eq!(retrieved, data);

        let meta = store.metadata(&hash).unwrap();
        assert_eq!(meta.size, data.len() as u64);
        assert_eq!(meta.namespace.org, "test-coop");
        assert_eq!(meta.namespace.app, "test-app");
    }

    #[test]
    fn blob_store_rejects_oversized_blob() {
        let dir = TempDir::new().unwrap();
        let blob_dir = dir.path().join("blobs");
        // Use a tiny max size for testing
        let store = HybridBlobStore {
            meta_db: sled::Config::new().temporary(true).open().unwrap(),
            blob_dir: blob_dir.clone(),
            max_blob_size: 100,
        };
        fs::create_dir_all(&blob_dir).unwrap();
        let ns = test_namespace();

        let big_data = vec![0u8; 101];
        let result = store.put(&ns, &big_data);
        assert!(result.is_err());
        match result.unwrap_err() {
            StateError::BlobOversize { size, max } => {
                assert_eq!(size, 101);
                assert_eq!(max, 100);
            }
            other => panic!("expected BlobOversize, got: {other:?}"),
        }
    }

    #[test]
    fn blob_store_hash_verification_fails_on_corruption() {
        let (store, _dir) = test_store();
        let ns = test_namespace();
        let data = b"important data";

        let hash = store.put(&ns, data).unwrap();

        // Corrupt the blob file on disk
        let path = store.blob_path(&hash);
        fs::write(&path, b"corrupted!").unwrap();

        let result = store.get(&hash);
        assert!(result.is_err());
        match result.unwrap_err() {
            StateError::BlobIntegrity { expected, actual } => {
                assert_eq!(expected, hex::encode(hash));
                assert_ne!(expected, actual);
            }
            other => panic!("expected BlobIntegrity, got: {other:?}"),
        }
    }

    #[test]
    fn blob_store_persistence_across_restart() {
        let dir = TempDir::new().unwrap();
        let meta_path = dir.path().join("meta_db");
        let blob_dir = dir.path().join("blobs");
        let ns = test_namespace();
        let data = b"persistent data";

        // Write with first store instance
        let hash = {
            let store = HybridBlobStore::open(&meta_path, &blob_dir, None).unwrap();
            store.put(&ns, data).unwrap()
        };

        // Read with second store instance (simulates restart)
        {
            let store = HybridBlobStore::open(&meta_path, &blob_dir, None).unwrap();
            assert!(store.exists(&hash).unwrap());
            let retrieved = store.get(&hash).unwrap();
            assert_eq!(retrieved, data);
        }
    }

    #[test]
    fn blob_store_delete() {
        let (store, _dir) = test_store();
        let ns = test_namespace();
        let data = b"deletable";

        let hash = store.put(&ns, data).unwrap();
        assert!(store.exists(&hash).unwrap());

        store.delete(&hash).unwrap();
        assert!(!store.exists(&hash).unwrap());
        assert!(store.get(&hash).is_err());
    }

    #[test]
    fn blob_store_deduplication() {
        let (store, _dir) = test_store();
        let ns = test_namespace();
        let data = b"same content";

        let hash1 = store.put(&ns, data).unwrap();
        let hash2 = store.put(&ns, data).unwrap();
        assert_eq!(hash1, hash2);
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn blob_store_not_found() {
        let (store, _dir) = test_store();
        let fake_hash = [0u8; 32];
        assert!(matches!(
            store.get(&fake_hash),
            Err(StateError::BlobNotFound)
        ));
        assert!(matches!(
            store.metadata(&fake_hash),
            Err(StateError::BlobNotFound)
        ));
    }

    #[test]
    fn artifact_proof_verifies() {
        let blob_hash = HybridBlobStore::hash(b"test blob");
        let owner = "did:icn:alice123";
        let scope = "coop:test-coop";

        let metadata_hash = ArtifactProof::compute_metadata_hash(&blob_hash, owner, scope);

        let proof = ArtifactProof {
            blob_hash,
            metadata_hash,
            owner_did: owner.to_string(),
            scope_id: scope.to_string(),
            signature: vec![0u8; 64], // placeholder — real sig checked externally
        };

        assert!(proof.verify_binding());
    }

    #[test]
    fn artifact_proof_rejects_wrong_owner() {
        let blob_hash = HybridBlobStore::hash(b"test blob");
        let owner = "did:icn:alice123";
        let scope = "coop:test-coop";

        let metadata_hash = ArtifactProof::compute_metadata_hash(&blob_hash, owner, scope);

        // Tamper with owner
        let proof = ArtifactProof {
            blob_hash,
            metadata_hash,
            owner_did: "did:icn:mallory".to_string(), // wrong owner
            scope_id: scope.to_string(),
            signature: vec![0u8; 64],
        };

        assert!(!proof.verify_binding());
    }
}
