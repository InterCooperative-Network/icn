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

use crate::treasury::{Treasury, TreasuryEntityIdPopulateResult, TreasuryManager};
use icn_entity::{CoopEntityBindingProvenance, CoopEntityMap, EntityId};
use icn_identity::Did;
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
    /// Skipped: the binding is ambiguous or conflicting and cannot safely confirm
    /// this `coop_id`'s target. Covers: a reverse index pointing to a *different*
    /// `coop_id`; a *missing* reverse (a one-sided / inconsistent store); or a
    /// *projectable* `coop_id` whose strict legacy projection differs from the
    /// trusted binding's target (the resolver would treat that as a conflict).
    /// Fail closed.
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
    // back to THIS coop_id. The CoopEntityMap writes the forward and reverse
    // indexes atomically, so for a consistent store a trusted forward binding
    // always has a matching reverse. Anything else fails closed: a reverse bound
    // to a DIFFERENT coop_id (ambiguous), or a MISSING reverse (a one-sided /
    // inconsistent store — a partial write, corruption, or a backend that does
    // not maintain the reverse index) — neither unambiguously confirms this
    // coop_id, so neither is eligible to populate entity_id. A reverse read error
    // also fails closed.
    match map.coop_for_entity(&binding.entity_id) {
        Ok(Some(reverse)) if reverse == target.coop_id => {}
        Ok(reverse_opt) => {
            let detail = match reverse_opt {
                Some(reverse) => format!("is reverse-bound to a different coop_id ({reverse:?})"),
                None => "has no reverse binding (one-sided / inconsistent store)".to_string(),
            };
            return make(
                TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding,
                Some(binding.entity_id.clone()),
                format!("resolved entity {detail}; fail closed"),
                None,
            );
        }
        Err(e) => {
            return make(
                TreasuryEntityIdBackfillAction::SkippedStorageError,
                Some(binding.entity_id.clone()),
                "reverse-index read failed; cannot verify the binding is unambiguous".into(),
                Some(e.to_string()),
            );
        }
    }

    // Resolver/legacy-projection conflict guard. If `target.coop_id` is itself
    // strictly projectable to a cooperative EntityId — same reject-not-normalize
    // semantics as the gateway's legacy fallback, which delegates to
    // `EntityId::cooperative` (used directly here to avoid an icn-gateway
    // dependency / layering violation) — and that projection DIFFERS from the
    // trusted binding's target, the gateway resolver classifies the same
    // condition as `Disagree`/`ResolverConflict`, NOT a trusted target. Do not
    // bless a treasury `entity_id` the auth gate would treat as a conflict: fail
    // closed. A NON-projectable `coop_id` (e.g. a default `coop:<uuid>`, or an
    // uppercase/underscore id) has no legacy projection to disagree with, so the
    // resolver-only / surrogate case below stays eligible.
    if let Ok(projected) = EntityId::cooperative(&target.coop_id) {
        if projected != binding.entity_id {
            return make(
                TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding,
                Some(binding.entity_id.clone()),
                format!(
                    "coop_id projects to {} but the trusted binding resolves to {} — the resolver would treat this as a conflict; fail closed",
                    projected.as_str(),
                    binding.entity_id.as_str()
                ),
                None,
            );
        }
    }

    // Eligible: a trusted, non-ambiguous, cooperative binding for a legacy
    // treasury that currently has no entity_id. The binding either matches the
    // legacy projection of `coop_id` or `coop_id` is non-projectable (resolver-
    // only / surrogate). READ-ONLY — report the intent; populate nothing.
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

// =====================================================================
// Controlled, mutating apply (ADR-0084).
// =====================================================================

/// What the apply did to a single eligible (`WouldPopulate`) treasury row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreasuryEntityIdBackfillApplyOutcome {
    /// `entity_id` was populated from the trusted binding (`None → Some`) and
    /// persisted.
    Populated,
    /// The row was eligible per the plan but the apply did **not** complete (a
    /// pre-write binding re-verification mismatch, a storage write error, or an
    /// inconsistent state at write time). The treasury was left unchanged — fail
    /// closed.
    Failed,
}

/// Per-row audit for the rows the apply acted on (the planner's `WouldPopulate`
/// set). `entity_id_before`/`entity_id_after` make the mutation explicit and
/// reversible to inspect; the full fail-closed classification of *every* row
/// lives in the embedded [`TreasuryEntityIdBackfillApplyReport::plan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasuryEntityIdBackfillApplyEntry {
    /// The treasury's flat `coop_id`, preserved byte-for-byte (the map key).
    pub coop_id: String,
    /// The treasury DID, for operator-facing identification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub treasury_did: Option<String>,
    /// What the apply did to this row.
    pub outcome: TreasuryEntityIdBackfillApplyOutcome,
    /// The treasury's `entity_id` before the apply. `None` for a legacy row (the
    /// only kind eligible), surfaced explicitly for a before/after audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id_before: Option<EntityId>,
    /// The treasury's `entity_id` after a successful apply (`Some` only for
    /// [`Populated`](TreasuryEntityIdBackfillApplyOutcome::Populated)).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id_after: Option<EntityId>,
    /// Trust provenance of the binding used (present once a trusted binding was
    /// re-verified for this row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<CoopEntityBindingProvenance>,
    /// A short, human-readable explanation of the outcome.
    pub reason: String,
    /// The underlying storage/map error string when one prevented the write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate, auditable result of a controlled treasury `entity_id` backfill
/// **apply** (ADR-0084).
///
/// Embeds the read-only [`plan`](Self::plan) verbatim (the same shape the
/// `entity-backfill-report` emits, with the full fail-closed classification and
/// counters) and adds the apply-specific outcome: how many `WouldPopulate` rows
/// were populated, how many failed closed, and a per-row before/after audit.
///
/// Invariant: `applied + failed == plan.would_populate` — every eligible row is
/// either populated or flagged failed; no eligible row is silently dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasuryEntityIdBackfillApplyReport {
    /// The read-only plan this apply was derived from (full classification +
    /// counters; `total`, `would_populate`, and every `skipped_*` bucket).
    pub plan: TreasuryEntityIdBackfillPlan,
    /// Rows whose `entity_id` was populated from a trusted binding and persisted.
    pub applied: usize,
    /// Eligible rows that did not complete (re-verification mismatch or storage
    /// error); each was left unchanged — fail closed.
    pub failed: usize,
    /// Per-row audit for the rows the apply acted on, in plan order.
    pub entries: Vec<TreasuryEntityIdBackfillApplyEntry>,
}

impl TreasuryEntityIdBackfillApplyReport {
    /// True when an apply did not fully complete (any eligible row failed to
    /// persist). Callers (e.g. the CLI) gate a non-zero exit on this.
    pub fn had_failures(&self) -> bool {
        self.failed > 0
    }
}

/// Outcome of the apply-time re-verification of a trusted, non-ambiguous binding.
enum ApplyReverify {
    /// Still a trusted, non-ambiguous match for the planned target; carries the
    /// audited binding provenance.
    Ok(CoopEntityBindingProvenance),
    /// No longer eligible — do not write. Carries an operator-facing `reason` and
    /// an optional underlying map error string.
    FailClosed {
        reason: String,
        error: Option<String>,
    },
}

/// Re-validate, **immediately before a write**, that `coop_id` still has a
/// trusted, non-ambiguous binding to exactly `resolved` — the SAME forward
/// binding + provenance + reverse-index requirement the read-only planner uses
/// (`plan_target`). A [`CoopEntityMap`] that changed, or that exposes
/// inconsistent forward/reverse indexes, between planning and the write must
/// fail closed here and never mutate (the stored `entity_id` would otherwise be
/// ambiguous). Read-only — performs only `binding_for_coop` / `coop_for_entity`
/// reads and binds/normalizes nothing.
fn reverify_trusted_non_ambiguous_binding(
    map: &dyn CoopEntityMap,
    coop_id: &str,
    resolved: &EntityId,
) -> ApplyReverify {
    // Forward binding must still resolve to the planned target with trusted
    // provenance.
    let provenance = match map.binding_for_coop(coop_id) {
        Ok(Some(binding))
            if &binding.entity_id == resolved && binding.provenance.is_trusted_for_resolution() =>
        {
            binding.provenance
        }
        Ok(_) => {
            return ApplyReverify::FailClosed {
                reason: "binding no longer a trusted match for the planned target; \
                         not applied (fail closed)"
                    .into(),
                error: None,
            };
        }
        Err(e) => {
            return ApplyReverify::FailClosed {
                reason: "map re-read failed; cannot confirm a safe binding (fail closed)".into(),
                error: Some(e.to_string()),
            };
        }
    };

    // Reverse index must still point byte-for-byte back to THIS coop_id (the
    // planner's non-ambiguity requirement). A reverse that was deleted, repointed
    // to a different coop_id, or errors fails closed — the forward binding alone
    // does not confirm the target is unambiguous.
    match map.coop_for_entity(resolved) {
        Ok(Some(reverse)) if reverse == coop_id => ApplyReverify::Ok(provenance),
        Ok(reverse_opt) => {
            let detail = match reverse_opt {
                Some(reverse) => format!("now points to a different coop_id ({reverse:?})"),
                None => "is missing".to_string(),
            };
            ApplyReverify::FailClosed {
                reason: format!(
                    "reverse binding {detail}; no longer points to this coop_id; \
                     not applied (fail closed)"
                ),
                error: None,
            }
        }
        Err(e) => ApplyReverify::FailClosed {
            reason: "reverse-index re-read failed; cannot confirm the binding is unambiguous \
                     (fail closed)"
                .into(),
            error: Some(e.to_string()),
        },
    }
}

impl TreasuryManager {
    /// Controlled, mutating **apply** of the treasury `entity_id` backfill
    /// (ADR-0084, #2082 lane).
    ///
    /// Populates `entity_id` for **exactly** the legacy treasuries the read-only
    /// [`plan_entity_id_backfill`](Self::plan_entity_id_backfill) classifies as
    /// [`WouldPopulate`](TreasuryEntityIdBackfillAction::WouldPopulate), and skips
    /// every fail-closed class unchanged (`SkippedAlreadyHasEntityId`,
    /// `SkippedNoMapping`, `SkippedUntrustedProvenance`,
    /// `SkippedNonCooperativeEntity`, `SkippedAmbiguousBinding`,
    /// `SkippedStorageError`). Apply is defined entirely in terms of the planner's
    /// `WouldPopulate` set — it never re-derives or relaxes eligibility.
    ///
    /// Each eligible row is re-verified against the canonical map immediately
    /// before mutation: the binding must still resolve to the planned target with
    /// trusted provenance, else the row fails closed and is left unchanged. The
    /// mutation touches only `entity_id` (`None → Some`); the `coop_id` is
    /// preserved byte-for-byte, the map is never written, and membership, roles,
    /// capabilities, auth configuration, and enforcement mode are untouched. A
    /// mapping grants **zero** authority — this writes an identity target only.
    ///
    /// Idempotent: a re-run re-classifies already-populated rows as
    /// `SkippedAlreadyHasEntityId`, so they never re-populate. Storage errors fail
    /// closed (per-row, since the treasury store has no all-or-nothing batch over
    /// rows): the affected row is reported `failed` and left unchanged.
    pub fn apply_entity_id_backfill(
        &mut self,
        map: &dyn CoopEntityMap,
    ) -> TreasuryEntityIdBackfillApplyReport {
        // Authoritative read-only classification first. Apply is defined ENTIRELY
        // in terms of this plan's WouldPopulate set (ADR-0084 §3).
        let plan = self.plan_entity_id_backfill(map);

        let mut entries: Vec<TreasuryEntityIdBackfillApplyEntry> = Vec::new();
        let mut applied = 0usize;
        let mut failed = 0usize;

        for row in &plan.entries {
            // Every fail-closed class is skipped, never coerced. Their full
            // classification is preserved in the embedded plan.
            if row.action != TreasuryEntityIdBackfillAction::WouldPopulate {
                continue;
            }

            let coop_id = row.coop_id.clone();
            let treasury_did = row.treasury_did.clone();

            // The planner only assigns WouldPopulate with a resolved cooperative
            // target; defend against a malformed row rather than unwrap.
            let Some(resolved) = row.resolved_entity_id.clone() else {
                failed += 1;
                entries.push(TreasuryEntityIdBackfillApplyEntry {
                    coop_id,
                    treasury_did,
                    outcome: TreasuryEntityIdBackfillApplyOutcome::Failed,
                    entity_id_before: None,
                    entity_id_after: None,
                    provenance: None,
                    reason: "eligible row carried no resolved entity_id; not applied (fail closed)"
                        .into(),
                    error: None,
                });
                continue;
            };

            // Re-verify the FULL trusted, non-ambiguous binding immediately before
            // mutating — the same forward binding + provenance + reverse-index
            // check the planner uses. On the CLI path the daemon is stopped, so
            // this matches the plan exactly; it is a defensive fail-closed re-check
            // that also sources the audited provenance. A map that changed, or that
            // exposes inconsistent forward/reverse indexes between planning and the
            // write, fails closed here — nothing is written.
            let provenance = match reverify_trusted_non_ambiguous_binding(map, &coop_id, &resolved)
            {
                ApplyReverify::Ok(provenance) => provenance,
                ApplyReverify::FailClosed { reason, error } => {
                    failed += 1;
                    entries.push(TreasuryEntityIdBackfillApplyEntry {
                        coop_id,
                        treasury_did,
                        outcome: TreasuryEntityIdBackfillApplyOutcome::Failed,
                        entity_id_before: None,
                        entity_id_after: None,
                        provenance: None,
                        reason,
                        error,
                    });
                    continue;
                }
            };

            // Resolve the planned treasury DID carried by this plan row. The write
            // MUST target this exact DID — not whichever row currently wins the
            // coop_id index — so a duplicate/inconsistent coop_id cannot mutate the
            // wrong row or leave the audit disagreeing with the write. A missing or
            // malformed DID fails closed.
            let planned_did = match treasury_did.as_deref().map(Did::from_str) {
                Some(Ok(did)) => did,
                _ => {
                    failed += 1;
                    entries.push(TreasuryEntityIdBackfillApplyEntry {
                        coop_id,
                        treasury_did,
                        outcome: TreasuryEntityIdBackfillApplyOutcome::Failed,
                        entity_id_before: None,
                        entity_id_after: None,
                        provenance: Some(provenance),
                        reason: "plan row carried no valid treasury DID; not applied (fail closed)"
                            .into(),
                        error: None,
                    });
                    continue;
                }
            };

            // Mutate exactly one row by its planned DID: entity_id None ->
            // Some(resolved), coop_id re-verified byte-for-byte, persisted before
            // the in-memory commit.
            match self.populate_treasury_entity_id_for_did(&planned_did, &coop_id, resolved.clone())
            {
                Ok(TreasuryEntityIdPopulateResult::Populated) => {
                    applied += 1;
                    let after = resolved.as_str().to_string();
                    entries.push(TreasuryEntityIdBackfillApplyEntry {
                        coop_id,
                        treasury_did,
                        outcome: TreasuryEntityIdBackfillApplyOutcome::Populated,
                        entity_id_before: None,
                        entity_id_after: Some(resolved),
                        provenance: Some(provenance),
                        reason: format!("populated entity_id from trusted binding to {after}"),
                        error: None,
                    });
                }
                // Unreachable on the single-writer CLI path (we hold &mut self
                // between plan and apply, so state cannot change underneath), but
                // flagged fail-closed rather than silently dropped.
                Ok(other) => {
                    failed += 1;
                    let reason = match other {
                        TreasuryEntityIdPopulateResult::AlreadyPopulated => {
                            "treasury already carried an entity_id at write time; \
                             not applied (fail closed)"
                        }
                        TreasuryEntityIdPopulateResult::TreasuryNotFound => {
                            "planned treasury DID not found at write time; \
                             not applied (fail closed)"
                        }
                        TreasuryEntityIdPopulateResult::CoopIdMismatch => {
                            "planned treasury DID no longer matches the planned coop_id; \
                             not applied (fail closed)"
                        }
                        TreasuryEntityIdPopulateResult::EntityIdConflict => {
                            "target entity_id is already bound to a different treasury; \
                             not applied (fail closed)"
                        }
                        TreasuryEntityIdPopulateResult::Populated => unreachable!(),
                    };
                    entries.push(TreasuryEntityIdBackfillApplyEntry {
                        coop_id,
                        treasury_did,
                        outcome: TreasuryEntityIdBackfillApplyOutcome::Failed,
                        entity_id_before: None,
                        entity_id_after: None,
                        provenance: Some(provenance),
                        reason: reason.into(),
                        error: None,
                    });
                }
                Err(e) => {
                    failed += 1;
                    entries.push(TreasuryEntityIdBackfillApplyEntry {
                        coop_id,
                        treasury_did,
                        outcome: TreasuryEntityIdBackfillApplyOutcome::Failed,
                        entity_id_before: None,
                        entity_id_after: None,
                        provenance: Some(provenance),
                        reason: "storage write failed; treasury left unchanged (fail closed)"
                            .into(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        TreasuryEntityIdBackfillApplyReport {
            plan,
            applied,
            failed,
            entries,
        }
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
    fn skipped_when_reverse_binding_missing_one_sided() {
        // A trusted forward binding but NO reverse binding (a one-sided /
        // inconsistent store). The atomic forward+reverse invariant is broken, so
        // the reverse does not confirm this coop_id — fail closed, never populate.
        let map = StubMap {
            binding: |coop_id| {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    entity_id: EntityId::cooperative("food-coop").unwrap(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            },
            // Reverse index is missing entirely.
            reverse: |_| Ok(None),
        };
        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(
            plan.would_populate, 0,
            "must not populate from a one-sided binding"
        );
        assert_eq!(plan.skipped_ambiguous_binding, 1);
        let entry = one(&plan);
        assert_eq!(
            entry.action,
            TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding
        );
        assert!(entry.reason.contains("no reverse binding"));
    }

    #[test]
    fn skipped_when_projectable_coop_id_conflicts_with_binding() {
        // `food-coop` is itself strictly projectable to `cooperative:food-coop`,
        // but the trusted map binds it to a DIFFERENT cooperative (`other-coop`)
        // with a matching reverse index. The gateway resolver would call this a
        // Disagree/ResolverConflict, so the planner must NOT bless it — fail
        // closed as SkippedAmbiguousBinding.
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("other-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        // Sanity: the binding is trusted with a matching reverse, so only the
        // projection-conflict guard can skip it.
        assert_eq!(
            map.coop_for_entity(&coop_eid("other-coop")).unwrap(),
            Some("food-coop".to_string())
        );

        let plan = plan_treasury_entity_id_backfill(vec![target("food-coop", None)], &map);
        assert_eq!(
            plan.would_populate, 0,
            "must not populate when the projectable coop_id disagrees with the binding"
        );
        assert_eq!(plan.skipped_ambiguous_binding, 1);
        let entry = one(&plan);
        assert_eq!(
            entry.action,
            TreasuryEntityIdBackfillAction::SkippedAmbiguousBinding
        );
        assert_eq!(entry.resolved_entity_id, Some(coop_eid("other-coop")));
        assert!(entry.reason.contains("conflict"));
    }

    #[test]
    fn would_populate_when_non_projectable_coop_id_has_trusted_surrogate() {
        // `coop_A` is NON-projectable (uppercase + underscore), so there is no
        // legacy projection to disagree with. A trusted surrogate binding with a
        // matching reverse is the resolver-only case and stays eligible.
        let map = InMemoryCoopEntityMap::new();
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        map.bind_resolved_with_provenance(
            "coop_A",
            &surrogate,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .unwrap();
        // `coop_A` must not be strictly projectable (else this would be a conflict).
        assert!(EntityId::cooperative("coop_A").is_err());

        let plan = plan_treasury_entity_id_backfill(vec![target("coop_A", None)], &map);
        assert_eq!(plan.would_populate, 1);
        assert_eq!(plan.skipped_ambiguous_binding, 0);
        let entry = one(&plan);
        assert_eq!(entry.action, TreasuryEntityIdBackfillAction::WouldPopulate);
        assert_eq!(entry.resolved_entity_id, Some(surrogate));
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

    // ==================================================================
    // Controlled apply (ADR-0084).
    // ==================================================================

    /// Build an in-memory manager holding one legacy treasury (`entity_id: None`)
    /// per `coop_id`, registered through the real `register_treasury` path.
    fn legacy_manager_with(coops: &[&str]) -> TreasuryManager {
        use icn_identity::KeyPair;
        let mut mgr = TreasuryManager::new();
        let creator = KeyPair::generate().unwrap().did().clone();
        for coop in coops {
            let treasury_did = KeyPair::generate().unwrap().did().clone();
            mgr.register_treasury(
                treasury_did,
                coop.to_string(),
                "USD".to_string(),
                creator.clone(),
                None,
            )
            .unwrap();
        }
        mgr
    }

    /// A map double that yields a trusted, non-ambiguous binding (so the row
    /// plans as `WouldPopulate`) yet makes `bind_resolved` `unreachable!` — proving
    /// the apply NEVER writes the map.
    struct ReadOnlyEligibleMap {
        coop_id: String,
        entity_id: EntityId,
    }
    impl CoopEntityMap for ReadOnlyEligibleMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("apply must never write the coop_id <-> EntityId map");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            Ok(Some(self.coop_id.clone()))
        }
        fn binding_for_coop(
            &self,
            coop_id: &str,
        ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
            Ok(Some(CoopEntityBinding {
                coop_id: coop_id.to_string(),
                entity_id: self.entity_id.clone(),
                provenance: CoopEntityBindingProvenance::Activation,
            }))
        }
    }

    /// A map double that returns a trusted binding on the FIRST `binding_for_coop`
    /// read (planning ⇒ `WouldPopulate`) and a storage error on the SECOND (the
    /// apply's pre-write re-verification) — exercising the fail-closed write guard.
    struct ReVerifyFailMap {
        coop_id: String,
        entity_id: EntityId,
        binding_calls: std::cell::Cell<u32>,
    }
    impl CoopEntityMap for ReVerifyFailMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("apply must never write the coop_id <-> EntityId map");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            Ok(Some(self.coop_id.clone()))
        }
        fn binding_for_coop(
            &self,
            coop_id: &str,
        ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
            let n = self.binding_calls.get();
            self.binding_calls.set(n + 1);
            if n == 0 {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    entity_id: self.entity_id.clone(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            } else {
                Err(CoopEntityMapError::Storage(
                    "re-verify read failed".to_string(),
                ))
            }
        }
    }

    #[test]
    fn apply_populates_only_would_populate_rows() {
        let mut mgr = legacy_manager_with(&["food-coop", "legacy-coop", "no-map-coop"]);
        let map = InMemoryCoopEntityMap::new();
        // food-coop: trusted Activation -> eligible.
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        // legacy-coop: plain bind -> UnknownLegacy -> untrusted.
        map.bind_resolved("legacy-coop", &coop_eid("legacy-coop"))
            .unwrap();
        // no-map-coop: nothing.

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.plan.would_populate, 1);
        assert_eq!(report.applied, 1);
        assert_eq!(report.failed, 0);
        // Invariant: every eligible row is either populated or flagged failed.
        assert_eq!(report.applied + report.failed, report.plan.would_populate);

        // Only the trusted row is populated; the others are untouched.
        assert_eq!(
            mgr.get_treasury_by_coop("food-coop").unwrap().entity_id(),
            Some(&coop_eid("food-coop"))
        );
        assert!(mgr
            .get_treasury_by_coop("legacy-coop")
            .unwrap()
            .entity_id()
            .is_none());
        assert!(mgr
            .get_treasury_by_coop("no-map-coop")
            .unwrap()
            .entity_id()
            .is_none());

        // The apply audit lists only the row it acted on.
        assert_eq!(report.entries.len(), 1);
        assert_eq!(
            report.entries[0].outcome,
            TreasuryEntityIdBackfillApplyOutcome::Populated
        );
        assert_eq!(report.entries[0].entity_id_before, None);
        assert_eq!(
            report.entries[0].entity_id_after,
            Some(coop_eid("food-coop"))
        );
        assert!(report.entries[0].provenance.is_some());
        assert!(!report.had_failures());
    }

    #[test]
    fn apply_is_idempotent_on_rerun() {
        let mut mgr = legacy_manager_with(&["food-coop"]);
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let first = mgr.apply_entity_id_backfill(&map);
        assert_eq!(first.applied, 1);
        assert_eq!(
            mgr.get_treasury_by_coop("food-coop").unwrap().entity_id(),
            Some(&coop_eid("food-coop"))
        );

        // Re-run: the row now classifies SkippedAlreadyHasEntityId and is a no-op.
        let second = mgr.apply_entity_id_backfill(&map);
        assert_eq!(second.applied, 0);
        assert_eq!(second.failed, 0);
        assert_eq!(second.plan.would_populate, 0);
        assert_eq!(second.plan.skipped_already_has_entity_id, 1);
        assert!(second.entries.is_empty());
        assert_eq!(
            mgr.get_treasury_by_coop("food-coop").unwrap().entity_id(),
            Some(&coop_eid("food-coop"))
        );
    }

    #[test]
    fn apply_skips_already_populated_row() {
        use icn_identity::KeyPair;
        let mut mgr = TreasuryManager::new();
        let creator = KeyPair::generate().unwrap().did().clone();
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        mgr.register_treasury_with_entity(
            treasury_did,
            coop_eid("food-coop"),
            "USD".to_string(),
            creator,
            None,
        )
        .unwrap();
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 0);
        assert_eq!(report.plan.skipped_already_has_entity_id, 1);
        assert!(report.entries.is_empty());
    }

    #[test]
    fn apply_skips_untrusted_provenance() {
        let mut mgr = legacy_manager_with(&["food-coop"]);
        let map = InMemoryCoopEntityMap::new();
        // Plain bind reads back as the fail-closed UnknownLegacy sentinel.
        map.bind_resolved("food-coop", &coop_eid("food-coop"))
            .unwrap();
        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 0);
        assert_eq!(report.plan.skipped_untrusted_provenance, 1);
        assert!(mgr
            .get_treasury_by_coop("food-coop")
            .unwrap()
            .entity_id()
            .is_none());
    }

    #[test]
    fn apply_skips_no_mapping() {
        let mut mgr = legacy_manager_with(&["food-coop"]);
        let map = InMemoryCoopEntityMap::new();
        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 0);
        assert_eq!(report.plan.skipped_no_mapping, 1);
        assert!(mgr
            .get_treasury_by_coop("food-coop")
            .unwrap()
            .entity_id()
            .is_none());
    }

    #[test]
    fn apply_skips_non_cooperative_entity() {
        let mut mgr = legacy_manager_with(&["food-coop"]);
        let map = StubMap {
            binding: |coop_id| {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    entity_id: EntityId::community("some-community").unwrap(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            },
            reverse: |_| Ok(None),
        };
        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 0);
        assert_eq!(report.plan.skipped_non_cooperative_entity, 1);
        assert!(mgr
            .get_treasury_by_coop("food-coop")
            .unwrap()
            .entity_id()
            .is_none());
    }

    #[test]
    fn apply_skips_ambiguous_reverse_mismatch() {
        let mut mgr = legacy_manager_with(&["food-coop"]);
        let map = StubMap {
            binding: |coop_id| {
                Ok(Some(CoopEntityBinding {
                    coop_id: coop_id.to_string(),
                    entity_id: EntityId::cooperative("food-coop").unwrap(),
                    provenance: CoopEntityBindingProvenance::Activation,
                }))
            },
            // Reverse points to a different coop_id -> ambiguous.
            reverse: |_| Ok(Some("other-coop".to_string())),
        };
        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 0);
        assert_eq!(report.plan.skipped_ambiguous_binding, 1);
        assert!(mgr
            .get_treasury_by_coop("food-coop")
            .unwrap()
            .entity_id()
            .is_none());
    }

    #[test]
    fn apply_preserves_coop_id_byte_for_byte() {
        // A non-mappable legacy id bound to a surrogate whose identifier DIFFERS
        // from the coop_id: proves apply sets entity_id without deriving/rewriting
        // coop_id.
        let coop_id = "coop:550e8400-e29b-41d4-a716-446655440000";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            coop_id,
            &surrogate,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .unwrap();

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 1);
        let treasury = mgr.get_treasury_by_coop(coop_id).unwrap();
        assert_eq!(
            treasury.coop_id(),
            coop_id,
            "coop_id must be preserved byte-for-byte"
        );
        assert_eq!(treasury.entity_id(), Some(&surrogate));
        assert_ne!(
            surrogate.identifier(),
            coop_id,
            "surrogate identifier differs from coop_id (no derivation)"
        );
    }

    #[test]
    fn apply_persists_entity_id_and_survives_reload() {
        use icn_identity::KeyPair;
        use icn_store::{SledStore, Store};
        use std::sync::Arc;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        // One shared Arc/Db reused on reopen — no second SledStore::open, no lock
        // contention.
        let store: Arc<dyn Store> = Arc::new(SledStore::open(tmp.path()).unwrap());
        let creator = KeyPair::generate().unwrap().did().clone();
        let treasury_did = KeyPair::generate().unwrap().did().clone();

        {
            let mut mgr = TreasuryManager::with_store(store.clone()).unwrap();
            mgr.register_treasury(
                treasury_did,
                "food-coop".to_string(),
                "USD".to_string(),
                creator,
                None,
            )
            .unwrap();
            let map = InMemoryCoopEntityMap::new();
            map.bind_resolved_with_provenance(
                "food-coop",
                &coop_eid("food-coop"),
                CoopEntityBindingProvenance::Activation,
            )
            .unwrap();
            let report = mgr.apply_entity_id_backfill(&map);
            assert_eq!(report.applied, 1);
            assert_eq!(
                mgr.get_treasury_by_coop("food-coop").unwrap().entity_id(),
                Some(&coop_eid("food-coop"))
            );
        }

        // Reopen from the same store: the populated entity_id is durable.
        let reopened = TreasuryManager::with_store(store).unwrap();
        let treasury = reopened.get_treasury_by_coop("food-coop").unwrap();
        assert_eq!(treasury.entity_id(), Some(&coop_eid("food-coop")));
        assert_eq!(treasury.coop_id(), "food-coop");
    }

    #[test]
    fn apply_report_json_includes_before_after_and_provenance() {
        let mut mgr = legacy_manager_with(&["food-coop"]);
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();
        let report = mgr.apply_entity_id_backfill(&map);

        let v: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(v["applied"].as_u64(), Some(1));
        assert_eq!(v["failed"].as_u64(), Some(0));
        // The embedded plan carries the full report-shape classification.
        assert_eq!(v["plan"]["would_populate"].as_u64(), Some(1));
        let entry = &v["entries"][0];
        assert_eq!(entry["outcome"].as_str(), Some("populated"));
        assert_eq!(entry["coop_id"].as_str(), Some("food-coop"));
        assert!(entry["entity_id_after"].as_str().is_some());
        // entity_id_before is None -> omitted from JSON.
        assert!(entry.get("entity_id_before").is_none());
        // Provenance of the trusted binding is surfaced for audit.
        assert!(entry.get("provenance").is_some());
    }

    #[test]
    fn apply_never_writes_the_map() {
        // `coop_A` is non-projectable, so no projection-conflict skip; the stub
        // yields a trusted binding with a matching reverse -> WouldPopulate. The
        // stub's bind_resolved is `unreachable!`, so a successful apply proves the
        // map is never written.
        let coop_id = "coop_A";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = ReadOnlyEligibleMap {
            coop_id: coop_id.to_string(),
            entity_id: surrogate.clone(),
        };

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 1);
        assert_eq!(
            mgr.get_treasury_by_coop(coop_id).unwrap().entity_id(),
            Some(&surrogate)
        );
    }

    #[test]
    fn apply_fails_closed_on_reverify_storage_error() {
        let coop_id = "coop_A"; // non-projectable: no projection-conflict at plan time.
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = ReVerifyFailMap {
            coop_id: coop_id.to_string(),
            entity_id: surrogate,
            binding_calls: std::cell::Cell::new(0),
        };

        let report = mgr.apply_entity_id_backfill(&map);
        // Planned eligible, but the apply re-verify hit a storage error.
        assert_eq!(report.plan.would_populate, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.failed, 1);
        assert!(report.had_failures());
        // Fail closed: the treasury is left unchanged.
        assert!(mgr
            .get_treasury_by_coop(coop_id)
            .unwrap()
            .entity_id()
            .is_none());
        assert_eq!(
            report.entries[0].outcome,
            TreasuryEntityIdBackfillApplyOutcome::Failed
        );
        assert!(report.entries[0].error.is_some());
    }

    /// How the reverse index has drifted by the time the apply re-verifies it.
    enum ReverseDrift {
        /// The reverse binding was deleted.
        Missing,
        /// The reverse binding now points to a different coop_id.
        Different(String),
        /// The reverse lookup errors.
        Error,
    }

    /// A map double whose forward binding stays trusted but whose reverse index
    /// DRIFTS between the planner pass and the apply re-verify pass: the first
    /// `coop_for_entity` call (planning) returns the matching coop_id (so the row
    /// plans as `WouldPopulate`), and every later call (the apply re-verify)
    /// returns the drifted reverse. `bind_resolved` is `unreachable!` — apply must
    /// never write the map.
    struct ReverseDriftMap {
        coop_id: String,
        entity_id: EntityId,
        coop_for_entity_calls: std::cell::Cell<u32>,
        drift: ReverseDrift,
    }
    impl CoopEntityMap for ReverseDriftMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("apply must never write the coop_id <-> EntityId map");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            let n = self.coop_for_entity_calls.get();
            self.coop_for_entity_calls.set(n + 1);
            if n == 0 {
                // Planner pass: matching reverse -> WouldPopulate.
                Ok(Some(self.coop_id.clone()))
            } else {
                // Apply re-verify pass: the reverse has drifted.
                match &self.drift {
                    ReverseDrift::Missing => Ok(None),
                    ReverseDrift::Different(other) => Ok(Some(other.clone())),
                    ReverseDrift::Error => Err(CoopEntityMapError::Storage(
                        "reverse index unreadable".to_string(),
                    )),
                }
            }
        }
        fn binding_for_coop(
            &self,
            coop_id: &str,
        ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
            Ok(Some(CoopEntityBinding {
                coop_id: coop_id.to_string(),
                entity_id: self.entity_id.clone(),
                provenance: CoopEntityBindingProvenance::Activation,
            }))
        }
    }

    #[test]
    fn apply_fails_closed_when_reverse_binding_missing_at_write() {
        // Plan classifies WouldPopulate (matching reverse at plan time), but the
        // reverse index is deleted before the apply write -> fail closed.
        let coop_id = "coop_A"; // non-projectable: no projection-conflict at plan time.
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = ReverseDriftMap {
            coop_id: coop_id.to_string(),
            entity_id: surrogate,
            coop_for_entity_calls: std::cell::Cell::new(0),
            drift: ReverseDrift::Missing,
        };

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(
            report.plan.would_populate, 1,
            "planner classified the row WouldPopulate"
        );
        assert_eq!(report.applied, 0);
        assert_eq!(report.failed, 1);
        assert!(report.had_failures());
        // Fail closed: the treasury is left unchanged.
        assert!(mgr
            .get_treasury_by_coop(coop_id)
            .unwrap()
            .entity_id()
            .is_none());
        let entry = &report.entries[0];
        assert_eq!(entry.outcome, TreasuryEntityIdBackfillApplyOutcome::Failed);
        assert!(entry.reason.contains("reverse binding"));
    }

    #[test]
    fn apply_fails_closed_when_reverse_binding_repointed_at_write() {
        // Plan classifies WouldPopulate, but the reverse index now points to a
        // DIFFERENT coop_id before the apply write -> fail closed.
        let coop_id = "coop_A";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = ReverseDriftMap {
            coop_id: coop_id.to_string(),
            entity_id: surrogate,
            coop_for_entity_calls: std::cell::Cell::new(0),
            drift: ReverseDrift::Different("other-coop".to_string()),
        };

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.plan.would_populate, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.failed, 1);
        assert!(mgr
            .get_treasury_by_coop(coop_id)
            .unwrap()
            .entity_id()
            .is_none());
        let entry = &report.entries[0];
        assert_eq!(entry.outcome, TreasuryEntityIdBackfillApplyOutcome::Failed);
        assert!(entry.reason.contains("different coop_id"));
    }

    #[test]
    fn apply_fails_closed_when_reverse_lookup_errors_at_write() {
        // Plan classifies WouldPopulate, but the reverse lookup errors during the
        // apply re-verify -> fail closed with the error surfaced.
        let coop_id = "coop_A";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = ReverseDriftMap {
            coop_id: coop_id.to_string(),
            entity_id: surrogate,
            coop_for_entity_calls: std::cell::Cell::new(0),
            drift: ReverseDrift::Error,
        };

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.plan.would_populate, 1);
        assert_eq!(report.applied, 0);
        assert_eq!(report.failed, 1);
        assert!(mgr
            .get_treasury_by_coop(coop_id)
            .unwrap()
            .entity_id()
            .is_none());
        let entry = &report.entries[0];
        assert_eq!(entry.outcome, TreasuryEntityIdBackfillApplyOutcome::Failed);
        assert!(entry.error.is_some());
        assert!(entry.reason.contains("reverse-index"));
    }

    // ------------------------------------------------------------------
    // Entity-index integrity after a (surrogate) backfill apply (Codex P2).
    // ------------------------------------------------------------------

    #[test]
    fn apply_surrogate_backfill_indexes_entity_lookup() {
        // A surrogate backfill: legacy coop_id "coop:<uuid>" bound to a surrogate
        // whose identifier differs from the coop_id. After apply, entity-keyed
        // lookup must find the treasury while the legacy coop_id is preserved.
        let coop_id = "coop:550e8400-e29b-41d4-a716-446655440000";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            coop_id,
            &surrogate,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .unwrap();

        assert_eq!(mgr.apply_entity_id_backfill(&map).applied, 1);

        let by_entity = mgr
            .get_treasury_by_entity(&surrogate)
            .expect("entity index finds the backfilled treasury");
        assert_eq!(by_entity.coop_id(), coop_id, "legacy coop_id preserved");
        assert_eq!(by_entity.entity_id(), Some(&surrogate));
        // The original coop-keyed lookup still resolves under the legacy coop_id.
        assert_eq!(
            mgr.get_treasury_by_coop(coop_id).unwrap().entity_id(),
            Some(&surrogate)
        );
    }

    #[test]
    fn apply_surrogate_backfill_blocks_duplicate_entity_registration() {
        use icn_identity::KeyPair;
        let coop_id = "coop:550e8400-e29b-41d4-a716-446655440000";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            coop_id,
            &surrogate,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .unwrap();
        assert_eq!(mgr.apply_entity_id_backfill(&map).applied, 1);

        // A later entity-based registration for the SAME surrogate entity must be
        // rejected, even though its derived coop_id differs from the backfilled
        // treasury's legacy coop_id.
        let creator = KeyPair::generate().unwrap().did().clone();
        let new_did = KeyPair::generate().unwrap().did().clone();
        let result = mgr.register_treasury_with_entity(
            new_did,
            surrogate.clone(),
            "USD".to_string(),
            creator,
            None,
        );
        assert!(
            result.is_err(),
            "duplicate entity registration must be rejected"
        );
        // No second treasury was created for the entity.
        assert_eq!(
            mgr.get_treasury_by_entity(&surrogate).unwrap().coop_id(),
            coop_id
        );
    }

    #[test]
    fn apply_surrogate_backfill_entity_index_survives_reload() {
        use icn_identity::KeyPair;
        use icn_store::{SledStore, Store};
        use std::sync::Arc;
        use tempfile::TempDir;

        let coop_id = "coop:550e8400-e29b-41d4-a716-446655440000";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let tmp = TempDir::new().unwrap();
        let store: Arc<dyn Store> = Arc::new(SledStore::open(tmp.path()).unwrap());
        let creator = KeyPair::generate().unwrap().did().clone();
        let treasury_did = KeyPair::generate().unwrap().did().clone();

        {
            let mut mgr = TreasuryManager::with_store(store.clone()).unwrap();
            mgr.register_treasury(
                treasury_did,
                coop_id.to_string(),
                "USD".to_string(),
                creator,
                None,
            )
            .unwrap();
            let map = InMemoryCoopEntityMap::new();
            map.bind_resolved_with_provenance(
                coop_id,
                &surrogate,
                CoopEntityBindingProvenance::OperatorBackfill,
            )
            .unwrap();
            assert_eq!(mgr.apply_entity_id_backfill(&map).applied, 1);
        }

        // Reopen: the entity index is rebuilt from persisted rows.
        let mut reopened = TreasuryManager::with_store(store).unwrap();
        let by_entity = reopened
            .get_treasury_by_entity(&surrogate)
            .expect("entity index rebuilt on hydration");
        assert_eq!(by_entity.coop_id(), coop_id);
        assert_eq!(by_entity.entity_id(), Some(&surrogate));

        // Duplicate entity registration is still rejected after reload.
        let creator2 = KeyPair::generate().unwrap().did().clone();
        let did2 = KeyPair::generate().unwrap().did().clone();
        assert!(reopened
            .register_treasury_with_entity(did2, surrogate, "USD".to_string(), creator2, None)
            .is_err());
    }

    #[test]
    fn failed_apply_does_not_index_entity() {
        use icn_identity::KeyPair;
        // Reverse binding drifts to missing at write -> apply fails closed. The
        // entity must NOT be indexed, and duplicate protection must not be
        // spuriously inserted.
        let coop_id = "coop_A";
        let surrogate = coop_eid("coop-legacy-deadbeef0123");
        let mut mgr = legacy_manager_with(&[coop_id]);
        let map = ReverseDriftMap {
            coop_id: coop_id.to_string(),
            entity_id: surrogate.clone(),
            coop_for_entity_calls: std::cell::Cell::new(0),
            drift: ReverseDrift::Missing,
        };

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 0);
        assert_eq!(report.failed, 1);
        // The entity was not indexed (lookup misses).
        assert!(mgr.get_treasury_by_entity(&surrogate).is_none());
        // And a fresh entity registration for that surrogate is NOT blocked (no
        // spurious index entry was left behind).
        let creator = KeyPair::generate().unwrap().did().clone();
        let did = KeyPair::generate().unwrap().did().clone();
        assert!(mgr
            .register_treasury_with_entity(did, surrogate, "USD".to_string(), creator, None)
            .is_ok());
    }

    #[test]
    fn apply_writes_and_audits_the_planned_treasury_did() {
        use icn_identity::KeyPair;
        // End-to-end: the row the audit reports as populated is the exact treasury
        // DID the planner carried, and that same DID's row is the one mutated.
        let mut mgr = TreasuryManager::new();
        let creator = KeyPair::generate().unwrap().did().clone();
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        mgr.register_treasury(
            treasury_did.clone(),
            "food-coop".to_string(),
            "USD".to_string(),
            creator,
            None,
        )
        .unwrap();
        let map = InMemoryCoopEntityMap::new();
        map.bind_resolved_with_provenance(
            "food-coop",
            &coop_eid("food-coop"),
            CoopEntityBindingProvenance::Activation,
        )
        .unwrap();

        let report = mgr.apply_entity_id_backfill(&map);
        assert_eq!(report.applied, 1);

        // The exact planned DID's row was mutated.
        assert_eq!(
            mgr.get_treasury(&treasury_did).unwrap().entity_id(),
            Some(&coop_eid("food-coop"))
        );
        // The audit entry names that same DID (write target == planned == audited).
        let entry = report
            .entries
            .iter()
            .find(|e| e.outcome == TreasuryEntityIdBackfillApplyOutcome::Populated)
            .expect("a populated entry");
        assert_eq!(entry.treasury_did.as_deref(), Some(treasury_did.as_str()));
    }
}
