#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration test: governance-authorized resource access.
//!
//! Proves that:
//! 1. An accepted `GrantAccess` effect writes a persisted record with decision provenance.
//! 2. A subsequent `RevokeAccess` effect marks that record revoked.
//! 3. Replaying the same `GrantAccess` is idempotent (same decision_hash → no-op).
//! 4. Without a wired store the executor returns an explicit hard failure (not a silent pass).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;
use icn_governance::InMemoryParameterStore;
use icn_kernel_api::effects::{KernelEffect, ResourceEffect};
use icn_kernel_api::governance::EffectExecutor;
use icn_kernel_api::resource::{ResourceAccessRecord, ResourceAccessStore};

// ─── In-memory store (test-only) ────────────────────────────────────────────

#[derive(Default)]
struct MemoryResourceAccessStore {
    records: Mutex<HashMap<(String, String), ResourceAccessRecord>>,
}

impl ResourceAccessStore for MemoryResourceAccessStore {
    fn grant(&self, record: &ResourceAccessRecord) -> Result<()> {
        let key = (record.resource_type.clone(), record.grantee_did.clone());
        let mut map = self.records.lock().unwrap();
        if let Some(existing) = map.get(&key) {
            if existing.decision_hash == record.decision_hash {
                return Ok(()); // Idempotent
            }
        }
        map.insert(key, record.clone());
        Ok(())
    }

    fn revoke(
        &self,
        resource_type: &str,
        grantee_did: &str,
        revoked_at: u64,
        reason: &str,
    ) -> Result<()> {
        let key = (resource_type.to_string(), grantee_did.to_string());
        let mut map = self.records.lock().unwrap();
        if let Some(rec) = map.get_mut(&key) {
            if !rec.is_revoked {
                rec.is_revoked = true;
                rec.revoked_at = Some(revoked_at);
                rec.revocation_reason = Some(reason.to_string());
            }
        }
        Ok(())
    }

    fn get(&self, resource_type: &str, grantee_did: &str) -> Result<Option<ResourceAccessRecord>> {
        let key = (resource_type.to_string(), grantee_did.to_string());
        Ok(self.records.lock().unwrap().get(&key).cloned())
    }

    fn list_active(&self) -> Result<Vec<ResourceAccessRecord>> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|r| !r.is_revoked)
            .cloned()
            .collect())
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn executor_with_store(store: Arc<MemoryResourceAccessStore>) -> KernelGovernanceExecutor {
    let param_store = Arc::new(InMemoryParameterStore::new());
    KernelGovernanceExecutor::new(param_store)
        .with_resource_access_store(store as Arc<dyn ResourceAccessStore>)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

/// An accepted GrantAccess effect persists a record with the decision hash.
#[tokio::test]
async fn grant_access_persists_record_with_decision_provenance() {
    let store = Arc::new(MemoryResourceAccessStore::default());
    let executor = executor_with_store(store.clone());

    let effect = KernelEffect::Resource(ResourceEffect::GrantAccess {
        grantee_did: "did:icn:alice".to_string(),
        resource_type: "compute-cluster-alpha".to_string(),
        access_model_hash: "hash-model-read-only".to_string(),
    });

    let result = executor
        .execute_effect(effect, "receipt-grant-001")
        .await
        .unwrap();

    assert!(
        result.success,
        "GrantAccess must succeed; got: {}",
        result.message
    );
    assert!(!result.not_executed, "GrantAccess must not be not_executed");

    let record = store
        .get("compute-cluster-alpha", "did:icn:alice")
        .unwrap()
        .expect("record must be persisted after GrantAccess");

    assert_eq!(record.grantee_did, "did:icn:alice");
    assert_eq!(record.resource_type, "compute-cluster-alpha");
    assert_eq!(record.access_model_hash, "hash-model-read-only");
    // The decision_hash stored in the record must match the receipt ID passed to execute_effect.
    assert_eq!(record.decision_hash, "receipt-grant-001");
    assert!(!record.is_revoked);
}

/// A subsequent RevokeAccess effect marks the record revoked.
#[tokio::test]
async fn revoke_access_marks_record_revoked() {
    let store = Arc::new(MemoryResourceAccessStore::default());
    let executor = executor_with_store(store.clone());

    let grant = KernelEffect::Resource(ResourceEffect::GrantAccess {
        grantee_did: "did:icn:bob".to_string(),
        resource_type: "storage-vol-1".to_string(),
        access_model_hash: "hash-rw".to_string(),
    });
    executor
        .execute_effect(grant, "receipt-grant-002")
        .await
        .unwrap();

    let revoke = KernelEffect::Resource(ResourceEffect::RevokeAccess {
        grantee_did: "did:icn:bob".to_string(),
        resource_type: "storage-vol-1".to_string(),
    });
    let result = executor
        .execute_effect(revoke, "receipt-revoke-002")
        .await
        .unwrap();

    assert!(
        result.success,
        "RevokeAccess must succeed; got: {}",
        result.message
    );

    let record = store
        .get("storage-vol-1", "did:icn:bob")
        .unwrap()
        .expect("record must still exist after revocation");

    assert!(record.is_revoked, "record must be marked revoked");
    assert!(record.revoked_at.is_some(), "revoked_at must be set");
    assert_eq!(
        record.revocation_reason.as_deref(),
        Some("governance decision")
    );

    let active = store.list_active().unwrap();
    assert!(
        active
            .iter()
            .all(|r| !(r.resource_type == "storage-vol-1" && r.grantee_did == "did:icn:bob")),
        "revoked record must not appear in list_active"
    );
}

/// Replaying a GrantAccess with the same decision hash is idempotent.
#[tokio::test]
async fn grant_access_replay_is_idempotent() {
    let store = Arc::new(MemoryResourceAccessStore::default());
    let executor = executor_with_store(store.clone());

    let make_effect = || {
        KernelEffect::Resource(ResourceEffect::GrantAccess {
            grantee_did: "did:icn:carol".to_string(),
            resource_type: "network-segment-3".to_string(),
            access_model_hash: "hash-net".to_string(),
        })
    };

    let r1 = executor
        .execute_effect(make_effect(), "receipt-idempotent-003")
        .await
        .unwrap();
    assert!(r1.success);
    let after_first = store
        .get("network-segment-3", "did:icn:carol")
        .unwrap()
        .unwrap();

    let r2 = executor
        .execute_effect(make_effect(), "receipt-idempotent-003")
        .await
        .unwrap();
    assert!(r2.success, "replay must succeed");
    let after_replay = store
        .get("network-segment-3", "did:icn:carol")
        .unwrap()
        .unwrap();

    assert_eq!(
        after_first.granted_at, after_replay.granted_at,
        "idempotent replay must not change granted_at"
    );
}

/// Without a wired store, GrantAccess returns an explicit hard failure.
#[tokio::test]
async fn grant_access_without_store_is_explicit_failure() {
    let param_store = Arc::new(InMemoryParameterStore::new());
    let executor = KernelGovernanceExecutor::new(param_store);

    let effect = KernelEffect::Resource(ResourceEffect::GrantAccess {
        grantee_did: "did:icn:dave".to_string(),
        resource_type: "resource-unwired".to_string(),
        access_model_hash: "hash-x".to_string(),
    });

    let result = executor
        .execute_effect(effect, "receipt-no-store")
        .await
        .unwrap();

    assert!(!result.success, "must fail without wired store");
    assert!(
        !result.not_executed,
        "must be hard failure, not not_executed"
    );
    assert!(
        result.message.contains("not wired"),
        "error message must mention missing store; got: {}",
        result.message
    );
}
