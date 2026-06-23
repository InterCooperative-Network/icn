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
//!
//! This is a **deliberate divergence** from
//! `docs/design/governance/mandate-gate-design.md` §6 step 8, which
//! contemplates the gate "consulting the existing suspension_checker as an
//! adjacent fail-closed condition." Because that checker is async and the
//! gate is synchronous (per §4 of the same design and the repo's
//! `PolicyOracle::evaluate` convention), suspension stays handler-composed
//! beside the gate, not folded into it. The design's intent (separate,
//! composable checks — never a god-auth function) is preserved.
//!
//! # Act-binding model (deliberately strict and coarse)
//!
//! `TypedScope` has no per-act target field; `Mandate` carries no
//! `(domain, act, target)` binding. To avoid silently letting one grant
//! authorize an unrelated act, the gate enforces a small static convention
//! (see [`expected_act_tokens`] / [`expected_class`]):
//!
//! - Class must match the act's natural [`AuthorityClass`] (ADR-0014
//!   non-conflation): `CastVote` is `Representation`; every other current
//!   variant is `Execution`.
//! - The grant must explicitly bind the domain (`scope.domain == Some(req.domain)`;
//!   `None` is not treated as wildcard authorization).
//! - The grant must carry an act token in `scope.action_kind` **or** the
//!   broader class token in `scope.proposal_class`. A grant with neither
//!   populated cannot prove an act binding and is rejected
//!   ([`MandateRejection::WrongTarget`]).
//!
//! This is intentionally strict for step 6: until grant minting expands
//! and/or `TypedScope` grows a precise binding surface, the gate fails
//! closed rather than encode a permissive default.

use std::sync::Arc;

use icn_governance::proof::Hash;
use icn_governance::{
    AuthorityClass, AuthorityGrant, GovernanceDomainId, Grantee, Mandate, MandateGrantRef,
    MandateGrantRefError, MandateGrantRefTarget, MandateId, MandateStatus, ProposalId, StructureId,
    Timestamp, TypedScope,
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
    /// Adopt (or change) a governed domain's current `DomainPolicy`.
    AdoptDomainPolicy,
    /// Declare (create) a governed `InstitutionalDomain`.
    DeclareInstitutionalDomain,
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
/// The wire-recordable form is [`icn_governance::MandateGrantRef`], reached
/// via [`MandateGrant::into_ref`]. No existing receipt embeds it yet; the
/// receipt-body schema work is the next slice in the #1868 ladder.
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

impl MandateAct {
    /// Snake_case wire-form discriminator for receipt recording.
    ///
    /// These strings are the `act` field on [`MandateGrantRef`]. They match
    /// the institutional act vocabulary (one token per variant) and are
    /// **distinct** from the `scope.action_kind` binding tokens used by
    /// [`expected_act_tokens`] (which describe a grant's permitted-act
    /// surface, not the executed act). Keeping the two vocabularies
    /// separate avoids tying receipt history to internal scope-binding
    /// conventions.
    pub fn as_wire_token(&self) -> &'static str {
        match self {
            MandateAct::ActivateCharter => "activate_charter",
            MandateAct::AddDomainMember => "add_domain_member",
            MandateAct::RemoveDomainMember => "remove_domain_member",
            MandateAct::CloseProposal => "close_proposal",
            MandateAct::CastVote => "cast_vote",
            MandateAct::AppointSteward => "appoint_steward",
            MandateAct::RemoveSteward => "remove_steward",
            MandateAct::JoinFederation => "join_federation",
            MandateAct::LeaveFederation => "leave_federation",
            MandateAct::AdoptDomainPolicy => "adopt_domain_policy",
            MandateAct::DeclareInstitutionalDomain => "declare_institutional_domain",
        }
    }
}

impl MandateTarget {
    /// Translate the app-side target into its wire-recordable form.
    ///
    /// Direction is `apps/governance → icn-governance` only — the kernel
    /// crates and `icn-governance` itself remain unaware of
    /// [`MandateAct`]/[`MandateTarget`] semantics.
    pub fn to_ref_target(&self) -> MandateGrantRefTarget {
        match self {
            MandateTarget::Domain(domain) => MandateGrantRefTarget::Domain {
                domain_id: domain.0.clone(),
            },
            MandateTarget::Proposal(proposal) => MandateGrantRefTarget::Proposal {
                proposal_id: proposal.0.clone(),
            },
            MandateTarget::Role {
                structure_id,
                holder,
            } => MandateGrantRefTarget::Role {
                structure_id: structure_id.0.clone(),
                holder: holder.to_string(),
            },
            MandateTarget::Federation(fed) => MandateGrantRefTarget::Federation {
                federation_id: fed.clone(),
            },
        }
    }
}

impl MandateGrant {
    /// Build the wire-recordable [`MandateGrantRef`] from this grant.
    ///
    /// Returns the same [`MandateGrantRefError`] surface the underlying
    /// checked constructor uses; the only failure paths are empty/
    /// whitespace target components (e.g. an empty `domain_id`). In
    /// practice these come from upstream data validation, not from the
    /// gate itself, so callers should treat an `Err` here as a wire-
    /// integrity bug rather than an authorization failure.
    pub fn into_ref(self) -> Result<MandateGrantRef, MandateGrantRefError> {
        let target = self.target.to_ref_target();
        MandateGrantRef::new(
            self.mandate_id,
            self.decision_hash,
            self.act.as_wire_token().to_string(),
            target,
            self.granted_at,
        )
    }
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

/// Natural [`AuthorityClass`] for a [`MandateAct`] (ADR-0014 non-conflation).
///
/// `CastVote` is `Representation`; every other current variant is `Execution`.
/// `Attestation` is reserved for downstream signed-statement issuance and
/// never authorizes acts gated here.
fn expected_class(act: &MandateAct) -> AuthorityClass {
    match act {
        MandateAct::CastVote => AuthorityClass::Representation,
        MandateAct::ActivateCharter
        | MandateAct::AddDomainMember
        | MandateAct::RemoveDomainMember
        | MandateAct::CloseProposal
        | MandateAct::AppointSteward
        | MandateAct::RemoveSteward
        | MandateAct::JoinFederation
        | MandateAct::LeaveFederation
        | MandateAct::AdoptDomainPolicy
        | MandateAct::DeclareInstitutionalDomain => AuthorityClass::Execution,
    }
}

/// Static convention mapping a [`MandateAct`] to the scope tokens a grant must
/// carry to prove it authorizes the act.
///
/// Returns `(action_kind_token, proposal_class_token)`. A grant authorizes the
/// act if `scope.action_kind` contains the precise action token **or**
/// `scope.proposal_class` contains the coarser class token. Lifecycle acts
/// (`CloseProposal`, `CastVote`) have no proposal-class token — they bind only
/// via the precise `action_kind` entry.
///
/// These tokens are private to the gate's act-binding layer; they encode the
/// step-6 minimum-binding convention and are not part of any persisted
/// schema.
fn expected_act_tokens(act: &MandateAct) -> (Option<&'static str>, Option<&'static str>) {
    match act {
        MandateAct::ActivateCharter => (Some("charter:activate"), Some("Charter")),
        MandateAct::AddDomainMember => (Some("membership:add"), Some("Membership")),
        MandateAct::RemoveDomainMember => (Some("membership:remove"), Some("Membership")),
        MandateAct::CloseProposal => (Some("proposal:close"), None),
        MandateAct::CastVote => (Some("proposal:vote"), None),
        MandateAct::AppointSteward => (Some("sdis:appoint_steward"), Some("Sdis")),
        MandateAct::RemoveSteward => (Some("sdis:remove_steward"), Some("Sdis")),
        MandateAct::JoinFederation => (Some("federation:join"), Some("Federation")),
        MandateAct::LeaveFederation => (Some("federation:leave"), Some("Federation")),
        MandateAct::AdoptDomainPolicy => (Some("domain_policy:adopt"), Some("DomainPolicy")),
        MandateAct::DeclareInstitutionalDomain => (
            Some("institutional_domain:declare"),
            Some("InstitutionalDomain"),
        ),
    }
}

/// True if `scope` carries an act-binding token for `act` via either the
/// precise `action_kind` entry or the broader `proposal_class` entry. A scope
/// with neither populated cannot prove an act binding.
fn scope_binds_act(scope: &TypedScope, act: &MandateAct) -> bool {
    let (action_token, class_token) = expected_act_tokens(act);
    let action_match = action_token
        .map(|tok| scope.action_kind.iter().any(|s| s == tok))
        .unwrap_or(false);
    let class_match = class_token
        .map(|tok| scope.proposal_class.iter().any(|s| s == tok))
        .unwrap_or(false);
    action_match || class_match
}

/// True if `grant` explicitly authorizes `req`: correct class
/// (ADR-0014 non-conflation), domain explicitly bound, and at least one act
/// binding token present. A `None` domain is **not** treated as wildcard —
/// the gate fails closed unless the grant proves the binding.
fn grant_authorizes_request(grant: &AuthorityGrant, req: &MandateRequest) -> bool {
    if grant.class != expected_class(&req.act) {
        return false;
    }
    let domain_matches = matches!(&grant.scope.domain, Some(d) if d == &req.domain);
    if !domain_matches {
        return false;
    }
    scope_binds_act(&grant.scope, &req.act)
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

    /// Load the [`AuthorityGrant`]s a mandate references.
    ///
    /// A mandate referencing a grant id the backend cannot retrieve is
    /// **index/integrity skew**, not "grant absent": silently dropping such a
    /// reference would let the resolver fall through to a misleading verdict
    /// (e.g. `WrongActor` because the actor's grant is "missing") and hide an
    /// infrastructure integrity failure. Fail closed via
    /// [`MandateGateError::Backend`] so the operator sees the skew.
    fn load_grants(&self, mandate: &Mandate) -> Result<Vec<AuthorityGrant>, MandateGateError> {
        let mut out = Vec::with_capacity(mandate.grants.len());
        for grant_id in &mandate.grants {
            match self
                .backend
                .get_authority_grant(grant_id)
                .map_err(MandateGateError::Backend)?
            {
                Some(grant) => out.push(grant),
                None => {
                    return Err(MandateGateError::Backend(format!(
                        "mandate_grant_index_skew: mandate {} references grant {} \
                         not retrievable from backend",
                        mandate.id, grant_id
                    )));
                }
            }
        }
        Ok(out)
    }

    /// Proposal target: locate via the existing by-proposal index, then run the
    /// full spine (lifecycle → actor-is-grantee → binding/class match →
    /// time validity).
    fn resolve_proposal(
        &self,
        proposal: &ProposalId,
        req: &MandateRequest,
    ) -> Result<Mandate, MandateGateError> {
        let mandate = self
            .backend
            .get_mandate_by_proposal(&proposal.0)
            .map_err(MandateGateError::Backend)?
            .ok_or_else(|| rejected(MandateRejection::NoMandate))?;

        self.validate_mandate_lifecycle(&mandate, req.at)?;

        // Authorization is grant-grantee-only: DecisionProvenance never
        // authorizes an actor, so a grantee match on an attached grant is
        // always required — there is no provenance-only fallback.
        let grants = self.load_grants(&mandate)?;
        let actor_grants: Vec<&AuthorityGrant> = grants
            .iter()
            .filter(|g| matches!(&g.grantee, Grantee::Person(p) if p == &req.actor))
            .collect();
        if actor_grants.is_empty() {
            return Err(rejected(MandateRejection::WrongActor));
        }

        // Binding/class match (ADR-0014 non-conflation): an actor grant must
        // actually authorize this specific act on this domain, not merely be
        // attached to the mandate. A grant for a different act, wrong class,
        // or unbound domain fails closed.
        let authorizing: Vec<&AuthorityGrant> = actor_grants
            .iter()
            .copied()
            .filter(|g| grant_authorizes_request(g, req))
            .collect();
        if authorizing.is_empty() {
            return Err(rejected(MandateRejection::WrongTarget));
        }

        if !authorizing.iter().any(|g| g.is_active_at(req.at)) {
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
    ///
    /// Selection requires the grant to **prove** it authorizes the requested
    /// act (correct class, explicit domain, act token in scope) via
    /// [`grant_authorizes_request`] — a `None`-domain grant or a wrong-act/
    /// wrong-class grant is not accepted as wildcard authority.
    fn resolve_domain(&self, req: &MandateRequest) -> Result<Mandate, MandateGateError> {
        let active = self
            .backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(req.actor.clone()), req.at)
            .map_err(MandateGateError::Backend)?;
        if active.is_empty() {
            return Err(rejected(MandateRejection::NoMandate));
        }

        // Strict binding: domain + act + class must be explicitly proven by
        // the grant. ADR-0014 non-conflation lives here.
        let authorizing: Vec<&AuthorityGrant> = active
            .iter()
            .filter(|g| grant_authorizes_request(g, req))
            .collect();
        if authorizing.is_empty() {
            return Err(rejected(MandateRejection::WrongTarget));
        }

        // Recover the authorizing mandate from a matching grant's provenance.
        // A charter-direct grant (`granted_by: None`) has no mandate to
        // validate; skip it. If no matching grant yields a mandate, there is no
        // mandate to authorize this act.
        for grant in authorizing {
            let Some(provenance) = &grant.granted_by else {
                continue;
            };
            let mandates = self
                .backend
                .list_mandates_by_decision(&provenance.decision_hash)
                .map_err(MandateGateError::Backend)?;
            if let Some(mandate) = mandates.into_iter().find(|m| m.grants.contains(&grant.id)) {
                // The grant came from `list_active_*` for this grantee, so
                // actor-is-grantee and grant-time-validity already hold; the
                // binding/class match was just enforced; only the mandate
                // lifecycle remains.
                self.validate_mandate_lifecycle(&mandate, req.at)?;
                return Ok(mandate);
            }
        }
        Err(rejected(MandateRejection::NoMandate))
    }
}

impl MandateGate for DefaultMandateGate {
    fn require(&self, req: &MandateRequest) -> Result<MandateGrant, MandateGateError> {
        let mandate = match &req.target {
            MandateTarget::Proposal(proposal) => self.resolve_proposal(proposal, req)?,
            MandateTarget::Domain(domain) => {
                // The target domain must be the request domain.
                if domain != &req.domain {
                    return Err(rejected(MandateRejection::WrongTarget));
                }
                self.resolve_domain(req)?
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

    /// Build a domain-only grant for `grantee`, bound to `decision`. Has
    /// **no** act/class binding tokens populated — used by fail-closed tests
    /// that prove the gate rejects unbound or misclassified grants.
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

    /// Build a grant that **explicitly** authorizes `act` for `grantee` in
    /// `dom`: correct `AuthorityClass` and the precise `action_kind` token.
    /// Used by success tests so the act-binding spine has something to match.
    fn bound_grant(
        grantee: Did,
        dom: &GovernanceDomainId,
        act: MandateAct,
        decision: &DecisionProvenance,
        valid_from: Timestamp,
        valid_until: Option<Timestamp>,
    ) -> AuthorityGrant {
        let (action_token, _class_token) = expected_act_tokens(&act);
        let action_kind: Vec<String> = action_token.into_iter().map(str::to_string).collect();
        AuthorityGrant {
            id: AuthorityGrantId::new(),
            class: expected_class(&act),
            grantor: GrantorEntityId(dom.0.clone()),
            grantee: Grantee::Person(grantee),
            scope: TypedScope {
                domain: Some(dom.clone()),
                action_kind,
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
        // Grant must explicitly bind the act (CloseProposal) for the spine to
        // accept it — an unbound or wrong-act grant fails closed.
        let g = bound_grant(
            actor.clone(),
            &dom,
            MandateAct::CloseProposal,
            &prov,
            100,
            Some(1000),
        );
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
        // Mandate is live with no deadline; the actor's grant is properly
        // bound for the act but its time window excludes the request time.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = bound_grant(
            actor.clone(),
            &dom,
            MandateAct::CloseProposal,
            &prov,
            100,
            Some(200),
        );
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
        // Proposal targets), recovering the bound mandate. The grant must
        // explicitly bind both domain and the act (AddDomainMember).
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = bound_grant(
            actor.clone(),
            &dom,
            MandateAct::AddDomainMember,
            &prov,
            0,
            Some(1000),
        );
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
    fn grant_bound_for_one_act_does_not_authorize_a_different_act() {
        // ADR-0014 non-conflation: a grant bound for AppointSteward must not
        // silently authorize CloseProposal even when actor/domain/class align.
        // Both acts are Execution-class; the discriminator is the act token
        // in `scope.action_kind`.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = bound_grant(
            actor.clone(),
            &dom,
            MandateAct::AppointSteward,
            &prov,
            0,
            Some(1000),
        );
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongTarget));
    }

    #[test]
    fn unbound_grant_on_proposal_target_rejects_wrong_target() {
        // A domain-only grant with empty `action_kind` and `proposal_class`
        // cannot prove an act binding and must fail closed — never silently
        // authorize on actor+domain alone.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = grant(actor.clone(), &dom, &prov, 0, Some(1000));
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongTarget));
    }

    #[test]
    fn wrong_class_grant_rejects_wrong_target() {
        // ADR-0014: classes are non-conflatable. An Attestation grant with the
        // correct domain + action token must NOT authorize an Execution act.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = AuthorityGrant {
            id: AuthorityGrantId::new(),
            class: AuthorityClass::Attestation, // CloseProposal demands Execution
            grantor: GrantorEntityId(dom.0.clone()),
            grantee: Grantee::Person(actor.clone()),
            scope: TypedScope {
                domain: Some(dom.clone()),
                action_kind: vec!["proposal:close".to_string()],
                ..TypedScope::default()
            },
            granted_by: Some(prov.clone()),
            valid_from: 0,
            valid_until: Some(1000),
            revoked_at: None,
        };
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .unwrap_err();
        assert_eq!(err, rejected(MandateRejection::WrongTarget));
    }

    #[test]
    fn missing_referenced_grant_surfaces_backend_integrity() {
        // A mandate referencing a grant id the backend cannot retrieve is
        // index/integrity skew, not "grant absent." Silently dropping the
        // reference would fall through to a misleading WrongActor; instead
        // fail closed as Backend so the operator sees the skew.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let orphan_id = AuthorityGrantId::new();
        let mandate = Mandate::new(prov, [9u8; 32], vec![orphan_id], None, None, 100).unwrap();

        // Backend has NO grants — the mandate references an id that won't load.
        let gate = FixtureBackend::new(vec![mandate], vec![]).gate();
        let err = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .unwrap_err();
        match err {
            MandateGateError::Backend(msg) => {
                assert!(
                    msg.contains("mandate_grant_index_skew"),
                    "expected integrity-skew Backend error, got: {msg}"
                );
            }
            other => panic!("expected Backend integrity error, got {other:?}"),
        }
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

    // ------- MandateGrant ↔ MandateGrantRef adapter ----------------------

    #[test]
    fn act_wire_tokens_are_distinct_and_snake_case() {
        // The wire tokens are bound into receipt hashes via
        // MandateGrantRef::compute_ref_hash, so every act variant must
        // map to a unique snake_case token. Adding a MandateAct variant
        // without updating as_wire_token() is the failure mode this
        // guards.
        // Every MandateAct variant must be listed here — this is the manual
        // enumeration that guards `as_wire_token()` coverage.
        let acts = [
            MandateAct::ActivateCharter,
            MandateAct::AddDomainMember,
            MandateAct::RemoveDomainMember,
            MandateAct::CloseProposal,
            MandateAct::CastVote,
            MandateAct::AppointSteward,
            MandateAct::RemoveSteward,
            MandateAct::JoinFederation,
            MandateAct::LeaveFederation,
            MandateAct::AdoptDomainPolicy,
            MandateAct::DeclareInstitutionalDomain,
        ];
        let mut seen = std::collections::HashSet::new();
        for act in acts {
            let tok = act.as_wire_token();
            assert!(
                !tok.is_empty()
                    && tok
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "wire token {tok:?} for {act:?} must be non-empty snake_case"
            );
            assert!(seen.insert(tok), "duplicate wire token {tok:?}");
        }
    }

    #[test]
    fn target_to_ref_target_preserves_each_variant_structurally() {
        let dom = domain("coop-a");
        let actor = did(1);

        let domain_t = MandateTarget::Domain(dom.clone());
        match domain_t.to_ref_target() {
            MandateGrantRefTarget::Domain { domain_id } => assert_eq!(domain_id, dom.0),
            other => panic!("expected Domain ref target, got {other:?}"),
        }

        let proposal_t = MandateTarget::Proposal(ProposalId("prop-1".into()));
        match proposal_t.to_ref_target() {
            MandateGrantRefTarget::Proposal { proposal_id } => {
                assert_eq!(proposal_id, "prop-1")
            }
            other => panic!("expected Proposal ref target, got {other:?}"),
        }

        let role_t = MandateTarget::Role {
            structure_id: StructureId("office-1".into()),
            holder: actor.clone(),
        };
        match role_t.to_ref_target() {
            MandateGrantRefTarget::Role {
                structure_id,
                holder,
            } => {
                assert_eq!(structure_id, "office-1");
                assert_eq!(holder, actor.to_string());
            }
            other => panic!("expected Role ref target, got {other:?}"),
        }

        let fed_t = MandateTarget::Federation("fed-1".into());
        match fed_t.to_ref_target() {
            MandateGrantRefTarget::Federation { federation_id } => {
                assert_eq!(federation_id, "fed-1")
            }
            other => panic!("expected Federation ref target, got {other:?}"),
        }
    }

    #[test]
    fn grant_into_ref_round_trips_and_hash_is_stable() {
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = bound_grant(
            actor.clone(),
            &dom,
            MandateAct::CloseProposal,
            &prov,
            100,
            Some(1000),
        );
        let mandate =
            Mandate::new(prov.clone(), [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate.clone()], vec![g]).gate();
        let grant = gate
            .require(&proposal_request(actor, dom, "prop-1", 500))
            .expect("valid tuple resolves");

        // Two consecutive into_ref calls produce equal refs and equal
        // hashes — proves the adapter is deterministic.
        let r1 = grant.clone().into_ref().expect("into_ref ok");
        let r2 = grant.clone().into_ref().expect("into_ref ok");
        assert_eq!(r1, r2);
        assert_eq!(r1.ref_hash(), r2.ref_hash());

        // Field-level: the adapter preserves identity from the gate's
        // result through to the wire form.
        assert_eq!(r1.mandate_id, mandate.id);
        assert_eq!(r1.decision_hash, mandate.decision.decision_hash);
        assert_eq!(r1.act, MandateAct::CloseProposal.as_wire_token());
        assert_eq!(r1.granted_at, 500);
        match r1.target {
            MandateGrantRefTarget::Proposal { proposal_id } => {
                assert_eq!(proposal_id, "prop-1")
            }
            other => panic!("expected Proposal ref target, got {other:?}"),
        }
    }

    #[test]
    fn grant_into_ref_for_domain_target_resolves_through_actor_first_path() {
        // Exercise the resolve_domain path so the adapter is tested end-
        // to-end for a Domain target as well, not only Proposal.
        let dom = domain("coop-a");
        let actor = did(1);
        let prov = decision("prop-1", 7);
        let g = bound_grant(
            actor.clone(),
            &dom,
            MandateAct::AddDomainMember,
            &prov,
            0,
            Some(1000),
        );
        let mandate = Mandate::new(prov, [9u8; 32], vec![g.id.clone()], None, None, 100).unwrap();

        let gate = FixtureBackend::new(vec![mandate], vec![g]).gate();
        let req = MandateRequest {
            actor,
            domain: dom.clone(),
            act: MandateAct::AddDomainMember,
            target: MandateTarget::Domain(dom.clone()),
            at: 500,
        };
        let grant = gate.require(&req).expect("domain actor-first resolves");

        let r = grant.into_ref().expect("into_ref ok");
        assert_eq!(r.act, "add_domain_member");
        match r.target {
            MandateGrantRefTarget::Domain { domain_id } => assert_eq!(domain_id, dom.0),
            other => panic!("expected Domain ref target, got {other:?}"),
        }
    }
}
