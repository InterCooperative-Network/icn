//! Sled-backed resource access grant store.
//!
//! Stores governance-authorized resource access records keyed by
//! `access:{resource_type}:{grantee_did}` under the `exec:resource` sled tree.
//!
//! Records carry the governance `decision_hash` for a complete audit trail
//! from proposal → effect → persisted grant.

use anyhow::Result;
use icn_kernel_api::resource::{ResourceAccessRecord, ResourceAccessStore};

/// Sled-backed implementation of [`ResourceAccessStore`].
pub struct SledResourceAccessStore {
    tree: sled::Tree,
}

impl SledResourceAccessStore {
    /// Open or create the resource access store in the given sled DB.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree("exec:resource")?;
        Ok(Self { tree })
    }

    fn key(resource_type: &str, grantee_did: &str) -> Vec<u8> {
        format!("access:{}:{}", resource_type, grantee_did).into_bytes()
    }

    fn active_prefix() -> &'static str {
        "access:"
    }
}

impl ResourceAccessStore for SledResourceAccessStore {
    fn grant(&self, record: &ResourceAccessRecord) -> Result<()> {
        let key = Self::key(&record.resource_type, &record.grantee_did);

        // Idempotency: if the exact same decision_hash is already stored, skip.
        if let Some(bytes) = self.tree.get(&key)? {
            let existing: ResourceAccessRecord = serde_json::from_slice(&bytes)?;
            if existing.decision_hash == record.decision_hash {
                return Ok(());
            }
        }

        let bytes = serde_json::to_vec(record)?;
        self.tree.insert(key, bytes)?;
        self.tree.flush()?;
        Ok(())
    }

    fn revoke(
        &self,
        resource_type: &str,
        grantee_did: &str,
        revoked_at: u64,
        reason: &str,
    ) -> Result<()> {
        let key = Self::key(resource_type, grantee_did);
        if let Some(bytes) = self.tree.get(&key)? {
            let mut record: ResourceAccessRecord = serde_json::from_slice(&bytes)?;
            if record.is_revoked {
                return Ok(()); // Already revoked — no-op.
            }
            record.is_revoked = true;
            record.revoked_at = Some(revoked_at);
            record.revocation_reason = Some(reason.to_string());
            let updated = serde_json::to_vec(&record)?;
            self.tree.insert(key, updated)?;
            self.tree.flush()?;
        }
        Ok(())
    }

    fn get(&self, resource_type: &str, grantee_did: &str) -> Result<Option<ResourceAccessRecord>> {
        let key = Self::key(resource_type, grantee_did);
        match self.tree.get(key)? {
            Some(bytes) => {
                let record: ResourceAccessRecord = serde_json::from_slice(&bytes)?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    fn list_active(&self) -> Result<Vec<ResourceAccessRecord>> {
        let prefix = Self::active_prefix();
        let mut records = Vec::new();

        for entry in self.tree.scan_prefix(prefix.as_bytes()) {
            let (_, bytes) = entry?;
            let record: ResourceAccessRecord = serde_json::from_slice(&bytes)?;
            if !record.is_revoked {
                records.push(record);
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn temp_db() -> sled::Db {
        sled::Config::new().temporary(true).open().unwrap()
    }

    fn make_record(
        resource_type: &str,
        grantee_did: &str,
        decision_hash: &str,
    ) -> ResourceAccessRecord {
        ResourceAccessRecord {
            resource_type: resource_type.to_string(),
            grantee_did: grantee_did.to_string(),
            access_model_hash: "hash-abc".to_string(),
            granted_at: 1_000_000,
            decision_hash: decision_hash.to_string(),
            is_revoked: false,
            revoked_at: None,
            revocation_reason: None,
        }
    }

    #[test]
    fn test_grant_and_get() {
        let db = temp_db();
        let store = SledResourceAccessStore::new(&db).unwrap();
        let rec = make_record("compute-cluster-alpha", "did:icn:alice", "decision-1");
        store.grant(&rec).unwrap();
        let got = store
            .get("compute-cluster-alpha", "did:icn:alice")
            .unwrap()
            .unwrap();
        assert_eq!(got.decision_hash, "decision-1");
        assert!(!got.is_revoked);
    }

    #[test]
    fn test_grant_is_idempotent_on_same_decision_hash() {
        let db = temp_db();
        let store = SledResourceAccessStore::new(&db).unwrap();
        let rec = make_record("resource-x", "did:icn:bob", "decision-2");
        store.grant(&rec).unwrap();
        store.grant(&rec).unwrap(); // Second call with same hash is no-op
        let got = store.get("resource-x", "did:icn:bob").unwrap().unwrap();
        assert_eq!(got.granted_at, 1_000_000);
    }

    #[test]
    fn test_revoke_sets_is_revoked() {
        let db = temp_db();
        let store = SledResourceAccessStore::new(&db).unwrap();
        let rec = make_record("storage-vol-1", "did:icn:carol", "decision-3");
        store.grant(&rec).unwrap();
        store
            .revoke(
                "storage-vol-1",
                "did:icn:carol",
                2_000_000,
                "governance revoked",
            )
            .unwrap();
        let got = store
            .get("storage-vol-1", "did:icn:carol")
            .unwrap()
            .unwrap();
        assert!(got.is_revoked);
        assert_eq!(got.revoked_at, Some(2_000_000));
        assert_eq!(got.revocation_reason.as_deref(), Some("governance revoked"));
    }

    #[test]
    fn test_revoke_is_idempotent() {
        let db = temp_db();
        let store = SledResourceAccessStore::new(&db).unwrap();
        let rec = make_record("resource-y", "did:icn:dave", "decision-4");
        store.grant(&rec).unwrap();
        store
            .revoke("resource-y", "did:icn:dave", 2_000_000, "first")
            .unwrap();
        store
            .revoke("resource-y", "did:icn:dave", 3_000_000, "second")
            .unwrap();
        let got = store.get("resource-y", "did:icn:dave").unwrap().unwrap();
        // Second revoke is a no-op; revoked_at stays at first value
        assert_eq!(got.revoked_at, Some(2_000_000));
    }

    #[test]
    fn test_list_active_excludes_revoked() {
        let db = temp_db();
        let store = SledResourceAccessStore::new(&db).unwrap();
        store
            .grant(&make_record("res-a", "did:icn:alice", "d-a"))
            .unwrap();
        store
            .grant(&make_record("res-b", "did:icn:alice", "d-b"))
            .unwrap();
        store
            .revoke("res-a", "did:icn:alice", 9_000, "test")
            .unwrap();
        let active = store.list_active().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].resource_type, "res-b");
    }

    #[test]
    fn test_revoke_nonexistent_is_noop() {
        let db = temp_db();
        let store = SledResourceAccessStore::new(&db).unwrap();
        // Should not error even if grant doesn't exist
        store
            .revoke("no-such-resource", "did:icn:nobody", 1_000, "reason")
            .unwrap();
    }
}
