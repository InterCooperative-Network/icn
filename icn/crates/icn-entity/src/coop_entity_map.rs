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
use serde::{Deserialize, Serialize};
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

    /// The supplied `entity_id` is not a cooperative [`EntityId`]. A flat
    /// `coop_id` denotes a cooperative target (RFC-0018 invariant), so binding
    /// it to a community, federation, or individual entity is refused before
    /// either index is written.
    #[error("coop/entity binding requires a cooperative EntityId: {0}")]
    InvalidEntityType(String),

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

/// Provenance index key: `coop_entity_provenance:{coop_id}` → serialized
/// [`CoopEntityBindingProvenance`]. Keyed by `coop_id` (parallel to the forward
/// index); the existing forward/reverse key schema is untouched.
fn provenance_key(coop_id: &str) -> Vec<u8> {
    format!("coop_entity_provenance:{coop_id}").into_bytes()
}

/// Where a `coop_id ↔ EntityId` binding came from.
///
/// A mapping is only a *name binding*, never authority (see the module docs).
/// Provenance records **how** a binding was established so a future trusted
/// resolver (A2c store-backed source) can decide whether the binding is
/// authoritative enough for Enforce/Issue. This module only persists and retrieves
/// provenance; it does **not** consume it for any authorization decision.
///
/// [`UnknownLegacy`](CoopEntityBindingProvenance::UnknownLegacy) is the fail-closed
/// sentinel: a binding written before provenance existed (or by a plain
/// [`bind_resolved`](CoopEntityMap::bind_resolved)) carries no provenance record and
/// reads back as `UnknownLegacy`. Missing provenance must never be treated as
/// trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoopEntityBindingProvenance {
    /// Bound at cooperative activation time.
    Activation,
    /// Bound by an operator-run backfill (e.g. `icnctl coop entity-backfill-surrogates`).
    OperatorBackfill,
    /// Bound to a generated surrogate `EntityId` for a non-mappable `coop_id`.
    Surrogate,
    /// Bound on the authority of a governance / decision receipt, identified by its
    /// receipt hash or id (stored as an opaque string).
    GovernanceReceipt {
        /// Opaque identifier (hash or id) of the governing decision receipt.
        receipt_id: String,
    },
    /// Provenance is unknown: a pre-provenance ("legacy") binding, or none recorded.
    /// MUST be treated as untrusted for Enforce/Issue.
    UnknownLegacy,
}

/// A `coop_id ↔ EntityId` binding together with its [`CoopEntityBindingProvenance`].
///
/// Returned by the provenance-aware reads. The `coop_id` is preserved byte-for-byte
/// (as bound); a binding still confers no authority on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoopEntityBinding {
    /// The original flat `coop_id`, byte-for-byte as bound.
    pub coop_id: String,
    /// The bound cooperative `EntityId`.
    pub entity_id: EntityId,
    /// How the binding was established (or [`CoopEntityBindingProvenance::UnknownLegacy`]).
    pub provenance: CoopEntityBindingProvenance,
}

/// Serialize provenance for durable storage (stable serde_json bytes).
fn encode_provenance(p: &CoopEntityBindingProvenance) -> Result<Vec<u8>, CoopEntityMapError> {
    serde_json::to_vec(p)
        .map_err(|e| CoopEntityMapError::Storage(format!("provenance encode error: {e}")))
}

/// Deserialize provenance from durable storage.
fn decode_provenance(bytes: &[u8]) -> Result<CoopEntityBindingProvenance, CoopEntityMapError> {
    serde_json::from_slice(bytes)
        .map_err(|e| CoopEntityMapError::Storage(format!("provenance decode error: {e}")))
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

/// Validate that `entity_id` is a **well-formed cooperative** [`EntityId`].
///
/// [`EntityId`] derives `Deserialize` as a raw newtype, so an instance obtained
/// from serde/backfill data (rather than the validating constructors) can carry
/// a malformed inner string whose type tag still reads as `cooperative` — e.g.
/// `entity:icn:cooperative:BAD_slug`, where [`EntityId::is_cooperative`] is
/// `true` but the slug is invalid. Re-parsing via [`EntityId::from_str`] re-runs
/// slug validation, so a value that `entity_for_coop` would later fail to read
/// back is rejected here before any index is written.
fn validate_cooperative_entity_id(entity_id: &EntityId) -> Result<(), CoopEntityMapError> {
    let parsed = EntityId::from_str(entity_id.as_str()).map_err(|e| {
        CoopEntityMapError::InvalidEntityType(format!(
            "{entity_id} is not a well-formed EntityId: {e}"
        ))
    })?;
    if !parsed.is_cooperative() {
        return Err(CoopEntityMapError::InvalidEntityType(format!(
            "{entity_id} has type {}",
            parsed.entity_type()
        )));
    }
    Ok(())
}

/// A canonical, reversible `coop_id ↔ EntityId` mapping store.
///
/// Implementations persist a bidirectional binding. Binding is idempotent for an
/// identical pair and rejects conflicts in both directions. See the module docs:
/// a binding is a name binding only and confers no authority.
pub trait CoopEntityMap {
    /// Project `coop_id` to its cooperative [`EntityId`] and bind the pair.
    ///
    /// Equivalent to `bind_resolved(coop_id, &project_coop_id(coop_id)?)`.
    /// Returns the bound [`EntityId`] on success. Non-mappable `coop_id`s return
    /// [`CoopEntityMapError::NotMappable`].
    fn bind_projected(&self, coop_id: &str) -> Result<EntityId, CoopEntityMapError> {
        let entity_id = project_coop_id(coop_id)?;
        self.bind_resolved(coop_id, &entity_id)?;
        Ok(entity_id)
    }

    /// Bind a caller-supplied `(coop_id, entity_id)` pair, requiring `coop_id`
    /// to itself be mappable (a valid cooperative slug).
    ///
    /// This is the reject-not-normalize entry point: a non-mappable `coop_id` is
    /// rejected with [`CoopEntityMapError::NotMappable`] because surrogate slug
    /// allocation is out of scope here. After the gate it delegates to
    /// [`bind_resolved`](CoopEntityMap::bind_resolved), which owns the shared
    /// cooperative-type validation, both-direction conflict detection, and
    /// atomic bidirectional write. A rejected bind leaves no one-sided write.
    fn bind_exact(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError> {
        // Reject-not-normalize gate: this entry point requires a mappable
        // coop_id; the actual write is the shared `bind_resolved` primitive.
        project_coop_id(coop_id)?;
        self.bind_resolved(coop_id, entity_id)
    }

    /// Bind an original `coop_id` to an already-resolved cooperative
    /// [`EntityId`], **without** projecting or normalizing the `coop_id`.
    ///
    /// Unlike [`bind_exact`](CoopEntityMap::bind_exact), this skips the
    /// `project_coop_id` reject-not-normalize gate, so a non-mappable legacy id
    /// (e.g. `coop:<uuid>`, `coop_A`, `food_coop`) is accepted and preserved
    /// byte-for-byte. The caller supplies the resolved cooperative `EntityId`
    /// (e.g. from
    /// [`propose_surrogate_entity_id`](crate::propose_surrogate_entity_id) or a
    /// projection). The `entity_id` must be a well-formed cooperative
    /// `EntityId`, otherwise the bind is refused with
    /// [`CoopEntityMapError::InvalidEntityType`] before either index is written.
    ///
    /// Idempotent for an identical pair; rejects
    /// [`CoopEntityMapError::Conflict`] if either side is already bound to a
    /// different counterpart; the forward and reverse indexes are written
    /// atomically (a rejected bind leaves no one-sided write). It never returns
    /// [`CoopEntityMapError::NotMappable`] — that gate is intentionally skipped.
    /// A binding is a name binding only and confers no authority.
    fn bind_resolved(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError>;

    /// Look up the [`EntityId`] bound to `coop_id`, if any.
    fn entity_for_coop(&self, coop_id: &str) -> Result<Option<EntityId>, CoopEntityMapError>;

    /// Look up the original `coop_id` bound to `entity_id`, if any.
    ///
    /// Returns the `coop_id` exactly as it was bound (byte-for-byte), which is
    /// what makes the mapping reversible.
    fn coop_for_entity(&self, entity_id: &EntityId) -> Result<Option<String>, CoopEntityMapError>;

    // ------------------------------------------------------------------
    // Provenance-aware extension (A2c prerequisite)
    //
    // Additive: these default impls keep every existing implementor working
    // without change, and are fail-closed — an implementor without a provenance
    // store reports `UnknownLegacy` (never a trusted value). The durable
    // implementors (`InMemoryCoopEntityMap`, `SledCoopEntityMap`) override them to
    // persist/retrieve real provenance. This module records provenance only; it
    // does not consume it for authorization.
    // ------------------------------------------------------------------

    /// Bind `(coop_id, entity_id)` and record its [`CoopEntityBindingProvenance`].
    ///
    /// Same cooperative-type validation and both-direction pair-conflict rules as
    /// [`bind_resolved`](CoopEntityMap::bind_resolved); like it, it does **not** run
    /// the `project_coop_id` reject-not-normalize gate, so a non-mappable legacy
    /// `coop_id` is preserved byte-for-byte. Rebinding the identical pair with the
    /// identical provenance is idempotent; rebinding the same pair with **different**
    /// recorded provenance is rejected with [`CoopEntityMapError::Conflict`].
    ///
    /// The default impl ignores provenance and delegates to
    /// [`bind_resolved`](CoopEntityMap::bind_resolved) (implementors with a
    /// provenance store override it); such a default implementor then reports
    /// [`CoopEntityBindingProvenance::UnknownLegacy`] on read.
    fn bind_resolved_with_provenance(
        &self,
        coop_id: &str,
        entity_id: &EntityId,
        _provenance: CoopEntityBindingProvenance,
    ) -> Result<(), CoopEntityMapError> {
        self.bind_resolved(coop_id, entity_id)
    }

    /// Read the binding for `coop_id` together with its provenance.
    ///
    /// Default impl composes [`entity_for_coop`](CoopEntityMap::entity_for_coop) with
    /// [`CoopEntityBindingProvenance::UnknownLegacy`] — fail-closed: an implementor
    /// without a provenance store never reports a trusted provenance.
    fn binding_for_coop(
        &self,
        coop_id: &str,
    ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
        Ok(self
            .entity_for_coop(coop_id)?
            .map(|entity_id| CoopEntityBinding {
                coop_id: coop_id.to_string(),
                entity_id,
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            }))
    }

    /// Read the binding for `entity_id` together with its provenance.
    ///
    /// Default impl composes [`coop_for_entity`](CoopEntityMap::coop_for_entity) with
    /// [`CoopEntityBindingProvenance::UnknownLegacy`] (fail-closed).
    fn binding_for_entity(
        &self,
        entity_id: &EntityId,
    ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
        Ok(self
            .coop_for_entity(entity_id)?
            .map(|coop_id| CoopEntityBinding {
                coop_id,
                entity_id: entity_id.clone(),
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            }))
    }
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
    /// `coop_id` → binding provenance (absent ⇒ `UnknownLegacy` on read).
    provenance: HashMap<String, CoopEntityBindingProvenance>,
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
    fn bind_resolved(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError> {
        // Shared write primitive: no project_coop_id gate here (bind_exact and
        // bind_projected run that gate, then delegate to this method). The caller
        // supplies an already-resolved cooperative EntityId, so a non-mappable
        // original coop_id (e.g. `coop:<uuid>`) is accepted and preserved as-is.
        //
        // A flat coop_id denotes a cooperative target (RFC-0018): refuse to bind
        // anything but a well-formed cooperative entity_id before touching either index.
        validate_cooperative_entity_id(entity_id)?;

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

    fn bind_resolved_with_provenance(
        &self,
        coop_id: &str,
        entity_id: &EntityId,
        provenance: CoopEntityBindingProvenance,
    ) -> Result<(), CoopEntityMapError> {
        // Same cooperative-type gate as bind_resolved, before touching any index.
        validate_cooperative_entity_id(entity_id)?;

        let mut inner = self
            .inner
            .write()
            .map_err(|_| CoopEntityMapError::Storage("in-memory map lock poisoned".into()))?;

        // Both-direction pair-conflict checks (identical to bind_resolved).
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
        // Provenance conflict: a recorded, differing provenance is rejected
        // (idempotent only when equal). No recorded provenance ⇒ record it now.
        if let Some(existing) = inner.provenance.get(coop_id) {
            if existing != &provenance {
                return Err(CoopEntityMapError::Conflict(format!(
                    "coop_id {coop_id:?} already has different provenance recorded"
                )));
            }
        }

        inner.forward.insert(coop_id.to_string(), entity_id.clone());
        inner
            .reverse
            .insert(entity_id.as_str().to_string(), coop_id.to_string());
        inner.provenance.insert(coop_id.to_string(), provenance);
        Ok(())
    }

    fn binding_for_coop(
        &self,
        coop_id: &str,
    ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
        let inner = self
            .inner
            .read()
            .map_err(|_| CoopEntityMapError::Storage("in-memory map lock poisoned".into()))?;
        Ok(inner
            .forward
            .get(coop_id)
            .map(|entity_id| CoopEntityBinding {
                coop_id: coop_id.to_string(),
                entity_id: entity_id.clone(),
                provenance: inner
                    .provenance
                    .get(coop_id)
                    .cloned()
                    .unwrap_or(CoopEntityBindingProvenance::UnknownLegacy),
            }))
    }

    fn binding_for_entity(
        &self,
        entity_id: &EntityId,
    ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
        let inner = self
            .inner
            .read()
            .map_err(|_| CoopEntityMapError::Storage("in-memory map lock poisoned".into()))?;
        Ok(inner
            .reverse
            .get(entity_id.as_str())
            .map(|coop_id| CoopEntityBinding {
                coop_id: coop_id.clone(),
                entity_id: entity_id.clone(),
                provenance: inner
                    .provenance
                    .get(coop_id)
                    .cloned()
                    .unwrap_or(CoopEntityBindingProvenance::UnknownLegacy),
            }))
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

    /// Read the stored provenance for `coop_id`, defaulting to
    /// [`CoopEntityBindingProvenance::UnknownLegacy`] when none is recorded
    /// (fail-closed: a pre-provenance binding is never trusted).
    fn read_provenance(
        &self,
        coop_id: &str,
    ) -> Result<CoopEntityBindingProvenance, CoopEntityMapError> {
        match self
            .db
            .get(provenance_key(coop_id))
            .map_err(|e| CoopEntityMapError::Storage(format!("db get error: {e}")))?
        {
            Some(bytes) => decode_provenance(&bytes),
            None => Ok(CoopEntityBindingProvenance::UnknownLegacy),
        }
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
    fn bind_resolved(&self, coop_id: &str, entity_id: &EntityId) -> Result<(), CoopEntityMapError> {
        // Shared write primitive: no project_coop_id gate here (bind_exact and
        // bind_projected run that gate, then delegate to this method). The caller
        // supplies an already-resolved cooperative EntityId, so a non-mappable
        // original coop_id (e.g. `coop:<uuid>`) is accepted and preserved as-is.
        //
        // A flat coop_id denotes a cooperative target (RFC-0018): refuse to bind
        // anything but a well-formed cooperative entity_id before the transaction writes either index.
        validate_cooperative_entity_id(entity_id)?;

        let fkey = forward_key(coop_id);
        let rkey = reverse_key(entity_id);
        let entity_str = entity_id.as_str().to_string();
        let coop_string = coop_id.to_string();

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
                                "entity {entity_str} is already bound to a different coop_id"
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

    fn bind_resolved_with_provenance(
        &self,
        coop_id: &str,
        entity_id: &EntityId,
        provenance: CoopEntityBindingProvenance,
    ) -> Result<(), CoopEntityMapError> {
        // Same cooperative-type gate as bind_resolved, before the transaction.
        validate_cooperative_entity_id(entity_id)?;

        let fkey = forward_key(coop_id);
        let rkey = reverse_key(entity_id);
        let pkey = provenance_key(coop_id);
        let entity_str = entity_id.as_str().to_string();
        let coop_string = coop_id.to_string();
        let prov_bytes = encode_provenance(&provenance)?;

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
                                "entity {entity_str} is already bound to a different coop_id"
                            )),
                        ));
                    }
                }
                // Provenance conflict: a recorded, differing provenance is rejected
                // (idempotent only when equal). No record yet ⇒ record it now.
                if let Some(bytes) = tx.get(&pkey)? {
                    if bytes.as_ref() != prov_bytes.as_slice() {
                        return Err(ConflictableTransactionError::Abort(
                            CoopEntityMapError::Conflict(format!(
                                "coop_id {coop_string:?} already has different provenance recorded"
                            )),
                        ));
                    }
                }

                // Forward, reverse, and provenance are written in the same
                // transaction: a binding's provenance is all-or-nothing with its
                // indexes (an aborted conflict above leaves nothing written).
                tx.insert(fkey.as_slice(), entity_str.as_bytes())?;
                tx.insert(rkey.as_slice(), coop_string.as_bytes())?;
                tx.insert(pkey.as_slice(), prov_bytes.as_slice())?;
                Ok(())
            })
            .map_err(map_tx_err)
    }

    fn binding_for_coop(
        &self,
        coop_id: &str,
    ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
        let entity_id = match self.entity_for_coop(coop_id)? {
            Some(id) => id,
            None => return Ok(None),
        };
        let provenance = self.read_provenance(coop_id)?;
        Ok(Some(CoopEntityBinding {
            coop_id: coop_id.to_string(),
            entity_id,
            provenance,
        }))
    }

    fn binding_for_entity(
        &self,
        entity_id: &EntityId,
    ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
        let coop_id = match self.coop_for_entity(entity_id)? {
            Some(c) => c,
            None => return Ok(None),
        };
        let provenance = self.read_provenance(&coop_id)?;
        Ok(Some(CoopEntityBinding {
            coop_id,
            entity_id: entity_id.clone(),
            provenance,
        }))
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

    // ------------------------------------------------------------------
    // Non-cooperative entity_id rejection
    // (RFC-0018 invariant: a flat coop_id denotes a cooperative target)
    // ------------------------------------------------------------------

    #[test]
    fn test_inmem_bind_exact_rejects_community_entity() {
        let map = InMemoryCoopEntityMap::new();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &community),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
    }

    #[test]
    fn test_inmem_bind_exact_rejects_federation_entity() {
        let map = InMemoryCoopEntityMap::new();
        let federation = EntityId::federation("some-federation").unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &federation),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
    }

    #[test]
    fn test_inmem_bind_exact_rejects_individual_entity() {
        use icn_identity::KeyPair;
        let map = InMemoryCoopEntityMap::new();
        let kp = KeyPair::generate().unwrap();
        let individual = EntityId::from_did(kp.did());
        assert!(matches!(
            map.bind_exact("coop-a", &individual),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
    }

    #[test]
    fn test_sled_bind_exact_rejects_community_entity() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &community),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
    }

    #[test]
    fn test_sled_bind_exact_rejects_federation_entity() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let federation = EntityId::federation("some-federation").unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &federation),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
    }

    #[test]
    fn test_sled_rejecting_non_cooperative_writes_no_index() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &community),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        // The rejection must happen before any write: neither index exists.
        assert_eq!(map.entity_for_coop("coop-a").unwrap(), None);
        assert_eq!(map.coop_for_entity(&community).unwrap(), None);
    }

    // ------------------------------------------------------------------
    // Malformed cooperative EntityId rejection
    // (EntityId derives Deserialize as a raw newtype, so serde/backfill data
    // can carry a cooperative-typed id whose slug is invalid; it must not be
    // bound, or the forward index would be unreadable via from_str.)
    // ------------------------------------------------------------------

    #[test]
    fn test_inmem_bind_exact_rejects_malformed_cooperative_entity_id() {
        let map = InMemoryCoopEntityMap::new();
        // Bypass the validating constructor: build a cooperative-typed id with
        // an invalid slug straight from serde.
        let malformed: EntityId =
            serde_json::from_str("\"entity:icn:cooperative:BAD_slug\"").unwrap();
        assert!(malformed.is_cooperative()); // type tag reads cooperative...
        assert!(matches!(
            map.bind_exact("coop-a", &malformed),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        // ...but nothing was persisted.
        assert_eq!(map.entity_for_coop("coop-a").unwrap(), None);
    }

    #[test]
    fn test_sled_bind_exact_rejects_malformed_cooperative_entity_id() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let malformed: EntityId =
            serde_json::from_str("\"entity:icn:cooperative:BAD_slug\"").unwrap();
        assert!(matches!(
            map.bind_exact("coop-a", &malformed),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        // Neither index written, and the forward lookup stays readable (None).
        assert_eq!(map.entity_for_coop("coop-a").unwrap(), None);
        assert_eq!(map.coop_for_entity(&malformed).unwrap(), None);
    }

    // ==================================================================
    // bind_resolved (#2082 PR5): persist an original coop_id ↔ resolved
    // cooperative EntityId WITHOUT projecting/normalizing the coop_id.
    //
    // The defining difference from bind_exact: a non-mappable legacy id
    // such as `coop:<uuid>` is ACCEPTED (not rejected as NotMappable),
    // while every other invariant — cooperative-type gate, both-direction
    // conflict detection, atomic write, byte-for-byte reverse, idempotency
    // — is preserved. A binding still confers no authority.
    // ==================================================================

    /// A non-mappable default cooperative id (`coop:<uuid>`) used throughout.
    const UUID_COOP: &str = "coop:550e8400-e29b-41d4-a716-446655440000";

    // ---- in-memory ----

    #[test]
    fn test_inmem_bind_resolved_non_mappable_uuid_binds_forward() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("canonical-coop");
        map.bind_resolved(UUID_COOP, &entity).unwrap();
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), Some(entity));
    }

    #[test]
    fn test_inmem_bind_resolved_reverse_returns_original_coop_id() {
        // The reverse index must return the ORIGINAL non-mappable coop_id
        // byte-for-byte (colon, uuid shape, and all) — that is what makes the
        // mapping reversible without normalizing the legacy id.
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("canonical-coop");
        map.bind_resolved(UUID_COOP, &entity).unwrap();
        assert_eq!(
            map.coop_for_entity(&entity).unwrap(),
            Some(UUID_COOP.to_string())
        );
    }

    #[test]
    fn test_inmem_bind_resolved_same_pair_idempotent() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("canonical-coop");
        map.bind_resolved(UUID_COOP, &entity).unwrap();
        map.bind_resolved(UUID_COOP, &entity).unwrap(); // no-op, must not error
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), Some(entity));
    }

    #[test]
    fn test_inmem_bind_resolved_same_coop_different_entity_conflicts() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved(UUID_COOP, &coop_eid("coop-a")).unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &coop_eid("coop-b")),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_inmem_bind_resolved_different_coop_same_entity_conflicts() {
        let map = InMemoryCoopEntityMap::new();
        let shared = coop_eid("shared-coop");
        map.bind_resolved(UUID_COOP, &shared).unwrap();
        assert!(matches!(
            map.bind_resolved("coop:other-uuid", &shared),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_inmem_bind_resolved_rejects_non_cooperative_entity() {
        let map = InMemoryCoopEntityMap::new();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &community),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        let federation = EntityId::federation("some-federation").unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &federation),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        let kp = icn_identity::KeyPair::generate().unwrap();
        let individual = EntityId::from_did(kp.did());
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &individual),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        // Rejection happens before any write.
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
    }

    #[test]
    fn test_inmem_bind_resolved_rejects_malformed_cooperative_entity_id() {
        let map = InMemoryCoopEntityMap::new();
        // Cooperative type tag, invalid slug (bypasses the validating ctor).
        let malformed: EntityId =
            serde_json::from_str("\"entity:icn:cooperative:BAD_slug\"").unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &malformed),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
    }

    #[test]
    fn test_inmem_bind_resolved_accepts_mappable_coop_id() {
        // bind_resolved is general: a mappable coop_id can also be bound to an
        // already-resolved projection. It simply does not REQUIRE mappability.
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved("food-coop", &coop_eid("food-coop"))
            .unwrap();
        assert_eq!(
            map.entity_for_coop("food-coop").unwrap(),
            Some(coop_eid("food-coop"))
        );
    }

    // ---- sled ----

    #[test]
    fn test_sled_bind_resolved_non_mappable_uuid_round_trips() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let entity = coop_eid("canonical-coop");
        map.bind_resolved(UUID_COOP, &entity).unwrap();
        assert_eq!(
            map.entity_for_coop(UUID_COOP).unwrap(),
            Some(entity.clone())
        );
        assert_eq!(
            map.coop_for_entity(&entity).unwrap(),
            Some(UUID_COOP.to_string())
        );
    }

    #[test]
    fn test_sled_bind_resolved_same_pair_idempotent() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let entity = coop_eid("canonical-coop");
        map.bind_resolved(UUID_COOP, &entity).unwrap();
        map.bind_resolved(UUID_COOP, &entity).unwrap();
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), Some(entity));
    }

    #[test]
    fn test_sled_bind_resolved_same_coop_different_entity_conflicts() {
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_resolved(UUID_COOP, &coop_eid("coop-a")).unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &coop_eid("coop-b")),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_sled_bind_resolved_different_coop_same_entity_conflicts() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let shared = coop_eid("shared-coop");
        map.bind_resolved(UUID_COOP, &shared).unwrap();
        assert!(matches!(
            map.bind_resolved("coop:other-uuid", &shared),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_sled_bind_resolved_rejects_non_cooperative_writes_no_index() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &community),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        // The rejection must happen before any write: neither index exists.
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(map.coop_for_entity(&community).unwrap(), None);
    }

    #[test]
    fn test_sled_bind_resolved_rejects_malformed_cooperative_entity_id() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let malformed: EntityId =
            serde_json::from_str("\"entity:icn:cooperative:BAD_slug\"").unwrap();
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &malformed),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(map.coop_for_entity(&malformed).unwrap(), None);
    }

    #[test]
    fn test_sled_bind_resolved_conflict_leaves_no_one_sided_write() {
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_resolved(UUID_COOP, &coop_eid("coop-a")).unwrap();
        // A forward-conflicting bind must be rejected AND must not leave a
        // reverse index for the rejected entity.
        let other = coop_eid("other-coop");
        assert!(matches!(
            map.bind_resolved(UUID_COOP, &other),
            Err(CoopEntityMapError::Conflict(_))
        ));
        assert_eq!(
            map.coop_for_entity(&other).unwrap(),
            None,
            "rejected bind must not leave a one-sided reverse write"
        );
    }

    #[test]
    fn test_sled_bind_resolved_persists_across_reopen() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("bind_resolved_persist_test");

        // Session 1: bind a non-mappable coop_id to a resolved cooperative id.
        {
            let db = sled::open(&db_path).unwrap();
            let map = SledCoopEntityMap::new(Arc::new(db));
            map.bind_resolved(UUID_COOP, &coop_eid("canonical-coop"))
                .unwrap();
        }
        // Session 2: reopen and verify both directions survived.
        {
            let db = sled::open(&db_path).unwrap();
            let map = SledCoopEntityMap::new(Arc::new(db));
            assert_eq!(
                map.entity_for_coop(UUID_COOP).unwrap(),
                Some(coop_eid("canonical-coop"))
            );
            assert_eq!(
                map.coop_for_entity(&coop_eid("canonical-coop")).unwrap(),
                Some(UUID_COOP.to_string())
            );
        }
    }

    // ---- contrast with bind_exact + PR4 surrogate composition ----

    #[test]
    fn test_bind_resolved_accepts_what_bind_exact_rejects_as_not_mappable() {
        // The single behavioral difference is the reject-not-normalize gate.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = crate::propose_surrogate_entity_id(UUID_COOP).unwrap();
        // bind_exact runs project_coop_id and rejects the non-mappable id...
        assert!(matches!(
            map.bind_exact(UUID_COOP, &surrogate),
            Err(CoopEntityMapError::NotMappable(_))
        ));
        // ...bind_resolved accepts the very same pair.
        map.bind_resolved(UUID_COOP, &surrogate).unwrap();
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), Some(surrogate));
    }

    #[test]
    fn test_bind_resolved_never_returns_not_mappable() {
        // Even for an id that project_coop_id would reject (uppercase +
        // underscore), bind_resolved must not surface NotMappable — that gate
        // is intentionally skipped.
        let map = InMemoryCoopEntityMap::new();
        let result = map.bind_resolved("coop_A", &coop_eid("legacy-coop-a"));
        assert!(!matches!(result, Err(CoopEntityMapError::NotMappable(_))));
        assert!(result.is_ok());
    }

    #[test]
    fn test_bind_resolved_pr4_golden_surrogate_round_trips() {
        // Pins PR4's golden vector composed with the PR5 write primitive.
        let surrogate = crate::propose_surrogate_entity_id(UUID_COOP).unwrap();
        assert_eq!(
            surrogate.as_str(),
            "entity:icn:cooperative:coop-legacy-edf6152975301bd80c13"
        );
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_resolved(UUID_COOP, &surrogate).unwrap();
        assert_eq!(
            map.entity_for_coop(UUID_COOP).unwrap(),
            Some(surrogate.clone())
        );
        assert_eq!(
            map.coop_for_entity(&surrogate).unwrap(),
            Some(UUID_COOP.to_string())
        );
    }

    // ================================================================
    // Provenance (A2c prerequisite)
    // ================================================================

    #[test]
    fn test_inmem_provenance_bind_round_trips_forward_and_reverse() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("food-coop");
        map.bind_resolved_with_provenance(
            "food-coop",
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let by_coop = map.binding_for_coop("food-coop").unwrap().unwrap();
        assert_eq!(by_coop.coop_id, "food-coop");
        assert_eq!(by_coop.entity_id, entity);
        assert_eq!(by_coop.provenance, CoopEntityBindingProvenance::Activation);

        let by_entity = map.binding_for_entity(&entity).unwrap().unwrap();
        assert_eq!(by_entity.coop_id, "food-coop");
        assert_eq!(by_entity.entity_id, entity);
        assert_eq!(
            by_entity.provenance,
            CoopEntityBindingProvenance::Activation
        );
    }

    #[test]
    fn test_inmem_provenance_governance_receipt_round_trips() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("gov-coop");
        let prov = CoopEntityBindingProvenance::GovernanceReceipt {
            receipt_id: "receipt-abc123".to_string(),
        };
        map.bind_resolved_with_provenance("gov-coop", &entity, prov.clone())
            .unwrap();
        assert_eq!(
            map.binding_for_coop("gov-coop")
                .unwrap()
                .unwrap()
                .provenance,
            prov
        );
    }

    #[test]
    fn test_inmem_missing_provenance_reads_unknown_legacy() {
        // A plain bind_resolved records no provenance; provenance-aware reads must
        // report UnknownLegacy (fail-closed: never trusted), not a real provenance.
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved("legacy-coop", &coop_eid("legacy-coop"))
            .unwrap();
        let b = map.binding_for_coop("legacy-coop").unwrap().unwrap();
        assert_eq!(b.provenance, CoopEntityBindingProvenance::UnknownLegacy);
        let b2 = map
            .binding_for_entity(&coop_eid("legacy-coop"))
            .unwrap()
            .unwrap();
        assert_eq!(b2.provenance, CoopEntityBindingProvenance::UnknownLegacy);
    }

    #[test]
    fn test_inmem_existing_apis_unchanged_alongside_provenance() {
        // bind_exact / bind_projected / entity_for_coop / coop_for_entity keep
        // working; their (provenance-less) rows read back as UnknownLegacy.
        let map = InMemoryCoopEntityMap::new();
        assert_eq!(
            map.bind_projected("proj-coop").unwrap(),
            coop_eid("proj-coop")
        );
        map.bind_exact("exact-coop", &coop_eid("exact-coop"))
            .unwrap();
        assert_eq!(
            map.entity_for_coop("proj-coop").unwrap(),
            Some(coop_eid("proj-coop"))
        );
        assert_eq!(
            map.coop_for_entity(&coop_eid("exact-coop")).unwrap(),
            Some("exact-coop".to_string())
        );
        assert_eq!(
            map.binding_for_coop("proj-coop")
                .unwrap()
                .unwrap()
                .provenance,
            CoopEntityBindingProvenance::UnknownLegacy
        );
    }

    #[test]
    fn test_inmem_provenance_same_pair_same_provenance_idempotent() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("idem-coop");
        map.bind_resolved_with_provenance(
            "idem-coop",
            &entity,
            CoopEntityBindingProvenance::Surrogate,
        )
        .unwrap();
        // Second identical bind is idempotent (Ok).
        map.bind_resolved_with_provenance(
            "idem-coop",
            &entity,
            CoopEntityBindingProvenance::Surrogate,
        )
        .unwrap();
        assert_eq!(
            map.binding_for_coop("idem-coop")
                .unwrap()
                .unwrap()
                .provenance,
            CoopEntityBindingProvenance::Surrogate
        );
    }

    #[test]
    fn test_inmem_provenance_same_pair_different_provenance_rejected() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_eid("clash-coop");
        map.bind_resolved_with_provenance(
            "clash-coop",
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        assert!(matches!(
            map.bind_resolved_with_provenance(
                "clash-coop",
                &entity,
                CoopEntityBindingProvenance::OperatorBackfill
            ),
            Err(CoopEntityMapError::Conflict(_))
        ));
        // Original provenance is preserved after the rejected rebind.
        assert_eq!(
            map.binding_for_coop("clash-coop")
                .unwrap()
                .unwrap()
                .provenance,
            CoopEntityBindingProvenance::Activation
        );
    }

    #[test]
    fn test_inmem_provenance_pair_conflicts_still_rejected() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "coop-one",
            &coop_eid("coop-one"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        // same coop_id → different entity
        assert!(matches!(
            map.bind_resolved_with_provenance(
                "coop-one",
                &coop_eid("coop-two"),
                CoopEntityBindingProvenance::Activation
            ),
            Err(CoopEntityMapError::Conflict(_))
        ));
        // different coop_id → same entity
        assert!(matches!(
            map.bind_resolved_with_provenance(
                "coop-three",
                &coop_eid("coop-one"),
                CoopEntityBindingProvenance::Activation
            ),
            Err(CoopEntityMapError::Conflict(_))
        ));
    }

    #[test]
    fn test_inmem_provenance_rejects_non_cooperative_before_write() {
        let map = InMemoryCoopEntityMap::new();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_resolved_with_provenance(
                UUID_COOP,
                &community,
                CoopEntityBindingProvenance::Activation
            ),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        // No write happened, including provenance.
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(map.binding_for_coop(UUID_COOP).unwrap(), None);
    }

    #[test]
    fn test_inmem_provenance_bind_preserves_non_mappable_coop_id() {
        // bind_resolved_with_provenance, like bind_resolved, skips the project gate
        // and preserves a non-mappable legacy coop_id byte-for-byte.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = coop_eid("coop-legacy-deadbeefdeadbeefdead");
        map.bind_resolved_with_provenance(
            UUID_COOP,
            &surrogate,
            CoopEntityBindingProvenance::Surrogate,
        )
        .unwrap();
        let b = map.binding_for_coop(UUID_COOP).unwrap().unwrap();
        assert_eq!(b.coop_id, UUID_COOP); // preserved byte-for-byte
        assert_eq!(b.entity_id, surrogate);
        assert_eq!(b.provenance, CoopEntityBindingProvenance::Surrogate);
    }

    #[test]
    fn test_sled_provenance_round_trips() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let entity = coop_eid("sled-coop");
        map.bind_resolved_with_provenance(
            "sled-coop",
            &entity,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .unwrap();
        let b = map.binding_for_coop("sled-coop").unwrap().unwrap();
        assert_eq!(b.entity_id, entity);
        assert_eq!(b.provenance, CoopEntityBindingProvenance::OperatorBackfill);
        assert_eq!(
            map.binding_for_entity(&entity).unwrap().unwrap().provenance,
            CoopEntityBindingProvenance::OperatorBackfill
        );
    }

    #[test]
    fn test_sled_missing_provenance_reads_unknown_legacy() {
        let map = SledCoopEntityMap::temporary().unwrap();
        map.bind_resolved("sled-legacy", &coop_eid("sled-legacy"))
            .unwrap();
        assert_eq!(
            map.binding_for_coop("sled-legacy")
                .unwrap()
                .unwrap()
                .provenance,
            CoopEntityBindingProvenance::UnknownLegacy
        );
    }

    #[test]
    fn test_sled_provenance_different_provenance_rejected() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let entity = coop_eid("sled-clash");
        map.bind_resolved_with_provenance(
            "sled-clash",
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        assert!(matches!(
            map.bind_resolved_with_provenance(
                "sled-clash",
                &entity,
                CoopEntityBindingProvenance::Surrogate
            ),
            Err(CoopEntityMapError::Conflict(_))
        ));
        // idempotent for identical provenance
        map.bind_resolved_with_provenance(
            "sled-clash",
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
    }

    #[test]
    fn test_sled_provenance_rejects_non_cooperative_before_write() {
        let map = SledCoopEntityMap::temporary().unwrap();
        let community = EntityId::community("some-community").unwrap();
        assert!(matches!(
            map.bind_resolved_with_provenance(
                UUID_COOP,
                &community,
                CoopEntityBindingProvenance::Activation
            ),
            Err(CoopEntityMapError::InvalidEntityType(_))
        ));
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(map.binding_for_coop(UUID_COOP).unwrap(), None);
    }

    #[test]
    fn test_sled_provenance_persists_across_reopen() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("provenance_persist_test");

        // Session 1: provenance-aware bind (governance receipt carries a String).
        {
            let db = sled::open(&db_path).unwrap();
            let map = SledCoopEntityMap::new(Arc::new(db));
            map.bind_resolved_with_provenance(
                UUID_COOP,
                &coop_eid("persist-coop"),
                CoopEntityBindingProvenance::GovernanceReceipt {
                    receipt_id: "rcpt-9f".to_string(),
                },
            )
            .unwrap();
        }
        // Session 2: reopen and verify the binding AND its provenance survived.
        {
            let db = sled::open(&db_path).unwrap();
            let map = SledCoopEntityMap::new(Arc::new(db));
            let b = map.binding_for_coop(UUID_COOP).unwrap().unwrap();
            assert_eq!(b.coop_id, UUID_COOP);
            assert_eq!(b.entity_id, coop_eid("persist-coop"));
            assert_eq!(
                b.provenance,
                CoopEntityBindingProvenance::GovernanceReceipt {
                    receipt_id: "rcpt-9f".to_string()
                }
            );
        }
    }
}
