//! Canonical, persisted, reversible `coop_id ↔ EntityId` mapping (foundation).
//!
//! # What this is
//!
//! The gateway authorizes requests on a flat string `coop_id` whose validation
//! rules ([`crate::EntityId`] slugs are stricter than the gateway's `validate_coop_id`)
//! do not line up with the typed [`EntityId`] namespace. This module provides the
//! durable, two-way mapping that lets the system move between the two without
//! lossy normalization or accidental collisions.
//!
//! - **Forward:** `coop_id` → [`EntityId`]
//! - **Reverse:** [`EntityId`] → original `coop_id` (preserved byte-for-byte)
//!
//! # A mapping is NOT authority
//!
//! A `coop_id ↔ EntityId` binding grants **zero** standing, role, capability,
//! membership, mandate, or permission. It is only a reversible *name binding*.
//! Authority in ICN still flows from memberships, charters, roles, capabilities,
//! governance decisions, decision receipts, and effect execution — never from the
//! mere existence of a mapping entry, and never from a global identity alone.
//! Resolving a `coop_id` to an `EntityId` through this map says nothing about what
//! the caller is allowed to do; it only says which entity the identifier denotes.
//!
//! # Reject, never normalize
//!
//! Projection from a flat `coop_id` to a cooperative [`EntityId`] is
//! reject-not-normalize: a `coop_id` that is not already a valid cooperative slug
//! (e.g. `coop_A`, `food_coop`, `café-coop`, `abc`) is rejected with
//! [`CoopEntityMapError::NotMappable`]. It is **never** lowercased, stripped of
//! underscores, transliterated, or otherwise rewritten — that would silently
//! collapse distinct identities (`coop_A` and `coop-a` are different namespaces).
//!
//! # Scope of this foundation (PR1)
//!
//! This is the store primitive only. It deliberately does **not**:
//! - allocate generated/surrogate slugs for non-mappable `coop_id`s,
//! - wire activation, backfill, treasury `entity_id` population, or authorization.
//!
//! Binding a non-mappable `coop_id` therefore returns
//! [`CoopEntityMapError::NotMappable`] in this revision; surrogate allocation and
//! backfill wiring are follow-up work.

use crate::entity::EntityId;
use sled::transaction::ConflictableTransactionError;
use sled::Db;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Errors from canonical `coop_id ↔ EntityId` mapping operations.
#[derive(Debug, Error)]
pub enum CoopEntityMapError {
    /// The `coop_id` is not a valid cooperative [`EntityId`] slug and this
    /// foundation does not allocate surrogate slugs. The binding is refused
    /// rather than silently normalized.
    #[error("coop_id is not mappable to a cooperative EntityId (reject, do not normalize): {0}")]
    NotMappable(String),

    /// The requested binding conflicts with an existing one (in either
    /// direction): the `coop_id` is already bound to a different `EntityId`,
    /// or the `EntityId` is already bound to a different `coop_id`.
    #[error("coop/entity mapping conflict: {0}")]
    Conflict(String),

    /// The underlying storage layer failed (I/O, serialization, lock poison).
    #[error("coop/entity map storage error: {0}")]
    Storage(String),
}

/// Forward index key: `coop_entity:{coop_id}` → `EntityId`.
fn forward_key(coop_id: &str) -> Vec<u8> {
    format!("coop_entity:{coop_id}").into_bytes()
}

/// Reverse index key: `entity_coop:{entity_id}` → original `coop_id`.
fn reverse_key(entity_id: &EntityId) -> Vec<u8> {
    format!("entity_coop:{}", entity_id.as_str()).into_bytes()
}

/// Project a flat `coop_id` to a cooperative [`EntityId`], reject-not-normalize.
///
/// Returns [`CoopEntityMapError::NotMappable`] for any `coop_id` that is not
/// already a valid cooperative slug. This is a pure function: it performs no
/// storage and has no side effects, and it never rewrites the input.
pub fn project_coop_id(coop_id: &str) -> Result<EntityId, CoopEntityMapError> {
    EntityId::cooperative(coop_id)
        .map_err(|e| CoopEntityMapError::NotMappable(format!("{coop_id:?}: {e}")))
}

/// A canonical, reversible `coop_id ↔ EntityId` mapping store.
///
/// Implementations persist a bidirectional binding. Binding is idempotent for an
/// identical pair and rejects conflicts in both directions. See the module docs:
/// a binding is a name binding only and confers no authority.
pub trait CoopEntityMap {
    /// Project `coop_id` to its cooperative [`EntityId`] and bind the pair.
    ///
    /// Equivalent to `bind_exact(coop_id, &project_coop_id(coop_id)?)`. Returns
    /// the bound [`EntityId`] on success. Non-mappable `coop_id`s return
    /// [`CoopEntityMapError::NotMappable`].
    fn bind_projected(&self, coop_id: &str) -> Result<EntityId, CoopEntityMapError> {
        let entity_id = project_coop_id(coop_id)?;
        self.bind_exact(coop_id, &entity_id)?;
        Ok(entity_id)
    }

    /// Bind a caller-supplied `(coop_id, entity_id)` pair.
    ///
    /// In this foundation the `coop_id` must itself be mappable (a valid
    /// cooperative slug); non-mappable `coop_id`s are rejected with
    /// [`CoopEntityMapError::NotMappable`] because surrogate slug allocation is
    /// out of scope here. Idempotent for an identical pair; rejects
    /// [`CoopEntityMapError::Conflict`] if either side is already bound to a
    /// different counterpart. The forward and reverse indexes are written
    /// atomically — a rejected bind leaves no one-sided write.
    fn bind_exact(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError>;

    /// Look up the [`EntityId`] bound to `coop_id`, if any.
    fn entity_for_coop(&self, coop_id: &str) -> Result<Option<EntityId>, CoopEntityMapError>;

    /// Look up the original `coop_id` bound to `entity_id`, if any.
    ///
    /// Returns the `coop_id` exactly as it was bound (byte-for-byte), which is
    /// what makes the mapping reversible.
    fn coop_for_entity(&self, entity_id: &EntityId) -> Result<Option<String>, CoopEntityMapError>;
}

// ============================================================================
// InMemoryCoopEntityMap
// ============================================================================

#[derive(Debug, Default)]
struct InMemoryInner {
    /// `coop_id` → `EntityId`
    forward: HashMap<String, EntityId>,
    /// `entity_id.as_str()` → original `coop_id`
    reverse: HashMap<String, String>,
}

/// In-memory `coop_id ↔ EntityId` map for testing and development.
///
/// Not persistent. Use [`SledCoopEntityMap`] for durable storage.
#[derive(Debug, Default)]
pub struct InMemoryCoopEntityMap {
    inner: RwLock<InMemoryInner>,
}

impl InMemoryCoopEntityMap {
    /// Create a new empty in-memory map.
    pub fn new() -> Self {
        Self::default()
    }
}

impl CoopEntityMap for InMemoryCoopEntityMap {
    fn bind_exact(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError> {
        // Reject-not-normalize gate: this foundation does not allocate surrogate
        // slugs, so a non-mappable coop_id is refused rather than rewritten.
        project_coop_id(coop_id)?;

        let mut inner = self
            .inner
            .write()
            .map_err(|_| CoopEntityMapError::Storage("in-memory map lock poisoned".into()))?;

        if let Some(bound) = inner.forward.get(coop_id) {
            if bound != entity_id {
                return Err(CoopEntityMapError::Conflict(format!(
                    "coop_id {coop_id:?} is already bound to {bound}"
                )));
            }
        }
        if let Some(bound_coop) = inner.reverse.get(entity_id.as_str()) {
            if bound_coop.as_str() != coop_id {
                return Err(CoopEntityMapError::Conflict(format!(
                    "entity {entity_id} is already bound to coop_id {bound_coop:?}"
                )));
            }
        }

        // Idempotent for an identical pair; otherwise insert both indexes.
        inner.forward.insert(coop_id.to_string(), entity_id.clone());
        inner
            .reverse
            .insert(entity_id.as_str().to_string(), coop_id.to_string());
        Ok(())
    }

    fn entity_for_coop(&self, coop_id: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
        let inner = self
            .inner
            .read()
            .map_err(|_| CoopEntityMapError::Storage("in-memory map lock poisoned".into()))?;
        Ok(inner.forward.get(coop_id).cloned())
    }

    fn coop_for_entity(&self, entity_id: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
        let inner = self
            .inner
            .read()
            .map_err(|_| CoopEntityMapError::Storage("in-memory map lock poisoned".into()))?;
        Ok(inner.reverse.get(entity_id.as_str()).cloned())
    }
}

// ============================================================================
// SledCoopEntityMap
// ============================================================================

/// Sled-backed persistent `coop_id ↔ EntityId` map.
///
/// # Key Schema
///
/// - `coop_entity:{coop_id}` → `EntityId` (as its canonical string form)
/// - `entity_coop:{entity_id}` → original `coop_id` (raw UTF-8 bytes)
///
/// Both keys are written inside a single sled transaction, so a binding is
/// all-or-nothing: a crash or a rejected conflict never leaves a one-sided index.
pub struct SledCoopEntityMap {
    db: Arc<Db>,
}

impl SledCoopEntityMap {
    /// Create a map backed by the given sled database handle.
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Create a temporary in-memory sled database for testing.
    #[cfg(test)]
    fn temporary() -> Result<Self, CoopEntityMapError> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| CoopEntityMapError::Storage(format!("failed to open temp db: {e}")))?;
        Ok(Self::new(Arc::new(db)))
    }
}

/// Map a sled transaction error back to a [`CoopEntityMapError`].
fn map_tx_err(e: sled::transaction::TransactionError<CoopEntityMapError>) -> CoopEntityMapError {
    match e {
        sled::transaction::TransactionError::Abort(err) => err,
        sled::transaction::TransactionError::Storage(e) => {
            CoopEntityMapError::Storage(format!("sled transaction storage error: {e}"))
        }
    }
}

impl CoopEntityMap for SledCoopEntityMap {
    fn bind_exact(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError> {
        // Reject-not-normalize gate (same invariant as the in-memory map).
        project_coop_id(coop_id)?;

        let fkey = forward_key(coop_id);
        let rkey = reverse_key(entity_id);
        let entity_str = entity_id.as_str().to_string();
        let coop_string = coop_id.to_string();
        let entity_display = entity_id.to_string();

        self.db
            .transaction(|tx| {
                let existing_fwd = tx.get(&fkey)?;
                let existing_rev = tx.get(&rkey)?;

                if let Some(bytes) = &existing_fwd {
                    if bytes.as_ref() != entity_str.as_bytes() {
                        return Err(ConflictableTransactionError::Abort(
                            CoopEntityMapError::Conflict(format!(
                                "coop_id {coop_string:?} is already bound to a different entity"
                            )),
                        ));
                    }
                }
                if let Some(bytes) = &existing_rev {
                    if bytes.as_ref() != coop_string.as_bytes() {
                        return Err(ConflictableTransactionError::Abort(
                            CoopEntityMapError::Conflict(format!(
                                "entity {entity_display} is already bound to a different coop_id"
                            )),
                        ));
                    }
                }

                // Idempotent for an identical pair; otherwise write both indexes
                // atomically (an aborted conflict above leaves nothing written).
                tx.insert(fkey.as_slice(), entity_str.as_bytes())?;
                tx.insert(rkey.as_slice(), coop_string.as_bytes())?;
                Ok(())
            })
            .map_err(map_tx_err)
    }

    fn entity_for_coop(&self, coop_id: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
        let key = forward_key(coop_id);
        match self
            .db
            .get(&key)
            .map_err(|e| CoopEntityMapError::Storage(format!("db get error: {e}")))?
        {
            Some(bytes) => {
                let s = std::str::from_utf8(&bytes).map_err(|e| {
                    CoopEntityMapError::Storage(format!("invalid utf-8 in forward index: {e}"))
                })?;
                let id = EntityId::from_str(s).map_err(|e| {
                    CoopEntityMapError::Storage(format!("invalid EntityId in forward index: {e}"))
                })?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    fn coop_for_entity(&self, entity_id: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
        let key = reverse_key(entity_id);
        match self
            .db
            .get(&key)
            .map_err(|e| CoopEntityMapError::Storage(format!("db get error: {e}")))?
        {
            Some(bytes) => {
                let s = std::str::from_utf8(&bytes).map_err(|e| {
                    CoopEntityMapError::Storage(format!("invalid utf-8 in reverse index: {e}"))
                })?;
                Ok(Some(s.to_string()))
            }
            None => Ok(None),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn coop_eid(slug: &str) -> EntityId {
        EntityId::cooperative(slug).unwrap()
    }

    // ------------------------------------------------------------------
    // Projection tests (pure, reject-not-normalize)
    // ------------------------------------------------------------------

    #[test]
    fn test_project_coop_a_ok() {
        let id = project_coop_id("coop-a").unwrap();
        assert!(id.is_cooperative());
        assert_eq!(id.identifier(), "coop-a");
    }

    #[test]
    fn test_project_food_coop_ok() {
        let id = project_coop_id("food-coop").unwrap();
        assert_eq!(id, coop_eid("food-coop"));
    }

    #[test]
    fn test_project_uppercase_underscore_not_mappable() {
        assert!(matches!(
            project_coop_id("coop_A"),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    #[test]
    fn test_project_lowercase_underscore_not_mappable() {
        assert!(matches!(
            project_coop_id("food_coop"),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    #[test]
    fn test_project_unicode_not_mappable() {
        assert!(matches!(
            project_coop_id("café-coop"),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    #[test]
    fn test_project_too_short_not_mappable() {
        assert!(matches!(
            project_coop_id("abc"),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    #[test]
    fn test_project_leading_digit_not_mappable() {
        assert!(matches!(
            project_coop_id("1coop"),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    #[test]
    fn test_project_consecutive_hyphens_not_mappable() {
        assert!(matches!(
            project_coop_id("coop--east"),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    #[test]
    fn test_coop_a_never_silently_mapped_to_coop_dash_a() {
        // `coop_A` must be rejected outright, never rewritten into the distinct
        // identifier `coop-a`.
        assert!(project_coop_id("coop_A").is_err());
        assert_eq!(project_coop_id("coop-a").unwrap(), coop_eid("coop-a"));
    }

    // ------------------------------------------------------------------
    // In-memory map tests
    // ------------------------------------------------------------------

    #[test]
    fn test_inmem_bind_projected_writes_forward() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_projected("coop-a").unwrap();
        assert_eq!(
            map.entity_for_coop("coop-a").unwrap(),
            Some(coop_eid("coop-a"))
        );
    }

    #[test]
    fn test_inmem_bind_projected_writes_reverse() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_projected("coop-a").unwrap();
        assert_eq!(
            map.coop_for_entity(&coop_eid("coop-a")).unwrap(),
            Some("coop-a".to_string())
        );
    }

    #[test]
    fn test_inmem_rebind_same_pair_idempotent() {
        let map = InMemoryCoopEntityMap::new();
        let e = coop_eid("coop-a");
        map.bind_exact("coop-a", &e).unwrap();
        map.bind_exact("coop-a", &e).unwrap(); // no-op, must not error
        assert_eq!(map.entity_for_coop("coop-a").unwrap(), Some(e));
    }

    #[test]
    fn test_inmem_same_coop_different_entity_conflicts() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_exact("coop-a", &coop_eid("coop-a")).unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &coop_eid("coop-b")),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_inmem_different_coop_same_entity_conflicts() {
        let map = InMemoryCoopEntityMap::new();
        let shared = coop_eid("shared-coop");
        map.bind_exact("coop-a", &shared).unwrap();
        assert!(matches!(
            map.bind_exact("coop-b", &shared),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_inmem_reverse_returns_original_coop_id() {
        // Bind a coop_id that differs from the entity slug; the reverse lookup
        // must return the ORIGINAL coop_id, not the entity's slug.
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("canonical-coop");
        map.bind_exact("legacy-coop", &entity).unwrap();
        assert_eq!(
            map.coop_for_entity(&entity).unwrap(),
            Some("legacy-coop".to_string())
        );
    }

    #[test]
    fn test_inmem_bind_exact_rejects_non_mappable_coop_id() {
        let map = InMemoryCoopEntityMap::new();
        assert!(matches!(
            map.bind_exact("coop_A", &coop_eid("coop-a")),
            Err(CoopEntityMapError::NotMappable(_))
        ));
    }

    // ------------------------------------------------------------------
    // Sled map tests
    // ------------------------------------------------------------------

    #[test]
    fn test_sled_bind_writes_forward() {
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_projected("coop-a").unwrap();
        assert_eq!(
            map.entity_for_coop("coop-a").unwrap(),
            Some(coop_eid("coop-a"))
        );
    }

    #[test]
    fn test_sled_bind_writes_reverse() {
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_projected("coop-a").unwrap();
        assert_eq!(
            map.coop_for_entity(&coop_eid("coop-a")).unwrap(),
            Some("coop-a".to_string())
        );
    }

    #[test]
    fn test_sled_rebind_same_pair_idempotent() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let e = coop_eid("coop-a");
        map.bind_exact("coop-a", &e).unwrap();
        map.bind_exact("coop-a", &e).unwrap();
        assert_eq!(map.entity_for_coop("coop-a").unwrap(), Some(e));
    }

    #[test]
    fn test_sled_same_coop_different_entity_conflicts() {
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_exact("coop-a", &coop_eid("coop-a")).unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &coop_eid("coop-b")),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_sled_different_coop_same_entity_conflicts() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let shared = coop_eid("shared-coop");
        map.bind_exact("coop-a", &shared).unwrap();
        assert!(matches!(
            map.bind_exact("coop-b", &shared),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_sled_persists_across_reopen() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("coop_entity_map_test");

        // Session 1: bind.
        {
            let db = sled::open(&db_path).unwrap();
            let map = SledCoopEntityMap::new(Arc::new(db));
            map.bind_exact("legacy-coop", &coop_eid("canonical-coop"))
                .unwrap();
        }

        // Session 2: reopen and verify both directions survived.
        {
            let db = sled::open(&db_path).unwrap();
            let map = SledCoopEntityMap::new(Arc::new(db));
            assert_eq!(
                map.entity_for_coop("legacy-coop").unwrap(),
                Some(coop_eid("canonical-coop"))
            );
            assert_eq!(
                map.coop_for_entity(&coop_eid("canonical-coop")).unwrap(),
                Some("legacy-coop".to_string())
            );
        }
    }

    #[test]
    fn test_sled_failed_conflict_leaves_no_one_sided_write() {
        let map = SledCoopEntityMap::temporary().unwrap();
        // Establish coop-a -> coop-a.
        map.bind_exact("coop-a", &coop_eid("coop-a")).unwrap();

        // Attempt a forward-conflicting bind: coop-a -> other-coop. This must be
        // rejected AND must not write the reverse index for `other-coop`.
        let other = coop_eid("other-coop");
        assert!(matches!(
            map.bind_exact("coop-a", &other),
            Err(CoopEntityMapError::Conflict(_))
        ));
        assert_eq!(
            map.coop_for_entity(&other).unwrap(),
            None,
            "rejected bind must not leave a one-sided reverse write"
        );
    }
}
