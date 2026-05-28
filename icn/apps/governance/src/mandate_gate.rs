//! MandateGate — the app-side, act-time authority resolver (#1868 step 6).
//!
//! A capability scope proves *technical permission* ("the bearer may call
//! this class of route"). It does **not** prove *institutional authority*
//! ("the institution authorized this actor to perform this act on this
//! target"). The kernel enforces the opaque capability string; the
//! [`MandateGate`] is the app-side gate that, at the moment a handler
//! performs a high/medium-blast act, resolves whether a *valid mandate*
//! authorizes **this actor** for **this act** on **this target** at **this
//! time** — and returns a [`MandateGrant`] reference the handler can later
//! record in a receipt.
//!
//! See `docs/design/governance/mandate-gate-design.md` (#1925) and
//! ADR-0014 for the frozen semantics. This module is the step-6 slice:
//! the trait, the request/response/rejection types, and a default resolver
//! over [`GovernanceReceiptBackend`], with unit tests.
//!
//! # Layer placement
//!
//! MandateGate is **app-side**, never kernel. The kernel never sees
//! [`MandateAct`], [`MandateTarget`], or the resolution logic — it
//! continues to enforce only opaque capability strings and constraint
//! sets. This preserves the Meaning Firewall.
//!
//! # Not wired yet (step-6 scope)
//!
//! Nothing calls this gate in step 6. Wiring `require()` into handlers,
//! adding the `GovernanceContext` field plus its production startup guard,
//! and recording the [`MandateGrant`] hash in receipts are later steps
//! (7 and the receipt-body schema step). This module is purely additive
//! and behavior-neutral.
//!
//! # Composability (deliberate)
//!
//! Capability scope (technical permission), membership standing
//! (`check_domain_membership`), mandate validity (this gate), and
//! suspension (the existing async `suspension_checker`) stay **separate,
//! composable checks** — not one god-auth function. The gate owns only
//! mandate validity. Because the existing `suspension_checker` is *async*
//! and this gate is *synchronous*, suspension is **not** consulted inside
//! the gate; the handler keeps its existing composed suspension check. The
//! [`MandateRejection::Suspended`] variant exists so the eventual HTTP
//! surface can render that reason when the handler's own check fires.

use std::sync::Arc;

use icn_governance::proof::Hash;
use icn_governance::{
    AuthorityGrant, GovernanceDomainId, Grantee, Mandate, MandateId, MandateStatus, ProposalId,
    StructureId, Timestamp,
};
use icn_identity::Did;

use crate::receipt_backend::GovernanceReceiptBackend;

/// A finite, named institutional act. Distinct variants per high/medium-blast
/// act; more are added as handlers are wired (steps 7/8).
///
/// `CastVote` and `CloseProposal` are deliberately distinct (different target
/// and semantics; do not collapse — decomposition §12 Q4).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MandateAct {
    /// Activate a charter (highest blast radius).
    ActivateCharter,
    /// Add a member to a governance domain.
    AddDomainMember,
    /// Remove a member from a governance domain.
    RemoveDomainMember,
    /// Close a proposal's voting period and finalize the outcome.
    CloseProposal,
    /// Cast a vote on a proposal.
    CastVote,
    /// Appoint a steward.
    AppointSteward,
    /// Remove a steward.
    RemoveSteward,
    /// Join a federation network.
    JoinFederation,
    /// Leave a federation network.
    LeaveFederation,
}

/// The subject an act operates on.
///
/// `Federation` carries a bare `String`: there is **no `FederationId` type**
/// in the workspace — federation identifiers are raw strings inside
/// `FederationProposal` (`icn-governance` `proposal.rs`). `Role` uses the
/// existing [`StructureId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MandateTarget {
    /// A governance domain, keyed by its [`GovernanceDomainId`].
    Domain(GovernanceDomainId),
    /// A specific proposal.
    Proposal(ProposalId),
    /// A role seat in a structure, held by a specific DID.
    Role {
        /// The structure the role belongs to.
        structure_id: StructureId,
        /// The DID holding (or to hold) the role.
        holder: Did,
    },
    /// A federation network, keyed by its raw string identifier.
    Federation(String),
}

/// The tuple a handler presents to the gate at act time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MandateRequest {
    /// The actor attempting the act.
    pub actor: Did,
    /// The governance domain the act is scoped to.
    pub domain: GovernanceDomainId,
    /// The institutional act being attempted.
    pub act: MandateAct,
    /// The subject of the act.
    pub target: MandateTarget,
    /// Act time, Unix seconds (aligned with mandate/grant `valid_*` units).
    pub at: Timestamp,
}

/// A reference to the mandate that authorized an act — **not** a new durable
/// record. It points at the existing [`Mandate`] and carries enough for a
/// handler to (eventually) record a stable reference in a receipt and for a
/// surface to render the authorization.
///
/// Note: the receipt-body wire shape (`MandateGrantRef`) and any stable hash
/// over this reference are intentionally **out of step-6 scope** — that work
/// belongs to the receipt-body schema step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MandateGrant {
    /// The authorizing mandate.
    pub mandate_id: MandateId,
    /// The decision the mandate is grounded in.
    pub decision_hash: Hash,
    /// The act the gate authorized.
    pub act: MandateAct,
    /// The target the gate authorized the act against.
    pub target: MandateTarget,
    /// Act time recorded at resolution.
    pub granted_at: Timestamp,
}

/// Structured reason an act was refused, for the surface to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MandateRejection {
    /// No authorizing mandate could be resolved (includes unbound,
    /// empty-grant "pending-grants" mandates, which carry no authority).
    NoMandate,
    /// The mandate (or the matched grant) is past its validity.
    Expired,
    /// A mandate was found but does not authorize this target.
    WrongTarget,
    /// The actor is not a grantee of any attached authority grant.
    WrongActor,
    /// The actor is suspended (produced by the handler's composed check,
    /// not by the synchronous gate — see module docs).
    Suspended,
    /// The mandate's authority has been withdrawn (revoked or discharged).
    Revoked,
}

impl MandateRejection {
    /// Stable, machine-readable reason code (snake_case) for surfaces and the
    /// eventual HTTP body.
    pub fn reason_code(&self) -> &'static str {
        match self {
            MandateRejection::NoMandate => "no_mandate",
            MandateRejection::Expired => "expired",
            MandateRejection::WrongTarget => "wrong_target",
            MandateRejection::WrongActor => "wrong_actor",
            MandateRejection::Suspended => "suspended",
            MandateRejection::Revoked => "revoked",
        }
    }
}

impl std::fmt::Display for MandateRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason_code())
    }
}

/// The error surface of [`MandateGate::require`].
///
/// This deliberately separates an **authorization rejection** (a client-level
/// verdict → 403 once wired) from an **infrastructure failure** (the backend
/// returned `Err` → server-level, fail-closed). A backend failure must never
/// be silently collapsed into [`MandateRejection::NoMandate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MandateGateError {
    /// The actor is not authorized for this act/target.
    Rejected(MandateRejection),
    /// A backend read failed; the gate fails closed.
    Backend(String),
}

impl std::fmt::Display for MandateGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MandateGateError::Rejected(r) => write!(f, "mandate rejected: {r}"),
            MandateGateError::Backend(e) => write!(f, "mandate gate backend error: {e}"),
        }
    }
}

impl std::error::Error for MandateGateError {}

#[inline]
fn rejected(reason: MandateRejection) -> MandateGateError {
    MandateGateError::Rejected(reason)
}

/// Map a non-live [`MandateStatus`] to its rejection, or `None` if live.
///
/// `Discharged` maps to [`MandateRejection::Revoked`]: a discharged mandate has
/// spent its authority and no longer authorizes acts; the step-6 rejection
/// enum has no distinct `Discharged` variant by design.
fn status_rejection(status: MandateStatus) -> Option<MandateRejection> {
    match status {
        MandateStatus::Pending | MandateStatus::InProgress => None,
        MandateStatus::Expired => Some(MandateRejection::Expired),
        MandateStatus::Revoked | MandateStatus::Discharged => Some(MandateRejection::Revoked),
    }
}

/// The app-side, act-time authority gate.
///
/// Synchronous by design: the backend is synchronous, and this mirrors the
/// repo's `PolicyOracle::evaluate` convention (no `.await`, no lock held
/// across an await point). It sits *beside* the capability and standing
/// checks, never inside or replacing them.
pub trait MandateGate {
    /// Resolve whether a valid mandate authorizes `req` right now.
    ///
    /// Returns a [`MandateGrant`] reference on success, a
    /// [`MandateRejection`] (wrapped in [`MandateGateError::Rejected`]) for an
    /// authorization failure, or [`MandateGateError::Backend`] when a backend
    /// read fails (fail-closed).
    fn require(&self, req: &MandateRequest) -> Result<MandateGrant, MandateGateError>;
}

/// Default resolver over an existing [`GovernanceReceiptBackend`].
///
/// Introduces **no** new persistence and duplicates **no** record. Non-proposal
/// domain targets resolve *actor-first* via
/// `list_active_authority_grants_by_grantee`, which needs no new index and
/// inherently enforces the grant-grantee actor binding.
pub struct DefaultMandateGate {
    backend: Arc<dyn GovernanceReceiptBackend>,
}

impl DefaultMandateGate {
    /// Construct a resolver over the given backend.
    pub fn new(backend: Arc<dyn GovernanceReceiptBackend>) -> Self {
        Self { backend }
    }

    /// Lifecycle checks shared by every target path, in invariant order:
    /// status liveness, then the **authoritative** mandate deadline (even
    /// when status is still live), then empty-grant fail-closed — all
    /// **before** any actor check.
    fn validate_mandate_lifecycle(
        &self,
        mandate: &Mandate,
        at: Timestamp,
    ) -> Result<(), MandateGateError> {
        if let Some(reason) = status_rejection(mandate.status) {
            return Err(rejected(reason));
        }
        // The mandate-level deadline is authoritative for expiry even when
        // status-transition enforcement (future work) has not moved a
        // Pending/InProgress mandate to Expired, and regardless of any grant's
        // wider/absent `valid_until`.
        if mandate.is_past_deadline(at) {
            return Err(rejected(MandateRejection::Expired));
        }
        // An empty-grant mandate is a "pending-grants" record: it attests a
        // decision occurred but carries no bounded authority. Reject it
        // fail-closed *before* the actor check so it can never become
        // act-time authority.
        if mandate.has_no_grants() {
            return Err(rejected(MandateRejection::NoMandate));
        }
        Ok(())
    }

    /// Load the [`AuthorityGrant`]s a mandate references. Grants that cannot be
    /// retrieved are skipped; a backend read error propagates.
    fn load_grants(&self, mandate: &Mandate) -> Result<Vec<AuthorityGrant>, MandateGateError> {
        let mut out = Vec::with_capacity(mandate.grants.len());
        for grant_id in &mandate.grants {
            if let Some(grant) = self
                .backend
                .get_authority_grant(grant_id)
                .map_err(MandateGateError::Backend)?
            {
                out.push(grant);
            }
        }
        Ok(out)
    }

    /// Proposal target: locate via the existing by-proposal index, then run the
    /// full spine (lifecycle → actor-is-grantee → matched-grant validity).
    fn resolve_proposal(
        &self,
        proposal: &ProposalId,
        actor: &Did,
        at: Timestamp,
    ) -> Result<Mandate, MandateGateError> {
        let mandate = self
            .backend
            .get_mandate_by_proposal(&proposal.0)
            .map_err(MandateGateError::Backend)?
            .ok_or_else(|| rejected(MandateRejection::NoMandate))?;

        self.validate_mandate_lifecycle(&mandate, at)?;

        // Authorization is grant-grantee-only: DecisionProvenance never
        // authorizes an actor, so a grantee match on an attached grant is
        // always required — there is no provenance-only fallback.
        let grants = self.load_grants(&mandate)?;
        let actor_grants: Vec<&AuthorityGrant> = grants
            .iter()
            .filter(|g| matches!(&g.grantee, Grantee::Person(p) if p == actor))
            .collect();
        if actor_grants.is_empty() {
            return Err(rejected(MandateRejection::WrongActor));
        }
        if !actor_grants.iter().any(|g| g.is_active_at(at)) {
            return Err(rejected(MandateRejection::Expired));
        }
        Ok(mandate)
    }

    /// Domain target, actor-first: the grant store is keyed by proposal/decision
    /// and a `Mandate` persists no `(domain, act, target)` binding, so a domain
    /// has no existing forward key. Resolve from the actor's active grants
    /// instead, then recover the authorizing mandate from the matched grant's
    /// provenance. This needs no new index and inherently enforces the
    /// grant-grantee actor binding.
    fn resolve_domain(
        &self,
        domain: &GovernanceDomainId,
        actor: &Did,
        at: Timestamp,
    ) -> Result<Mandate, MandateGateError> {
        let active = self
            .backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(actor.clone()), at)
            .map_err(MandateGateError::Backend)?;
        if active.is_empty() {
            return Err(rejected(MandateRejection::NoMandate));
        }

        // A grant binds a domain via `scope.domain`; `None` means unbounded on
        // the domain axis. The actor holds authority, but if none of it covers
        // this domain the target does not match.
        let in_domain: Vec<&AuthorityGrant> = active
            .iter()
            .filter(|g| match &g.scope.domain {
                Some(d) => d == domain,
                None => true,
            })
            .collect();
        if in_domain.is_empty() {
            return Err(rejected(MandateRejection::WrongTarget));
        }

        // Recover the authorizing mandate from a matching grant's provenance.
        // A charter-direct grant (`granted_by: None`) has no mandate to
        // validate; skip it. If no matching grant yields a mandate, there is no
        // mandate to authorize this act.
        for grant in in_domain {
            let Some(provenance) = &grant.granted_by else {
                continue;
            };
            let mandates = self
                .backend
                .list_mandates_by_decision(&provenance.decision_hash)
                .map_err(MandateGateError::Backend)?;
            if let Some(mandate) = mandates.into_iter().find(|m| m.grants.contains(&grant.id)) {
                // The grant came from `list_active_*` for this grantee, so
                // actor-is-grantee and grant-time-validity already hold; only
                // the mandate lifecycle remains.
                self.validate_mandate_lifecycle(&mandate, at)?;
                return Ok(mandate);
            }
        }
        Err(rejected(MandateRejection::NoMandate))
    }
}

impl MandateGate for DefaultMandateGate {
    fn require(&self, req: &MandateRequest) -> Result<MandateGrant, MandateGateError> {
        let mandate = match &req.target {
            MandateTarget::Proposal(proposal) => {
                self.resolve_proposal(proposal, &req.actor, req.at)?
            }
            MandateTarget::Domain(domain) => {
                // The target domain must be the request domain.
                if domain != &req.domain {
                    return Err(rejected(MandateRejection::WrongTarget));
                }
                self.resolve_domain(domain, &req.actor, req.at)?
            }
            // `TypedScope` carries no federation or role/structure binding, so
            // these targets cannot be matched to a grant yet. Fail closed
            // honestly rather than fake a match; a binding surface for these is
            // a named follow-up.
            MandateTarget::Federation(_) | MandateTarget::Role { .. } => {
                return Err(rejected(MandateRejection::WrongTarget));
            }
        };

        Ok(MandateGrant {
            mandate_id: mandate.id.clone(),
            decision_hash: mandate.decision.decision_hash,
            act: req.act.clone(),
            target: req.target.clone(),
            granted_at: req.at,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use icn_governance::{
        AuthorityClass, AuthorityGrant, AuthorityGrantId, DecisionProvenance, GrantorEntityId,
        TypedScope,
    };

    // ----- fixtures ---------------------------------------------------------

    /// Minimal in-memory backend seeded with pre-built mandates and grants.
    /// Implements only the methods the gate reads; the rest are trivial.
    /// One-shot fault switches simulate backend read failures.
    struct FixtureBackend {
        mandates: Vec<Mandate>,
        grants: Vec<AuthorityGrant>,
        fail_get_mandate: bool,
        fail_list_active: bool,
    }

    impl FixtureBackend {
        fn new(mandates: Vec<Mandate>, grants: Vec<AuthorityGrant>) -> Self {
            Self {
                mandates,
                grants,
                fail_get_mandate: false,
                fail_list_active: false,
            }
        }

        fn gate(self) -> DefaultMandateGate {
            DefaultMandateGate::new(Arc::new(self))
        }
    }

    impl GovernanceReceiptBackend for FixtureBackend {
        fn put_governance(
            &self,
            _: &icn_governance::GovernanceDecisionReceipt,
        ) -> Result<(), String> {
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            _: &str,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn put_allocation(
            &self,
            _: &icn_kernel_api::AllocationReceipt,
        ) -> Result<icn_kernel_api::Hash, String> {
            Ok([0u8; 32])
        }
        fn get_governance_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn list_allocations_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
            Ok(vec![])
        }

        fn get_mandate_by_proposal(&self, proposal_id: &str) -> Result<Option<Mandate>, String> {
            if self.fail_get_mandate {
                return Err("transient_backend_error: simulated get_mandate failure".into());
            }
            Ok(self
                .mandates
                .iter()
                .find(|m| m.decision.proposal_id == proposal_id)
                .cloned())
        }

        fn list_mandates_by_decision(&self, decision_hash: &Hash) -> Result<Vec<Mandate>, String> {
            Ok(self
                .mandates
                .iter()
                .filter(|m| &m.decision.decision_hash == decision_hash)
                .cloned()
                .collect())
        }

        fn get_authority_grant(
            &self,
            grant_id: &AuthorityGrantId,
        ) -> Result<Option<AuthorityGrant>, String> {
            Ok(self.grants.iter().find(|g| &g.id == grant_id).cloned())
        }

        fn list_active_authority_grants_by_grantee(
            &self,
            grantee: &Grantee,
            now: Timestamp,
        ) -> Result<Vec<AuthorityGrant>, String> {
            if self.fail_list_active {
                return Err("transient_backend_error: simulated list_active failure".into());
            }
            Ok(self
                .grants
                .iter()
                .filter(|g| &g.grantee == grantee && g.is_active_at(now))
                .cloned()
                .collect())
        }
    }

    fn did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    fn domain(name: &str) -> GovernanceDomainId {
        GovernanceDomainId(name.to_string())
    }

    fn decision(proposal_id: &str, hash_seed: u8) -> DecisionProvenance {
        DecisionProvenance {
            proposal_id: proposal_id.to_string(),
            decision_hash: [hash_seed; 32],
        }
    }

    /// Build a domain-scoped grant for `grantee`, bound to `decision`.
    fn grant(
        grantee: Did,
        dom: &GovernanceDomainId,
        decision: &DecisionProvenance,
        valid_from: Timestamp,
        valid_until: Option<Timestamp>,
    ) -> AuthorityGrant {
        AuthorityGrant {
            id: AuthorityGrantId::new(),
            class: AuthorityClass::Execution,
            grantor: GrantorEntityId(dom.0.clone()),
            grantee: Grantee::Person(grantee),
            scope: TypedScope {
                domain: Some(dom.clone()),
                ..TypedScope::default()
            },
            granted_by: Some(decision.clone()),
            valid_from,
            valid_until,
            revoked_at: None,
        }
    }

    fn proposal_request(
        actor: Did,
        dom: GovernanceDomainId,
        proposal_id: &str,
        at: Timestamp,
    ) -> MandateRequest {
        MandateRequest {
            actor,
            domain: dom,
            act: MandateAct::CloseProposal,
            target: MandateTarget::Proposal(ProposalId(proposal_id.to_string())),
            at,
        }
    }

    // ----- tests ------------------------------------------------------------

    #[test]
    fn valid_proposal_tuple_returns_grant() {
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = grant(actor.clone(), &dom, &prov, 100, Some(1000));
        let mandate =
            Mandate::new(prov.clone(), [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate.clone()], vec![g]).gate();
        let out = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .expect("valid tuple should resolve");
        assert_eq!(out.mandate_id, mandate.id);
        assert_eq!(out.decision_hash, prov.decision_hash);
        assert_eq!(out.act, MandateAct::CloseProposal);
    }

    #[test]
    fn no_mandate_for_unknown_proposal() {
        let gate = FixtureBackend::new(vec![], vec![]).gate();
        let err = gate
            .require(&proposal_request(did(1), domain("coop-a"), "prop-x", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::NoMandate));
    }

    #[test]
    fn empty_grant_pending_mandate_rejects_no_mandate_before_actor() {
        // A pending-grants record (status live, but `has_no_grants()`) must
        // reject NoMandate even though actor/target/time would otherwise match.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let mandate = Mandate::new_pending_grants(prov, [1u8; 32], None, None, 100);
        assert!(mandate.has_no_grants());

        let gate = FixtureBackend::new(vec![mandate], vec![]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::NoMandate));
    }

    #[test]
    fn past_deadline_with_live_status_rejects_expired() {
        // Status is still Pending, and the only grant's `valid_until` is wider
        // (None) — yet the mandate deadline is authoritative.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = grant(actor.clone(), &dom, &prov, 0, None); // grant never expires
        let mandate =
            Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, Some(500), 100).unwrap();
        assert_eq!(mandate.status, MandateStatus::Pending);

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 600))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::Expired));
    }

    #[test]
    fn provenance_only_actor_rejects_wrong_actor() {
        // The acting DID is associated with the decision conceptually, but is
        // NOT a grantee of any attached grant — there is no provenance-only
        // authorization path.
        let dom = domain("coop-a");
        let actor = did(1);
        let other_grantee = did(2);
        let prov = decision("prop-1", 7);
        let g = grant(other_grantee, &dom, &prov, 0, None);
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongActor));
    }

    #[test]
    fn unrelated_actor_rejects_wrong_actor() {
        let dom = domain("coop-a");
        let prov = decision("prop-1", 7);
        let g = grant(did(2), &dom, &prov, 0, None);
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(did(9), dom, "prop-1", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongActor));
    }

    #[test]
    fn revoked_and_discharged_status_reject_revoked_expired_status_rejects_expired() {
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = grant(actor.clone(), &dom, &prov, 0, None);

        for (status, expected) in [
            (MandateStatus::Revoked, MandateRejection::Revoked),
            (MandateStatus::Discharged, MandateRejection::Revoked),
            (MandateStatus::Expired, MandateRejection::Expired),
        ] {
            let mut mandate =
                Mandate::new(prov.clone(), [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();
            mandate.status = status;
            let gate = FixtureBackend::new(vec![mandate], vec![g.clone()]).gate();
            let err = gate
                .require(&proposal_request(actor.clone(), dom.clone(), "prop-1", 500))
                .unwrap_err();
            assert_eq!(err, rejected(expected), "status {status:?}");
        }
    }

    #[test]
    fn grant_outside_validity_window_rejects_expired() {
        // Mandate is live with no deadline, but the actor's grant window
        // excludes the act time.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = grant(actor.clone(), &dom, &prov, 100, Some(200));
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 300))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::Expired));
    }

    #[test]
    fn actor_first_domain_lookup_returns_grant() {
        // A Domain target resolves via the grantee's active grants (not only
        // Proposal targets), recovering the bound mandate.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = grant(actor.clone(), &dom, &prov, 0, Some(1000));
        let mandate =
            Mandate::new(prov.clone(), [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate.clone()], vec![g]).gate();
        let req = MandateRequest {
            actor,
            domain: dom.clone(),
            act: MandateAct::AddDomainMember,
            target: MandateTarget::Domain(dom),
            at: 500,
        };
        let out = gate
            .require(&req)
            .expect("domain actor-first should resolve");
        assert_eq!(out.mandate_id, mandate.id);
        assert_eq!(out.decision_hash, prov.decision_hash);
    }

    #[test]
    fn domain_target_with_authority_in_other_domain_rejects_wrong_target() {
        let dom_a = domain("coop-a");
        let dom_b = domain("coop-b");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        // Actor has an active grant in coop-a only.
        let g = grant(actor.clone(), &dom_a, &prov, 0, Some(1000));
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let req = MandateRequest {
            actor,
            domain: dom_b.clone(),
            act: MandateAct::AddDomainMember,
            target: MandateTarget::Domain(dom_b),
            at: 500,
        };
        let err = gate.require(&req).unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongTarget));
    }

    #[test]
    fn domain_target_mismatch_with_request_domain_rejects_wrong_target() {
        // Target Domain differs from the request's domain field.
        let gate = FixtureBackend::new(vec![], vec![]).gate();
        let req = MandateRequest {
            actor: did(1),
            domain: domain("coop-a"),
            act: MandateAct::AddDomainMember,
            target: MandateTarget::Domain(domain("coop-b")),
            at: 500,
        };
        let err = gate.require(&req).unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongTarget));
    }

    #[test]
    fn federation_and_role_targets_reject_wrong_target() {
        let gate = FixtureBackend::new(vec![], vec![]).gate();

        let fed = MandateRequest {
            actor: did(1),
            domain: domain("coop-a"),
            act: MandateAct::JoinFederation,
            target: MandateTarget::Federation("fed-1".to_string()),
            at: 500,
        };
        assert_eq!(
            gate.require(&fed).unwrap_err(),
            rejected(MandateRejection::WrongTarget)
        );

        let role = MandateRequest {
            actor: did(1),
            domain: domain("coop-a"),
            act: MandateAct::AppointSteward,
            target: MandateTarget::Role {
                structure_id: StructureId("office-1".to_string()),
                holder: did(2),
            },
            at: 500,
        };
        assert_eq!(
            gate.require(&role).unwrap_err(),
            rejected(MandateRejection::WrongTarget)
        );
    }

    #[test]
    fn backend_error_surfaces_as_backend_not_no_mandate() {
        let dom = domain("coop-a");
        let actor = did(1);

        // Proposal path: get_mandate_by_proposal fails.
        let mut backend = FixtureBackend::new(vec![], vec![]);
        backend.fail_get_mandate = true;
        let err = backend
            .gate()
            .require(&proposal_request(actor.clone(), dom.clone(), "prop-1", 500))
            .unwrap_err();
        assert!(matches!(err, MandateGateError::Backend(_)), "got {err:?}");

        // Domain path: list_active_authority_grants_by_grantee fails.
        let mut backend = FixtureBackend::new(vec![], vec![]);
        backend.fail_list_active = true;
        let req = MandateRequest {
            actor,
            domain: dom.clone(),
            act: MandateAct::AddDomainMember,
            target: MandateTarget::Domain(dom),
            at: 500,
        };
        let err = backend.gate().require(&req).unwrap_err();
        assert!(matches!(err, MandateGateError::Backend(_)), "got {err:?}");
    }

    #[test]
    fn rejection_reason_codes_are_stable() {
        assert_eq!(MandateRejection::NoMandate.reason_code(), "no_mandate");
        assert_eq!(MandateRejection::Expired.reason_code(), "expired");
        assert_eq!(MandateRejection::WrongTarget.reason_code(), "wrong_target");
        assert_eq!(MandateRejection::WrongActor.reason_code(), "wrong_actor");
        assert_eq!(MandateRejection::Suspended.reason_code(), "suspended");
        assert_eq!(MandateRejection::Revoked.reason_code(), "revoked");
    }
}
