//! Custodian-owned admission requirements for resource pools (experimental).
//!
//! A pool may durably declare that every reservation against it must be admitted by one specific,
//! versioned admission policy. The store enforces that declaration at its own commit boundary.
//!
//! # Trust model
//!
//! * **Who decides which policy applies:** the pool record, written by the custodian through
//!   [`crate::LocalResourceReservations::initialize_pool_v1`]. A request cannot name, weaken or
//!   omit it. The pool's opaque `policy_hash` is *not* an admission class and is never read as one.
//! * **Who verifies:** only a [`CustodianAdmissionVerifierV0`] the custodian installed when it
//!   built the store ([`crate::LocalResourceReservations::with_admission`]). A caller of
//!   `reserve_*` holds `&self` and cannot add, replace or select a verifier. Any crate can
//!   *implement* the trait; that is not a bypass, because an implementation the custodian did not
//!   install is never consulted.
//! * **What the verifier returns:** a `CheckpointedPreflight`, which only
//!   `icn_authz::effect_v0::preflight_checkpointed` can construct. The verifier chooses the
//!   frontier and rule resolver it feeds into that function from its own custodian-held trust
//!   state, so the caller supplies untrusted evidence only.
//! * **In-process only.** The admitted result never leaves this process. A remote custodian must
//!   re-verify raw evidence with its own installed verifier; a Rust value is not proof across a
//!   process boundary, and no attestation protocol is defined here.
//! * **Pool creation is not capability-gated (known limit).** `initialize_pool` and
//!   `initialize_pool_v1` take `&self`, like `reserve_*`, so any holder of the store can create a
//!   pool id first, including as a generic pool. Creation never overwrites
//!   (`PoolAlreadyExists`), so a custodian must treat that error as a conflict and compare
//!   [`crate::LocalResourceReservations::inspect_pool_record`]'s requirement with its own, never
//!   as idempotent success. A custodian-only creation capability is follow-up work.
//! * **Not a storage-integrity mechanism.** Anyone with raw write access to the underlying
//!   database can rewrite or delete a pool record, including replacing a policy pool with a
//!   generic one. The API gate prevents the *caller* from choosing weaker admission; it does not
//!   protect against an adversary who already owns the store.
//!
//! The store never interprets the verifier's `supplement`, its recognition rules or institutional
//! meaning (Meaning Firewall). It compares a policy identity, a version and a grade.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use icn_authz::effect_v0::{CheckpointedEvidenceV0, CheckpointedPreflight};

/// Identity and version of a custodian admission policy. Opaque to the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdmissionPolicyRefV0 {
    pub id: [u8; 32],
    pub version: u32,
}

/// Generic strength of an admission, ordered weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdmissionGradeV0 {
    /// Valid only for rehearsal: never production authority.
    Rehearsal,
    /// Admissible as production authority.
    Production,
}

impl AdmissionGradeV0 {
    pub(crate) fn tag(self) -> u8 {
        match self {
            AdmissionGradeV0::Rehearsal => 1,
            AdmissionGradeV0::Production => 2,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(AdmissionGradeV0::Rehearsal),
            2 => Some(AdmissionGradeV0::Production),
            _ => None,
        }
    }
}

/// What a pool requires of every reservation against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionRequirementV1 {
    /// Explicitly generic: the caller supplies the frontier and rule resolver (the V0 contract).
    CallerResolved,
    /// Only the custodian-installed verifier for `policy`, at `min_grade` or stronger.
    Policy {
        policy: AdmissionPolicyRefV0,
        min_grade: AdmissionGradeV0,
    },
}

/// What a successful verification was performed under. Minted only inside this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmittedUnder {
    CallerResolved,
    Policy {
        policy: AdmissionPolicyRefV0,
        grade: AdmissionGradeV0,
    },
}

impl AdmittedUnder {
    /// The single commit-time comparison between authoritative pool state and the admission.
    /// Evaluated inside the reservation transaction against the pool record read there.
    pub(crate) fn satisfies(self, required: AdmissionRequirementV1) -> Result<(), String> {
        match (required, self) {
            (AdmissionRequirementV1::CallerResolved, AdmittedUnder::CallerResolved) => Ok(()),
            (AdmissionRequirementV1::CallerResolved, AdmittedUnder::Policy { .. }) => {
                Err("pool does not declare a custodian admission policy".into())
            }
            (AdmissionRequirementV1::Policy { .. }, AdmittedUnder::CallerResolved) => {
                Err("pool requires custodian admission; caller-resolved rules refused".into())
            }
            (
                AdmissionRequirementV1::Policy { policy, min_grade },
                AdmittedUnder::Policy {
                    policy: used,
                    grade,
                },
            ) => {
                if used != policy {
                    Err("admission was verified under a different policy or version".into())
                } else if grade < min_grade {
                    Err("admission grade is below the pool's minimum".into())
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// A custodian-installed verifier's successful result.
pub struct VerifiedAdmissionV0 {
    /// Non-consuming preflight produced by `preflight_checkpointed` under custodian-held trust.
    pub preflight: CheckpointedPreflight,
    /// The grade the verifier attests for this admission.
    pub grade: AdmissionGradeV0,
}

/// A verifier's refusal. The text is diagnostic only; the store never branches on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRefusalV0(pub String);

/// A custodian-installed admission policy implementation.
///
/// Implementations must take every trust input (authority context, frontier, rules, recognition
/// requirements) from their own custodian-supplied state, treat `evidence` and `supplement` as
/// untrusted, and never consume an operation.
pub trait CustodianAdmissionVerifierV0: Send + Sync {
    /// The policy this verifier implements. Registration is keyed by it.
    fn policy(&self) -> AdmissionPolicyRefV0;

    /// Verify untrusted evidence for `target` at `now` without consuming anything.
    ///
    /// On the custodian path ([`crate::LocalResourceReservations::reserve_admitted`]) `now` is
    /// always the installed [`CustodianClockV0`]'s reading, never a caller value.
    fn preflight(
        &self,
        evidence: &CheckpointedEvidenceV0,
        supplement: &dyn Any,
        target: [u8; 32],
        now: u64,
    ) -> Result<VerifiedAdmissionV0, AdmissionRefusalV0>;
}

/// The custodian's source of time for custodian-policy admission (Part 8 F1, experimental).
///
/// # Unit
///
/// [`Self::now`] returns **unix seconds**: whole seconds since 1970-01-01T00:00:00Z, ignoring leap
/// seconds. That is the unit in which an authorization's `not_after` is compared on this path, so
/// evidence must state `not_after` in unix seconds to be judged correctly here.
///
/// # What this does and does not guarantee
///
/// Installing a clock moves the *choice* of admission time from the caller of
/// [`crate::LocalResourceReservations::reserve_admitted`] to the custodian: a caller can no longer
/// name the time at which its own evidence is judged. It does **not** make the time correct. The
/// unit specifies an interface, not integrity: a wrong, skewed, rolled-back or attacker-controlled
/// host clock gives wrong admission decisions, and nothing here detects that. Nor is the reading
/// monotonic: successive calls may go backwards, and the store keeps no high-water mark across
/// calls or restarts. Trustworthy time (a monotonic floor, trusted time sources) is out of scope.
///
/// Implementations must be cheap, must not panic, and must not consult caller-controlled input.
///
/// # Failing closed
///
/// A clock that cannot determine the time (for example a host clock that reads before the epoch)
/// returns **`None`**. [`crate::LocalResourceReservations::reserve_admitted`] then refuses with
/// [`crate::ReservationError::AdmissionUnavailable`] before reading the pool, calling a verifier
/// or opening a transaction, so nothing is consumed, whatever the evidence's `not_after`.
///
/// `None` is the only failure signal. Every `Some(t)` is judged as a real time `t`: `Some(0)` is
/// the epoch (it admits any authorization not yet expired then), and `Some(u64::MAX)` is the far
/// future (it refuses every authorization except one whose `not_after` is itself `u64::MAX`). So
/// never map a failure to a substitute reading, such as `unwrap_or_default()` (`Some(0)`, which
/// fails open). The direct host form is already correct:
/// `SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())`.
pub trait CustodianClockV0: Send + Sync {
    /// The current time in unix seconds, or `None` if it cannot be determined.
    fn now(&self) -> Option<u64>;
}

/// Error installing a verifier or the clock.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionRegistryError {
    #[error("a verifier for this admission policy and version is already installed")]
    Duplicate,
    #[error("a custodian clock is already installed")]
    DuplicateClock,
}

/// The custodian's verifiers, keyed by exact policy and version, and its admission clock.
#[derive(Default, Clone)]
pub struct AdmissionRegistryV0 {
    verifiers: BTreeMap<AdmissionPolicyRefV0, Arc<dyn CustodianAdmissionVerifierV0>>,
    clock: Option<Arc<dyn CustodianClockV0>>,
}

impl AdmissionRegistryV0 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install one verifier. A second verifier for the same policy and version is refused rather
    /// than silently replacing the first.
    pub fn install(
        &mut self,
        verifier: Arc<dyn CustodianAdmissionVerifierV0>,
    ) -> Result<(), AdmissionRegistryError> {
        let policy = verifier.policy();
        if self.verifiers.contains_key(&policy) {
            return Err(AdmissionRegistryError::Duplicate);
        }
        self.verifiers.insert(policy, verifier);
        Ok(())
    }

    /// Install the custodian clock that custodian-policy admission reads its time from. A second
    /// clock is refused rather than silently replacing the first. Without one, every
    /// [`crate::LocalResourceReservations::reserve_admitted`] call fails closed.
    pub fn install_clock(
        &mut self,
        clock: Arc<dyn CustodianClockV0>,
    ) -> Result<(), AdmissionRegistryError> {
        if self.clock.is_some() {
            return Err(AdmissionRegistryError::DuplicateClock);
        }
        self.clock = Some(clock);
        Ok(())
    }

    pub(crate) fn get(
        &self,
        policy: &AdmissionPolicyRefV0,
    ) -> Option<&Arc<dyn CustodianAdmissionVerifierV0>> {
        self.verifiers.get(policy)
    }

    pub(crate) fn clock(&self) -> Option<&Arc<dyn CustodianClockV0>> {
        self.clock.as_ref()
    }
}

impl std::fmt::Debug for AdmissionRegistryV0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmissionRegistryV0")
            .field("policies", &self.verifiers.keys().collect::<Vec<_>>())
            .field("clock_installed", &self.clock.is_some())
            .finish()
    }
}
