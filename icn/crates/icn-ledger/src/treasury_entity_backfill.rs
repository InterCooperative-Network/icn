//! Read-only **plan** for populating a legacy treasury's `entity_id` from the
//! canonical, trusted [`CoopEntityMap`] (#2082 lane).
//!
//! # What this is
//!
//! A treasury created via the legacy [`Treasury::new`](crate::Treasury::new)
//! path carries `entity_id: None` (its only organizational identity is the flat
//! `coop_id`). This module computes — **read-only, no mutation** — which such
//! treasuries *could* have their `entity_id` populated from a **trusted,
//! non-ambiguous** `coop_id → EntityId` binding in the canonical
//! [`CoopEntityMap`], and *why* each ineligible treasury is skipped. It writes
//! nothing and changes no authorization state.
//!
//! # Why a plan, not an apply
//!
//! Populating `treasury.entity_id` is **not** authorization-neutral under the
//! operator-opt-in `EnforceTrustedResolver` treasury gate: that gate uses
//! `treasury.entity_id()` as the entity-membership *target* (see
//! `icn-gateway`'s `compute_treasury_observation` / `decide_treasury_gate`). In
//! the default `ObserveOnly` mode it changes nothing (the observation is
//! discarded), but in enforce mode flipping `None → Some` can change which
//! entity membership is checked against — e.g. a non-mappable `coop:<uuid>`
//! would move from an indeterminate "missing target" to an evaluable one. So
//! the *mutation* belongs in a separate, explicitly-reviewed follow-up; this
//! slice ships only the safe, read-only classification core. A mapping is a
//! name binding and grants **zero** authority — this plan moves no power and,
//! being read-only, changes no decision in any mode.
//!
//! # Trust is fail-closed
//!
//! A treasury is eligible only when its `coop_id` has a binding whose provenance
//! is trusted-for-resolution
//! ([`CoopEntityBindingProvenance::is_trusted_for_resolution`]) **and** whose
//! target is a well-formed cooperative [`EntityId`] **and** whose reverse index
//! points back to the same `coop_id` (unambiguous). Every other case —
//! `UnknownLegacy`/unprovenanced/gossip-originated provenance, a missing
//! binding, a non-cooperative or malformed target, an ambiguous reverse binding,
//! or a storage read error — is **skipped** (fail closed).

use crate::treasury::{Treasury, TreasuryManager};
use icn_entity::{CoopEntityMap, EntityId};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// The outcome the plan assigns to a single treasury row. Exactly one action is
/// assigned per row, and every action maps to exactly one aggregate counter, so
/// the per-row actions partition [`TreasuryEntityIdBackfillPlan::total`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreasuryEntityIdBackfillAction {
    /// Eligible: a legacy treasury (`entity_id: None`) whose `coop_id` has a
    /// trusted, non-ambiguous cooperative binding. Under a (future, separate)
    /// apply it *would* be populated; this plan writes nothing.
    WouldPopulate,
    /// Skipped: the treasury already carries an `entity_id` (nothing to do).
    SkippedAlreadyHasEntityId,
    /// Skipped: the `coop_id` has no binding in the canonical map.
    SkippedNoMapping,
    /// Skipped: a binding exists but its provenance is not trusted for resolution
    /// (`UnknownLegacy` / unprovenanced / gossip-originated). Fail closed.
    SkippedUntrustedProvenance,
    /// Skipped: the bound `EntityId` is not a well-formed cooperative entity
    /// (a flat `coop_id` denotes a cooperative target — RFC-0018). Fail closed.
    SkippedNonCooperativeEntity,
    /// Skipped: the binding is internally inconsistent — the resolved entity's
    /// reverse binding points to a *different* `coop_id`. Fail closed.
    SkippedAmbiguousBinding,
    /// Skipped: a storage read error prevented establishing safety. Fail closed.
    SkippedStorageError,
}

/// One treasury row to plan over. The original `coop_id` is used as-is (the
/// lookup key into the map) and is never normalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasuryEntityIdBackfillTarget {
    /// The treasury's flat `coop_id` (the map lookup key).
    pub coop_id: String,
    /// The treasury DID, for operator-facing identification. `None` when planning
    /// over raw `(coop_id, entity_id)` rows rather than concrete treasuries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub treasury_did: Option<String>,
    /// The treasury's current `entity_id` (`None` for a legacy treasury — the
    /// only kind that is a backfill candidate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_entity_id: Option<EntityId>,
}

/// Per-treasury audit detail. The `coop_id` is preserved byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasuryEntityIdBackfillEntry {
    /// The treasury's flat `coop_id`, exactly as supplied.
    pub coop_id: String,
    /// The treasury DID, when planning over concrete treasuries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub treasury_did: Option<String>,
    /// The trusted cooperative `EntityId` that *would* populate this treasury
    /// (present for [`WouldPopulate`](TreasuryEntityIdBackfillAction::WouldPopulate)),
    /// and surfaced for several skip reasons so an operator can see the rejected
    /// candidate. `None` for no-mapping / storage-error-on-lookup rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_entity_id: Option<EntityId>,
    /// The action assigned to this row.
    pub action: TreasuryEntityIdBackfillAction,
    /// A short, human-readable explanation of the action.
    pub reason: String,
    /// The underlying map read error string when one exists. `None` for clean
    /// outcomes and for policy findings (ambiguity is surfaced in `reason`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate, auditable result of a read-only treasury `entity_id` backfill plan.
///
/// The counters partition [`total`](Self::total): `total == would_populate +
/// skipped_already_has_entity_id + skipped_no_mapping +
/// skipped_untrusted_provenance + skipped_non_cooperative_entity +
/// skipped_ambiguous_binding + skipped_storage_error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasuryEntityIdBackfillPlan {
    /// Total number of treasuries considered (`== entries.len()`).
    pub total: usize,
    /// Eligible: trusted, non-ambiguous cooperative bindings for legacy rows.
    pub would_populate: usize,
    /// Skipped because the treasury already has an `entity_id`.
    pub skipped_already_has_entity_id: usize,
    /// Skipped because the `coop_id` has no binding in the map.
    pub skipped_no_mapping: usize,
    /// Skipped because the binding's provenance is not trusted (fail closed).
    pub skipped_untrusted_provenance: usize,
    /// Skipped because the bound `EntityId` is not a well-formed cooperative.
    pub skipped_non_cooperative_entity: usize,
    /// Skipped because the binding's reverse index is ambiguous (fail closed).
    pub skipped_ambiguous_binding: usize,
    /// Skipped because a storage read error prevented establishing safety.
    pub skipped_storage_error: usize,
    /// Per-treasury detail, in input order.
    pub entries: Vec<TreasuryEntityIdBackfillEntry>,
}

impl TreasuryEntityIdBackfillPlan {
    /// An empty plan (used for an empty / missing treasury set).
    fn new() -> Self {
        Self {
            total: 0,
            would_populate: 0,
            skipped_already_has_entity_id: 0,
            skipped_no_mapping: 0,
            skipped_untrusted_provenance: 0,
            skipped_non_cooperative_entity: 0,
            skipped_ambiguous_binding: 0,
            skipped_storage_error: 0,
            entries: Vec::new(),
        }
    }

    /// Record one classified entry, bumping the matching counter and `total`.
    fn record(&mut self, entry: TreasuryEntityIdBackfillEntry) {
        match entry.action {
            TreasuryEntityIdBackfillAction::WouldPopulate => self.would_populate += 1,
            TreasuryEntityIdBackfillAction::SkippedAlreadyHasEntityId => {
                self.skipped_already_has_entity_id += 1
            }
            TreasuryEntityIdBackfillAction::SkippedNoMapping => self.skipped_no_mapping += 1,
            TreasuryEntityIdBackfillAction::SkippedUntrustedProvenance => {
                self.skipped_untrusted_provenance += 1
            }
            TreasuryEntityIdBackfillAction::SkippedNonCooperativeEntity => {
                self.skipped_non_cooperative_entity += 1
            }
            TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding => {
                self.skipped_ambiguous_binding += 1
            }
            TreasuryEntityIdBackfillAction::SkippedStorageError => self.skipped_storage_error += 1,
        }
        self.total += 1;
        self.entries.push(entry);
    }
}

/// Plan (read-only) which treasury rows could have their `entity_id` populated
/// from a **trusted, non-ambiguous** cooperative binding in `map`.
///
/// Read-only by construction: it performs only `binding_for_coop` /
/// `coop_for_entity` reads and **never** binds or mutates the map or any
/// treasury. A binding confers no authority; this plan changes no authorization
/// decision in any mode.
pub fn plan_treasury_entity_id_backfill(
    targets: impl IntoIterator<Item = TreasuryEntityIdBackfillTarget>,
    map: &dyn CoopEntityMap,
) -> TreasuryEntityIdBackfillPlan {
    let mut plan = TreasuryEntityIdBackfillPlan::new();
    for target in targets {
        plan.record(plan_target(&target, map));
    }
    plan
}

/// Decide the (read-only) action for a single treasury row.
fn plan_target(
    target: &TreasuryEntityIdBackfillTarget,
    map: &dyn CoopEntityMap,
) -> TreasuryEntityIdBackfillEntry {
    let make = |action: TreasuryEntityIdBackfillAction,
                resolved: Option<EntityId>,
                reason: String,
                error: Option<String>| TreasuryEntityIdBackfillEntry {
        coop_id: target.coop_id.clone(),
        treasury_did: target.treasury_did.clone(),
        resolved_entity_id: resolved,
        action,
        reason,
        error,
    };

    // Already populated: nothing to plan (idempotent — a re-run after a future
    // apply re-classifies these rows here, never re-populating them).
    if target.current_entity_id.is_some() {
        return make(
            TreasuryEntityIdBackfillAction::SkippedAlreadyHasEntityId,
            None,
            "treasury already has an entity_id".into(),
            None,
        );
    }

    // Look up the binding (entity + provenance) for this coop_id. A read error
    // fails closed: safety cannot be established, so nothing is proposed.
    let binding = match map.binding_for_coop(&target.coop_id) {
        Ok(Some(binding)) => binding,
        Ok(None) => {
            return make(
                TreasuryEntityIdBackfillAction::SkippedNoMapping,
                None,
                "no coop_id -> EntityId binding in the canonical map".into(),
                None,
            )
        }
        Err(e) => {
            return make(
                TreasuryEntityIdBackfillAction::SkippedStorageError,
                None,
                "map read failed; cannot establish a safe binding".into(),
                Some(e.to_string()),
            )
        }
    };

    // Fail-closed on untrusted provenance (UnknownLegacy / unprovenanced /
    // gossip-originated). This is the canonical trust predicate shared with the
    // gateway resolver.
    if !binding.provenance.is_trusted_for_resolution() {
        return make(
            TreasuryEntityIdBackfillAction::SkippedUntrustedProvenance,
            Some(binding.entity_id.clone()),
            "binding provenance is not trusted for resolution (fail closed)".into(),
            None,
        );
    }

    // A flat coop_id denotes a cooperative target (RFC-0018): the resolved
    // EntityId must be a *well-formed* cooperative. Re-parse via FromStr so a
    // cooperative-typed but invalid-slug id (constructible via serde, never via
    // the validating constructors) is rejected rather than populated.
    let is_well_formed_cooperative = EntityId::from_str(binding.entity_id.as_str())
        .map(|parsed| parsed.is_cooperative())
        .unwrap_or(false);
    if !is_well_formed_cooperative {
        return make(
            TreasuryEntityIdBackfillAction::SkippedNonCooperativeEntity,
            Some(binding.entity_id.clone()),
            "bound EntityId is not a well-formed cooperative entity".into(),
            None,
        );
    }

    // Non-ambiguity cross-check: the resolved entity's reverse binding must point
    // back to THIS coop_id. A mismatch (the entity is reverse-bound to a different
    // coop_id) or a reverse read error fails closed.
    match map.coop_for_entity(&binding.entity_id) {
        Ok(Some(reverse)) if reverse != target.coop_id => {
            return make(
                TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding,
                Some(binding.entity_id.clone()),
                format!("resolved entity is reverse-bound to a different coop_id ({reverse:?})"),
                None,
            );
        }
        Ok(_) => {}
        Err(e) => {
            return make(
                TreasuryEntityIdBackfillAction::SkippedStorageError,
                Some(binding.entity_id.clone()),
                "reverse-index read failed; cannot verify the binding is unambiguous".into(),
                Some(e.to_string()),
            );
        }
    }

    // Eligible: a trusted, non-ambiguous, cooperative binding for a legacy
    // treasury that currently has no entity_id. READ-ONLY — report the intent;
    // populate nothing.
    let resolved = binding.entity_id.as_str().to_string();
    make(
        TreasuryEntityIdBackfillAction::WouldPopulate,
        Some(binding.entity_id),
        format!("would populate entity_id from trusted binding to {resolved}"),
        None,
    )
}

impl TreasuryManager {
    /// Read-only: plan which legacy treasuries (those with `entity_id: None`)
    /// could have their `entity_id` populated from a **trusted, non-ambiguous**
    /// cooperative binding in `map`.
    ///
    /// Performs **no** writes — neither to the treasury store nor to the map —
    /// and changes no authorization state. The mutating apply is a separate,
    /// explicitly-reviewed follow-up (populating `treasury.entity_id` affects the
    /// operator-opt-in `EnforceTrustedResolver` treasury gate's membership
    /// target; see the module docs). A mapping confers no authority.
    pub fn plan_entity_id_backfill(&self, map: &dyn CoopEntityMap) -> TreasuryEntityIdBackfillPlan {
        let targets = self.list_treasuries().into_iter().map(treasury_to_target);
        plan_treasury_entity_id_backfill(targets, map)
    }
}

/// Project a concrete [`Treasury`] to its backfill target row (read-only).
fn treasury_to_target(treasury: &Treasury) -> TreasuryEntityIdBackfillTarget {
    TreasuryEntityIdBackfillTarget {
        coop_id: treasury.coop_id().to_string(),
        treasury_did: Some(treasury.treasury_did.to_string()),
        current_entity_id: treasury.entity_id().cloned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_entity::{
        CoopEntityBinding, CoopEntityBindingProvenance, CoopEntityMapError, InMemoryCoopEntityMap,
    };

    fn coop_eid(slug: &str) -> EntityId {
        EntityId::cooperative(slug).unwrap()
    }

    fn target(coop_id: &str, current: Option<EntityId>) -> TreasuryEntityIdBackfillTarget {
        TreasuryEntityIdBackfillTarget {
            coop_id: coop_id.to_string(),
            treasury_did: None,
            current_entity_id: current,
        }
    }

    fn one(plan: &TreasuryEntityIdBackfillPlan) -> &TreasuryEntityIdBackfillEntry {
        assert_eq!(plan.entries.len(), 1, "expected exactly one entry");
        &plan.entries[0]
    }

    // ------------------------------------------------------------------
    // Eligible: trusted, non-ambiguous cooperative binding for a legacy row.
    // ------------------------------------------------------------------

    #[test]
    fn would_populate_on_trusted_activation_binding() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);

        assert_eq!(plan.total, 1);
        assert_eq!(plan.would_populate, 1);
        let entry = one(&plan);
        assert_eq!(entry.action, TreasuryEntityIdBackfillAction::WouldPopulate);
        assert_eq!(entry.resolved_entity_id, Some(coop_eid("food-coop")));
        assert!(entry.error.is_none());
    }

    #[test]
    fn would_populate_on_trusted_operator_backfill_surrogate() {
        // A non-mappable legacy id (coop:<uuid>) bound to a surrogate via the
        // operator backfill is a trusted, non-ambiguous binding.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        map.bind_resolved_with_provenance(
            "coop:550e8400-e29b-41d4-a716-446655440000",
            &surrogate,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .unwrap();

        let plan = plan_treasury_entity_id_backfill(
            vec![target("coop:550e8400-e29b-41d4-a716-446655440000", None)],
            &map,
        );
        assert_eq!(plan.would_populate, 1);
        assert_eq!(one(&plan).resolved_entity_id, Some(surrogate));
    }

    // ------------------------------------------------------------------
    // Skips.
    // ------------------------------------------------------------------

    #[test]
    fn skipped_when_treasury_already_has_entity_id() {
        // A trusted binding exists, but the treasury already carries entity_id —
        // there is nothing to populate, regardless of the map.
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let plan = plan_treasury_entity_id_backfill(
            vec![target("food-coop", Some(coop_eid("food-coop")))],
            &map,
        );
        assert_eq!(plan.skipped_already_has_entity_id, 1);
        assert_eq!(plan.would_populate, 0);
        assert_eq!(
            one(&plan).action,
            TreasuryEntityIdBackfillAction::SkippedAlreadyHasEntityId
        );
    }

    #[test]
    fn skipped_when_no_mapping() {
        let map = InMemoryCoopEntityMap::new();
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(plan.skipped_no_mapping, 1);
        assert_eq!(
            one(&plan).action,
            TreasuryEntityIdBackfillAction::SkippedNoMapping
        );
        assert!(one(&plan).resolved_entity_id.is_none());
    }

    #[test]
    fn skipped_when_provenance_untrusted_unknown_legacy() {
        // A plain bind_resolved records no provenance; it reads back as the
        // fail-closed UnknownLegacy sentinel and must NOT be populated.
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved("food-coop", &coop_eid("food-coop"))
            .unwrap();

        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(plan.skipped_untrusted_provenance, 1);
        assert_eq!(plan.would_populate, 0);
        let entry = one(&plan);
        assert_eq!(
            entry.action,
            TreasuryEntityIdBackfillAction::SkippedUntrustedProvenance
        );
        // The rejected candidate is still surfaced for operator visibility.
        assert_eq!(entry.resolved_entity_id, Some(coop_eid("food-coop")));
    }

    // ------------------------------------------------------------------
    // Fail-closed paths exercised via test doubles (the InMemory/Sled maps
    // refuse non-cooperative binds and keep both indexes consistent, so these
    // states only arise from an inconsistent/erroring backend).
    // ------------------------------------------------------------------

    /// A configurable read-only double. `bind_resolved` is `unreachable!` so a
    /// test fails loudly if the plan ever tries to WRITE — proving read-only.
    struct StubMap {
        binding: fn(&str) -> Result<Option<CoopEntityBinding>, CoopEntityMapError>,
        reverse: fn(&EntityId) -> Result<Option<String>, CoopEntityMapError>,
    }
    impl CoopEntityMap for StubMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("read-only backfill plan must never bind/mutate the map");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(&self, entity: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            (self.reverse)(entity)
        }
        fn binding_for_coop(
            &self,
            coop_id: &str,
        ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
            (self.binding)(coop_id)
        }
    }

    #[test]
    fn skipped_when_bound_entity_is_not_cooperative() {
        let map = StubMap {
            binding: |coop_id| {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    // Trusted provenance, but a community (non-cooperative) target.
                    entity_id: EntityId::community("some-community").unwrap(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            },
            reverse: |_| Ok(None),
        };
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(plan.skipped_non_cooperative_entity, 1);
        assert_eq!(plan.would_populate, 0);
        assert_eq!(
            one(&plan).action,
            TreasuryEntityIdBackfillAction::SkippedNonCooperativeEntity
        );
    }

    #[test]
    fn skipped_when_binding_is_ambiguous_reverse_mismatch() {
        let map = StubMap {
            binding: |coop_id| {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    entity_id: EntityId::cooperative("food-coop").unwrap(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            },
            // The resolved entity is reverse-bound to a DIFFERENT coop_id.
            reverse: |_| Ok(Some("other-coop".to_string())),
        };
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(plan.skipped_ambiguous_binding, 1);
        assert_eq!(plan.would_populate, 0);
        let entry = one(&plan);
        assert_eq!(
            entry.action,
            TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding
        );
        assert!(entry.reason.contains("other-coop"));
    }

    #[test]
    fn skipped_on_forward_storage_error() {
        let map = StubMap {
            binding: |_| {
                Err(CoopEntityMapError::Storage(
                    "forward index unreadable".into(),
                ))
            },
            reverse: |_| Ok(None),
        };
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(plan.skipped_storage_error, 1);
        let entry = one(&plan);
        assert_eq!(
            entry.action,
            TreasuryEntityIdBackfillAction::SkippedStorageError
        );
        assert!(entry.error.as_ref().unwrap().contains("forward index"));
    }

    #[test]
    fn skipped_on_reverse_storage_error() {
        let map = StubMap {
            binding: |coop_id| {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    entity_id: EntityId::cooperative("food-coop").unwrap(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            },
            reverse: |_| {
                Err(CoopEntityMapError::Storage(
                    "reverse index unreadable".into(),
                ))
            },
        };
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(plan.skipped_storage_error, 1);
        assert!(one(&plan).error.as_ref().unwrap().contains("reverse index"));
    }

    // ------------------------------------------------------------------
    // Aggregate: counters partition the input on a mixed batch.
    // ------------------------------------------------------------------

    #[test]
    fn counts_partition_total_on_mixed_batch() {
        let map = InMemoryCoopEntityMap::new();
        // food-coop: trusted Activation binding -> eligible.
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        // legacy-coop: plain bind -> UnknownLegacy -> untrusted.
        map.bind_resolved("legacy-coop", &coop_eid("legacy-coop"))
            .unwrap();

        let plan = plan_treasury_entity_id_backfill(
            vec![
                target("food-coop", None),                         // would_populate
                target("legacy-coop", None),                       // untrusted
                target("no-map-coop", None),                       // no_mapping
                target("already", Some(coop_eid("already-coop"))), // already_has
            ],
            &map,
        );

        assert_eq!(plan.total, 4);
        assert_eq!(
            plan.would_populate
                + plan.skipped_already_has_entity_id
                + plan.skipped_no_mapping
                + plan.skipped_untrusted_provenance
                + plan.skipped_non_cooperative_entity
                + plan.skipped_ambiguous_binding
                + plan.skipped_storage_error,
            plan.total
        );
        assert_eq!(plan.would_populate, 1);
        assert_eq!(plan.skipped_untrusted_provenance, 1);
        assert_eq!(plan.skipped_no_mapping, 1);
        assert_eq!(plan.skipped_already_has_entity_id, 1);
    }

    // ------------------------------------------------------------------
    // JSON shape: counts and entry fields serialize with expected keys.
    // ------------------------------------------------------------------

    #[test]
    fn json_includes_expected_counts_and_entry_fields() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        let v: serde_json::Value = serde_json::to_value(&plan).unwrap();
        assert_eq!(v["total"].as_u64(), Some(1));
        assert_eq!(v["would_populate"].as_u64(), Some(1));
        assert!(v.get("skipped_untrusted_provenance").is_some());
        let entry = &v["entries"][0];
        assert_eq!(entry["coop_id"].as_str(), Some("food-coop"));
        assert_eq!(entry["action"].as_str(), Some("would_populate"));
        assert!(entry["resolved_entity_id"].as_str().is_some());
        assert!(entry["reason"].as_str().is_some());
    }

    // ------------------------------------------------------------------
    // End-to-end via the real TreasuryManager + a concrete legacy treasury.
    // ------------------------------------------------------------------

    #[test]
    fn treasury_manager_plans_over_real_legacy_treasuries() {
        use icn_identity::KeyPair;

        let creator = KeyPair::generate().unwrap().did().clone();
        let treasury_did = KeyPair::generate().unwrap().did().clone();

        // Register a legacy treasury (entity_id: None) for `food-coop` via the
        // real registration API (Treasury::new under the hood).
        let mut mgr = TreasuryManager::new();
        mgr.register_treasury(
            treasury_did,
            "food-coop".to_string(),
            "USD".to_string(),
            creator,
            None,
        )
        .unwrap();

        // A trusted Activation binding for that coop.
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let plan = mgr.plan_entity_id_backfill(&map);
        assert_eq!(plan.total, 1);
        assert_eq!(plan.would_populate, 1);
        let entry = one(&plan);
        assert_eq!(entry.action, TreasuryEntityIdBackfillAction::WouldPopulate);
        assert_eq!(entry.resolved_entity_id, Some(coop_eid("food-coop")));
        assert!(entry.treasury_did.is_some(), "treasury_did is surfaced");
    }
}
