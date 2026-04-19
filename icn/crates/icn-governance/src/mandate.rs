//! Constitutional mandate — ADR-0014.
//!
//! A [`Mandate`] is the constitutional mediation layer between a governance
//! decision and the execution of its effects. It is the object the repo was
//! missing before ADR-0014 and the one that makes the full chain
//!
//! ```text
//!     Charter → Decision → Mandate → Action → Receipt → Evidence
//! ```
//!
//! expressible end-to-end.
//!
//! # What a Mandate *is*
//!
//! An accepted decision has produced a bounded institutional authorization
//! to carry out one or more classes of action, optionally assigning an
//! executor, a deadline, and one or more authority grants. Executing the
//! mandated action under the mandate is what makes the effect legitimate
//! — not merely the fact that the decision was recorded.
//!
//! `Mandate` is also, in its own right, a **first-class institutional-memory
//! record**: the durable answer to the question "under whose authority,
//! grounded in which decision, was this action permitted?" That answer
//! must survive personnel turnover, platform upgrades, and the passage of
//! time.
//!
//! # What a Mandate is *not*
//!
//! A `Mandate` is deliberately distinct from the adjacent forms already
//! in the repo:
//!
//! - **Not a [`crate::delegation::Delegation`].** Delegation is strictly
//!   vote delegation (Representation authority, proposal-scoped). A
//!   mandate typically binds Execution authority.
//! - **Not a [`crate::structure::RoleAssignment`].** A role assignment
//!   places a DID in an ongoing structural position (e.g. Treasurer). A
//!   mandate is issued per-decision (or per-charter-clause) and is
//!   bounded.
//! - **Not a [`crate::structure::Structure`] of kind `Office`.** An office
//!   is an institutional seat; a mandate is an authorization to act, often
//!   by a seat-holder, but distinct from the seat itself.
//! - **Not an `InstitutionalEffectRecord`.** That record (see
//!   `apps/governance::institutional_effect`) is *evidence* that a
//!   translation from accepted payload to kernel effect occurred. Mandate
//!   is *authority* to perform that translation in the first place.
//! - **Not a [`crate::proof::GovernanceDecisionReceipt`].** The receipt
//!   records that a decision was reached under canonical process. The
//!   mandate binds that decision to a bounded authorization.
//! - **Not a federation `Agreement`.** An agreement is a bilateral or
//!   multilateral compact between entities. A mandate is an internal
//!   authorization within an entity (or from an entity to a named
//!   executor) to carry out a decision.
//! - **Not a [`crate::charter::Charter`].** The charter is the sovereign
//!   constitutional source. A mandate is an authorization grounded in
//!   the charter's process.
//!
//! # Layer placement
//!
//! Like the rest of the ADR-0014 types, `Mandate` lives at the governance
//! app layer and must not be imported by kernel crates. The Meaning
//! Firewall remains intact.
//!
//! # Behavioral status
//!
//! **Types-first, behavior-neutral.** This module introduces no executor
//! gating and no dispatcher wiring. Later tranches consult `Mandate` when
//! they land execution enforcement; this tranche only names the object.
//!
//! See `docs/adr/ADR-0014-constitutional-object-model.md` for full
//! semantics.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::authority::{AuthorityGrantId, DecisionProvenance};
use crate::proof::Hash;
use crate::Timestamp;
use icn_identity::Did;

/// Unique identifier for a [`Mandate`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MandateId(pub Uuid);

impl MandateId {
    /// Generate a fresh v4 identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for MandateId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MandateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lifecycle status of a [`Mandate`].
///
/// Transitions follow the ADR-0014 state machine
/// `Pending → InProgress → Discharged | Expired | Revoked`. This module
/// only names the states; transition enforcement is future work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MandateStatus {
    /// Issued but no execution attempt has started.
    Pending,
    /// Execution has begun; at least one effect has been dispatched
    /// under this mandate.
    InProgress,
    /// All authorized effects have been applied; the mandate is closed.
    Discharged,
    /// Deadline passed without full discharge. Recorded as institutional
    /// memory that the authorization existed and was not fully exercised.
    Expired,
    /// Explicit revocation by the grantor entity or by superseding
    /// governance decision. Blocks further execution even if prior
    /// effects have already landed.
    Revoked,
}

/// A `Mandate` — bounded institutional authorization to carry out the
/// effects of a specific governance decision.
///
/// See the module-level documentation for full distinctness and layer
/// placement. See ADR-0014 for frozen semantics.
///
/// # Construction
///
/// Use [`Mandate::new`] for standard construction: it auto-generates a
/// fresh [`MandateId`] and initializes status to
/// [`MandateStatus::Pending`]. Callers must supply the decision
/// provenance, payload hash, and at least one [`AuthorityGrantId`]. A
/// mandate with an empty `grants` vector is not a valid constitutional
/// authorization; downstream code minting mandates should refuse that
/// case (the ADR-0014 rule that authority must be bounded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mandate {
    /// Stable identifier for the mandate.
    pub id: MandateId,
    /// Provenance back to the originating governance decision.
    pub decision: DecisionProvenance,
    /// Hash of the accepted proposal payload; binds the mandate to the
    /// concrete content of the decision, not just its identity.
    pub payload_hash: Hash,
    /// References to one or more [`crate::authority::AuthorityGrant`]s
    /// that together confer the authority this mandate relies on.
    /// Typically Execution-class, but a mandate may bundle
    /// Representation or Attestation grants if the decision names them.
    pub grants: Vec<AuthorityGrantId>,
    /// The party named to carry out the action. `None` when the mandate
    /// is self-dispatching (the decision body itself executes) or is
    /// addressed to a role rather than a specific DID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<Did>,
    /// Unix timestamp after which the mandate expires without further
    /// execution. `None` means charter- or decision-bounded without a
    /// distinct deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<Timestamp>,
    /// Lifecycle status. Initialized to [`MandateStatus::Pending`] at
    /// construction.
    pub status: MandateStatus,
    /// Unix timestamp when the mandate was issued.
    pub issued_at: Timestamp,
}

impl Mandate {
    /// Construct a new mandate with a fresh identifier and
    /// [`MandateStatus::Pending`].
    pub fn new(
        decision: DecisionProvenance,
        payload_hash: Hash,
        grants: Vec<AuthorityGrantId>,
        executor: Option<Did>,
        deadline: Option<Timestamp>,
        issued_at: Timestamp,
    ) -> Self {
        Self {
            id: MandateId::new(),
            decision,
            payload_hash,
            grants,
            executor,
            deadline,
            status: MandateStatus::Pending,
            issued_at,
        }
    }

    /// Return `true` if `now >= deadline` (when a deadline is set).
    ///
    /// This is a time-check helper, **not** an enforcement gate. Callers
    /// that must observe expiration semantics should also consult
    /// [`Self::status`] (a mandate may have been explicitly revoked or
    /// already discharged, which overrides deadline expiry).
    pub fn is_past_deadline(&self, now: Timestamp) -> bool {
        matches!(self.deadline, Some(d) if now >= d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{AuthorityGrantId, DecisionProvenance};

    fn did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    fn sample_decision() -> DecisionProvenance {
        DecisionProvenance {
            proposal_id: "prop-42".into(),
            decision_hash: [7u8; 32],
        }
    }

    #[test]
    fn mandate_new_is_pending() {
        let m = Mandate::new(
            sample_decision(),
            [9u8; 32],
            vec![AuthorityGrantId::new()],
            Some(did(1)),
            Some(1000),
            100,
        );
        assert_eq!(m.status, MandateStatus::Pending);
    }

    #[test]
    fn mandate_ids_are_distinct_for_same_decision() {
        let d = sample_decision();
        let p = [0u8; 32];
        let g = AuthorityGrantId::new();
        let m1 = Mandate::new(d.clone(), p, vec![g.clone()], None, None, 0);
        let m2 = Mandate::new(d.clone(), p, vec![g.clone()], None, None, 0);
        assert_ne!(m1.id, m2.id);
        assert_eq!(m1.decision, m2.decision);
    }

    #[test]
    fn mandate_deadline_check() {
        let m = Mandate::new(
            sample_decision(),
            [0u8; 32],
            vec![AuthorityGrantId::new()],
            None,
            Some(500),
            100,
        );
        assert!(!m.is_past_deadline(499));
        assert!(m.is_past_deadline(500));
        assert!(m.is_past_deadline(1000));

        let m_no_deadline = Mandate::new(
            sample_decision(),
            [0u8; 32],
            vec![AuthorityGrantId::new()],
            None,
            None,
            100,
        );
        assert!(!m_no_deadline.is_past_deadline(u64::MAX));
    }

    #[test]
    fn mandate_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&MandateStatus::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&MandateStatus::Discharged).unwrap(),
            "\"discharged\""
        );
        let round: MandateStatus = serde_json::from_str("\"revoked\"").unwrap();
        assert_eq!(round, MandateStatus::Revoked);
    }

    #[test]
    fn mandate_roundtrip_serde() {
        // Use seed=1 because `Did::from_anchor_id` does not validate that
        // arbitrary bytes decompress to a valid Ed25519 point, but
        // `Did`'s deserializer does. Seed 1 is known to round-trip.
        let m = Mandate::new(
            sample_decision(),
            [3u8; 32],
            vec![AuthorityGrantId::new(), AuthorityGrantId::new()],
            Some(did(1)),
            Some(2000),
            1500,
        );
        let json = serde_json::to_string(&m).unwrap();
        let back: Mandate = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
