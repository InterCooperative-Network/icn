//! Persistence tests for HybridBlobStore.
//!
//! These tests verify that blob data and metadata survive a "restart" -- i.e., dropping
//! the store instance and re-opening it from the same on-disk paths. They also verify
//! that corruption on disk is detected via blake3 integrity checks.

// Integration tests are compiled as separate crates and do not inherit the
// `#![cfg_attr(test, allow(...))]` from lib.rs.  Panics via unwrap/expect are
// acceptable in tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_kernel_api::{
    state::{BlobService, StateError},
    types::Namespace,
};
use icn_store::blob_store::HybridBlobStore;
use std::fs;
use tempfile::TempDir;

/// Helper: create a persistent store rooted at `dir` with separate meta_db and blob
/// subdirectories. Returns the store plus the two subdirectory paths for later re-open.
fn open_store(dir: &std::path::Path) -> (HybridBlobStore, std::path::PathBuf, std::path::PathBuf) {
    let meta_path = dir.join("meta_db");
    let blob_dir = dir.join("blobs");
    let store =
        HybridBlobStore::open(&meta_path, &blob_dir, None).expect("failed to open HybridBlobStore");
    (store, meta_path, blob_dir)
}

fn test_namespace() -> Namespace {
    Namespace::new("test-coop", "persistence-app")
}

// ---------------------------------------------------------------------------
// 1. Write-and-read persistence across restart
// ---------------------------------------------------------------------------

#[test]
fn test_write_and_read_persists() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let data = b"hello, cooperative persistence!";

    // Phase 1: write a blob with the first store instance.
    let hash = {
        let (store, _, _) = open_store(dir.path());
        store.put(&test_namespace(), data).expect("put failed")
    };
    // The first store is dropped here -- sled is closed, file handles released.

    // Phase 2: open a fresh store at the same paths and read the blob back.
    {
        let (store, _, _) = open_store(dir.path());
        assert!(store.exists(&hash).expect("exists check failed"));
        let retrieved = store.get(&hash).expect("get after restart failed");
        assert_eq!(
            retrieved.as_slice(),
            data,
            "data read after restart must match original"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Metadata survives restart
// ---------------------------------------------------------------------------

#[test]
fn test_metadata_survives_restart() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let ns = Namespace::new("coop-alpha", "docs").with_sub("invoices");
    let data = b"invoice #42 payload";

    // Phase 1: store.
    let hash = {
        let (store, _, _) = open_store(dir.path());
        store.put(&ns, data).expect("put failed")
    };

    // Phase 2: re-open and verify metadata fields.
    {
        let (store, _, _) = open_store(dir.path());
        let meta = store
            .metadata(&hash)
            .expect("metadata after restart failed");
        assert_eq!(meta.size, data.len() as u64);
        assert_eq!(meta.namespace.org, "coop-alpha");
        assert_eq!(meta.namespace.app, "docs");
        assert_eq!(meta.namespace.sub.as_deref(), Some("invoices"));
        // created_at should be a reasonable Unix timestamp (after 2024-01-01).
        assert!(
            meta.created_at > 1_704_067_200,
            "created_at looks too old: {}",
            meta.created_at
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Corrupt blob detected on read
// ---------------------------------------------------------------------------

#[test]
fn test_corrupt_blob_detected() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let data = b"important cooperative data";

    let (store, _, blob_dir) = open_store(dir.path());
    let hash = store.put(&test_namespace(), data).expect("put failed");

    // Locate the blob file on disk:  <blob_dir>/<hash[0:2]>/<hash>.blob
    let hex = hex::encode(hash);
    let prefix = &hex[..2];
    let blob_path = blob_dir.join(prefix).join(format!("{hex}.blob"));
    assert!(blob_path.exists(), "blob file must exist on disk");

    // Corrupt the file: read, flip the first byte, write back.
    let mut raw = fs::read(&blob_path).expect("failed to read blob file");
    assert!(!raw.is_empty(), "blob file should not be empty");
    raw[0] ^= 0xFF; // flip all bits of the first byte
    fs::write(&blob_path, &raw).expect("failed to write corrupted blob");

    // Reading should now fail with BlobIntegrity.
    let result = store.get(&hash);
    match result {
        Err(StateError::BlobIntegrity { expected, actual }) => {
            assert_eq!(expected, hex, "expected hash should match original");
            assert_ne!(
                expected, actual,
                "actual hash must differ from expected after corruption"
            );
        }
        Err(other) => panic!("expected BlobIntegrity error, got: {other:?}"),
        Ok(_) => panic!("expected error on corrupted blob, but got Ok"),
    }
}

// ---------------------------------------------------------------------------
// 4. Multiple blobs persist across restart
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_blobs_persist() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let ns = test_namespace();

    let blobs: Vec<(&[u8], &str)> = vec![
        (b"blob one: governance proposal", "governance"),
        (b"blob two: mutual credit ledger entry", "ledger"),
        (b"blob three: federation treaty", "federation"),
        (b"blob four: member roster update", "membership"),
        (b"blob five: compute task result", "compute"),
    ];

    // Phase 1: store all blobs.
    let hashes: Vec<[u8; 32]> = {
        let (store, _, _) = open_store(dir.path());
        blobs
            .iter()
            .map(|(data, _label)| store.put(&ns, data).expect("put failed"))
            .collect()
    };

    // Phase 2: re-open and verify each blob.
    {
        let (store, _, _) = open_store(dir.path());

        for (i, ((data, label), hash)) in blobs.iter().zip(hashes.iter()).enumerate() {
            assert!(
                store.exists(hash).expect("exists failed"),
                "blob {i} ({label}) should exist after restart"
            );
            let retrieved = store.get(hash).expect("get failed");
            assert_eq!(
                retrieved.as_slice(),
                *data,
                "blob {i} ({label}) data mismatch after restart"
            );
            let meta = store.metadata(hash).expect("metadata failed");
            assert_eq!(meta.size, data.len() as u64);
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Delete actually removes (survives restart)
// ---------------------------------------------------------------------------

#[test]
fn test_delete_actually_removes() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let ns = test_namespace();
    let data = b"ephemeral cooperative data";

    // Phase 1: store and then delete.
    let hash = {
        let (store, _, _) = open_store(dir.path());
        let h = store.put(&ns, data).expect("put failed");
        assert!(store.exists(&h).expect("exists failed"));
        store.delete(&h).expect("delete failed");
        assert!(
            !store.exists(&h).expect("exists after delete failed"),
            "blob should not exist after delete"
        );
        h
    };

    // Phase 2: re-open and verify the blob is still gone.
    {
        let (store, _, _) = open_store(dir.path());
        assert!(
            !store.exists(&hash).expect("exists after restart failed"),
            "deleted blob must remain absent after restart"
        );

        // get() should return BlobNotFound.
        match store.get(&hash) {
            Err(StateError::BlobNotFound) => { /* expected */ }
            Err(other) => panic!("expected BlobNotFound, got: {other:?}"),
            Ok(d) => panic!("expected error for deleted blob, got {} bytes", d.len()),
        }

        // metadata() should also return BlobNotFound.
        match store.metadata(&hash) {
            Err(StateError::BlobNotFound) => { /* expected */ }
            Err(other) => panic!("expected BlobNotFound from metadata(), got: {other:?}"),
            Ok(m) => panic!(
                "expected error for deleted blob metadata, got size={}",
                m.size
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Delete removes the blob file from disk
// ---------------------------------------------------------------------------

#[test]
fn test_delete_removes_blob_file() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let data = b"blob to delete from disk";

    let (store, _, blob_dir) = open_store(dir.path());
    let hash = store.put(&test_namespace(), data).expect("put failed");

    // Verify the file exists.
    let hex = hex::encode(hash);
    let prefix = &hex[..2];
    let blob_path = blob_dir.join(prefix).join(format!("{hex}.blob"));
    assert!(blob_path.exists(), "blob file should exist before delete");

    store.delete(&hash).expect("delete failed");

    assert!(
        !blob_path.exists(),
        "blob file should be removed from disk after delete"
    );
}

// ---------------------------------------------------------------------------
// 7. Corruption detected after restart
// ---------------------------------------------------------------------------

#[test]
fn test_corrupt_blob_detected_after_restart() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let data = b"data that will be corrupted between restarts";

    // Phase 1: store the blob.
    let (hash, blob_dir) = {
        let (store, _, blob_dir) = open_store(dir.path());
        let h = store.put(&test_namespace(), data).expect("put failed");
        (h, blob_dir)
    };

    // Corrupt the file while the store is closed.
    let hex = hex::encode(hash);
    let prefix = &hex[..2];
    let blob_path = blob_dir.join(prefix).join(format!("{hex}.blob"));
    let mut raw = fs::read(&blob_path).expect("failed to read blob file");
    // Flip a byte in the middle.
    let mid = raw.len() / 2;
    raw[mid] ^= 0x01;
    fs::write(&blob_path, &raw).expect("failed to write corrupted file");

    // Phase 2: re-open and attempt to read.
    {
        let (store, _, _) = open_store(dir.path());
        // exists() should still return true (metadata is intact).
        assert!(
            store.exists(&hash).expect("exists failed"),
            "metadata should still indicate blob exists"
        );
        // But get() should detect the integrity violation.
        match store.get(&hash) {
            Err(StateError::BlobIntegrity { .. }) => { /* expected */ }
            Err(other) => panic!("expected BlobIntegrity, got: {other:?}"),
            Ok(_) => panic!("expected integrity error on corrupted blob after restart"),
        }
    }
}
