//! Read-only inventory/classification of `coop_id`s against the canonical
//! [`CoopEntityMap`](crate::CoopEntityMap) (#2082 lane, PR3).
//!
//! # What this is
//!
//! A pure, **read-only** classifier. Given a set of flat `coop_id`s and a
//! reference to a [`CoopEntityMap`], it reports how each id stands relative to
//! the canonical `coop_id ↔ EntityId` mapping: already bound, mappable but
//! unbound, mappable but blocked by a reverse conflict, not mappable at all, or
//! unreadable due to a storage error. The result quantifies — before any
//! mutation is designed — how prevalent each case is in an existing population.
//!
//! It exists because default cooperative ids are shaped like `coop:<uuid>`
//! (the colon is not a valid [`EntityId`](crate::EntityId) slug character), so
//! [`CoopEntityClass::NonMappable`] is expected to be the common case, not an
//! edge case. This report measures that, so the later surrogate-allocation /
//! backfill work can be designed from data.
//!
//! # Read-only, no authority, no normalization
//!
//! - **No writes.** The classifier calls only the map's read methods
//!   ([`entity_for_coop`](crate::CoopEntityMap::entity_for_coop),
//!   [`coop_for_entity`](crate::CoopEntityMap::coop_for_entity)) and the pure,
//!   side-effect-free [`project_coop_id`]. It never calls `bind_projected`,
//!   `bind_exact`, or any other write path.
//! - **No authority.** Classifying — like binding — grants no membership, role,
//!   capability, mandate, permission, or standing. A mapping is only a name
//!   binding; this report is only an observation about names.
//! - **No normalization.** A non-mappable `coop_id` is reported as
//!   [`CoopEntityClass::NonMappable`]; it is never lowercased, stripped, or
//!   rewritten into a different identifier.
//! - **No surrogate allocation.** This module does not invent canonical slugs
//!   for non-mappable ids; that is deliberately later work.

use crate::coop_entity_map::{project_coop_id, CoopEntityMap, CoopEntityMapError};
use crate::coop_entity_surrogate::propose_surrogate_entity_id;
use crate::entity::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serde helper: omit a `usize` field when it is zero, so the surrogate-preview
/// aggregates do not appear in the default (non-preview) report — keeping that
/// output byte-identical to the pre-surrogate inventory.
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// How a single `coop_id` stands relative to the canonical mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoopEntityClass {
    /// A healthy bidirectional binding exists for this `coop_id`: the forward
    /// index resolves to an `EntityId` and that entity's reverse index agrees
    /// (points back at this same `coop_id`).
    AlreadyBound,
    /// No binding exists, the id projects to a valid cooperative `EntityId`,
    /// and that `EntityId` is free (binding it would succeed).
    MappableUnbound,
    /// No forward binding exists and the id projects to a valid cooperative
    /// `EntityId`, but that `EntityId` is already reverse-bound to a *different*
    /// `coop_id` — so a naive bind would be rejected as a conflict.
    MappableReverseConflict,
    /// The `coop_id` is not a valid cooperative `EntityId` slug (e.g. the
    /// default `coop:<uuid>` shape). It is reported, never normalized.
    NonMappable,
    /// The persisted state for this `coop_id` is not trustworthy: either a map
    /// read failed (I/O, lock poison, decode error), or the mapping is
    /// internally inconsistent — a *one-sided* binding where only one of the
    /// forward/reverse indexes is present (forward-only, or reverse-only), or
    /// where a present forward binding's reverse index points at a *different*
    /// `coop_id`. Atomic binds keep a healthy map consistent, so this signals
    /// corruption or a partial write, which a pre-mutation inventory must
    /// surface distinctly from [`AlreadyBound`] and [`MappableUnbound`].
    StorageError,
}

/// Per-`coop_id` classification detail. The original `coop_id` is preserved
/// byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoopEntityInventoryEntry {
    /// The original flat `coop_id`, exactly as supplied.
    pub coop_id: String,
    /// The classification outcome.
    pub class: CoopEntityClass,
    /// The `EntityId` this `coop_id` would project to, when it is mappable
    /// (`MappableUnbound` / `MappableReverseConflict`). `None` otherwise.
    pub projected_entity_id: Option<EntityId>,
    /// The `EntityId` this `coop_id`'s forward index resolves to, when one
    /// exists (`AlreadyBound`, or a `StorageError` caused by an inconsistent
    /// forward-without-agreeing-reverse binding). `None` otherwise.
    pub bound_entity_id: Option<EntityId>,
    /// The `coop_id` currently occupying the relevant `EntityId`'s reverse
    /// index: for `AlreadyBound`, the (agreeing) reverse value; for
    /// `MappableReverseConflict`, the *other* `coop_id` blocking the bind; for
    /// an inconsistent `StorageError`, the mismatched reverse value (if any).
    pub reverse_bound_coop_id: Option<String>,
    /// A human-readable reason, for `NonMappable` (why the slug is invalid) and
    /// `StorageError` (the read failure or the consistency violation). `None`
    /// otherwise. When surrogate preview is enabled, a collision note for the
    /// proposed surrogate is appended here.
    pub error: Option<String>,
    /// The deterministic surrogate cooperative `EntityId` a later allocation PR
    /// would propose for this `coop_id`. Populated **only** by
    /// [`classify_coop_ids_with_surrogate_preview`] and **only** for
    /// [`CoopEntityClass::NonMappable`] entries; `None` otherwise (including the
    /// default [`classify_coop_ids`] path). A surrogate is a name proposal only
    /// and grants no authority; nothing is bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_surrogate_entity_id: Option<EntityId>,
}

/// Aggregate, read-only inventory of a set of `coop_id`s against the canonical
/// mapping. Counts are mutually exclusive and sum to `total`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoopEntityInventory {
    /// Total number of `coop_id`s classified (`== entries.len()`).
    pub total: usize,
    /// Count of [`CoopEntityClass::AlreadyBound`].
    pub already_bound: usize,
    /// Count of [`CoopEntityClass::MappableUnbound`].
    pub mappable_unbound: usize,
    /// Count of [`CoopEntityClass::MappableReverseConflict`].
    pub mappable_reverse_conflict: usize,
    /// Count of [`CoopEntityClass::NonMappable`].
    pub non_mappable: usize,
    /// Count of [`CoopEntityClass::StorageError`].
    pub storage_error: usize,
    /// Count of `NonMappable` entries for which a surrogate `EntityId` was
    /// proposed. Set only by [`classify_coop_ids_with_surrogate_preview`]; `0`
    /// (and omitted from JSON) on the default [`classify_coop_ids`] path. This
    /// is a preview statistic, **not** part of the class partition.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub surrogate_proposed: usize,
    /// Count of proposed surrogates that would collide on a later bind — either
    /// already reverse-bound to a *different* `coop_id`, or duplicated by another
    /// `coop_id` within this same batch. Set only by the preview path; `0` (and
    /// omitted from JSON) otherwise. A collision is reported, never auto-resolved.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub surrogate_collision: usize,
    /// Per-`coop_id` detail, in input order.
    pub entries: Vec<CoopEntityInventoryEntry>,
}

impl CoopEntityInventory {
    /// Record one classified entry, bumping the matching counter and `total`.
    fn record(&mut self, entry: CoopEntityInventoryEntry) {
        match entry.class {
            CoopEntityClass::AlreadyBound => self.already_bound += 1,
            CoopEntityClass::MappableUnbound => self.mappable_unbound += 1,
            CoopEntityClass::MappableReverseConflict => self.mappable_reverse_conflict += 1,
            CoopEntityClass::NonMappable => self.non_mappable += 1,
            CoopEntityClass::StorageError => self.storage_error += 1,
        }
        self.total += 1;
        self.entries.push(entry);
    }
}

/// Classify a single `coop_id` against the map using only read operations.
fn classify_one(coop_id: &str, map: &dyn CoopEntityMap) -> CoopEntityInventoryEntry {
    let base = |class, projected, bound, reverse, error| CoopEntityInventoryEntry {
        coop_id: coop_id.to_string(),
        class,
        projected_entity_id: projected,
        bound_entity_id: bound,
        reverse_bound_coop_id: reverse,
        error,
        // Surrogate preview is opt-in; the base classifier never proposes one.
        proposed_surrogate_entity_id: None,
    };

    // Forward lookup first: a present binding means AlreadyBound regardless of
    // whether the id would project (it may have been bound via bind_exact).
    match map.entity_for_coop(coop_id) {
        Err(e) => {
            return base(
                CoopEntityClass::StorageError,
                None,
                None,
                None,
                Some(e.to_string()),
            )
        }
        Ok(Some(bound)) => {
            // A forward binding exists. It is only AlreadyBound if the reverse
            // index agrees (points back at this coop_id). Atomic binds keep a
            // healthy map consistent, so a missing or mismatched reverse means a
            // corrupt/partial one-sided binding — surfaced as StorageError so a
            // pre-mutation inventory does not mistake it for a healthy mapping.
            return match map.coop_for_entity(&bound) {
                Err(e) => base(
                    CoopEntityClass::StorageError,
                    None,
                    Some(bound),
                    None,
                    Some(e.to_string()),
                ),
                Ok(Some(rev)) if rev.as_str() == coop_id => base(
                    CoopEntityClass::AlreadyBound,
                    None,
                    Some(bound),
                    Some(rev),
                    None,
                ),
                Ok(Some(rev)) => {
                    let msg = format!(
                        "inconsistent mapping: forward binds {coop_id:?} -> {bound}, \
                         but the reverse index maps {bound} -> {rev:?}"
                    );
                    base(
                        CoopEntityClass::StorageError,
                        None,
                        Some(bound),
                        Some(rev),
                        Some(msg),
                    )
                }
                Ok(None) => {
                    let msg = format!(
                        "inconsistent mapping: forward binds {coop_id:?} -> {bound}, \
                         but no reverse index entry exists"
                    );
                    base(
                        CoopEntityClass::StorageError,
                        None,
                        Some(bound),
                        None,
                        Some(msg),
                    )
                }
            };
        }
        Ok(None) => {}
    }

    // Unbound: does it project to a valid cooperative EntityId?
    let projected = match project_coop_id(coop_id) {
        Ok(p) => p,
        Err(CoopEntityMapError::NotMappable(reason)) => {
            return base(CoopEntityClass::NonMappable, None, None, None, Some(reason))
        }
        // project_coop_id only ever yields NotMappable; treat anything else as a
        // read/observation failure rather than silently miscategorizing.
        Err(other) => {
            return base(
                CoopEntityClass::StorageError,
                None,
                None,
                None,
                Some(other.to_string()),
            )
        }
    };

    // Mappable, with no forward binding: inspect the projected EntityId's
    // reverse index.
    match map.coop_for_entity(&projected) {
        Err(e) => base(
            CoopEntityClass::StorageError,
            Some(projected),
            None,
            None,
            Some(e.to_string()),
        ),
        // Reverse points back at this coop_id while the forward index is empty:
        // a one-sided (reverse-only) binding, the mirror of the forward-only
        // case above. Atomic binds keep both indexes in step, so this is
        // corruption/partial state — surface it as StorageError rather than
        // reporting the target as free (a later bind would silently repair the
        // missing forward entry instead of operating on a truly unbound pair).
        Ok(Some(occupant)) if occupant.as_str() == coop_id => {
            let msg = format!(
                "inconsistent mapping: reverse index maps {projected} -> {coop_id:?}, \
                 but no forward binding for {coop_id:?} exists"
            );
            base(
                CoopEntityClass::StorageError,
                Some(projected),
                None,
                Some(occupant),
                Some(msg),
            )
        }
        // A different coop_id legitimately occupies the projected EntityId
        // (its own forward+reverse are consistent): a naive bind would conflict.
        Ok(Some(other)) => base(
            CoopEntityClass::MappableReverseConflict,
            Some(projected),
            None,
            Some(other),
            None,
        ),
        // Reverse index is free and forward is empty: a truly unbound, bindable pair.
        Ok(None) => base(
            CoopEntityClass::MappableUnbound,
            Some(projected),
            None,
            None,
            None,
        ),
    }
}

/// Classify a set of `coop_id`s against the canonical [`CoopEntityMap`],
/// read-only.
///
/// Returns a [`CoopEntityInventory`] with mutually-exclusive counts and a
/// per-`coop_id` detail list in input order. This performs **no writes** and
/// **no normalization**, and leaves the map unchanged.
pub fn classify_coop_ids(
    coop_ids: impl IntoIterator<Item = String>,
    map: &dyn CoopEntityMap,
) -> CoopEntityInventory {
    let mut inventory = CoopEntityInventory::default();
    for coop_id in coop_ids {
        inventory.record(classify_one(&coop_id, map));
    }
    inventory
}

/// Classify a set of `coop_id`s and, for every [`CoopEntityClass::NonMappable`]
/// entry, attach the deterministic surrogate cooperative `EntityId` that a later
/// allocation/backfill PR would propose to bind — **read-only**, binding nothing.
///
/// Beyond the base [`classify_coop_ids`] classification this:
/// - sets [`CoopEntityInventoryEntry::proposed_surrogate_entity_id`] for
///   `NonMappable` entries (and leaves it `None` for every other class),
/// - counts proposals in [`CoopEntityInventory::surrogate_proposed`],
/// - flags, in [`CoopEntityInventory::surrogate_collision`] and the entry's
///   `error`, any proposed surrogate that would collide on a later bind: one
///   already reverse-bound to a *different* `coop_id`, or one duplicated by
///   another `coop_id` within this same batch.
///
/// It performs **no writes** and **no normalization**: the only map access is
/// the read-only [`CoopEntityMap::coop_for_entity`], used to detect collisions.
/// A surrogate is a name proposal only and grants no authority.
pub fn classify_coop_ids_with_surrogate_preview(
    coop_ids: impl IntoIterator<Item = String>,
    map: &dyn CoopEntityMap,
) -> CoopEntityInventory {
    // Base classification first (read-only, identical to classify_coop_ids).
    let mut inventory = classify_coop_ids(coop_ids, map);

    // Pass 1 (pure): propose a surrogate for each NonMappable entry and tally,
    // per EntityId, how many entries in this batch would occupy it on a later
    // bind. The occupancy count includes BOTH proposed surrogates and the
    // projected/bound EntityId of mappable/already-bound entries — otherwise a
    // surrogate that coincides with a same-batch mappable coop's projection (its
    // slug literally equals the surrogate) would go undetected until PR5's bind.
    let mut surrogate_for: Vec<Option<EntityId>> = Vec::with_capacity(inventory.entries.len());
    let mut entity_claims: HashMap<String, usize> = HashMap::new();
    for entry in &inventory.entries {
        let surrogate = if entry.class == CoopEntityClass::NonMappable {
            // The surrogate is valid by construction; an (unreachable) error is
            // treated as "no proposal" rather than panicking.
            propose_surrogate_entity_id(&entry.coop_id).ok()
        } else {
            None
        };
        // The EntityId this entry would occupy on a bind: a NonMappable claims its
        // proposed surrogate; a mappable claims its projection; an already-bound
        // entry its bound id. StorageError entries claim nothing.
        if let Some(claimed) = surrogate
            .as_ref()
            .or(entry.projected_entity_id.as_ref())
            .or(entry.bound_entity_id.as_ref())
        {
            *entity_claims
                .entry(claimed.as_str().to_string())
                .or_insert(0) += 1;
        }
        surrogate_for.push(surrogate);
    }

    // Pass 2: attach proposals, detect collisions (read-only: coop_for_entity
    // only), and update the preview aggregates.
    for (entry, surrogate) in inventory.entries.iter_mut().zip(surrogate_for) {
        let Some(surrogate) = surrogate else { continue };
        inventory.surrogate_proposed += 1;

        // Collision notes count toward `surrogate_collision` (a later bind would
        // be rejected); error notes are surfaced but are read failures, not
        // collisions, so they must not inflate the collision count.
        let mut collision_notes: Vec<String> = Vec::new();
        let mut error_notes: Vec<String> = Vec::new();

        // Within-batch: another entry in this batch would occupy the same
        // EntityId (another surrogate proposal, or a mappable/bound entry whose
        // projected/bound id equals this surrogate).
        if entity_claims.get(surrogate.as_str()).copied().unwrap_or(0) > 1 {
            collision_notes.push(format!(
                "proposed surrogate {} collides within-batch with another coop_id",
                surrogate.as_str()
            ));
        }

        // Reverse-binding: the surrogate target is already bound to a *different*
        // coop_id, so a later bind_exact would be rejected as a conflict. A read
        // failure is surfaced as an error, not counted as a collision.
        match map.coop_for_entity(&surrogate) {
            Ok(Some(occupant)) if occupant != entry.coop_id => collision_notes.push(format!(
                "proposed surrogate {} already reverse-bound to {occupant:?}",
                surrogate.as_str()
            )),
            Ok(_) => {}
            Err(e) => error_notes.push(format!(
                "proposed surrogate {} reverse-lookup failed: {e}",
                surrogate.as_str()
            )),
        }

        if !collision_notes.is_empty() {
            inventory.surrogate_collision += 1;
        }

        // Surface both collision and lookup-error notes on the entry.
        let notes: Vec<String> = collision_notes.into_iter().chain(error_notes).collect();
        if !notes.is_empty() {
            let joined = notes.join("; ");
            entry.error = Some(match entry.error.take() {
                Some(existing) => format!("{existing}; {joined}"),
                None => joined,
            });
        }

        entry.proposed_surrogate_entity_id = Some(surrogate);
    }

    inventory
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::coop_entity_map::{CoopEntityMap, InMemoryCoopEntityMap};

    fn coop_eid(slug: &str) -> EntityId {
        EntityId::cooperative(slug).unwrap()
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// Read-only test double returning caller-controlled forward/reverse
    /// values, so the classifier can be exercised against inconsistent
    /// (one-sided / cross-linked) states that the atomic real maps cannot
    /// produce through their public API. Writing through it is a test failure.
    struct FakeMap {
        forward: Option<EntityId>,
        reverse: Option<String>,
    }

    impl CoopEntityMap for FakeMap {
        fn bind_exact(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("classify_coop_ids must never write through the map");
        }
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("classify_coop_ids must never write through the map");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(self.forward.clone())
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            Ok(self.reverse.clone())
        }
    }

    #[test]
    fn forward_without_reverse_classifies_as_storage_error() {
        // One-sided: forward resolves, reverse index is missing (corruption).
        let map = FakeMap {
            forward: Some(coop_eid("ghost-coop")),
            reverse: None,
        };
        let inv = classify_coop_ids(ids(&["ghost-coop"]), &map);
        assert_eq!(inv.total, 1);
        assert_eq!(inv.storage_error, 1);
        assert_eq!(inv.already_bound, 0);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::StorageError);
        assert_eq!(entry.bound_entity_id, Some(coop_eid("ghost-coop")));
        assert!(entry.error.is_some());
    }

    #[test]
    fn forward_with_mismatched_reverse_classifies_as_storage_error() {
        // Cross-linked: forward resolves, but the reverse points elsewhere.
        let map = FakeMap {
            forward: Some(coop_eid("ghost-coop")),
            reverse: Some("other-coop".to_string()),
        };
        let inv = classify_coop_ids(ids(&["ghost-coop"]), &map);
        assert_eq!(inv.storage_error, 1);
        assert_eq!(inv.already_bound, 0);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::StorageError);
        assert_eq!(entry.bound_entity_id, Some(coop_eid("ghost-coop")));
        assert_eq!(entry.reverse_bound_coop_id, Some("other-coop".to_string()));
        assert!(entry.error.is_some());
    }

    #[test]
    fn forward_with_agreeing_reverse_classifies_as_already_bound() {
        // Healthy bidirectional binding via the double: reverse agrees.
        let map = FakeMap {
            forward: Some(coop_eid("ghost-coop")),
            reverse: Some("ghost-coop".to_string()),
        };
        let inv = classify_coop_ids(ids(&["ghost-coop"]), &map);
        assert_eq!(inv.already_bound, 1);
        assert_eq!(inv.storage_error, 0);
        assert_eq!(inv.entries[0].class, CoopEntityClass::AlreadyBound);
    }

    #[test]
    fn reverse_only_without_forward_classifies_as_storage_error() {
        // One-sided: no forward binding, but the projected entity's reverse
        // index points back at this coop_id (corruption mirror of forward-only).
        let map = FakeMap {
            forward: None,
            reverse: Some("ghost-coop".to_string()),
        };
        let inv = classify_coop_ids(ids(&["ghost-coop"]), &map);
        assert_eq!(inv.storage_error, 1);
        assert_eq!(inv.mappable_unbound, 0);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::StorageError);
        assert_eq!(entry.projected_entity_id, Some(coop_eid("ghost-coop")));
        assert!(entry.error.is_some());
    }

    #[test]
    fn empty_input_returns_all_zero_counts() {
        let map = InMemoryCoopEntityMap::new();
        let inv = classify_coop_ids(Vec::<String>::new(), &map);
        assert_eq!(inv.total, 0);
        assert_eq!(inv.already_bound, 0);
        assert_eq!(inv.mappable_unbound, 0);
        assert_eq!(inv.mappable_reverse_conflict, 0);
        assert_eq!(inv.non_mappable, 0);
        assert_eq!(inv.storage_error, 0);
        assert!(inv.entries.is_empty());
    }

    #[test]
    fn mappable_unbound_id_classifies_as_mappable_unbound() {
        let map = InMemoryCoopEntityMap::new();
        let inv = classify_coop_ids(ids(&["food-coop"]), &map);
        assert_eq!(inv.total, 1);
        assert_eq!(inv.mappable_unbound, 1);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::MappableUnbound);
        assert_eq!(entry.coop_id, "food-coop");
        assert_eq!(entry.projected_entity_id, Some(coop_eid("food-coop")));
        assert_eq!(entry.bound_entity_id, None);
    }

    #[test]
    fn bound_mappable_id_classifies_as_already_bound() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_projected("coop-a").unwrap();
        let inv = classify_coop_ids(ids(&["coop-a"]), &map);
        assert_eq!(inv.total, 1);
        assert_eq!(inv.already_bound, 1);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::AlreadyBound);
        assert_eq!(entry.bound_entity_id, Some(coop_eid("coop-a")));
        assert_eq!(entry.reverse_bound_coop_id, Some("coop-a".to_string()));
    }

    #[test]
    fn coop_uuid_shape_classifies_as_non_mappable() {
        let map = InMemoryCoopEntityMap::new();
        let coop_id = "coop:550e8400-e29b-41d4-a716-446655440000";
        let inv = classify_coop_ids(ids(&[coop_id]), &map);
        assert_eq!(inv.total, 1);
        assert_eq!(inv.non_mappable, 1);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::NonMappable);
        assert_eq!(entry.coop_id, coop_id);
        assert!(entry.error.is_some());
    }

    #[test]
    fn non_mappable_shape_set_all_classify_as_non_mappable() {
        let map = InMemoryCoopEntityMap::new();
        let shapes = ids(&[
            "coop:550e8400-e29b-41d4-a716-446655440000",
            "coop_A",
            "food_coop",
            "café-coop",
            "abc",
            "1coop",
            "coop--east",
        ]);
        let count = shapes.len();
        let inv = classify_coop_ids(shapes, &map);
        assert_eq!(inv.total, count);
        assert_eq!(inv.non_mappable, count);
        assert!(inv
            .entries
            .iter()
            .all(|e| e.class == CoopEntityClass::NonMappable));
    }

    #[test]
    fn reverse_conflict_classifies_as_mappable_reverse_conflict() {
        // Bind a *different* mappable coop_id to the EntityId that `food-coop`
        // would project to. `food-coop` is then unbound, projects fine, but its
        // target is already occupied by `alpha-coop`.
        let map = InMemoryCoopEntityMap::new();
        map.bind_exact("alpha-coop", &coop_eid("food-coop"))
            .unwrap();

        let inv = classify_coop_ids(ids(&["food-coop"]), &map);
        assert_eq!(inv.total, 1);
        assert_eq!(inv.mappable_reverse_conflict, 1);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::MappableReverseConflict);
        assert_eq!(entry.projected_entity_id, Some(coop_eid("food-coop")));
        assert_eq!(entry.reverse_bound_coop_id, Some("alpha-coop".to_string()));
    }

    #[test]
    fn classifier_does_not_mutate_the_map() {
        // An empty map classified twice must stay empty and yield identical
        // results: the report writes nothing.
        let map = InMemoryCoopEntityMap::new();
        let batch = ids(&["food-coop", "coop:550e8400-e29b-41d4-a716-446655440000"]);

        let first = classify_coop_ids(batch.clone(), &map);
        let second = classify_coop_ids(batch, &map);
        assert_eq!(first, second);

        // Nothing was bound as a side effect.
        assert_eq!(map.entity_for_coop("food-coop").unwrap(), None);
        assert_eq!(
            map.entity_for_coop("coop:550e8400-e29b-41d4-a716-446655440000")
                .unwrap(),
            None
        );
    }

    #[test]
    fn mixed_input_produces_correct_counts() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_projected("bound-coop").unwrap();
        map.bind_exact("alpha-coop", &coop_eid("conflict-coop"))
            .unwrap();

        let inv = classify_coop_ids(
            ids(&[
                "bound-coop",                                // AlreadyBound
                "free-coop",                                 // MappableUnbound
                "conflict-coop",                             // MappableReverseConflict
                "coop:550e8400-e29b-41d4-a716-446655440000", // NonMappable
                "coop_A",                                    // NonMappable
            ]),
            &map,
        );

        assert_eq!(inv.total, 5);
        assert_eq!(inv.already_bound, 1);
        assert_eq!(inv.mappable_unbound, 1);
        assert_eq!(inv.mappable_reverse_conflict, 1);
        assert_eq!(inv.non_mappable, 2);
        assert_eq!(inv.storage_error, 0);
        // Counts partition the input.
        assert_eq!(
            inv.already_bound
                + inv.mappable_unbound
                + inv.mappable_reverse_conflict
                + inv.non_mappable
                + inv.storage_error,
            inv.total
        );
    }

    // ------------------------------------------------------------------
    // Surrogate preview (#2082 PR4) — read-only, proposes but never binds.
    // ------------------------------------------------------------------

    const DEFAULT_COOP: &str = "coop:550e8400-e29b-41d4-a716-446655440000";

    #[test]
    fn default_classify_attaches_no_surrogate() {
        // The non-preview path must not propose surrogates or set the aggregates.
        let map = InMemoryCoopEntityMap::new();
        let inv = classify_coop_ids(ids(&[DEFAULT_COOP]), &map);
        assert_eq!(inv.non_mappable, 1);
        assert_eq!(inv.surrogate_proposed, 0);
        assert_eq!(inv.surrogate_collision, 0);
        assert_eq!(inv.entries[0].proposed_surrogate_entity_id, None);
    }

    #[test]
    fn preview_attaches_surrogate_to_non_mappable() {
        let map = InMemoryCoopEntityMap::new();
        let inv = classify_coop_ids_with_surrogate_preview(ids(&[DEFAULT_COOP]), &map);
        assert_eq!(inv.non_mappable, 1);
        assert_eq!(inv.surrogate_proposed, 1);
        assert_eq!(inv.surrogate_collision, 0);
        let entry = &inv.entries[0];
        assert_eq!(entry.class, CoopEntityClass::NonMappable);
        assert_eq!(
            entry.proposed_surrogate_entity_id,
            Some(propose_surrogate_entity_id(DEFAULT_COOP).unwrap())
        );
        // The golden vector pins the exact proposed handle.
        assert_eq!(
            entry
                .proposed_surrogate_entity_id
                .as_ref()
                .unwrap()
                .as_str(),
            "entity:icn:cooperative:coop-legacy-edf6152975301bd80c13"
        );
    }

    #[test]
    fn preview_leaves_mappable_unbound_without_surrogate() {
        let map = InMemoryCoopEntityMap::new();
        let inv = classify_coop_ids_with_surrogate_preview(ids(&["food-coop"]), &map);
        assert_eq!(inv.mappable_unbound, 1);
        assert_eq!(inv.surrogate_proposed, 0);
        assert_eq!(inv.entries[0].proposed_surrogate_entity_id, None);
    }

    #[test]
    fn preview_leaves_already_bound_without_surrogate() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_projected("coop-a").unwrap();
        let inv = classify_coop_ids_with_surrogate_preview(ids(&["coop-a"]), &map);
        assert_eq!(inv.already_bound, 1);
        assert_eq!(inv.surrogate_proposed, 0);
        assert_eq!(inv.entries[0].proposed_surrogate_entity_id, None);
    }

    #[test]
    fn preview_detects_reverse_binding_collision() {
        // Occupy the surrogate target with a DIFFERENT (mappable) coop, so the
        // proposed surrogate for DEFAULT_COOP would conflict on a later bind.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = propose_surrogate_entity_id(DEFAULT_COOP).unwrap();
        map.bind_exact("alpha-coop", &surrogate).unwrap();

        let inv = classify_coop_ids_with_surrogate_preview(ids(&[DEFAULT_COOP]), &map);
        assert_eq!(inv.non_mappable, 1);
        assert_eq!(inv.surrogate_proposed, 1);
        assert_eq!(inv.surrogate_collision, 1);
        let entry = &inv.entries[0];
        // The surrogate is still surfaced...
        assert_eq!(entry.proposed_surrogate_entity_id, Some(surrogate));
        // ...and the collision is reported in the entry's error.
        let err = entry.error.as_ref().expect("collision note expected");
        assert!(
            err.contains("alpha-coop") && err.to_lowercase().contains("reverse-bound"),
            "got error: {err}"
        );
    }

    #[test]
    fn preview_detects_within_batch_collision() {
        // Two coop_ids that propose the same surrogate. SHA-256 makes distinct
        // inputs collide only astronomically, so we exercise the dedup logic with
        // a duplicated id (real `list_cooperatives` input is distinct).
        let map = InMemoryCoopEntityMap::new();
        let inv =
            classify_coop_ids_with_surrogate_preview(ids(&[DEFAULT_COOP, DEFAULT_COOP]), &map);
        assert_eq!(inv.non_mappable, 2);
        assert_eq!(inv.surrogate_proposed, 2);
        assert_eq!(inv.surrogate_collision, 2, "both colliding entries flagged");
        for entry in &inv.entries {
            let err = entry.error.as_ref().expect("collision note expected");
            assert!(err.contains("within-batch"), "got error: {err}");
        }
    }

    #[test]
    fn preview_writes_nothing() {
        let map = InMemoryCoopEntityMap::new();
        let _ = classify_coop_ids_with_surrogate_preview(ids(&[DEFAULT_COOP, "food-coop"]), &map);
        // Neither the non-mappable id, its surrogate target, nor the mappable id
        // was bound as a side effect.
        assert_eq!(map.entity_for_coop(DEFAULT_COOP).unwrap(), None);
        assert_eq!(map.entity_for_coop("food-coop").unwrap(), None);
        let surrogate = propose_surrogate_entity_id(DEFAULT_COOP).unwrap();
        assert_eq!(map.coop_for_entity(&surrogate).unwrap(), None);
    }

    #[test]
    fn preview_class_counts_still_partition_total() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_projected("bound-coop").unwrap();
        let inv = classify_coop_ids_with_surrogate_preview(
            ids(&["bound-coop", "free-coop", DEFAULT_COOP, "coop_A"]),
            &map,
        );
        // surrogate_proposed/collision are separate statistics, not classes.
        assert_eq!(
            inv.already_bound
                + inv.mappable_unbound
                + inv.mappable_reverse_conflict
                + inv.non_mappable
                + inv.storage_error,
            inv.total
        );
        assert_eq!(inv.non_mappable, 2);
        assert_eq!(inv.surrogate_proposed, 2);
    }

    #[test]
    fn preview_reverse_lookup_error_is_not_counted_as_collision() {
        // A `coop_for_entity` *read failure* during preview is surfaced in the
        // entry's error, but it is NOT a collision: `surrogate_collision` counts
        // bind conflicts (within-batch dup / already-reverse-bound) only.
        struct ReverseErrMap;
        impl CoopEntityMap for ReverseErrMap {
            fn bind_exact(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
                unreachable!("preview must never write through the map");
            }
            fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
                unreachable!("preview must never write through the map");
            }
            // Ok(None) keeps "coop_A" classified NonMappable (unbound + unmappable).
            fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
                Ok(None)
            }
            // The surrogate reverse-lookup fails.
            fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
                Err(CoopEntityMapError::Storage(
                    "reverse index unreadable".into(),
                ))
            }
        }

        let inv = classify_coop_ids_with_surrogate_preview(ids(&["coop_A"]), &ReverseErrMap);
        assert_eq!(inv.non_mappable, 1);
        assert_eq!(inv.surrogate_proposed, 1);
        assert_eq!(
            inv.surrogate_collision, 0,
            "a reverse-lookup error is a read failure, not a bind collision"
        );
        let err = inv.entries[0]
            .error
            .as_ref()
            .expect("lookup error surfaced");
        assert!(
            err.contains("reverse-lookup failed"),
            "lookup error must still be surfaced; got: {err}"
        );
    }

    #[test]
    fn preview_flags_surrogate_colliding_with_same_batch_mappable_projection() {
        // A mappable coop literally named the surrogate slug of a non-mappable
        // coop projects to the EXACT EntityId being proposed. Neither is bound,
        // so the reverse-index check is empty — but binding both later would
        // conflict on the same entity id, so the in-batch occupancy check must
        // include mappable projections, not just other surrogate proposals.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = propose_surrogate_entity_id(DEFAULT_COOP).unwrap();
        let collider = surrogate.identifier().to_string(); // valid slug, unbound

        let inv = classify_coop_ids_with_surrogate_preview(
            vec![DEFAULT_COOP.to_string(), collider.clone()],
            &map,
        );

        assert_eq!(inv.non_mappable, 1);
        assert_eq!(inv.mappable_unbound, 1);
        assert_eq!(inv.surrogate_proposed, 1);
        assert_eq!(
            inv.surrogate_collision, 1,
            "surrogate must collide with a same-batch mappable projection"
        );
        let nm = inv
            .entries
            .iter()
            .find(|e| e.coop_id == DEFAULT_COOP)
            .unwrap();
        assert!(nm
            .error
            .as_ref()
            .expect("collision note")
            .contains("within-batch"));
    }
}
