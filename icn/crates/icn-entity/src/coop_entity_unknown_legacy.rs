//! Read-only report of `UnknownLegacy` / untrusted `coop_id ↔ EntityId`
//! bindings that would be *repair candidates* (#2082 lane).
//!
//! # What this is
//!
//! A pure, **read-only** classifier. Given a set of flat `coop_id`s and a
//! reference to a [`CoopEntityMap`](crate::CoopEntityMap), it reports which of
//! their *existing* map bindings are **not trusted** — and therefore cannot be
//! acted on as resolution evidence — and, for each, *why* and *what class of
//! evidence a later repair workflow would require*. It never repairs anything.
//!
//! It complements the existing coverage report
//! ([`classify_coop_ids`](crate::classify_coop_ids), #2082 PR3), which measures
//! whether a `coop_id` is bound at all. This report looks *inside* the bound
//! rows and asks a different question: of the rows that are bound, which are
//! [`UnknownLegacy`](crate::CoopEntityBindingProvenance::UnknownLegacy) or
//! otherwise untrusted, and thus candidates for an evidence-bearing repair?
//!
//! # Why "repair candidate", not "repair"
//!
//! [`UnknownLegacy`] is the fail-closed provenance sentinel: a binding written
//! before provenance existed, or via a plain
//! [`bind_resolved`](crate::CoopEntityMap::bind_resolved), or by a future binary
//! whose provenance variant this one does not recognize, all read back as
//! `UnknownLegacy` and are **never** trusted for resolution. Such a binding may
//! be perfectly correct — but it carries no accountable origin, so a system that
//! must not guess treats it as untrusted until an explicit, evidence-bearing
//! workflow re-establishes provenance. This report *names* those rows and the
//! evidence class a repair would need. It does **not** upgrade trust, re-bind,
//! or write anything.
//!
//! # Read-only, no authority, no normalization, no trust upgrade
//!
//! - **No writes.** Only the map's read methods are called
//!   ([`binding_for_coop`](crate::CoopEntityMap::binding_for_coop),
//!   [`coop_for_entity`](crate::CoopEntityMap::coop_for_entity)). It never binds.
//! - **No trust upgrade.** An [`UnknownLegacy`] row is *reported* as untrusted;
//!   it is never reclassified as trusted, and the required-evidence field states
//!   what a repair would need without performing it.
//! - **No authority.** Reporting — like binding — grants no membership, role,
//!   capability, mandate, permission, or standing. A mapping is only a name
//!   binding; this report is only an observation about names.
//! - **No normalization.** Each `coop_id` is preserved byte-for-byte.

use crate::coop_entity_map::CoopEntityBindingProvenance;
use crate::coop_entity_map::CoopEntityMap;
use crate::entity::EntityId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// How a single bound `coop_id` stands relative to being *trusted* resolution
/// evidence. Values are mutually exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownLegacyStatus {
    /// A healthy, trusted binding: provenance
    /// [`is_trusted_for_resolution`](CoopEntityBindingProvenance::is_trusted_for_resolution),
    /// the reverse index agrees byte-for-byte, and the target is a well-formed
    /// cooperative [`EntityId`]. **Not** a repair candidate.
    Trusted,
    /// No binding exists for this `coop_id` in the map, so there is nothing to
    /// repair here (coverage is a separate report's concern). **Not** a repair
    /// candidate.
    NotBound,
    /// A binding exists and is structurally consistent, but its provenance is
    /// [`UnknownLegacy`](CoopEntityBindingProvenance::UnknownLegacy) — untrusted,
    /// fail-closed. A repair candidate.
    UntrustedProvenance,
    /// The forward binding resolves to an `EntityId` whose reverse index does
    /// **not** point back at this `coop_id` (missing or cross-linked). Ambiguous
    /// and unsafe regardless of provenance. A repair candidate.
    ReverseMismatch,
    /// The bound `EntityId` is not a well-formed cooperative identifier (e.g. an
    /// invalid slug that only *tags* as cooperative, or a non-cooperative type).
    /// Unsafe regardless of provenance. A repair candidate.
    MalformedTarget,
    /// The persisted state for this `coop_id` could not be read (I/O, lock
    /// poison, decode error). Fail-closed: surfaced as untrusted, never assumed
    /// healthy. A repair candidate.
    StorageError,
}

impl UnknownLegacyStatus {
    /// Whether this status marks the row as a repair candidate (i.e. an
    /// untrusted or unreadable binding), as opposed to [`Trusted`](Self::Trusted)
    /// or [`NotBound`](Self::NotBound).
    pub fn is_repair_candidate(&self) -> bool {
        matches!(
            self,
            UnknownLegacyStatus::UntrustedProvenance
                | UnknownLegacyStatus::ReverseMismatch
                | UnknownLegacyStatus::MalformedTarget
                | UnknownLegacyStatus::StorageError
        )
    }
}

/// The class of evidence an explicit, out-of-band repair workflow would require
/// to make a candidate binding trusted. This report **states** the class; it
/// never gathers the evidence or applies the repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairEvidence {
    /// No repair needed ([`Trusted`](UnknownLegacyStatus::Trusted)) or nothing to
    /// repair ([`NotBound`](UnknownLegacyStatus::NotBound)).
    None,
    /// An accountable provenance attestation — an
    /// [`Activation`](CoopEntityBindingProvenance::Activation),
    /// [`OperatorBackfill`](CoopEntityBindingProvenance::OperatorBackfill),
    /// [`Surrogate`](CoopEntityBindingProvenance::Surrogate), or
    /// [`GovernanceReceipt`](CoopEntityBindingProvenance::GovernanceReceipt)
    /// establishing where this binding came from. Required for
    /// [`UntrustedProvenance`](UnknownLegacyStatus::UntrustedProvenance).
    AccountableProvenanceAttestation,
    /// Adjudication of which `coop_id` legitimately owns the target `EntityId`,
    /// because the reverse index disagrees. Required for
    /// [`ReverseMismatch`](UnknownLegacyStatus::ReverseMismatch); not
    /// auto-repairable.
    BindingDisambiguation,
    /// A corrected, well-formed cooperative `EntityId` target (plus accountable
    /// provenance for the corrected binding). Required for
    /// [`MalformedTarget`](UnknownLegacyStatus::MalformedTarget).
    CorrectedCooperativeTarget,
    /// Repair of the underlying store so the row can be read at all. Required for
    /// [`StorageError`](UnknownLegacyStatus::StorageError).
    StoreIntegrityRepair,
}

/// Per-`coop_id` detail. The original `coop_id` is preserved byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownLegacyEntry {
    /// The original flat `coop_id`, exactly as supplied.
    pub coop_id: String,
    /// The `EntityId` the forward index binds this `coop_id` to, when a binding
    /// exists and was readable. `None` for [`UnknownLegacyStatus::NotBound`] and
    /// an unreadable [`UnknownLegacyStatus::StorageError`].
    pub bound_entity_id: Option<EntityId>,
    /// The provenance observed on the binding, especially
    /// [`UnknownLegacy`](CoopEntityBindingProvenance::UnknownLegacy). `None` when
    /// there was no binding or it was unreadable.
    pub provenance_observed: Option<CoopEntityBindingProvenance>,
    /// Whether the target `EntityId`'s reverse index points back at this
    /// `coop_id` byte-for-byte. `false` when it disagrees, is absent, unreadable,
    /// or there is no binding.
    pub reverse_index_agrees: bool,
    /// Whether the bound `EntityId` re-parses as a well-formed cooperative
    /// identifier. `false` when malformed, non-cooperative, absent, or
    /// unreadable.
    pub target_is_well_formed_cooperative: bool,
    /// The mutually-exclusive classification outcome.
    pub status: UnknownLegacyStatus,
    /// The evidence class a repair would require. [`RepairEvidence::None`] for
    /// non-candidates.
    pub required_evidence: RepairEvidence,
    /// Human-readable reason the binding is not trusted (or the read failure).
    /// `None` for [`UnknownLegacyStatus::Trusted`] and
    /// [`UnknownLegacyStatus::NotBound`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Aggregate, read-only report over a set of `coop_id`s. Counts are mutually
/// exclusive and sum to `total`; `repair_candidates` is the sum of the four
/// candidate counts (a derived convenience, not a separate partition).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownLegacyReport {
    /// Total number of `coop_id`s inspected (`== entries.len()`).
    pub total: usize,
    /// Count of [`UnknownLegacyStatus::Trusted`].
    pub trusted: usize,
    /// Count of [`UnknownLegacyStatus::NotBound`].
    pub not_bound: usize,
    /// Count of [`UnknownLegacyStatus::UntrustedProvenance`].
    pub untrusted_provenance: usize,
    /// Count of [`UnknownLegacyStatus::ReverseMismatch`].
    pub reverse_mismatch: usize,
    /// Count of [`UnknownLegacyStatus::MalformedTarget`].
    pub malformed_target: usize,
    /// Count of [`UnknownLegacyStatus::StorageError`].
    pub storage_error: usize,
    /// Total repair candidates (`untrusted_provenance + reverse_mismatch +
    /// malformed_target + storage_error`). Derived; not part of the partition.
    pub repair_candidates: usize,
    /// Per-`coop_id` detail, in input order.
    pub entries: Vec<UnknownLegacyEntry>,
}

impl UnknownLegacyReport {
    /// Record one classified entry, bumping the matching counter, `total`, and
    /// `repair_candidates` when the status is a candidate.
    fn record(&mut self, entry: UnknownLegacyEntry) {
        match entry.status {
            UnknownLegacyStatus::Trusted => self.trusted += 1,
            UnknownLegacyStatus::NotBound => self.not_bound += 1,
            UnknownLegacyStatus::UntrustedProvenance => self.untrusted_provenance += 1,
            UnknownLegacyStatus::ReverseMismatch => self.reverse_mismatch += 1,
            UnknownLegacyStatus::MalformedTarget => self.malformed_target += 1,
            UnknownLegacyStatus::StorageError => self.storage_error += 1,
        }
        if entry.status.is_repair_candidate() {
            self.repair_candidates += 1;
        }
        self.total += 1;
        self.entries.push(entry);
    }
}

/// Whether `entity_id` re-parses as a well-formed cooperative [`EntityId`].
///
/// [`EntityId`] deserializes as a raw newtype, so a value from serde/backfill
/// data can carry a malformed slug whose type tag still reads `cooperative`.
/// Re-parsing via [`EntityId::from_str`] re-runs slug validation, matching the
/// map's own `validate_cooperative_entity_id` invariant.
fn is_well_formed_cooperative(entity_id: &EntityId) -> bool {
    EntityId::from_str(entity_id.as_str())
        .map(|parsed| parsed.is_cooperative())
        .unwrap_or(false)
}

/// Classify a single `coop_id` against the map using only read operations.
///
/// Read order: the provenance-aware binding first (an unreadable binding is a
/// fail-closed [`StorageError`](UnknownLegacyStatus::StorageError); a missing one
/// is [`NotBound`](UnknownLegacyStatus::NotBound)), then the target's reverse
/// index. Structural checks (reverse agreement, target well-formedness) are run
/// **independently of provenance**, and the outcome is chosen by descending
/// severity — a read failure, then a reverse mismatch, then a malformed target,
/// then untrusted provenance — so a trusted-but-corrupt row is never reported as
/// healthy. A binding grants no authority; this only observes names.
fn classify_one(coop_id: &str, map: &dyn CoopEntityMap) -> UnknownLegacyEntry {
    let entry = |status,
                 evidence,
                 bound_entity_id,
                 provenance_observed,
                 reverse_index_agrees,
                 target_is_well_formed_cooperative,
                 detail| UnknownLegacyEntry {
        coop_id: coop_id.to_string(),
        bound_entity_id,
        provenance_observed,
        reverse_index_agrees,
        target_is_well_formed_cooperative,
        status,
        required_evidence: evidence,
        detail,
    };

    // 1. Read the provenance-aware binding. Unreadable => fail-closed
    //    StorageError; absent => NotBound (coverage is a separate report).
    let binding = match map.binding_for_coop(coop_id) {
        Err(e) => {
            return entry(
                UnknownLegacyStatus::StorageError,
                RepairEvidence::StoreIntegrityRepair,
                None,
                None,
                false,
                false,
                Some(format!("binding read failed for coop_id {coop_id:?}: {e}")),
            );
        }
        Ok(None) => {
            return entry(
                UnknownLegacyStatus::NotBound,
                RepairEvidence::None,
                None,
                None,
                false,
                false,
                None,
            );
        }
        Ok(Some(b)) => b,
    };

    let entity_id = binding.entity_id;
    let provenance = binding.provenance;
    let target_ok = is_well_formed_cooperative(&entity_id);

    // 2. Read the target's reverse index; an unreadable reverse is itself a
    //    fail-closed StorageError.
    let (reverse_agrees, reverse_bad_detail) = match map.coop_for_entity(&entity_id) {
        Err(e) => {
            return entry(
                UnknownLegacyStatus::StorageError,
                RepairEvidence::StoreIntegrityRepair,
                Some(entity_id),
                Some(provenance),
                false,
                target_ok,
                Some(format!("reverse index read failed: {e}")),
            );
        }
        Ok(Some(rev)) if rev.as_str() == coop_id => (true, None),
        Ok(Some(rev)) => (
            false,
            Some(format!(
                "reverse index maps {entity_id} -> {rev:?}, not {coop_id:?} \
                 (cross-linked; a mapping grants no authority)"
            )),
        ),
        Ok(None) => (
            false,
            Some(format!(
                "no reverse index entry for {entity_id} (one-sided binding)"
            )),
        ),
    };

    // 3. Choose by descending severity. Structural problems (reverse, target)
    //    outrank provenance and apply even to trusted provenance.
    let (status, evidence, detail) = if !reverse_agrees {
        (
            UnknownLegacyStatus::ReverseMismatch,
            RepairEvidence::BindingDisambiguation,
            reverse_bad_detail,
        )
    } else if !target_ok {
        (
            UnknownLegacyStatus::MalformedTarget,
            RepairEvidence::CorrectedCooperativeTarget,
            Some(format!(
                "bound EntityId {entity_id} is not a well-formed cooperative identifier"
            )),
        )
    } else if !provenance.is_trusted_for_resolution() {
        (
            UnknownLegacyStatus::UntrustedProvenance,
            RepairEvidence::AccountableProvenanceAttestation,
            Some(
                "binding provenance is UnknownLegacy: untrusted for resolution \
                 (fail-closed until an accountable provenance re-establishes it)"
                    .to_string(),
            ),
        )
    } else {
        (UnknownLegacyStatus::Trusted, RepairEvidence::None, None)
    };

    entry(
        status,
        evidence,
        Some(entity_id),
        Some(provenance),
        reverse_agrees,
        target_ok,
        detail,
    )
}

/// Report the [`UnknownLegacy`](CoopEntityBindingProvenance::UnknownLegacy) /
/// untrusted repair candidates among a set of `coop_id`s, **read-only**.
///
/// Returns an [`UnknownLegacyReport`] with mutually-exclusive counts and a
/// per-`coop_id` detail list in input order. Performs **no writes**, **no
/// normalization**, and **no trust upgrade**, and leaves the map unchanged.
pub fn report_unknown_legacy(
    coop_ids: impl IntoIterator<Item = String>,
    map: &dyn CoopEntityMap,
) -> UnknownLegacyReport {
    let mut report = UnknownLegacyReport::default();
    for coop_id in coop_ids {
        report.record(classify_one(&coop_id, map));
    }
    report
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::coop_entity_map::{
        CoopEntityBinding, CoopEntityMap, CoopEntityMapError, InMemoryCoopEntityMap,
    };

    fn coop_eid(slug: &str) -> EntityId {
        EntityId::cooperative(slug).unwrap()
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// A read-only test double returning caller-controlled provenance-aware
    /// binding and reverse values, so the classifier can be exercised against
    /// states (untrusted-but-consistent, cross-linked reverse, malformed target,
    /// read failure) the atomic real maps cannot produce through their public
    /// API. Writing through it is a test failure.
    struct FakeMap {
        binding: Result<Option<CoopEntityBinding>, ()>,
        reverse: Result<Option<String>, ()>,
    }

    impl CoopEntityMap for FakeMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("report_unknown_legacy must never write through the map");
        }
        fn entity_for_coop(&self, _: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            match &self.binding {
                Ok(Some(b)) => Ok(Some(b.entity_id.clone())),
                Ok(None) => Ok(None),
                Err(()) => Err(CoopEntityMapError::Storage("forward unreadable".into())),
            }
        }
        fn coop_for_entity(&self, _: &EntityId) -> Result<Option<String>, CoopEntityMapError> {
            match &self.reverse {
                Ok(v) => Ok(v.clone()),
                Err(()) => Err(CoopEntityMapError::Storage("reverse unreadable".into())),
            }
        }
        fn binding_for_coop(
            &self,
            _: &str,
        ) -> Result<Option<CoopEntityBinding>, CoopEntityMapError> {
            match &self.binding {
                Ok(v) => Ok(v.clone()),
                Err(()) => Err(CoopEntityMapError::Storage("binding unreadable".into())),
            }
        }
    }

    /// A minimal map with **no provenance store**, exercising the fail-closed
    /// default `binding_for_coop` (which reports `UnknownLegacy`). Backed by a
    /// single consistent forward/reverse pair.
    struct NoProvenanceMap {
        coop_id: String,
        entity_id: EntityId,
    }
    impl CoopEntityMap for NoProvenanceMap {
        fn bind_resolved(&self, _: &str, _: &EntityId) -> Result<(), CoopEntityMapError> {
            unreachable!("report_unknown_legacy must never write through the map");
        }
        fn entity_for_coop(&self, coop_id: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Ok((coop_id == self.coop_id).then(|| self.entity_id.clone()))
        }
        fn coop_for_entity(
            &self,
            entity_id: &EntityId,
        ) -> Result<Option<String>, CoopEntityMapError> {
            Ok((entity_id == &self.entity_id).then(|| self.coop_id.clone()))
        }
        // Deliberately does NOT override binding_for_coop: it uses the trait
        // default, which reports UnknownLegacy (fail-closed).
    }

    fn trusted_binding(
        coop_id: &str,
        provenance: CoopEntityBindingProvenance,
    ) -> CoopEntityBinding {
        CoopEntityBinding {
            coop_id: coop_id.to_string(),
            entity_id: coop_eid(coop_id),
            provenance,
        }
    }

    // ---- Trust preservation -------------------------------------------------

    #[test]
    fn unknown_legacy_row_is_reported_untrusted_and_not_upgraded() {
        // A consistent binding whose provenance is UnknownLegacy: structurally
        // fine, but never trusted, and never reclassified as trusted.
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: "food-coop".into(),
                entity_id: coop_eid("food-coop"),
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            })),
            reverse: Ok(Some("food-coop".into())),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.total, 1);
        assert_eq!(report.untrusted_provenance, 1);
        assert_eq!(report.trusted, 0);
        assert_eq!(report.repair_candidates, 1);
        let e = &report.entries[0];
        assert_eq!(e.status, UnknownLegacyStatus::UntrustedProvenance);
        assert_eq!(
            e.provenance_observed,
            Some(CoopEntityBindingProvenance::UnknownLegacy),
            "provenance must be reported as-is, never upgraded"
        );
        assert_eq!(
            e.required_evidence,
            RepairEvidence::AccountableProvenanceAttestation
        );
        assert!(e.reverse_index_agrees);
        assert!(e.target_is_well_formed_cooperative);
    }

    #[test]
    fn missing_provenance_reads_unknown_legacy_and_is_reported_fail_closed() {
        // A map without a provenance store: the default binding_for_coop reports
        // UnknownLegacy, which must be treated as an untrusted repair candidate.
        let map = NoProvenanceMap {
            coop_id: "food-coop".into(),
            entity_id: coop_eid("food-coop"),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.untrusted_provenance, 1);
        let e = &report.entries[0];
        assert_eq!(e.status, UnknownLegacyStatus::UntrustedProvenance);
        assert_eq!(
            e.provenance_observed,
            Some(CoopEntityBindingProvenance::UnknownLegacy)
        );
        assert!(e.status.is_repair_candidate());
    }

    // ---- Structural integrity (orthogonal to provenance) --------------------

    #[test]
    fn reverse_mismatch_is_reported_unsafe_even_with_trusted_provenance() {
        // Trusted provenance, well-formed target, but the reverse index points
        // at a DIFFERENT coop_id: ambiguous/unsafe, takes precedence.
        let map = FakeMap {
            binding: Ok(Some(trusted_binding(
                "food-coop",
                CoopEntityBindingProvenance::Activation,
            ))),
            reverse: Ok(Some("other-coop".into())),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.reverse_mismatch, 1);
        assert_eq!(report.trusted, 0);
        let e = &report.entries[0];
        assert_eq!(e.status, UnknownLegacyStatus::ReverseMismatch);
        assert_eq!(e.required_evidence, RepairEvidence::BindingDisambiguation);
        assert!(!e.reverse_index_agrees);
        assert!(e.detail.as_ref().unwrap().contains("other-coop"));
    }

    #[test]
    fn absent_reverse_is_reported_reverse_mismatch() {
        let map = FakeMap {
            binding: Ok(Some(trusted_binding(
                "food-coop",
                CoopEntityBindingProvenance::Activation,
            ))),
            reverse: Ok(None),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.reverse_mismatch, 1);
        assert_eq!(
            report.entries[0].status,
            UnknownLegacyStatus::ReverseMismatch
        );
    }

    #[test]
    fn malformed_cooperative_target_is_reported_unsafe() {
        // A target that tags as cooperative but has an invalid slug (only
        // constructible via serde, since EntityId::cooperative validates).
        let malformed: EntityId =
            serde_json::from_str("\"entity:icn:cooperative:BAD slug\"").unwrap();
        assert!(malformed.is_cooperative(), "tags as cooperative");
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: "food-coop".into(),
                entity_id: malformed.clone(),
                provenance: CoopEntityBindingProvenance::Activation,
            })),
            reverse: Ok(Some("food-coop".into())),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.malformed_target, 1);
        let e = &report.entries[0];
        assert_eq!(e.status, UnknownLegacyStatus::MalformedTarget);
        assert_eq!(
            e.required_evidence,
            RepairEvidence::CorrectedCooperativeTarget
        );
        assert!(!e.target_is_well_formed_cooperative);
    }

    #[test]
    fn non_cooperative_target_is_reported_unsafe() {
        // A well-formed, but non-cooperative (community) target is not a valid
        // cooperative binding.
        let community = EntityId::community("some-community").unwrap();
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: "food-coop".into(),
                entity_id: community.clone(),
                provenance: CoopEntityBindingProvenance::OperatorBackfill,
            })),
            reverse: Ok(Some("food-coop".into())),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.malformed_target, 1);
        assert!(!report.entries[0].target_is_well_formed_cooperative);
    }

    #[test]
    fn storage_error_is_reported_fail_closed() {
        let map = FakeMap {
            binding: Err(()),
            reverse: Ok(None),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.storage_error, 1);
        let e = &report.entries[0];
        assert_eq!(e.status, UnknownLegacyStatus::StorageError);
        assert_eq!(e.required_evidence, RepairEvidence::StoreIntegrityRepair);
        assert!(e.status.is_repair_candidate());
    }

    #[test]
    fn reverse_read_error_is_reported_storage_error() {
        let map = FakeMap {
            binding: Ok(Some(trusted_binding(
                "food-coop",
                CoopEntityBindingProvenance::Activation,
            ))),
            reverse: Err(()),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.storage_error, 1);
        assert_eq!(report.entries[0].status, UnknownLegacyStatus::StorageError);
    }

    // ---- Clean trusted rows are NOT candidates ------------------------------

    #[test]
    fn clean_trusted_rows_are_not_repair_candidates() {
        for provenance in [
            CoopEntityBindingProvenance::Activation,
            CoopEntityBindingProvenance::OperatorBackfill,
            CoopEntityBindingProvenance::Surrogate,
            CoopEntityBindingProvenance::GovernanceReceipt {
                receipt_id: "receipt-abc".into(),
            },
        ] {
            let map = FakeMap {
                binding: Ok(Some(trusted_binding("food-coop", provenance.clone()))),
                reverse: Ok(Some("food-coop".into())),
            };
            let report = report_unknown_legacy(ids(&["food-coop"]), &map);
            assert_eq!(
                report.trusted, 1,
                "provenance {provenance:?} must be trusted"
            );
            assert_eq!(report.repair_candidates, 0);
            let e = &report.entries[0];
            assert_eq!(e.status, UnknownLegacyStatus::Trusted);
            assert_eq!(e.required_evidence, RepairEvidence::None);
            assert!(!e.status.is_repair_candidate());
            assert!(e.detail.is_none());
        }
    }

    #[test]
    fn unbound_coop_id_is_not_bound_not_a_candidate() {
        let map = InMemoryCoopEntityMap::new();
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.not_bound, 1);
        assert_eq!(report.repair_candidates, 0);
        let e = &report.entries[0];
        assert_eq!(e.status, UnknownLegacyStatus::NotBound);
        assert_eq!(e.bound_entity_id, None);
    }

    // ---- Invariants, byte-for-byte, and no-write ----------------------------

    #[test]
    fn counts_partition_total_and_repair_candidates_sum() {
        // Build a mixed batch through the real in-memory map plus fakes are not
        // mixable in one map; instead assert the invariant on each report.
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: "food-coop".into(),
                entity_id: coop_eid("food-coop"),
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            })),
            reverse: Ok(Some("food-coop".into())),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        assert_eq!(report.total, report.entries.len());
        assert_eq!(
            report.trusted
                + report.not_bound
                + report.untrusted_provenance
                + report.reverse_mismatch
                + report.malformed_target
                + report.storage_error,
            report.total
        );
        assert_eq!(
            report.untrusted_provenance
                + report.reverse_mismatch
                + report.malformed_target
                + report.storage_error,
            report.repair_candidates
        );
    }

    #[test]
    fn coop_id_is_preserved_byte_for_byte() {
        // A legacy, non-mappable coop_id shape must be echoed exactly.
        let legacy = "coop:11111111-2222-3333-4444-555555555555";
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: legacy.into(),
                entity_id: coop_eid("surrogate-slug"),
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            })),
            reverse: Ok(Some(legacy.into())),
        };
        let report = report_unknown_legacy(vec![legacy.to_string()], &map);
        assert_eq!(report.entries[0].coop_id, legacy);
    }

    #[test]
    fn empty_input_returns_all_zero_counts() {
        let map = InMemoryCoopEntityMap::new();
        let report = report_unknown_legacy(Vec::<String>::new(), &map);
        assert_eq!(report.total, 0);
        assert_eq!(report.repair_candidates, 0);
        assert!(report.entries.is_empty());
    }

    #[test]
    fn report_writes_nothing_through_the_map() {
        // FakeMap::bind_resolved is unreachable!(); a completed report proves the
        // classifier took no write path.
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: "food-coop".into(),
                entity_id: coop_eid("food-coop"),
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            })),
            reverse: Ok(Some("food-coop".into())),
        };
        let _ = report_unknown_legacy(ids(&["food-coop", "food-coop"]), &map);
    }

    // ---- JSON stability -----------------------------------------------------

    #[test]
    fn json_is_stable_and_machine_readable() {
        let map = FakeMap {
            binding: Ok(Some(CoopEntityBinding {
                coop_id: "food-coop".into(),
                entity_id: coop_eid("food-coop"),
                provenance: CoopEntityBindingProvenance::UnknownLegacy,
            })),
            reverse: Ok(Some("food-coop".into())),
        };
        let report = report_unknown_legacy(ids(&["food-coop"]), &map);
        let json = serde_json::to_string(&report).unwrap();
        // Machine-readable snake_case status + action.
        assert!(
            json.contains("\"status\":\"untrusted_provenance\""),
            "got: {json}"
        );
        assert!(
            json.contains("\"required_evidence\":\"accountable_provenance_attestation\""),
            "got: {json}"
        );
        // Round-trips losslessly.
        let back: UnknownLegacyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }
}
