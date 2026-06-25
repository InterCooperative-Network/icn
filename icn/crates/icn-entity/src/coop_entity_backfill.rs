//! Controlled, auditable surrogate backfill/apply over `coop_id`s (#2082 lane,
//! PR6).
//!
//! # What this is
//!
//! The first **mutation orchestrator** in the lane. It composes three earlier,
//! single-purpose primitives behind one operator-controlled, dry-run-by-default
//! operation:
//!
//! - read-only classification ([`classify_coop_ids`](crate::classify_coop_ids),
//!   PR3),
//! - deterministic surrogate proposal
//!   ([`propose_surrogate_entity_id`](crate::propose_surrogate_entity_id), PR4),
//! - the reversible, provenance-recording write primitive
//!   ([`CoopEntityMap::bind_resolved_with_provenance`](crate::CoopEntityMap::bind_resolved_with_provenance),
//!   PR5 + A2c).
//!
//! Given a set of flat `coop_id`s and the canonical map, it selects the *safe,
//! non-mappable* entries (a default `coop:<uuid>` cannot project to a valid
//! cooperative slug), proposes each one's deterministic surrogate, and — only
//! under an explicit [`SurrogateBackfillMode::Apply`] — binds the pair with
//! `bind_resolved_with_provenance`, recording
//! [`CoopEntityBindingProvenance::OperatorBackfill`](crate::CoopEntityBindingProvenance::OperatorBackfill).
//! [`SurrogateBackfillMode::DryRun`] is the default and writes nothing.
//!
//! # A mapping is NOT authority
//!
//! Binding a `coop_id` to a surrogate cooperative `EntityId` grants **zero**
//! membership, role, capability, mandate, permission, or standing. It is only a
//! reversible *name binding*. Authority in ICN still flows from identity →
//! standing → membership → role/mandate/capability → governance decision →
//! receipt → effect → durable execution record — never from the existence of a
//! mapping entry. This operation moves names, not power.
//!
//! # No normalization, no silent writes, no stealing
//!
//! - **No normalization.** The original `coop_id` is preserved byte-for-byte;
//!   the surrogate is a *new generated handle*, never a rewrite of the input.
//! - **No silent writes.** Dry-run is the default; a write happens only in
//!   [`SurrogateBackfillMode::Apply`], and only for collision-free entries.
//! - **No stealing.** A proposed surrogate that would occupy an `EntityId`
//!   already claimed by another `coop_id` (already reverse-bound, or another
//!   same-batch proposal/projection) is reported as a
//!   [`SurrogateBackfillAction::SurrogateCollision`] and is **never** bound.
//!   This mirrors the read-only collision rule of
//!   [`classify_coop_ids_with_surrogate_preview`](crate::classify_coop_ids_with_surrogate_preview),
//!   so the orchestrator refuses the whole colliding set up front instead of
//!   binding one entry and letting `bind_resolved` reject its twin (which would
//!   leave an ambiguous partial result).

use crate::coop_entity_inventory::{classify_coop_ids, CoopEntityClass, CoopEntityInventoryEntry};
use crate::coop_entity_map::{CoopEntityBindingProvenance, CoopEntityMap, CoopEntityMapError};
use crate::coop_entity_surrogate::propose_surrogate_entity_id;
use crate::entity::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Whether the backfill only previews (`DryRun`) or actually binds (`Apply`).
///
/// `DryRun` is the default for the operation: it computes the identical plan as
/// `Apply` but performs **no** writes, so it is safe to run repeatedly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurrogateBackfillMode {
    /// Compute and report the plan; bind nothing.
    DryRun,
    /// Bind each eligible, collision-free non-mappable `coop_id` to its
    /// deterministic surrogate via
    /// [`CoopEntityMap::bind_resolved_with_provenance`], recording
    /// [`CoopEntityBindingProvenance::OperatorBackfill`].
    Apply,
}

/// The outcome (or, in dry-run, the *intended* outcome) for a single `coop_id`.
///
/// Exactly one action is assigned per entry, and every action maps to exactly
/// one aggregate counter, so the per-entry actions partition
/// [`CoopSurrogateBackfill::total`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurrogateBackfillAction {
    /// Dry-run: an eligible, collision-free non-mappable id that *would* be
    /// bound to its surrogate under `--apply`. No write performed.
    WouldBind,
    /// Apply: an eligible non-mappable id successfully bound to its surrogate.
    Bound,
    /// Skipped: the `coop_id` already has a healthy binding (nothing to do).
    SkippedAlreadyBound,
    /// Skipped: the `coop_id` is mappable and resolves by projection; surrogate
    /// backfill is only for non-mappable ids, so it is left untouched.
    SkippedMappableUnbound,
    /// Skipped: the `coop_id` is mappable but its projected `EntityId` is
    /// already reverse-bound to a different `coop_id`.
    SkippedReverseConflict,
    /// Skipped: classification (or the surrogate reverse-lookup) hit a storage
    /// read error or an internally inconsistent mapping; safety cannot be
    /// established, so nothing is proposed or bound.
    SkippedStorageError,
    /// Not applied: the proposed surrogate `EntityId` would collide on bind —
    /// already reverse-bound to a different `coop_id`, or claimed by another
    /// `coop_id` in this same batch. Reported, never bound.
    SurrogateCollision,
    /// Apply: `bind_resolved` rejected the surrogate as a forward/reverse
    /// mapping conflict (a real concurrent-state change between plan and write).
    BindConflict,
    /// Apply: `bind_resolved` failed with a storage/other error.
    BindError,
}

/// Per-`coop_id` audit detail. The original `coop_id` is preserved
/// byte-for-byte; nothing here is normalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurrogateBackfillEntry {
    /// The original flat `coop_id`, exactly as supplied.
    pub coop_id: String,
    /// The deterministic surrogate cooperative `EntityId` proposed for this
    /// `coop_id`. Present for every non-mappable entry that produced a proposal
    /// (eligible, collision, or surrogate-lookup error); `None` for the
    /// skipped-by-class entries that never get a proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_entity_id: Option<EntityId>,
    /// The action taken (or, in dry-run, that would be taken) for this entry.
    pub action: SurrogateBackfillAction,
    /// A short, human-readable explanation of the action.
    pub reason: String,
    /// The underlying map/bind error string when one exists (storage read
    /// failure, bind conflict, or bind error). `None` for clean outcomes and
    /// for collisions (a collision is a policy finding, surfaced in `reason`,
    /// not a map error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate, auditable result of a surrogate backfill (dry-run or apply).
///
/// The skip/collision/eligible counters partition [`total`](Self::total):
/// `total == eligible + surrogate_collision + skipped_already_bound +
/// skipped_mappable_unbound + skipped_reverse_conflict + skipped_storage_error`.
/// `eligible` is the count of collision-free non-mappable proposals selected for
/// binding; in `Apply` mode `eligible == applied + bind_conflict + bind_error`,
/// and in `DryRun` mode `eligible` equals the number of `would_bind` entries
/// (with `applied`, `bind_conflict`, and `bind_error` all zero).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoopSurrogateBackfill {
    /// The mode this result was produced under.
    pub mode: SurrogateBackfillMode,
    /// Total number of `coop_id`s processed (`== entries.len()`).
    pub total: usize,
    /// Collision-free non-mappable proposals selected for binding.
    pub eligible: usize,
    /// Eligible entries actually bound (`Apply` only; always `0` in `DryRun`).
    pub applied: usize,
    /// Skipped because already healthily bound.
    pub skipped_already_bound: usize,
    /// Skipped because mappable-and-unbound (projection path, not surrogate).
    pub skipped_mappable_unbound: usize,
    /// Skipped because mappable but the projected `EntityId` is reverse-bound
    /// to a different `coop_id`.
    pub skipped_reverse_conflict: usize,
    /// Skipped because classification or the surrogate reverse-lookup hit a
    /// storage error / inconsistent state.
    pub skipped_storage_error: usize,
    /// Non-mappable entries whose proposed surrogate would collide on bind.
    pub surrogate_collision: usize,
    /// Eligible entries whose `bind_resolved` was rejected as a conflict
    /// (`Apply` only).
    pub bind_conflict: usize,
    /// Eligible entries whose `bind_resolved` failed with a storage/other error
    /// (`Apply` only).
    pub bind_error: usize,
    /// Per-`coop_id` detail, in input order.
    pub entries: Vec<SurrogateBackfillEntry>,
}

impl CoopSurrogateBackfill {
    /// An empty result for the given mode (used for an empty / missing store).
    fn new(mode: SurrogateBackfillMode) -> Self {
        Self {
            mode,
            total: 0,
            eligible: 0,
            applied: 0,
            skipped_already_bound: 0,
            skipped_mappable_unbound: 0,
            skipped_reverse_conflict: 0,
            skipped_storage_error: 0,
            surrogate_collision: 0,
            bind_conflict: 0,
            bind_error: 0,
            entries: Vec::new(),
        }
    }

    /// Record one classified entry, bumping the matching counter and `total`.
    fn record(&mut self, entry: SurrogateBackfillEntry) {
        match entry.action {
            SurrogateBackfillAction::WouldBind => self.eligible += 1,
            SurrogateBackfillAction::Bound => {
                self.eligible += 1;
                self.applied += 1;
            }
            SurrogateBackfillAction::BindConflict => {
                self.eligible += 1;
                self.bind_conflict += 1;
            }
            SurrogateBackfillAction::BindError => {
                self.eligible += 1;
                self.bind_error += 1;
            }
            SurrogateBackfillAction::SurrogateCollision => self.surrogate_collision += 1,
            SurrogateBackfillAction::SkippedAlreadyBound => self.skipped_already_bound += 1,
            SurrogateBackfillAction::SkippedMappableUnbound => self.skipped_mappable_unbound += 1,
            SurrogateBackfillAction::SkippedReverseConflict => self.skipped_reverse_conflict += 1,
            SurrogateBackfillAction::SkippedStorageError => self.skipped_storage_error += 1,
        }
        self.total += 1;
        self.entries.push(entry);
    }

    /// Whether an `Apply` run finished with anything that blocked a clean,
    /// complete backfill: a bind conflict/error, a refused collision, or a
    /// storage error. Callers use this to choose a non-zero process exit.
    /// Always `false` for a `DryRun` (a preview reports findings, it does not
    /// fail).
    pub fn apply_had_failures(&self) -> bool {
        self.mode == SurrogateBackfillMode::Apply
            && (self.bind_conflict > 0
                || self.bind_error > 0
                || self.surrogate_collision > 0
                || self.skipped_storage_error > 0)
    }
}

/// Plan (and, in `Apply` mode, perform) a controlled surrogate backfill over
/// `coop_ids` against the canonical [`CoopEntityMap`].
///
/// This is dry-run-safe by construction: in [`SurrogateBackfillMode::DryRun`] it
/// performs only read operations and binds nothing; in
/// [`SurrogateBackfillMode::Apply`] it binds each eligible, collision-free
/// non-mappable `coop_id` to its deterministic surrogate via
/// [`CoopEntityMap::bind_resolved_with_provenance`] — recording
/// [`CoopEntityBindingProvenance::OperatorBackfill`] — preserving the original
/// `coop_id` byte-for-byte. A binding confers no authority.
pub fn backfill_coop_surrogates(
    coop_ids: impl IntoIterator<Item = String>,
    map: &dyn CoopEntityMap,
    mode: SurrogateBackfillMode,
) -> CoopSurrogateBackfill {
    // Step 1: read-only classification — the same machinery `coop entity-report`
    // uses, so a backfill plan lines up exactly with the inventory the operator
    // just inspected. No writes, no normalization.
    let inventory = classify_coop_ids(coop_ids, map);

    // Step 2 (pure): propose a surrogate for each NonMappable entry and tally,
    // per EntityId, how many entries in this batch would occupy it on a bind. The
    // occupancy count includes BOTH proposed surrogates AND the projected/bound
    // id of mappable/already-bound entries, so a surrogate that coincides with a
    // same-batch mappable projection is caught here — mirroring
    // `classify_coop_ids_with_surrogate_preview`'s read-only collision rule (PR4).
    let mut surrogate_for: Vec<Option<EntityId>> = Vec::with_capacity(inventory.entries.len());
    let mut entity_claims: HashMap<String, usize> = HashMap::new();
    for entry in &inventory.entries {
        let surrogate = if entry.class == CoopEntityClass::NonMappable {
            // Valid by construction; an (unreachable) error means "no proposal".
            propose_surrogate_entity_id(&entry.coop_id).ok()
        } else {
            None
        };
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

    // Step 3: decide one action per entry and, in Apply mode, perform the bind for
    // each collision-free non-mappable entry. Dry-run writes nothing.
    let mut report = CoopSurrogateBackfill::new(mode);
    for (entry, surrogate) in inventory.entries.iter().zip(surrogate_for) {
        report.record(plan_entry(entry, surrogate, &entity_claims, map, mode));
    }
    report
}

/// Decide the backfill action for one classified entry — and, in `Apply` mode,
/// perform the `bind_resolved_with_provenance` write for a collision-free
/// non-mappable entry.
///
/// Pure for every non-`NonMappable` class and for collisions/storage errors; the
/// only write is `bind_resolved_with_provenance` (recording
/// [`CoopEntityBindingProvenance::OperatorBackfill`]) on an eligible entry in
/// `Apply` mode.
fn plan_entry(
    entry: &CoopEntityInventoryEntry,
    surrogate: Option<EntityId>,
    entity_claims: &HashMap<String, usize>,
    map: &dyn CoopEntityMap,
    mode: SurrogateBackfillMode,
) -> SurrogateBackfillEntry {
    let make = |action: SurrogateBackfillAction,
                proposed: Option<EntityId>,
                reason: String,
                error: Option<String>| SurrogateBackfillEntry {
        coop_id: entry.coop_id.clone(),
        proposed_entity_id: proposed,
        action,
        reason,
        error,
    };

    match entry.class {
        CoopEntityClass::AlreadyBound => {
            let to = entry
                .bound_entity_id
                .as_ref()
                .map(|e| e.as_str())
                .unwrap_or("<unknown>");
            make(
                SurrogateBackfillAction::SkippedAlreadyBound,
                None,
                format!("already bound to {to}"),
                None,
            )
        }
        CoopEntityClass::MappableUnbound => make(
            SurrogateBackfillAction::SkippedMappableUnbound,
            entry.projected_entity_id.clone(),
            "mappable coop_id resolves by projection; surrogate backfill is only for non-mappable ids"
                .into(),
            None,
        ),
        CoopEntityClass::MappableReverseConflict => make(
            SurrogateBackfillAction::SkippedReverseConflict,
            entry.projected_entity_id.clone(),
            format!(
                "mappable but the projected EntityId is reverse-bound to another coop_id ({:?})",
                entry.reverse_bound_coop_id
            ),
            None,
        ),
        CoopEntityClass::StorageError => make(
            SurrogateBackfillAction::SkippedStorageError,
            None,
            "classification hit a storage error or inconsistent mapping".into(),
            entry.error.clone(),
        ),
        CoopEntityClass::NonMappable => {
            let Some(surrogate) = surrogate else {
                // propose_surrogate_entity_id failed — an internal-invariant guard
                // that never fires in practice; with no surrogate there is nothing
                // to bind, so it is surfaced rather than panicking.
                return make(
                    SurrogateBackfillAction::SkippedStorageError,
                    None,
                    "could not derive a surrogate EntityId".into(),
                    entry
                        .error
                        .clone()
                        .or_else(|| Some("surrogate derivation failed".into())),
                );
            };

            // Within-batch collision: another entry in this batch would occupy the
            // same EntityId (another surrogate proposal, or a mappable/bound
            // projection). Refuse the whole colliding set here so Apply never binds
            // one twin and lets `bind_resolved` reject the other — which would leave
            // an ambiguous partial result.
            if entity_claims.get(surrogate.as_str()).copied().unwrap_or(0) > 1 {
                return make(
                    SurrogateBackfillAction::SurrogateCollision,
                    Some(surrogate.clone()),
                    format!(
                        "proposed surrogate {} collides within-batch with another coop_id",
                        surrogate.as_str()
                    ),
                    None,
                );
            }

            // Reverse-binding collision (or read failure) against persisted state.
            match map.coop_for_entity(&surrogate) {
                Ok(Some(occupant)) if occupant != entry.coop_id => {
                    return make(
                        SurrogateBackfillAction::SurrogateCollision,
                        Some(surrogate.clone()),
                        format!(
                            "proposed surrogate {} already reverse-bound to {occupant:?}",
                            surrogate.as_str()
                        ),
                        None,
                    );
                }
                // Free, or already reverse-bound to THIS coop_id (a one-sided state
                // that `bind_resolved` completes): eligible.
                Ok(_) => {}
                Err(e) => {
                    return make(
                        SurrogateBackfillAction::SkippedStorageError,
                        Some(surrogate.clone()),
                        format!(
                            "surrogate {} reverse-lookup failed; cannot verify it is free",
                            surrogate.as_str()
                        ),
                        Some(e.to_string()),
                    );
                }
            }

            // Eligible, collision-free. Dry-run reports the intent; Apply binds the
            // original coop_id to its surrogate (preserved byte-for-byte).
            match mode {
                SurrogateBackfillMode::DryRun => make(
                    SurrogateBackfillAction::WouldBind,
                    Some(surrogate.clone()),
                    format!("would bind to surrogate {}", surrogate.as_str()),
                    None,
                ),
                SurrogateBackfillMode::Apply => {
                    // Record OperatorBackfill provenance: this is an operator-run
                    // backfill (the `OperatorBackfill` variant's doc names this exact
                    // command). The surrogate nature of the target is already evident
                    // from its `coop-legacy-<hash>` slug, so provenance records HOW the
                    // binding was established. A recorded provenance is what lets the
                    // store-backed A2c resolver treat the binding as trusted; absent it,
                    // the row reads back as the fail-closed UnknownLegacy sentinel.
                    // A binding still confers no authority (see the module docs).
                    match map.bind_resolved_with_provenance(
                        &entry.coop_id,
                        &surrogate,
                        CoopEntityBindingProvenance::OperatorBackfill,
                    ) {
                        Ok(()) => make(
                            SurrogateBackfillAction::Bound,
                            Some(surrogate.clone()),
                            format!("bound to surrogate {}", surrogate.as_str()),
                            None,
                        ),
                        Err(CoopEntityMapError::Conflict(msg)) => make(
                            SurrogateBackfillAction::BindConflict,
                            Some(surrogate.clone()),
                            "bind_resolved rejected the surrogate as a forward/reverse conflict"
                                .into(),
                            Some(msg),
                        ),
                        Err(e) => make(
                            SurrogateBackfillAction::BindError,
                            Some(surrogate.clone()),
                            "bind_resolved failed".into(),
                            Some(e.to_string()),
                        ),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::coop_entity_map::InMemoryCoopEntityMap;

    /// A non-mappable default cooperative id (`coop:<uuid>`).
    const UUID_COOP: &str = "coop:550e8400-e29b-41d4-a716-446655440000";
    /// PR4's golden surrogate for [`UUID_COOP`].
    const UUID_SURROGATE: &str = "entity:icn:cooperative:coop-legacy-edf6152975301bd80c13";

    fn coop_eid(slug: &str) -> EntityId {
        EntityId::cooperative(slug).unwrap()
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // ------------------------------------------------------------------
    // Dry-run (default): reports findings, writes nothing.
    // ------------------------------------------------------------------

    #[test]
    fn dry_run_reports_non_mappable_with_proposed_surrogate_and_writes_nothing() {
        let map = InMemoryCoopEntityMap::new();
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::DryRun);

        assert_eq!(report.mode, SurrogateBackfillMode::DryRun);
        assert_eq!(report.total, 1);
        assert_eq!(report.eligible, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.entries.len(), 1);

        let entry = &report.entries[0];
        assert_eq!(entry.coop_id, UUID_COOP);
        assert_eq!(entry.action, SurrogateBackfillAction::WouldBind);
        assert_eq!(
            entry.proposed_entity_id.as_ref().unwrap().as_str(),
            UUID_SURROGATE
        );

        // Read-only: nothing bound.
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(
            map.coop_for_entity(&coop_eid("coop-legacy-edf6152975301bd80c13"))
                .unwrap(),
            None
        );
    }

    #[test]
    fn dry_run_is_default_safe_to_run_repeatedly() {
        let map = InMemoryCoopEntityMap::new();
        let first =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::DryRun);
        let second =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::DryRun);
        assert_eq!(first, second);
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
    }

    // ------------------------------------------------------------------
    // Apply: binds the deterministic surrogate, idempotently.
    // ------------------------------------------------------------------

    #[test]
    fn apply_binds_non_mappable_to_deterministic_surrogate() {
        let map = InMemoryCoopEntityMap::new();
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);

        assert_eq!(report.mode, SurrogateBackfillMode::Apply);
        assert_eq!(report.eligible, 1);
        assert_eq!(report.applied, 1);
        assert_eq!(report.entries[0].action, SurrogateBackfillAction::Bound);

        // The forward binding now resolves to the golden surrogate.
        assert_eq!(
            map.entity_for_coop(UUID_COOP).unwrap().unwrap().as_str(),
            UUID_SURROGATE
        );
    }

    #[test]
    fn apply_preserves_original_coop_id_exactly_in_reverse_lookup() {
        let map = InMemoryCoopEntityMap::new();
        backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        let surrogate = propose_surrogate_entity_id(UUID_COOP).unwrap();
        // Reverse lookup returns the original coop_id byte-for-byte (colon, uuid).
        assert_eq!(
            map.coop_for_entity(&surrogate).unwrap(),
            Some(UUID_COOP.to_string())
        );
    }

    #[test]
    fn apply_is_idempotent_when_rerun() {
        let map = InMemoryCoopEntityMap::new();
        let first = backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(first.applied, 1);

        // Rerun: the id is now AlreadyBound, so it is skipped, not re-bound.
        let second =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(second.applied, 0);
        assert_eq!(second.skipped_already_bound, 1);
        assert_eq!(second.eligible, 0);
        assert_eq!(
            second.entries[0].action,
            SurrogateBackfillAction::SkippedAlreadyBound
        );
        // The binding is unchanged.
        assert_eq!(
            map.entity_for_coop(UUID_COOP).unwrap().unwrap().as_str(),
            UUID_SURROGATE
        );
    }

    // ------------------------------------------------------------------
    // Skips by class.
    // ------------------------------------------------------------------

    #[test]
    fn dry_run_skips_already_bound() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved(UUID_COOP, &propose_surrogate_entity_id(UUID_COOP).unwrap())
            .unwrap();
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::DryRun);
        assert_eq!(report.skipped_already_bound, 1);
        assert_eq!(report.eligible, 0);
        assert_eq!(
            report.entries[0].action,
            SurrogateBackfillAction::SkippedAlreadyBound
        );
    }

    #[test]
    fn apply_skips_mappable_unbound_without_binding_a_surrogate() {
        // A mappable coop_id resolves by projection; the backfill must not give
        // it a surrogate.
        let map = InMemoryCoopEntityMap::new();
        let report =
            backfill_coop_surrogates(ids(&["food-coop"]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.skipped_mappable_unbound, 1);
        assert_eq!(report.applied, 0);
        let entry = &report.entries[0];
        assert_eq!(
            entry.action,
            SurrogateBackfillAction::SkippedMappableUnbound
        );
        assert_eq!(entry.proposed_entity_id, Some(coop_eid("food-coop")));
        // Nothing bound by the surrogate path.
        assert_eq!(map.entity_for_coop("food-coop").unwrap(), None);
    }

    #[test]
    fn apply_skips_mappable_reverse_conflict() {
        // `food-coop` projects to an EntityId already reverse-bound to a
        // different coop.
        let map = InMemoryCoopEntityMap::new();
        map.bind_exact("alpha-coop", &coop_eid("food-coop"))
            .unwrap();
        let report =
            backfill_coop_surrogates(ids(&["food-coop"]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.skipped_reverse_conflict, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(
            report.entries[0].action,
            SurrogateBackfillAction::SkippedReverseConflict
        );
    }

    // ------------------------------------------------------------------
    // Surrogate collisions: reported, never applied.
    // ------------------------------------------------------------------

    #[test]
    fn apply_does_not_bind_a_surrogate_already_reverse_bound_to_another_coop() {
        let map = InMemoryCoopEntityMap::new();
        let surrogate = propose_surrogate_entity_id(UUID_COOP).unwrap();
        // A DIFFERENT coop already occupies the surrogate target.
        map.bind_exact("alpha-coop", &surrogate).unwrap();

        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.surrogate_collision, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.eligible, 0);
        let entry = &report.entries[0];
        assert_eq!(entry.action, SurrogateBackfillAction::SurrogateCollision);
        assert!(
            entry.reason.to_lowercase().contains("reverse-bound"),
            "reason should explain the reverse-bound collision; got: {}",
            entry.reason
        );
        // The non-mappable id was NOT bound (its surrogate stays with alpha-coop).
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(
            map.coop_for_entity(&surrogate).unwrap(),
            Some("alpha-coop".to_string())
        );
    }

    #[test]
    fn apply_refuses_whole_within_batch_colliding_set_no_partial_bind() {
        // Two identical non-mappable ids propose the same surrogate. The backfill
        // must refuse BOTH (no partial bind of the first), so the colliding set
        // is reported and the map stays untouched.
        let map = InMemoryCoopEntityMap::new();
        let report = backfill_coop_surrogates(
            ids(&[UUID_COOP, UUID_COOP]),
            &map,
            SurrogateBackfillMode::Apply,
        );
        assert_eq!(
            report.surrogate_collision, 2,
            "both colliding entries flagged"
        );
        assert_eq!(
            report.applied, 0,
            "no partial application of a colliding set"
        );
        for entry in &report.entries {
            assert_eq!(entry.action, SurrogateBackfillAction::SurrogateCollision);
            assert!(entry.reason.contains("within-batch"));
        }
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
    }

    #[test]
    fn apply_flags_surrogate_colliding_with_same_batch_mappable_projection() {
        // A mappable coop literally named the surrogate slug projects to the
        // exact EntityId being proposed for the non-mappable coop. Binding the
        // surrogate would steal the mappable's projection, so it must collide.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = propose_surrogate_entity_id(UUID_COOP).unwrap();
        let collider = surrogate.identifier().to_string(); // a valid, unbound slug

        let report = backfill_coop_surrogates(
            vec![UUID_COOP.to_string(), collider.clone()],
            &map,
            SurrogateBackfillMode::Apply,
        );
        assert_eq!(report.surrogate_collision, 1);
        assert_eq!(report.skipped_mappable_unbound, 1);
        assert_eq!(report.applied, 0);
        // Neither the surrogate nor the mappable projection was bound.
        assert_eq!(map.entity_for_coop(UUID_COOP).unwrap(), None);
        assert_eq!(map.entity_for_coop(&collider).unwrap(), None);
    }

    // ------------------------------------------------------------------
    // Apply-time bind conflict / bind error via test doubles.
    // (Eligible-at-plan-time but failing at write-time: only a test double
    // returning inconsistent reads vs. writes can manufacture this.)
    // ------------------------------------------------------------------

    /// A map that classifies `coop_A`-style ids as non-mappable-and-unbound
    /// (forward None) with a free surrogate target (reverse None), then fails
    /// the actual `bind_resolved` write with a caller-chosen error.
    struct BindFailMap {
        err: fn() -> CoopEntityMapError,
    }
    impl CoopEntityMap for BindFailMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            Err((self.err)())
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            Ok(None)
        }
    }

    #[test]
    fn apply_reports_bind_conflict_clearly() {
        let map = BindFailMap {
            err: || {
                CoopEntityMapError::Conflict("entity already bound to a different coop_id".into())
            },
        };
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.eligible, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.bind_conflict, 1);
        assert_eq!(report.bind_error, 0);
        let entry = &report.entries[0];
        assert_eq!(entry.action, SurrogateBackfillAction::BindConflict);
        assert!(entry.error.as_ref().unwrap().contains("different coop_id"));
    }

    #[test]
    fn apply_reports_bind_error_clearly() {
        let map = BindFailMap {
            err: || CoopEntityMapError::Storage("sled write failed".into()),
        };
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.eligible, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.bind_error, 1);
        assert_eq!(report.bind_conflict, 0);
        let entry = &report.entries[0];
        assert_eq!(entry.action, SurrogateBackfillAction::BindError);
        assert!(entry.error.as_ref().unwrap().contains("sled write failed"));
    }

    // ------------------------------------------------------------------
    // Storage error paths: classification read failure and surrogate
    // reverse-lookup failure both report as SkippedStorageError, no write.
    // ------------------------------------------------------------------

    /// Forward read fails -> classification reports StorageError.
    struct ForwardErrMap;
    impl CoopEntityMap for ForwardErrMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("must not bind when classification storage-errored");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "forward index unreadable".into(),
            ))
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            Ok(None)
        }
    }

    #[test]
    fn apply_reports_classification_storage_error_and_binds_nothing() {
        let report = backfill_coop_surrogates(
            ids(&[UUID_COOP]),
            &ForwardErrMap,
            SurrogateBackfillMode::Apply,
        );
        assert_eq!(report.skipped_storage_error, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.eligible, 0);
        assert_eq!(
            report.entries[0].action,
            SurrogateBackfillAction::SkippedStorageError
        );
    }

    /// Forward read says "unbound" (None), but the surrogate reverse-lookup
    /// fails -> the entry cannot be verified safe, so SkippedStorageError.
    struct ReverseErrMap;
    impl CoopEntityMap for ReverseErrMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("must not bind when the surrogate reverse-lookup errored");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "reverse index unreadable".into(),
            ))
        }
    }

    #[test]
    fn apply_reports_surrogate_lookup_storage_error_and_binds_nothing() {
        let report = backfill_coop_surrogates(
            ids(&[UUID_COOP]),
            &ReverseErrMap,
            SurrogateBackfillMode::Apply,
        );
        assert_eq!(report.skipped_storage_error, 1);
        assert_eq!(
            report.surrogate_collision, 0,
            "a lookup error is not a collision"
        );
        assert_eq!(report.applied, 0);
        let entry = &report.entries[0];
        assert_eq!(entry.action, SurrogateBackfillAction::SkippedStorageError);
        assert!(entry
            .error
            .as_ref()
            .unwrap()
            .contains("reverse index unreadable"));
    }

    // ------------------------------------------------------------------
    // Counts partition the input; mixed batch.
    // ------------------------------------------------------------------

    #[test]
    fn counts_partition_total_on_mixed_batch() {
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved("bound-uuid:1", &coop_eid("bound-surrogate"))
            .unwrap();
        let report = backfill_coop_surrogates(
            ids(&[
                "bound-uuid:1", // AlreadyBound
                "free-coop",    // MappableUnbound
                UUID_COOP,      // NonMappable -> eligible
                "coop_A",       // NonMappable -> eligible
            ]),
            &map,
            SurrogateBackfillMode::DryRun,
        );
        assert_eq!(report.total, 4);
        assert_eq!(
            report.eligible
                + report.surrogate_collision
                + report.skipped_already_bound
                + report.skipped_mappable_unbound
                + report.skipped_reverse_conflict
                + report.skipped_storage_error,
            report.total
        );
        assert_eq!(report.skipped_already_bound, 1);
        assert_eq!(report.skipped_mappable_unbound, 1);
        assert_eq!(report.eligible, 2);
    }

    #[test]
    fn apply_eligible_partitions_into_applied_conflict_error() {
        let map = InMemoryCoopEntityMap::new();
        let report = backfill_coop_surrogates(
            ids(&[UUID_COOP, "coop_A"]),
            &map,
            SurrogateBackfillMode::Apply,
        );
        assert_eq!(
            report.applied + report.bind_conflict + report.bind_error,
            report.eligible
        );
        assert_eq!(report.applied, 2);
    }

    // ------------------------------------------------------------------
    // JSON shape: counts and entry fields serialize with expected keys.
    // ------------------------------------------------------------------

    #[test]
    fn json_includes_expected_counts_and_entry_fields() {
        let map = InMemoryCoopEntityMap::new();
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::DryRun);
        let v: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(v["mode"].as_str(), Some("dry_run"));
        assert_eq!(v["total"].as_u64(), Some(1));
        assert_eq!(v["eligible"].as_u64(), Some(1));
        assert_eq!(v["applied"].as_u64(), Some(0));
        assert!(v.get("skipped_already_bound").is_some());
        assert!(v.get("bind_conflict").is_some());
        let entry = &v["entries"][0];
        assert_eq!(entry["coop_id"].as_str(), Some(UUID_COOP));
        assert_eq!(entry["action"].as_str(), Some("would_bind"));
        assert_eq!(entry["proposed_entity_id"].as_str(), Some(UUID_SURROGATE));
        assert!(entry["reason"].as_str().is_some());
    }

    // ------------------------------------------------------------------
    // Provenance recording (A2c): apply records OperatorBackfill provenance on
    // the surrogate name binding; dry-run and refused entries record nothing.
    // ------------------------------------------------------------------

    use crate::coop_entity_map::CoopEntityBindingProvenance;

    #[test]
    fn apply_records_operator_backfill_provenance_on_surrogate_binding() {
        let map = InMemoryCoopEntityMap::new();
        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.applied, 1);

        // The binding carries the operator-backfill provenance, not the
        // fail-closed UnknownLegacy sentinel a plain bind_resolved would leave.
        let binding = map.binding_for_coop(UUID_COOP).unwrap().unwrap();
        assert_eq!(binding.entity_id.as_str(), UUID_SURROGATE);
        assert_eq!(
            binding.provenance,
            CoopEntityBindingProvenance::OperatorBackfill
        );
        // The reverse binding records the same provenance.
        let by_entity = map
            .binding_for_entity(&coop_eid("coop-legacy-edf6152975301bd80c13"))
            .unwrap()
            .unwrap();
        assert_eq!(
            by_entity.provenance,
            CoopEntityBindingProvenance::OperatorBackfill
        );
    }

    #[test]
    fn apply_rerun_keeps_operator_backfill_provenance() {
        let map = InMemoryCoopEntityMap::new();
        backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        // Rerun: the id is AlreadyBound, so it is skipped (not re-bound) and the
        // recorded provenance is unchanged.
        let second =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(second.applied, 0);
        assert_eq!(second.skipped_already_bound, 1);
        assert_eq!(
            map.binding_for_coop(UUID_COOP).unwrap().unwrap().provenance,
            CoopEntityBindingProvenance::OperatorBackfill
        );
    }

    #[test]
    fn dry_run_records_no_binding_or_provenance() {
        let map = InMemoryCoopEntityMap::new();
        backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::DryRun);
        // Dry-run writes nothing: no binding at all (and therefore no provenance).
        assert_eq!(map.binding_for_coop(UUID_COOP).unwrap(), None);
    }

    #[test]
    fn collision_records_no_provenance_and_leaves_occupant_untouched() {
        // A different coop already occupies the surrogate target, so the
        // non-mappable id collides and is never bound — no provenance is written
        // for it, and the occupant's (provenance-less) binding is untouched.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = propose_surrogate_entity_id(UUID_COOP).unwrap();
        map.bind_exact("alpha-coop", &surrogate).unwrap();

        let report =
            backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.surrogate_collision, 1);
        assert_eq!(report.applied, 0);

        // The collided non-mappable id has no binding/provenance of its own.
        assert_eq!(map.binding_for_coop(UUID_COOP).unwrap(), None);
        // The occupant binding is unchanged (bind_exact records no provenance).
        let occupant = map.binding_for_entity(&surrogate).unwrap().unwrap();
        assert_eq!(occupant.coop_id, "alpha-coop");
        assert_eq!(
            occupant.provenance,
            CoopEntityBindingProvenance::UnknownLegacy
        );
    }

    #[test]
    fn apply_leaves_mappable_unbound_id_without_binding_or_provenance() {
        // A mappable coop resolves by projection; the surrogate backfill must not
        // bind it, so it has no binding (and thus no provenance) afterward.
        let map = InMemoryCoopEntityMap::new();
        let report =
            backfill_coop_surrogates(ids(&["food-coop"]), &map, SurrogateBackfillMode::Apply);
        assert_eq!(report.skipped_mappable_unbound, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(map.binding_for_coop("food-coop").unwrap(), None);
    }

    #[test]
    fn apply_had_failures_only_for_apply_with_problems() {
        let map = InMemoryCoopEntityMap::new();
        // Clean apply: no failures.
        let clean = backfill_coop_surrogates(ids(&[UUID_COOP]), &map, SurrogateBackfillMode::Apply);
        assert!(!clean.apply_had_failures());

        // Dry-run with a collision: never a "failure" (it is a preview).
        let map2 = InMemoryCoopEntityMap::new();
        let dry = backfill_coop_surrogates(
            ids(&[UUID_COOP, UUID_COOP]),
            &map2,
            SurrogateBackfillMode::DryRun,
        );
        assert!(!dry.apply_had_failures());

        // Apply with a collision: a failure.
        let map3 = InMemoryCoopEntityMap::new();
        let apply = backfill_coop_surrogates(
            ids(&[UUID_COOP, UUID_COOP]),
            &map3,
            SurrogateBackfillMode::Apply,
        );
        assert!(apply.apply_had_failures());
    }
}
