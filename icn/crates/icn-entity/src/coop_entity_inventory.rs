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
use crate::entity::EntityId;
use serde::{Deserialize, Serialize};

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
    /// internally inconsistent — a forward binding exists whose reverse index
    /// is missing or points at a *different* `coop_id`. Atomic binds keep a
    /// healthy map consistent, so this signals corruption or a partial write,
    /// which a pre-mutation inventory must surface distinctly from
    /// [`AlreadyBound`].
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
    /// otherwise.
    pub error: Option<String>,
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

    // Mappable + unbound: is the projected EntityId already taken by a
    // different coop_id (which would make a bind conflict)?
    match map.coop_for_entity(&projected) {
        Err(e) => base(
            CoopEntityClass::StorageError,
            Some(projected),
            None,
            None,
            Some(e.to_string()),
        ),
        Ok(Some(other)) if other != coop_id => base(
            CoopEntityClass::MappableReverseConflict,
            Some(projected),
            None,
            Some(other),
            None,
        ),
        // No reverse occupant, or a (degenerate) reverse pointing back at us
        // while the forward index is empty: binding would succeed/be idempotent.
        Ok(_) => base(
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
}
