//! Gateway-layer jurisdiction authority helpers.
//!
//! Provides the two canonical authority gates used across membership and steward
//! handlers.  Both gates resolve the caller's DID to a commons holder, then
//! check a specific `MembershipCapability` within a given jurisdiction.
//!
//! ## Authority levels
//!
//! | Level | Capability | Used for |
//! |-------|-----------|----------|
//! | Office-holding | `HoldOffice` | Governance mutations (promote, suspend, grant capability, …) |
//! | Member-standing | `Vote` | Member-rights actions (dispute filing, …) |
//!
//! Neither level implies global elevated status.  Both are scoped to the
//! exact jurisdiction supplied by the caller — there is no cross-jurisdiction
//! propagation.

use crate::commons_mgr::CommonsManager;
use crate::coop_entity_resolver::{
    CoopEntityResolution, CoopEntityResolver, ResolutionUnavailable,
};
use crate::entity_map::legacy_coop_id_to_entity_id_fallback;
use crate::entity_mgr::EntityManager;
use crate::error::{GatewayError, Result};
use icn_entity::{EntityId, Membership, MembershipRole};
use icn_identity::{Did, JurisdictionId, MembershipCapability, MembershipStatus};
use icn_obs::metrics::gateway as gateway_metrics;
use tracing::{debug, warn};

/// Require that `caller_did` holds the `HoldOffice` capability in `jurisdiction`.
///
/// Used to gate jurisdiction-governance mutations: membership promotion/suspension,
/// capability grants, role changes, steward status updates, and similar operations
/// that require a legitimately delegated cooperative office in the target domain.
///
/// Returns `Err(GatewayError::AuthorizationFailed)` when the caller is not a
/// commons holder or lacks `HoldOffice` in the given jurisdiction.
pub(crate) async fn require_office_in_jurisdiction(
    commons_manager: &CommonsManager,
    caller_did: &Did,
    jurisdiction: &JurisdictionId,
) -> Result<()> {
    let caller_holder = commons_manager
        .get_holder_by_did(caller_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed("Caller is not a commons holder".to_string())
        })?;

    let holder_id_hex = hex::encode(caller_holder.holder_id);
    let has_authority = commons_manager
        .member_has_capability(
            &holder_id_hex,
            jurisdiction,
            MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);

    if !has_authority {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Caller does not hold office in '{jurisdiction}' (HoldOffice capability required)"
        )));
    }

    Ok(())
}

/// Require that `caller_did` is a full member (`Member` status) in `jurisdiction`.
///
/// Used to gate member-rights actions that should be available to any full member
/// of a jurisdiction but not to unenrolled outsiders or provisional candidates.
/// Dispute filing is the canonical example: any member may report a concern, but
/// random authenticated users cannot inflate a steward's dispute count.
///
/// This checks membership status directly rather than a specific capability,
/// because capabilities are not automatically granted during the enrollment flow —
/// status is the authoritative marker that full membership was conferred.
///
/// Returns `Err(GatewayError::AuthorizationFailed)` when the caller is not a
/// commons holder or is not a full member of the given jurisdiction.
pub(crate) async fn require_membership_in_jurisdiction(
    commons_manager: &CommonsManager,
    caller_did: &Did,
    jurisdiction: &JurisdictionId,
) -> Result<()> {
    let caller_holder = commons_manager
        .get_holder_by_did(caller_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed("Caller is not a commons holder".to_string())
        })?;

    let is_full_member = caller_holder
        .get_affiliation(jurisdiction)
        .map(|a| a.membership_status == MembershipStatus::Member)
        .unwrap_or(false);

    if !is_full_member {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Caller is not a full member of '{jurisdiction}' (member standing required)"
        )));
    }

    Ok(())
}

// ============================================================================
// Entity-aware request authorization (RFC-0018, first slice)
// ============================================================================

/// The action a caller intends to take against a target entity.
///
/// The gateway translates an institutional action into a generic membership
/// requirement here; the kernel never sees the action's meaning (Meaning
/// Firewall). Keep this enum minimal — add a variant only when a real endpoint
/// family needs it (no speculative `Invite`/`Delegate`/`Settle`/… variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntityAction {
    /// Mutate the entity itself (settings, membership). Authorized by a role
    /// threshold (`Founder` or `BoardMember`). Mirrors the historical
    /// `require_entity_write_access` exactly — role-only, no active-standing gate.
    ModifyEntity,
    /// Read treasury state. Authorized by active membership of any role.
    TreasuryRead,
    /// Mutate treasury state. Authorized by the `TreasuryAccess` capability on an
    /// active membership (Founder/BoardMember/Officer hold it by default; a plain
    /// member only if explicitly granted) — capability, not role-name.
    TreasuryWrite,
}

impl EntityAction {
    /// Human-readable description of what this action requires, for deny messages.
    fn required_basis(self) -> &'static str {
        match self {
            EntityAction::ModifyEntity => "Founder or BoardMember role",
            EntityAction::TreasuryRead => "active membership",
            EntityAction::TreasuryWrite => "active membership with the TreasuryAccess capability",
        }
    }

    /// Stable, low-cardinality metric label for this action.
    fn metric_label(self) -> &'static str {
        match self {
            EntityAction::ModifyEntity => "modify_entity",
            EntityAction::TreasuryRead => "treasury_read",
            EntityAction::TreasuryWrite => "treasury_write",
        }
    }
}

/// Entity-aware request authorization (RFC-0018, first slice).
///
/// Generalizes `require_entity_write_access`: resolves the caller's membership in
/// `target` and checks that the membership authorizes `action`. Returns
/// `Err(GatewayError::Forbidden)` naming the required authority basis.
///
/// `ModifyEntity` preserves the historical role-only check (no active-standing
/// gate) for behavior parity; the treasury actions additionally require active
/// standing. See `docs/adr/ADR-0035-entity-aware-request-authorization.md`.
pub(crate) async fn require_entity_access(
    entity_mgr: &EntityManager,
    caller: &EntityId,
    target: &EntityId,
    action: EntityAction,
) -> Result<()> {
    let members = entity_mgr
        .get_members(target)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to resolve membership: {e}")))?;

    let authorized = members
        .iter()
        .filter(|m| &m.member_id == caller)
        .any(|m| action_authorized(m, action));

    if authorized {
        return Ok(());
    }

    Err(GatewayError::Forbidden(format!(
        "Caller {caller} is not authorized for {action:?} on entity {target} \
         (requires {})",
        action.required_basis()
    )))
}

/// Whether a single membership satisfies `action`'s authority requirement.
///
/// `ModifyEntity` is role-only (no active-standing gate) to exactly preserve the
/// historical `require_entity_write_access` behavior. Treasury actions require
/// active standing; `TreasuryWrite` additionally requires the `TreasuryAccess`
/// capability (capability, not role-name). See ADR-0035.
fn action_authorized(m: &Membership, action: EntityAction) -> bool {
    match action {
        EntityAction::ModifyEntity => {
            matches!(
                m.role,
                MembershipRole::Founder | MembershipRole::BoardMember
            )
        }
        EntityAction::TreasuryRead => m.is_active(),
        EntityAction::TreasuryWrite => {
            m.is_active() && m.has_capability(&icn_entity::MembershipCapability::TreasuryAccess)
        }
    }
}

// ----------------------------------------------------------------------------
// Observe mode (RFC-0018 treasury slice)
//
// The entity-aware path is computed ALONGSIDE the authoritative flat
// `require_coop_access` guard and recorded as an observation. It is NOT enforced:
// `observe_treasury_entity_access` returns an `EntityAccessObservation` (data), not
// a `Result` the caller propagates, so it is structurally incapable of denying a
// request. The flat guard remains the sole enforced gate this slice. See ADR-0035.
// ----------------------------------------------------------------------------

/// Why the entity-aware path would deny a request the flat guard allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DenyReason {
    /// Caller has no membership in the target entity.
    NonMember,
    /// Caller is a member but not in active standing.
    InactiveMember,
    /// Caller is an active member but lacks the capability the action requires.
    MissingCapability,
}

impl DenyReason {
    fn label(self) -> &'static str {
        match self {
            DenyReason::NonMember => "non_member",
            DenyReason::InactiveMember => "inactive_member",
            DenyReason::MissingCapability => "missing_capability",
        }
    }
}

/// Why the entity-aware path could not produce a decision (a data gap, not a deny).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndeterminateReason {
    /// The treasury carried no stored `entity_id` and its `coop_id` is not a
    /// projectable `EntityId` slug — the target cannot be resolved at all.
    MissingTreasuryEntityId,
    /// A target `EntityId` was resolved, but no such entity is registered.
    MissingEntityRecord,
    /// The target entity exists but has zero memberships.
    NoMemberships,
}

impl IndeterminateReason {
    fn label(self) -> &'static str {
        match self {
            IndeterminateReason::MissingTreasuryEntityId => "missing_treasury_entity_id",
            IndeterminateReason::MissingEntityRecord => "missing_entity_record",
            IndeterminateReason::NoMemberships => "no_memberships",
        }
    }
}

/// Outcome of computing the entity-aware decision in observe mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntityAccessObservation {
    /// The entity path agrees with the flat guard (would also allow).
    AgreesAllow,
    /// Flat guard allowed, but the entity path would deny (recorded, not acted on).
    EntityDeny(DenyReason),
    /// The entity path could not produce a decision (data gap).
    Indeterminate(IndeterminateReason),
    /// A resolver error occurred while computing the entity decision.
    Error(String),
}

impl EntityAccessObservation {
    fn result_label(&self) -> &'static str {
        match self {
            EntityAccessObservation::AgreesAllow => "agree_allow",
            EntityAccessObservation::EntityDeny(_) => "flat_allow_entity_deny",
            EntityAccessObservation::Indeterminate(_) => "entity_indeterminate",
            EntityAccessObservation::Error(_) => "entity_error",
        }
    }

    fn reason_label(&self) -> &'static str {
        match self {
            EntityAccessObservation::AgreesAllow => "agree",
            EntityAccessObservation::EntityDeny(r) => r.label(),
            EntityAccessObservation::Indeterminate(r) => r.label(),
            EntityAccessObservation::Error(_) => "entity_manager_error",
        }
    }
}

// ===========================================================================
// A2d: treasury entity-authorization gate scaffold (observe → measure → gate).
//
// This is the explicit, testable seam that decides what to do with the now-real
// treasury resolver observation. The shipped mode is `ObserveOnly` — it NEVER alters
// a route outcome. `EnforceTrustedResolver` is a decision-only mode exercised solely
// by `decide_treasury_gate` and its tests; this slice wires it to NO route and NO
// production config. A later, deliberate cutover would set the mode from an
// operator-gated, off-by-default knob and consume the decision at the route layer.
// ===========================================================================

/// The treasury route family's entity-authorization **mode** (A2d).
///
/// `ObserveOnly` is the default and the only mode the gateway ships *wired* — it never
/// changes a route outcome. `EnforceTrustedResolver` exists for the pure
/// [`decide_treasury_gate`] decision function and its tests; it is **not** reachable
/// from any production config in this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TreasuryEntityAuthMode {
    /// Record/measure only; the flat `require_coop_access` guard stays authoritative.
    #[default]
    ObserveOnly,
    /// Trusted-resolver enforcement *semantics* (decision-only; never wired to a route
    /// here).
    EnforceTrustedResolver,
}

/// The active treasury entity-auth mode the gateway runs with. Hardcoded to the
/// fail-safe default — this slice ships **no** path that sets it to enforcement.
pub(crate) const ACTIVE_TREASURY_ENTITY_AUTH_MODE: TreasuryEntityAuthMode =
    TreasuryEntityAuthMode::ObserveOnly;

/// What the A2d gate decides for a treasury request that has ALREADY passed the flat
/// `require_coop_access` guard.
///
/// `ProceedUnchanged` leaves the route outcome exactly as the flat guard decided.
/// `WouldDeny` is produced **only** under
/// [`TreasuryEntityAuthMode::EnforceTrustedResolver`] and is **not** acted on in this
/// slice — it is measured/logged for a future cutover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreasuryGateDecision {
    /// Route proceeds as the flat guard decided (every `ObserveOnly` decision, and the
    /// allow case under enforcement).
    ProceedUnchanged,
    /// Under enforcement, this request would be denied (carries a stable reason). Never
    /// produced under `ObserveOnly`.
    WouldDeny(TreasuryGateDenyReason),
}

/// Stable reason a trusted-resolver enforcement decision would deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreasuryGateDenyReason {
    /// The entity-membership observation says the caller is not an (active) member.
    NotMember,
    /// No trusted `coop_id → EntityId` basis: the resolver was unavailable, the binding
    /// was unprovenanced (`UnknownLegacy`, incl. gossip-originated rows), ambiguous,
    /// entity-type-mismatched, or the store errored. A name binding is never authority,
    /// and an untrusted one is never a basis for enforcement.
    UntrustedResolution,
    /// The legacy projection and the trusted resolver DISAGREE on the `EntityId` — a
    /// migration collision; fail closed.
    ResolverConflict,
    /// A trusted `ResolverOnly` basis exists (the resolver produced an `EntityId` the
    /// legacy projection could not), and the entity-membership observation is an
    /// affirmative allow, but the target it was evaluated against does NOT match the
    /// resolver's resolved target — the membership target is missing, or differs from
    /// the resolved `EntityId`. The allow is therefore target-unverified; fail closed.
    /// (Membership *denies* surface as `NotMember` regardless — denying never trusts an
    /// unverified target.)
    ResolverOnlyTargetUnverified,
    /// A trusted `Agree` basis exists (the legacy projection and the resolver name the
    /// SAME `EntityId` for the path `coop_id`), and the entity-membership observation is
    /// an affirmative allow, but the target it was evaluated against does NOT match that
    /// agreed `EntityId` — the membership target is missing, or differs. The allow is
    /// therefore target-unverified; fail closed. (Membership *denies* surface as
    /// `NotMember` regardless — denying never trusts an unverified target.)
    AgreeTargetUnverified,
    /// A trusted basis exists but the entity-membership observation is indeterminate (a
    /// data gap); fail closed.
    IndeterminateMembership,
    /// The entity-membership observation errored; fail closed.
    ObservationError,
}

impl TreasuryGateDecision {
    /// Stable, lowercase outcome label for logs / future metrics.
    fn outcome_label(self) -> &'static str {
        match self {
            TreasuryGateDecision::ProceedUnchanged => "proceed_unchanged",
            TreasuryGateDecision::WouldDeny(_) => "would_deny",
        }
    }
}

impl TreasuryGateDenyReason {
    /// Stable, lowercase reason label for logs / future metrics.
    fn label(self) -> &'static str {
        match self {
            TreasuryGateDenyReason::NotMember => "not_member",
            TreasuryGateDenyReason::UntrustedResolution => "untrusted_resolution",
            TreasuryGateDenyReason::ResolverConflict => "resolver_conflict",
            TreasuryGateDenyReason::ResolverOnlyTargetUnverified => {
                "resolver_only_target_unverified"
            }
            TreasuryGateDenyReason::AgreeTargetUnverified => "agree_target_unverified",
            TreasuryGateDenyReason::IndeterminateMembership => "indeterminate_membership",
            TreasuryGateDenyReason::ObservationError => "observation_error",
        }
    }
}

// ---------------------------------------------------------------------------
// A2d target evidence.
//
// The pure gate decision must answer "was the entity-membership observation
// evaluated against the SAME `EntityId` the trusted resolution established?". The
// bare `EntityAccessObservation` and `CoopResolutionObservation` classifications both
// discard the concrete `EntityId`s they compared, so the gate cannot answer that on
// its own. These narrow, internal evidence types carry those targets through the
// observe-only path so the decision can verify them — they add no authority, change no
// route, and a resolver target alone still never grants access.
// ---------------------------------------------------------------------------

/// How the treasury membership observation chose the `EntityId` it evaluated against.
///
/// Mirrors the target selection in [`compute_treasury_observation`]: the treasury's
/// stored `entity_id()` is preferred; the lossy legacy `coop_id → EntityId` projection
/// is the fallback; otherwise no target could be resolved at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MembershipTargetSource {
    /// The treasury's stored `entity_id()` (the authoritative target).
    StoredTreasuryEntityId,
    /// The best-effort legacy `coop_id → EntityId` projection (fallback only).
    LegacyProjection,
    /// No target could be resolved — membership was not evaluated against any entity.
    None,
}

impl MembershipTargetSource {
    /// Stable, lowercase label for logs / future metrics.
    fn label(self) -> &'static str {
        match self {
            MembershipTargetSource::StoredTreasuryEntityId => "stored_treasury_entity_id",
            MembershipTargetSource::LegacyProjection => "legacy_projection",
            MembershipTargetSource::None => "none",
        }
    }
}

/// The entity-membership observation PLUS the `EntityId` target it was evaluated
/// against (A2d target evidence).
///
/// `membership_target` is the entity membership was actually checked in;
/// `target_source` records how it was chosen. They are `Some(_)` / a concrete source
/// for every outcome where a target could be resolved (even a membership data gap or
/// error — the lookup still targeted that entity), and `None` / [`MembershipTargetSource::None`]
/// only when no target could be resolved at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreasuryMembershipObservation {
    /// The membership verdict (allow / entity-deny / indeterminate / error).
    pub(crate) observation: EntityAccessObservation,
    /// The `EntityId` membership was evaluated against, if any.
    pub(crate) membership_target: Option<EntityId>,
    /// How `membership_target` was chosen.
    pub(crate) target_source: MembershipTargetSource,
}

/// The `coop_id → EntityId` resolution classification PLUS the concrete legacy and
/// resolver targets it compared (A2d target evidence).
///
/// [`CoopResolutionObservation`] alone records only *how* the legacy projection and
/// the resolver compared, discarding the `EntityId`s — this carries them so the gate
/// can verify the membership target against the trusted target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoopResolutionEvidence {
    /// The classification (Agree / Disagree / ResolverOnly / LegacyOnly / NeitherResolved).
    pub(crate) classification: CoopResolutionObservation,
    /// The `EntityId` the legacy projection produced for the path `coop_id`, if any.
    pub(crate) legacy_target: Option<EntityId>,
    /// The `EntityId` the resolver produced, if any.
    pub(crate) resolver_target: Option<EntityId>,
}

impl CoopResolutionEvidence {
    /// The trusted `coop_id → EntityId` target this evidence establishes, if any.
    ///
    /// Only an `Agree` or `ResolverOnly` classification yields a trusted target — the
    /// resolver's `EntityId`. For `Agree` the legacy and resolver targets must actually
    /// be present AND equal (defense in depth — the pure decision does not assume its
    /// inputs are well-formed); for `ResolverOnly` the resolver's target is used.
    /// Every other classification is untrusted and yields `None`. Never fabricates a
    /// target.
    fn trusted_target(&self) -> Option<&EntityId> {
        match self.classification {
            CoopResolutionObservation::Agree => {
                match (self.legacy_target.as_ref(), self.resolver_target.as_ref()) {
                    (Some(legacy), Some(resolver)) if legacy == resolver => Some(resolver),
                    _ => None,
                }
            }
            CoopResolutionObservation::ResolverOnly => self.resolver_target.as_ref(),
            CoopResolutionObservation::Disagree
            | CoopResolutionObservation::LegacyOnly(_)
            | CoopResolutionObservation::NeitherResolved(_) => None,
        }
    }
}

/// All the evidence the pure A2d gate decision needs: the target-aware membership
/// observation and the target-aware resolution evidence.
///
/// The target-verification helpers answer the central A2d question — was the
/// membership observation evaluated against the SAME `EntityId` the trusted resolution
/// established? — without the decision having to reach back into upstream state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreasuryGateEvidence {
    /// Membership verdict + the `EntityId` it was evaluated against.
    pub(crate) membership: TreasuryMembershipObservation,
    /// Resolution classification + the legacy/resolver targets it compared.
    pub(crate) resolution: CoopResolutionEvidence,
}

impl TreasuryGateEvidence {
    /// Whether the membership observation was evaluated against the SAME `EntityId` the
    /// trusted resolution established.
    ///
    /// Fail-closed: a missing target on *either* side — no trusted resolver/agreed
    /// target, or membership had no resolvable target — is NEVER a match. Only two
    /// concrete, equal `EntityId`s verify.
    fn membership_target_verified(&self) -> bool {
        match (
            self.membership.membership_target.as_ref(),
            self.resolution.trusted_target(),
        ) {
            (Some(member), Some(trusted)) => member == trusted,
            _ => false,
        }
    }
}

/// Pure A2d gate decision for a treasury request (no side effects).
///
/// Input is the already-computed observe-mode [`TreasuryGateEvidence`] — the
/// target-aware membership observation and the target-aware `coop_id → EntityId`
/// resolution evidence — plus the `mode`. The function NEVER consults a mapping as
/// authority: under enforcement the resolution is used only as a *trust qualifier* and
/// *target verifier* (is there a trusted basis, and does the membership target match
/// it?), while the actual allow/deny signal stays the entity-membership observation.
///
/// - [`TreasuryEntityAuthMode::ObserveOnly`] (the shipped default) ALWAYS returns
///   [`TreasuryGateDecision::ProceedUnchanged`]: byte-identical to the flat guard,
///   regardless of the evidence.
/// - [`TreasuryEntityAuthMode::EnforceTrustedResolver`] (decision-only; not wired to any
///   route or config here) requires a trusted basis first, then defers to membership and
///   gates the would-allow path on target verification:
///   - `Disagree` → `WouldDeny(ResolverConflict)` (migration collision; fail closed);
///   - `LegacyOnly(_)` / `NeitherResolved(_)` → `WouldDeny(UntrustedResolution)` — covers
///     `UnknownLegacy`, gossip-originated/unprovenanced rows, ambiguous bindings,
///     entity-type mismatch, and backend/source errors (all fail closed);
///   - `Agree` / `ResolverOnly` are trusted bases. With one, the membership observation
///     decides:
///     - `EntityDeny` → `WouldDeny(NotMember)`; `Indeterminate` →
///       `WouldDeny(IndeterminateMembership)`; `Error` → `WouldDeny(ObservationError)`
///       (membership denies/gaps surface regardless of the target);
///     - `AgreesAllow` returns `ProceedUnchanged` ONLY when the membership target equals
///       the trusted target — the agreed `EntityId` for `Agree`, the resolver's
///       `EntityId` for `ResolverOnly`. Otherwise (target missing or mismatched) it fails
///       closed: `WouldDeny(AgreeTargetUnverified)` for `Agree`,
///       `WouldDeny(ResolverOnlyTargetUnverified)` for `ResolverOnly`.
///
/// A resolver/agreed mapping is only ever a trust *qualifier* and *target verifier* — it
/// is never authority. The authority signal remains entity membership.
pub(crate) fn decide_treasury_gate(
    mode: TreasuryEntityAuthMode,
    evidence: &TreasuryGateEvidence,
) -> TreasuryGateDecision {
    match mode {
        // Default, shipped behavior: never alter the route.
        TreasuryEntityAuthMode::ObserveOnly => TreasuryGateDecision::ProceedUnchanged,
        TreasuryEntityAuthMode::EnforceTrustedResolver => {
            // (1) A trusted coop→EntityId basis is required before any enforcement, and it
            //     fixes which "target-unverified" reason a later unverified allow reports.
            //     Untrusted / conflicting / unavailable bases fail closed BEFORE the
            //     membership signal is consulted — you cannot enforce on a membership
            //     target whose coop→entity mapping you cannot trust.
            let target_unverified_reason = match evidence.resolution.classification {
                // Strongest basis: legacy projection and resolver name the SAME EntityId
                // for the path coop_id. An unverified would-allow is `AgreeTargetUnverified`.
                CoopResolutionObservation::Agree => TreasuryGateDenyReason::AgreeTargetUnverified,
                // The resolver produced a target the legacy projection could not — now a
                // trusted basis, because the resolved target is carried in the evidence and
                // can be checked against the membership target. An unverified would-allow
                // is `ResolverOnlyTargetUnverified`.
                CoopResolutionObservation::ResolverOnly => {
                    TreasuryGateDenyReason::ResolverOnlyTargetUnverified
                }
                CoopResolutionObservation::Disagree => {
                    return TreasuryGateDecision::WouldDeny(
                        TreasuryGateDenyReason::ResolverConflict,
                    );
                }
                CoopResolutionObservation::LegacyOnly(_)
                | CoopResolutionObservation::NeitherResolved(_) => {
                    return TreasuryGateDecision::WouldDeny(
                        TreasuryGateDenyReason::UntrustedResolution,
                    );
                }
            };
            // (2) With a trusted basis, the entity-membership observation decides. The
            //     would-allow path additionally requires the membership target to equal the
            //     trusted target — a resolver/agreed mapping *verifies the target*, it never
            //     grants authority. Membership DENIES and gaps surface unchanged; denying
            //     never trusts an unverified target.
            match &evidence.membership.observation {
                EntityAccessObservation::AgreesAllow => {
                    if evidence.membership_target_verified() {
                        TreasuryGateDecision::ProceedUnchanged
                    } else {
                        TreasuryGateDecision::WouldDeny(target_unverified_reason)
                    }
                }
                EntityAccessObservation::EntityDeny(_) => {
                    TreasuryGateDecision::WouldDeny(TreasuryGateDenyReason::NotMember)
                }
                EntityAccessObservation::Indeterminate(_) => {
                    TreasuryGateDecision::WouldDeny(TreasuryGateDenyReason::IndeterminateMembership)
                }
                EntityAccessObservation::Error(_) => {
                    TreasuryGateDecision::WouldDeny(TreasuryGateDenyReason::ObservationError)
                }
            }
        }
    }
}

/// Record (observe-only) the A2d treasury gate decision for cutover-readiness
/// telemetry. `active` is the decision under the shipped
/// [`ACTIVE_TREASURY_ENTITY_AUTH_MODE`] (always `ProceedUnchanged` today);
/// `would_enforce` is the hypothetical [`TreasuryEntityAuthMode::EnforceTrustedResolver`]
/// decision. `evidence` supplies the target-verification context (how the membership
/// target was chosen, and whether it matched the trusted resolver/agreed target) so a
/// future cutover can see how often a trusted basis actually verifies its target.
/// Logging only — this denies nothing and changes no route outcome.
fn record_treasury_gate(
    caller: &EntityId,
    action: EntityAction,
    evidence: &TreasuryGateEvidence,
    active: TreasuryGateDecision,
    would_enforce: TreasuryGateDecision,
) {
    let target_source = evidence.membership.target_source.label();
    let target_verified = evidence.membership_target_verified();
    match would_enforce {
        // Reserve `warn!` for the high-signal migration collision (a true
        // resolver/legacy `EntityId` disagreement). Ordinary hypothetical denies
        // (unverified-target/untrusted resolution, non-member, data gaps) are expected to
        // be common in observe-only mode — especially while the resolver is unwired or
        // most coops still classify as `LegacyOnly`/`NeitherResolved` — so they stay
        // low-noise `debug!` measurement rather than alert-worthy warnings.
        TreasuryGateDecision::WouldDeny(TreasuryGateDenyReason::ResolverConflict) => warn!(
            target: "entity_authz_gate",
            caller = %caller,
            action = action.metric_label(),
            active = active.outcome_label(),
            would_enforce = would_enforce.outcome_label(),
            reason = TreasuryGateDenyReason::ResolverConflict.label(),
            target_source,
            target_verified,
            "treasury entity gate WOULD DENY under enforcement on a resolver/legacy collision (observe-only; flat coop guard remains authoritative)"
        ),
        TreasuryGateDecision::WouldDeny(reason) => debug!(
            target: "entity_authz_gate",
            caller = %caller,
            action = action.metric_label(),
            active = active.outcome_label(),
            would_enforce = would_enforce.outcome_label(),
            reason = reason.label(),
            target_source,
            target_verified,
            "treasury entity gate would deny under enforcement (observe-only measurement)"
        ),
        TreasuryGateDecision::ProceedUnchanged => debug!(
            target: "entity_authz_gate",
            caller = %caller,
            action = action.metric_label(),
            active = active.outcome_label(),
            would_enforce = would_enforce.outcome_label(),
            target_source,
            target_verified,
            "treasury entity gate would proceed under enforcement (observe-only)"
        ),
    }
}

/// Compute, record, and return the entity-aware authorization observation for a
/// treasury request that has ALREADY passed the flat `require_coop_access` guard.
///
/// This is observation-only — it emits a metric and a log line and returns the
/// observation. It never denies the request (the caller discards the result).
/// `treasury_entity_id` is the treasury's stored `EntityId` (`treasury.entity_id()`);
/// `coop_id` is the path coop id used only as a best-effort fallback target.
pub(crate) async fn observe_treasury_entity_access(
    resolver: &dyn CoopEntityResolver,
    entity_mgr: &EntityManager,
    caller: &EntityId,
    treasury_entity_id: Option<&EntityId>,
    coop_id: &str,
    action: EntityAction,
) -> EntityAccessObservation {
    let membership =
        compute_treasury_observation(entity_mgr, caller, treasury_entity_id, coop_id, action).await;

    gateway_metrics::entity_authz_observation_inc(
        "treasury",
        action.metric_label(),
        membership.observation.result_label(),
        membership.observation.reason_label(),
    );

    match &membership.observation {
        EntityAccessObservation::AgreesAllow => {
            debug!(
                target: "entity_authz_observe",
                caller = %caller,
                action = action.metric_label(),
                "entity-aware path agrees with flat guard (allow)"
            );
        }
        EntityAccessObservation::EntityDeny(reason) => {
            warn!(
                target: "entity_authz_observe",
                caller = %caller,
                action = action.metric_label(),
                reason = reason.label(),
                "entity-aware path would DENY (observe-only; flat coop guard remains authoritative)"
            );
        }
        EntityAccessObservation::Indeterminate(reason) => {
            debug!(
                target: "entity_authz_observe",
                caller = %caller,
                action = action.metric_label(),
                reason = reason.label(),
                "entity-aware path indeterminate (data gap)"
            );
        }
        EntityAccessObservation::Error(err) => {
            warn!(
                target: "entity_authz_observe",
                caller = %caller,
                action = action.metric_label(),
                error = %err,
                "entity-aware observation errored (observe-only)"
            );
        }
    }

    // A2b/A2c: observe-only `coop_id → EntityId` resolution discrepancy. Consults the
    // governed resolver seam (#2188) and classifies how it compares to the legacy
    // projection the observe path already uses. The resolver is injected by the caller
    // (A2c): the gateway wires a trusted, provenance-gated `StoreBackedCoopEntityResolver`
    // when a `CoopEntityMap` handle is configured, and otherwise the fail-closed
    // `UnwiredCoopEntityResolver`. Either way this never affects this observation, the
    // flat guard, or any authorization decision.
    let resolution = observe_coop_entity_resolution(resolver, coop_id).await;

    // A2d: observe → MEASURE the treasury entity-auth gate. The active gate runs under
    // `ACTIVE_TREASURY_ENTITY_AUTH_MODE` (the shipped fail-safe default `ObserveOnly`),
    // so its decision is always `ProceedUnchanged` and the route is NEVER altered here —
    // this function stays observe-only and its return value is still discarded by the
    // caller. We additionally evaluate what `EnforceTrustedResolver` WOULD decide and
    // record it, so a future cutover can be measured first. Neither path denies anything.
    // The membership observation and the resolution evidence are bundled so the decision
    // can verify whether membership was evaluated against the trusted resolver/agreed
    // target (A2d target verification).
    let evidence = TreasuryGateEvidence {
        membership,
        resolution,
    };
    let active_gate = decide_treasury_gate(ACTIVE_TREASURY_ENTITY_AUTH_MODE, &evidence);
    let would_enforce_gate =
        decide_treasury_gate(TreasuryEntityAuthMode::EnforceTrustedResolver, &evidence);
    record_treasury_gate(caller, action, &evidence, active_gate, would_enforce_gate);

    evidence.membership.observation
}

/// Outcome of comparing the legacy `coop_id → EntityId` projection against the
/// governed [`CoopEntityResolver`] seam, in observe mode.
///
/// This is data for migration visibility only (a discrepancy log). It is never an
/// authorization input: the flat `require_coop_access` guard remains authoritative,
/// and a mapping is not authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoopResolutionObservation {
    /// Legacy projection and the resolver produced the same cooperative `EntityId`.
    Agree,
    /// Both produced an `EntityId`, but they differ (a migration hazard — log loudly).
    Disagree,
    /// The resolver produced an `EntityId` but the legacy projection rejected the
    /// `coop_id`. Not reachable from the unwired default; possible once A2c wires a
    /// trusted source.
    ResolverOnly,
    /// The legacy projection produced an `EntityId` but the resolver was unavailable
    /// (carries the machine-readable reason). The common case while the resolver is
    /// unwired.
    LegacyOnly(ResolutionUnavailable),
    /// Neither path produced an `EntityId`: the `coop_id` is not a projectable slug
    /// and the resolver was unavailable.
    NeitherResolved(ResolutionUnavailable),
}

impl CoopResolutionObservation {
    /// Stable, lowercase classification label for logs / future metrics.
    fn result_label(self) -> &'static str {
        match self {
            CoopResolutionObservation::Agree => "agree",
            CoopResolutionObservation::Disagree => "disagree",
            CoopResolutionObservation::ResolverOnly => "resolver_only",
            CoopResolutionObservation::LegacyOnly(_) => "legacy_only",
            CoopResolutionObservation::NeitherResolved(_) => "neither_resolved",
        }
    }

    /// Stable reason label (the resolver's unavailability reason where applicable).
    fn reason_label(self) -> &'static str {
        match self {
            CoopResolutionObservation::Agree => "agree",
            CoopResolutionObservation::Disagree => "disagree",
            CoopResolutionObservation::ResolverOnly => "resolver_only",
            CoopResolutionObservation::LegacyOnly(reason)
            | CoopResolutionObservation::NeitherResolved(reason) => reason.label(),
        }
    }
}

/// Pure classification of legacy projection vs resolver outcome (no side effects).
///
/// `legacy_target` is the cooperative `EntityId` the legacy reject-not-normalize
/// projection produced for the `coop_id` (`None` if the `coop_id` is not a
/// projectable slug). It never fabricates an `EntityId`.
fn classify_coop_resolution(
    legacy_target: Option<&EntityId>,
    resolver_outcome: &CoopEntityResolution,
) -> CoopResolutionObservation {
    match (legacy_target, resolver_outcome) {
        (Some(legacy), CoopEntityResolution::Resolved { entity_id }) => {
            if legacy == entity_id {
                CoopResolutionObservation::Agree
            } else {
                CoopResolutionObservation::Disagree
            }
        }
        (None, CoopEntityResolution::Resolved { .. }) => CoopResolutionObservation::ResolverOnly,
        (Some(_), CoopEntityResolution::Unavailable { reason }) => {
            CoopResolutionObservation::LegacyOnly(*reason)
        }
        (None, CoopEntityResolution::Unavailable { reason }) => {
            CoopResolutionObservation::NeitherResolved(*reason)
        }
    }
}

/// Compute and log (observe-only) the `coop_id → EntityId` resolution discrepancy
/// between the legacy projection and the governed resolver seam, returning the
/// [`CoopResolutionEvidence`] — the classification plus the concrete legacy and
/// resolver `EntityId` targets it compared.
///
/// Production callers discard it. It performs no authorization, denies nothing, and
/// never fabricates an `EntityId`. The targets are carried so the A2d gate can verify
/// the membership target against the trusted target.
pub(crate) async fn observe_coop_entity_resolution(
    resolver: &dyn CoopEntityResolver,
    coop_id: &str,
) -> CoopResolutionEvidence {
    let resolver_outcome = resolver.resolve_coop_entity(coop_id).await;
    let legacy_target = legacy_coop_id_to_entity_id_fallback(coop_id).ok();
    let resolver_target = resolver_outcome.resolved_entity_id().cloned();
    let classification = classify_coop_resolution(legacy_target.as_ref(), &resolver_outcome);

    match classification {
        CoopResolutionObservation::Disagree => {
            // Log both identifiers so the collision is actionable for diagnosing
            // provenance/mapping issues once A2c wires a real resolver. An `EntityId`
            // is an institutional identifier (slug), not a secret.
            warn!(
                target: "coop_resolution_observe",
                coop_id = %coop_id,
                result = classification.result_label(),
                legacy_entity_id = legacy_target.as_ref().map_or("<none>", |e| e.as_str()),
                resolver_entity_id = resolver_target.as_ref().map_or("<none>", |e| e.as_str()),
                "legacy projection and resolver DISAGREE on coop_id -> EntityId (observe-only; flat coop guard remains authoritative)"
            );
        }
        _ => {
            debug!(
                target: "coop_resolution_observe",
                coop_id = %coop_id,
                result = classification.result_label(),
                reason = classification.reason_label(),
                "coop_id -> EntityId resolution observation (observe-only)"
            );
        }
    }

    CoopResolutionEvidence {
        classification,
        legacy_target,
        resolver_target,
    }
}

/// Evaluate the entity-membership verdict for `caller` against an already-resolved
/// `target` entity (no target resolution, no side effects).
///
/// Split out of [`compute_treasury_observation`] so target resolution and membership
/// evaluation are separately testable; the verdict is exactly the historical observe
/// logic, unchanged.
async fn evaluate_membership(
    entity_mgr: &EntityManager,
    caller: &EntityId,
    target: &EntityId,
    action: EntityAction,
) -> EntityAccessObservation {
    match entity_mgr.get(target).await {
        Err(e) => return EntityAccessObservation::Error(e.to_string()),
        Ok(None) => {
            return EntityAccessObservation::Indeterminate(IndeterminateReason::MissingEntityRecord)
        }
        Ok(Some(_)) => {}
    }

    let members = match entity_mgr.get_members(target).await {
        Ok(members) => members,
        Err(e) => return EntityAccessObservation::Error(e.to_string()),
    };
    if members.is_empty() {
        return EntityAccessObservation::Indeterminate(IndeterminateReason::NoMemberships);
    }

    match members.iter().find(|m| &m.member_id == caller) {
        None => EntityAccessObservation::EntityDeny(DenyReason::NonMember),
        Some(m) if action_authorized(m, action) => EntityAccessObservation::AgreesAllow,
        Some(m) if !m.is_active() => {
            EntityAccessObservation::EntityDeny(DenyReason::InactiveMember)
        }
        Some(_) => EntityAccessObservation::EntityDeny(DenyReason::MissingCapability),
    }
}

/// Pure decision half of [`observe_treasury_entity_access`] (no side effects).
///
/// Resolves the membership target — preferring the treasury's stored `EntityId`, else
/// the best-effort (reject-not-normalize) legacy projection of the flat `coop_id` —
/// records which source produced it, then evaluates membership against it. Returns the
/// [`TreasuryMembershipObservation`] so the A2d gate knows the exact `EntityId`
/// membership was evaluated against (A2d target evidence).
async fn compute_treasury_observation(
    entity_mgr: &EntityManager,
    caller: &EntityId,
    treasury_entity_id: Option<&EntityId>,
    coop_id: &str,
    action: EntityAction,
) -> TreasuryMembershipObservation {
    let (target, target_source) = match treasury_entity_id {
        Some(id) => (id.clone(), MembershipTargetSource::StoredTreasuryEntityId),
        None => match legacy_coop_id_to_entity_id_fallback(coop_id) {
            Ok(id) => (id, MembershipTargetSource::LegacyProjection),
            // No stored entity_id and a non-projectable coop_id: no target at all.
            Err(_) => {
                return TreasuryMembershipObservation {
                    observation: EntityAccessObservation::Indeterminate(
                        IndeterminateReason::MissingTreasuryEntityId,
                    ),
                    membership_target: None,
                    target_source: MembershipTargetSource::None,
                };
            }
        },
    };

    // A concrete target was resolved, so membership was evaluated against `target` for
    // EVERY subsequent outcome (allow / deny / data gap / error) — it is the membership
    // target regardless of the verdict.
    let observation = evaluate_membership(entity_mgr, caller, &target, action).await;
    TreasuryMembershipObservation {
        observation,
        membership_target: Some(target),
        target_source,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod entity_access_tests {
    use super::*;
    use crate::coop_entity_resolver::{StoreBackedCoopEntityResolver, UnwiredCoopEntityResolver};
    use icn_entity::{
        CoopEntityBindingProvenance, CoopEntityMap, CooperativeEntity, InMemoryCoopEntityMap,
        MembershipCapability, MembershipStatus,
    };
    use icn_identity::KeyPair;

    /// Register a fresh in-memory coop entity; return the manager and its EntityId.
    async fn setup() -> (EntityManager, EntityId) {
        let mgr = EntityManager::new();
        let coop = CooperativeEntity::cooperative("treasury-coop", "Treasury Coop").unwrap();
        let target = coop.id.clone();
        mgr.register(coop).await.unwrap();
        (mgr, target)
    }

    /// Register an individual and grant it an *active* membership of `role` in `target`.
    async fn add_active_member(
        mgr: &EntityManager,
        target: &EntityId,
        role: MembershipRole,
    ) -> EntityId {
        let kp = KeyPair::generate().unwrap();
        let indiv = CooperativeEntity::individual(kp.did(), "Member");
        let caller = indiv.id.clone();
        mgr.register(indiv).await.unwrap();
        mgr.add_membership(Membership::active(caller.clone(), target.clone(), role))
            .await
            .unwrap();
        caller
    }

    #[tokio::test]
    async fn founder_allowed_modify_and_treasury() {
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::ModifyEntity)
                .await
                .is_ok()
        );
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryRead)
                .await
                .is_ok()
        );
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryWrite)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn board_member_allowed_treasury_write() {
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::BoardMember).await;
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryWrite)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn active_member_reads_but_cannot_write_treasury() {
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Member).await;
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryRead)
                .await
                .is_ok()
        );
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryWrite)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn capability_beats_role_for_treasury_write() {
        // A plain Member explicitly granted TreasuryAccess can write — proving the
        // decision is capability-driven, not a Founder/BoardMember role shortcut.
        let (mgr, target) = setup().await;
        let kp = KeyPair::generate().unwrap();
        let indiv = CooperativeEntity::individual(kp.did(), "Granted Member");
        let caller = indiv.id.clone();
        mgr.register(indiv).await.unwrap();
        let m = Membership::active(caller.clone(), target.clone(), MembershipRole::Member)
            .with_capabilities(vec![
                MembershipCapability::Vote,
                MembershipCapability::TreasuryAccess,
            ]);
        mgr.add_membership(m).await.unwrap();
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryWrite)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn non_member_denied() {
        let (mgr, target) = setup().await;
        let kp = KeyPair::generate().unwrap();
        let outsider = EntityId::from_did(kp.did());
        assert!(
            require_entity_access(&mgr, &outsider, &target, EntityAction::TreasuryRead)
                .await
                .is_err()
        );
        assert!(
            require_entity_access(&mgr, &outsider, &target, EntityAction::ModifyEntity)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn suspended_member_denied_treasury() {
        let (mgr, target) = setup().await;
        let kp = KeyPair::generate().unwrap();
        let indiv = CooperativeEntity::individual(kp.did(), "Suspended");
        let caller = indiv.id.clone();
        mgr.register(indiv).await.unwrap();
        let mut m = Membership::active(caller.clone(), target.clone(), MembershipRole::Founder);
        m.status = MembershipStatus::Suspended;
        mgr.add_membership(m).await.unwrap();
        // Treasury actions require active standing -> denied even for a Founder.
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryRead)
                .await
                .is_err()
        );
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::TreasuryWrite)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn modify_entity_is_role_only_preserving_legacy_behavior() {
        // require_entity_write_access historically checked role only (no active
        // gate). ModifyEntity preserves that: a suspended Founder still passes.
        // This guards against an accidental tightening that would change the
        // existing entity-write path. (ADR-0035)
        let (mgr, target) = setup().await;
        let kp = KeyPair::generate().unwrap();
        let indiv = CooperativeEntity::individual(kp.did(), "Suspended Founder");
        let caller = indiv.id.clone();
        mgr.register(indiv).await.unwrap();
        let mut m = Membership::active(caller.clone(), target.clone(), MembershipRole::Founder);
        m.status = MembershipStatus::Suspended;
        mgr.add_membership(m).await.unwrap();
        assert!(
            require_entity_access(&mgr, &caller, &target, EntityAction::ModifyEntity)
                .await
                .is_ok()
        );
    }

    // ---- observe mode ----

    #[tokio::test]
    async fn observe_agrees_allow_for_founder() {
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        let obs = observe_treasury_entity_access(
            &UnwiredCoopEntityResolver,
            &mgr,
            &caller,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        assert_eq!(obs, EntityAccessObservation::AgreesAllow);
    }

    #[tokio::test]
    async fn observe_entity_deny_is_observation_only_never_denies() {
        // A caller who passed the flat guard but is NOT a member yields an
        // EntityDeny *observation* — a value, not an error. The handler discards
        // it, so the request is never denied by this path. This is the core
        // observe-mode safety property.
        let (mgr, target) = setup().await;
        let _founder = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        let kp = KeyPair::generate().unwrap();
        let outsider = EntityId::from_did(kp.did());
        let obs = observe_treasury_entity_access(
            &UnwiredCoopEntityResolver,
            &mgr,
            &outsider,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryRead,
        )
        .await;
        assert_eq!(
            obs,
            EntityAccessObservation::EntityDeny(DenyReason::NonMember)
        );
    }

    #[tokio::test]
    async fn observe_indeterminate_when_entity_has_no_members() {
        let (mgr, target) = setup().await; // coop registered, no members added
        let kp = KeyPair::generate().unwrap();
        let caller = EntityId::from_did(kp.did());
        let obs = observe_treasury_entity_access(
            &UnwiredCoopEntityResolver,
            &mgr,
            &caller,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryRead,
        )
        .await;
        assert_eq!(
            obs,
            EntityAccessObservation::Indeterminate(IndeterminateReason::NoMemberships)
        );
    }

    #[tokio::test]
    async fn observe_missing_treasury_entity_id_when_unresolvable() {
        // No stored entity_id and a coop_id that is not a projectable slug.
        let (mgr, _target) = setup().await;
        let kp = KeyPair::generate().unwrap();
        let caller = EntityId::from_did(kp.did());
        let obs = observe_treasury_entity_access(
            &UnwiredCoopEntityResolver,
            &mgr,
            &caller,
            None,
            "bad_coop", // underscore -> not a valid EntityId slug
            EntityAction::TreasuryRead,
        )
        .await;
        assert_eq!(
            obs,
            EntityAccessObservation::Indeterminate(IndeterminateReason::MissingTreasuryEntityId)
        );
    }

    #[tokio::test]
    async fn observe_resolves_via_fallback_when_entity_id_none() {
        // entity_id is None, but the coop_id is a valid slug matching the
        // registered coop, so the fallback projection resolves the target.
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        let obs = observe_treasury_entity_access(
            &UnwiredCoopEntityResolver,
            &mgr,
            &caller,
            None,
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        assert_eq!(obs, EntityAccessObservation::AgreesAllow);
    }

    // A2c: the KEY safety property — with a fully-resolving, trusted store-backed
    // resolver wired, the treasury `EntityAccessObservation` (and therefore every
    // route outcome) is computed purely from the entity-membership path: byte-
    // identical to the unwired result. The resolver is observe-only and never an
    // authorization input.
    #[tokio::test]
    async fn store_backed_resolver_does_not_change_treasury_observation() {
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        let map = InMemoryCoopEntityMap::new();
        let entity = EntityId::cooperative("treasury-coop").expect("valid coop slug");
        map.bind_resolved_with_provenance(
            "treasury-coop",
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .expect("seed trusted binding");
        let store_backed = StoreBackedCoopEntityResolver::new(std::sync::Arc::new(map));

        // A founder is a member: AgreesAllow with EITHER resolver, byte-identical.
        let with_store = observe_treasury_entity_access(
            &store_backed,
            &mgr,
            &caller,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        let with_unwired = observe_treasury_entity_access(
            &UnwiredCoopEntityResolver,
            &mgr,
            &caller,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        assert_eq!(with_store, EntityAccessObservation::AgreesAllow);
        assert_eq!(with_store, with_unwired);

        // A non-member still observes EntityDeny with the trusted resolver wired —
        // resolving a coop_id→EntityId binding never upgrades a non-member to allow.
        let kp = KeyPair::generate().unwrap();
        let outsider = EntityId::from_did(kp.did());
        let deny = observe_treasury_entity_access(
            &store_backed,
            &mgr,
            &outsider,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryRead,
        )
        .await;
        assert_eq!(
            deny,
            EntityAccessObservation::EntityDeny(DenyReason::NonMember)
        );
    }

    // ---- A2d target evidence: membership target source / identity ----
    //
    // `compute_treasury_observation` now reports WHICH `EntityId` membership was
    // evaluated against and how it was chosen, so the gate can verify it against the
    // trusted resolver/agreed target.

    #[tokio::test]
    async fn membership_target_is_stored_treasury_entity_id_when_present() {
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        let obs = compute_treasury_observation(
            &mgr,
            &caller,
            Some(&target),
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        assert_eq!(obs.observation, EntityAccessObservation::AgreesAllow);
        assert_eq!(obs.membership_target.as_ref(), Some(&target));
        assert_eq!(
            obs.target_source,
            MembershipTargetSource::StoredTreasuryEntityId
        );
    }

    #[tokio::test]
    async fn membership_target_falls_back_to_legacy_projection_when_stored_absent() {
        // No stored entity_id, but the coop_id is a valid slug matching the registered
        // coop: the legacy projection supplies the target, and the source records that.
        let (mgr, target) = setup().await;
        let caller = add_active_member(&mgr, &target, MembershipRole::Founder).await;
        let obs = compute_treasury_observation(
            &mgr,
            &caller,
            None,
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        assert_eq!(obs.observation, EntityAccessObservation::AgreesAllow);
        assert_eq!(obs.membership_target.as_ref(), Some(&target));
        assert_eq!(obs.target_source, MembershipTargetSource::LegacyProjection);
    }

    #[tokio::test]
    async fn membership_target_absent_when_unresolvable_yields_missing_treasury_entity_id() {
        // No stored entity_id and a non-projectable coop_id: no target at all.
        let (mgr, _target) = setup().await;
        let kp = KeyPair::generate().unwrap();
        let caller = EntityId::from_did(kp.did());
        let obs = compute_treasury_observation(
            &mgr,
            &caller,
            None,
            "bad_coop", // underscore -> not a valid EntityId slug
            EntityAction::TreasuryRead,
        )
        .await;
        assert_eq!(
            obs.observation,
            EntityAccessObservation::Indeterminate(IndeterminateReason::MissingTreasuryEntityId)
        );
        assert!(obs.membership_target.is_none());
        assert_eq!(obs.target_source, MembershipTargetSource::None);
    }
}

// ============================================================================
// Community Proof Spine 0.1 (#2084) — fixture/dev runtime proof
// ============================================================================
//
// Proves the civic loop for a COMMUNITY-shaped entity end to end:
//
//   belonging (entity membership) -> standing -> authority (require_entity_access)
//     -> action -> receipt (blake3 bind) -> verification (recompute) -> explanation
//
// The community is modeled as an `icn-entity` Community entity
// (`EntityId::community(..)`) with `icn-entity` memberships — NOT the `icn-community`
// civic crate. This is deliberately the SMALLEST honest proof that the
// entity-authority spine (require_entity_access, RFC-0018 / #2079) can carry a
// community-shaped civic action; it is **not** the final community domain model and
// does not reconcile `icn-community` with `icn-entity`. The receipt hash is a
// self-contained ADR-0026-style binding, not a new persisted production receipt
// type. See `docs/spec/community-proof-spine-0.1.md`. A human-viewable mirror lives
// in the member-shell community fixture (clearly labeled fixture/dev).

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod community_proof_spine {
    use super::*;
    use icn_entity::CooperativeEntity;
    use icn_identity::KeyPair;

    /// Deterministic fixture timestamp (so the receipt hash is reproducible).
    const FIXTURE_AT: u64 = 1_750_000_000;

    /// The canonical content of a discharged community action card — the record a
    /// completion receipt binds over.
    #[derive(Clone)]
    struct CommunityActionRecord {
        community_id: String,
        action_id: String,
        title: String,
        actor_did: String,
        authority_basis: String,
        transition: String,
        at: u64,
    }

    impl CommunityActionRecord {
        /// ADR-0026-style binding: blake3 over a domain-separated, length-prefixed
        /// canonical encoding of the record's fields. The domain tag is written raw
        /// (no terminator) followed by length-prefixed fields, matching the
        /// icn-governance receipt-hash convention (`proof.rs` `DOMAIN_TAG`).
        /// Deterministic.
        fn record_hash(&self) -> [u8; 32] {
            let mut h = blake3::Hasher::new();
            h.update(b"icn:community:action_completion:v0");
            for field in [
                self.community_id.as_str(),
                self.action_id.as_str(),
                self.title.as_str(),
                self.actor_did.as_str(),
                self.authority_basis.as_str(),
                self.transition.as_str(),
            ] {
                h.update(&(field.len() as u64).to_le_bytes());
                h.update(field.as_bytes());
            }
            h.update(&self.at.to_le_bytes());
            *h.finalize().as_bytes()
        }
    }

    /// Plain-language "why you can / cannot act" for a community ModifyEntity action.
    /// Test-only seed for later member-shell surfacing (not a production API yet).
    fn explain_community_authority(allowed: bool) -> String {
        if allowed {
            "You can act on this community because you are a Founder or Board Member of it."
                .to_string()
        } else {
            "You cannot act on this community: this action requires Founder or Board Member \
             standing, which your membership does not carry."
                .to_string()
        }
    }

    /// Register a Community-type entity + an individual member with `role`, returning
    /// (manager, community EntityId, member EntityId).
    async fn community_with_member(role: MembershipRole) -> (EntityManager, EntityId, EntityId) {
        let mgr = EntityManager::new();
        let community =
            CooperativeEntity::community("maple-street-mutual-aid", "Maple Street Mutual Aid")
                .unwrap();
        let community_id = community.id.clone();
        mgr.register(community).await.unwrap();

        let kp = KeyPair::generate().unwrap();
        let member = CooperativeEntity::individual(kp.did(), "Member");
        let member_id = member.id.clone();
        mgr.register(member).await.unwrap();
        mgr.add_membership(Membership::active(
            member_id.clone(),
            community_id.clone(),
            role,
        ))
        .await
        .unwrap();
        (mgr, community_id, member_id)
    }

    /// Full positive civic loop: a Steward (Founder) of a community is authorized to
    /// complete a community action; the completion binds a receipt that verifies; the
    /// authority is explained in plain language.
    #[tokio::test]
    async fn community_civic_loop_steward_can_complete() {
        // belonging + standing: the steward holds an active Founder membership.
        let (mgr, community_id, steward) = community_with_member(MembershipRole::Founder).await;

        // authority: require_entity_access (the real RFC-0018 primitive) authorizes.
        let decision =
            require_entity_access(&mgr, &steward, &community_id, EntityAction::ModifyEntity).await;
        assert!(
            decision.is_ok(),
            "a Founder of the community must be authorized for the community action"
        );

        // action -> receipt: bind the discharged action card as a blake3 receipt.
        let record = CommunityActionRecord {
            community_id: community_id.as_str().to_string(),
            action_id: "action-charter-ratify-0001".to_string(),
            title: "Ratify the Maple Street Mutual Aid charter".to_string(),
            // The receipt binds a DID, not an entity id: recover the member's
            // did:icn DID from the individual EntityId (to_did is Some for individuals).
            actor_did: steward.to_did().unwrap().to_string(),
            authority_basis: "founder_of_community".to_string(),
            transition: "completed".to_string(),
            at: FIXTURE_AT,
        };
        let receipt_hash = record.record_hash();

        // verification: recomputing the hash over the same record matches.
        assert_eq!(
            receipt_hash,
            record.record_hash(),
            "receipt hash must verify by recompute"
        );

        // explanation: a plain-language authority basis is produced.
        let why = explain_community_authority(true);
        assert!(why.contains("Founder or Board Member"));
    }

    /// Negative path: a plain Member of the same community is denied the action, and
    /// the denial is explained in plain language.
    #[tokio::test]
    async fn community_civic_loop_plain_member_denied() {
        let (mgr, community_id, member) = community_with_member(MembershipRole::Member).await;

        let decision =
            require_entity_access(&mgr, &member, &community_id, EntityAction::ModifyEntity).await;
        assert!(
            decision.is_err(),
            "a plain Member must NOT be authorized for the Founder/BoardMember community action"
        );

        let why = explain_community_authority(false);
        assert!(why.contains("requires Founder or Board Member"));
    }

    /// Tampering with ANY bound field changes the receipt hash (integrity).
    /// Mutates each field of the canonical record once — including the timestamp —
    /// so the proof actually backs the spec's "any bound field" claim: a field
    /// silently dropped from `record_hash()` would fail here, not pass.
    #[tokio::test]
    async fn community_action_receipt_detects_tampering() {
        let baseline = CommunityActionRecord {
            community_id: "entity:icn:community:maple-street-mutual-aid".to_string(),
            action_id: "action-charter-ratify-0001".to_string(),
            title: "Ratify the Maple Street Mutual Aid charter".to_string(),
            actor_did: "did:icn:zsteward-demo-not-live".to_string(),
            authority_basis: "founder_of_community".to_string(),
            transition: "completed".to_string(),
            at: FIXTURE_AT,
        };
        let base_hash = baseline.record_hash();

        // Each closure mutates exactly one bound field; the resulting hash must differ.
        type FieldMutation = (&'static str, fn(&mut CommunityActionRecord));
        let cases: [FieldMutation; 7] = [
            ("community_id", |r| {
                r.community_id = "entity:icn:community:elm-street-mutual-aid".to_string()
            }),
            ("action_id", |r| {
                r.action_id = "action-charter-ratify-0002".to_string()
            }),
            ("title", |r| {
                r.title = "Ratify a DIFFERENT charter".to_string()
            }),
            ("actor_did", |r| {
                r.actor_did = "did:icn:zother-demo-not-live".to_string()
            }),
            ("authority_basis", |r| {
                r.authority_basis = "board_member_of_community".to_string()
            }),
            ("transition", |r| r.transition = "rejected".to_string()),
            ("at", |r| r.at = FIXTURE_AT + 1),
        ];
        for (field, mutate) in cases {
            let mut tampered = baseline.clone();
            mutate(&mut tampered);
            assert_ne!(
                base_hash,
                tampered.record_hash(),
                "tampering with `{field}` must change the receipt hash"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod coop_resolution_observe_tests {
    use super::*;

    fn coop(slug: &str) -> EntityId {
        EntityId::cooperative(slug).unwrap()
    }

    #[test]
    fn classify_agree_when_legacy_and_resolver_match() {
        let id = coop("food-coop");
        let out = classify_coop_resolution(
            Some(&id),
            &CoopEntityResolution::Resolved {
                entity_id: id.clone(),
            },
        );
        assert_eq!(out, CoopResolutionObservation::Agree);
        assert_eq!(out.result_label(), "agree");
    }

    #[test]
    fn classify_disagree_when_legacy_and_resolver_differ() {
        let legacy = coop("food-coop");
        let resolved = coop("other-coop");
        let out = classify_coop_resolution(
            Some(&legacy),
            &CoopEntityResolution::Resolved {
                entity_id: resolved,
            },
        );
        assert_eq!(out, CoopResolutionObservation::Disagree);
        assert_eq!(out.result_label(), "disagree");
    }

    #[test]
    fn classify_resolver_only_when_legacy_not_mappable() {
        let resolved = coop("food-coop");
        let out = classify_coop_resolution(
            None,
            &CoopEntityResolution::Resolved {
                entity_id: resolved,
            },
        );
        assert_eq!(out, CoopResolutionObservation::ResolverOnly);
    }

    #[test]
    fn classify_legacy_only_when_resolver_unavailable() {
        let legacy = coop("food-coop");
        let out = classify_coop_resolution(
            Some(&legacy),
            &CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::NoTrustedSourceWired,
            },
        );
        assert_eq!(
            out,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::NoTrustedSourceWired)
        );
        assert_eq!(out.result_label(), "legacy_only");
        assert_eq!(out.reason_label(), "no_trusted_source_wired");
    }

    #[test]
    fn classify_neither_when_legacy_unmappable_and_resolver_unavailable() {
        let out = classify_coop_resolution(
            None,
            &CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::NoTrustedSourceWired,
            },
        );
        assert_eq!(
            out,
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::NoTrustedSourceWired)
        );
        assert_eq!(out.result_label(), "neither_resolved");
    }

    // The default (unwired) resolver must never let the observation become a
    // success-bearing one: with the fail-closed default, a mappable coop_id can only
    // ever land on LegacyOnly (never Agree/ResolverOnly), because the resolver
    // fabricates no EntityId.
    #[tokio::test]
    async fn default_resolver_observe_mappable_coop_is_legacy_only_never_fabricates() {
        let out = observe_coop_entity_resolution(&UnwiredCoopEntityResolver, "food-coop").await;
        assert_eq!(
            out.classification,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::NoTrustedSourceWired)
        );
        // A2d evidence: `LegacyOnly` carries the legacy target, no resolver target.
        assert_eq!(out.legacy_target.as_ref(), Some(&coop("food-coop")));
        assert!(out.resolver_target.is_none());
    }

    #[tokio::test]
    async fn default_resolver_observe_unmappable_coop_is_neither_resolved() {
        // `coop_A` is not a valid cooperative slug (uppercase), so the legacy
        // projection rejects it and the unwired resolver is unavailable.
        let out = observe_coop_entity_resolution(&UnwiredCoopEntityResolver, "coop_A").await;
        assert_eq!(
            out.classification,
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::NoTrustedSourceWired)
        );
        // A2d evidence: `NeitherResolved` carries no target on either side.
        assert!(out.legacy_target.is_none());
        assert!(out.resolver_target.is_none());
    }

    // The unwired default never produces a Resolved/Agree/ResolverOnly outcome for
    // any coop_id, mappable or not — proving observe-mode consults the resolver but
    // can never treat it as a trusted mapping in A2b.
    #[tokio::test]
    async fn default_resolver_observe_is_never_a_trusted_resolution() {
        for coop_id in ["food-coop", "coop_A", "", "coop:7f3a2b", "x-coop"] {
            let out = observe_coop_entity_resolution(&UnwiredCoopEntityResolver, coop_id).await;
            assert!(
                !matches!(
                    out.classification,
                    CoopResolutionObservation::Agree
                        | CoopResolutionObservation::ResolverOnly
                        | CoopResolutionObservation::Disagree
                ),
                "unwired resolver must never yield a resolver-trusting outcome for {coop_id:?}, got {out:?}"
            );
            // The unwired resolver fabricates no target.
            assert!(out.resolver_target.is_none());
        }
    }

    // ----------------------------------------------------------------------
    // A2c: observe-mode wiring of the trusted, store-backed resolver.
    //
    // These prove the observe path consults a real `StoreBackedCoopEntityResolver`
    // when one is wired — producing genuine Agree / ResolverOnly / LegacyOnly
    // classifications from trusted provenance — while the entity-membership
    // observation (and therefore every treasury route outcome) is UNCHANGED.
    // The default (no store) path is proven by the `default_resolver_*` tests above.
    // ----------------------------------------------------------------------
    use crate::coop_entity_resolver::{StoreBackedCoopEntityResolver, UnwiredCoopEntityResolver};
    use icn_entity::{
        CoopEntityBindingProvenance, CoopEntityMap, CoopEntityMapError, InMemoryCoopEntityMap,
    };

    fn store_backed_for(
        map: impl CoopEntityMap + Send + Sync + 'static,
    ) -> StoreBackedCoopEntityResolver {
        StoreBackedCoopEntityResolver::new(std::sync::Arc::new(map))
    }

    // (B) Trusted Activation binding: `food-coop` legacy-projects to
    // `EntityId::cooperative("food-coop")`, and a trusted binding records the same
    // EntityId, so the observation agrees.
    #[tokio::test]
    async fn store_backed_observe_activation_binding_is_agree() {
        let map = InMemoryCoopEntityMap::new();
        let entity = EntityId::cooperative("food-coop").expect("valid coop slug");
        map.bind_resolved_with_provenance(
            "food-coop",
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .expect("seed trusted activation binding");
        let out = observe_coop_entity_resolution(&store_backed_for(map), "food-coop").await;
        assert_eq!(out.classification, CoopResolutionObservation::Agree);
        // A2d evidence: `Agree` carries the SAME EntityId from BOTH the legacy projection
        // and the resolver.
        assert_eq!(out.legacy_target.as_ref(), Some(&entity));
        assert_eq!(out.resolver_target.as_ref(), Some(&entity));
    }

    // Disagree: a trusted binding maps `food-coop` to a DIFFERENT cooperative than the
    // legacy projection (`food-coop`), so the evidence carries BOTH conflicting targets.
    #[tokio::test]
    async fn store_backed_observe_disagree_carries_both_targets() {
        let map = InMemoryCoopEntityMap::new();
        let resolver_entity = EntityId::cooperative("other-coop").expect("valid coop slug");
        map.bind_resolved_with_provenance(
            "food-coop",
            &resolver_entity,
            CoopEntityBindingProvenance::Activation,
        )
        .expect("seed trusted (conflicting) binding");
        let out = observe_coop_entity_resolution(&store_backed_for(map), "food-coop").await;
        assert_eq!(out.classification, CoopResolutionObservation::Disagree);
        assert_eq!(out.legacy_target.as_ref(), Some(&coop("food-coop")));
        assert_eq!(out.resolver_target.as_ref(), Some(&resolver_entity));
        assert_ne!(out.legacy_target, out.resolver_target);
    }

    // (C) Trusted OperatorBackfill surrogate: a non-mappable default `coop:<uuid>`
    // (the legacy projection rejects it) bound to its cooperative surrogate with
    // OperatorBackfill provenance resolves through the existing trust gate — so the
    // resolver produces an EntityId the legacy path cannot: ResolverOnly.
    #[tokio::test]
    async fn store_backed_observe_operator_backfill_surrogate_is_resolver_only() {
        let map = InMemoryCoopEntityMap::new();
        let surrogate = EntityId::cooperative("coop-legacy-abc123").expect("valid coop slug");
        map.bind_resolved_with_provenance(
            "coop:7f3a2b",
            &surrogate,
            CoopEntityBindingProvenance::OperatorBackfill,
        )
        .expect("seed operator-backfill binding");
        let out = observe_coop_entity_resolution(&store_backed_for(map), "coop:7f3a2b").await;
        assert_eq!(out.classification, CoopResolutionObservation::ResolverOnly);
        // A2d evidence: `ResolverOnly` carries the resolver target; the legacy projection
        // produced none.
        assert!(out.legacy_target.is_none());
        assert_eq!(out.resolver_target.as_ref(), Some(&surrogate));
    }

    // (D) UnknownLegacy: an unprovenanced binding (plain `bind_resolved`) is the
    // fail-closed sentinel — it must never resolve. The observation is
    // LegacyOnly(UnverifiedProvenance), never Agree.
    #[tokio::test]
    async fn store_backed_observe_unknown_legacy_is_not_trusted() {
        let map = InMemoryCoopEntityMap::new();
        let entity = EntityId::cooperative("food-coop").expect("valid coop slug");
        map.bind_resolved("food-coop", &entity)
            .expect("seed unprovenanced (UnknownLegacy) binding");
        let out = observe_coop_entity_resolution(&store_backed_for(map), "food-coop").await;
        assert_eq!(
            out.classification,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::UnverifiedProvenance)
        );
        assert!(!matches!(
            out.classification,
            CoopResolutionObservation::Agree
        ));
        // A2d evidence: an untrusted binding yields NO resolver target — only the legacy
        // projection. It can never be a trusted target for enforcement.
        assert_eq!(out.legacy_target.as_ref(), Some(&entity));
        assert!(out.resolver_target.is_none());
    }

    /// A `CoopEntityMap` whose reads always fail — proves observe fails closed on a
    /// backend error (never a trusted agreement).
    struct FailingObserveMap;
    // NOTE: `Result` in this module is the gateway's `crate::error::Result`
    // (GatewayError) alias, so the `CoopEntityMap` trait methods are spelled with
    // fully-qualified `std::result::Result<_, CoopEntityMapError>`.
    impl CoopEntityMap for FailingObserveMap {
        fn bind_resolved(
            &self,
            _c: &str,
            _e: &EntityId,
        ) -> std::result::Result<(), CoopEntityMapError> {
            Ok(())
        }
        fn entity_for_coop(
            &self,
            _c: &str,
        ) -> std::result::Result<Option<EntityId>, CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "simulated backend failure".into(),
            ))
        }
        fn coop_for_entity(
            &self,
            _e: &EntityId,
        ) -> std::result::Result<Option<String>, CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "simulated backend failure".into(),
            ))
        }
    }

    // (E) Backend error fails closed: `food-coop` legacy-projects, but the store
    // errors, so the resolver is SourceUnavailable → LegacyOnly(SourceUnavailable),
    // never a trusted Agree. (Ambiguous / entity-type-mismatch fail-closed cases are
    // proven at the resolver layer in `coop_entity_resolver.rs`.)
    #[tokio::test]
    async fn store_backed_observe_backend_error_fails_closed() {
        let out =
            observe_coop_entity_resolution(&store_backed_for(FailingObserveMap), "food-coop").await;
        assert_eq!(
            out.classification,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::SourceUnavailable)
        );
        // A2d evidence: a backend error yields no resolver target (fail closed).
        assert!(out.resolver_target.is_none());
    }

    // ----------------------------------------------------------------------
    // A2d: treasury entity-auth gate decision (pure), WITH target verification.
    //
    // The shipped default (ObserveOnly) never alters a route. EnforceTrustedResolver
    // requires a TRUSTED coop→EntityId basis (Agree / ResolverOnly), then the
    // entity-membership observation decides — and a would-allow additionally requires
    // the membership target to equal the trusted target. Route safety
    // (observe_treasury_entity_access returns the unchanged observation) is proven by
    // `entity_access_tests::store_backed_resolver_does_not_change_treasury_observation`.
    // ----------------------------------------------------------------------
    use TreasuryEntityAuthMode::{EnforceTrustedResolver, ObserveOnly};
    use TreasuryGateDecision::{ProceedUnchanged, WouldDeny};
    use TreasuryGateDenyReason::{
        AgreeTargetUnverified, IndeterminateMembership, NotMember, ObservationError,
        ResolverConflict, ResolverOnlyTargetUnverified, UntrustedResolution,
    };

    fn allow() -> EntityAccessObservation {
        EntityAccessObservation::AgreesAllow
    }
    fn deny() -> EntityAccessObservation {
        EntityAccessObservation::EntityDeny(DenyReason::NonMember)
    }
    fn indeterminate() -> EntityAccessObservation {
        EntityAccessObservation::Indeterminate(IndeterminateReason::NoMemberships)
    }
    fn errored() -> EntityAccessObservation {
        EntityAccessObservation::Error("entity manager unavailable".into())
    }
    /// UnknownLegacy / unprovenanced rows (incl. gossip-originated) read back as
    /// `UnverifiedProvenance` → the resolver is unavailable → `LegacyOnly`.
    fn untrusted() -> CoopResolutionObservation {
        CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::UnverifiedProvenance)
    }

    /// Build gate evidence from parts. `target_source` is derived from whether a
    /// membership target is present (it is logged but does not affect the decision —
    /// only `membership_target` and the resolution targets do).
    fn evidence(
        observation: EntityAccessObservation,
        membership_target: Option<EntityId>,
        classification: CoopResolutionObservation,
        legacy_target: Option<EntityId>,
        resolver_target: Option<EntityId>,
    ) -> TreasuryGateEvidence {
        let target_source = if membership_target.is_some() {
            MembershipTargetSource::StoredTreasuryEntityId
        } else {
            MembershipTargetSource::None
        };
        TreasuryGateEvidence {
            membership: TreasuryMembershipObservation {
                observation,
                membership_target,
                target_source,
            },
            resolution: CoopResolutionEvidence {
                classification,
                legacy_target,
                resolver_target,
            },
        }
    }

    /// `Agree` evidence: the legacy projection and resolver both name `agreed`.
    fn agree(
        observation: EntityAccessObservation,
        membership_target: Option<EntityId>,
        agreed: EntityId,
    ) -> TreasuryGateEvidence {
        evidence(
            observation,
            membership_target,
            CoopResolutionObservation::Agree,
            Some(agreed.clone()),
            Some(agreed),
        )
    }

    /// `ResolverOnly` evidence: only the resolver names `resolved` (legacy is None).
    fn resolver_only(
        observation: EntityAccessObservation,
        membership_target: Option<EntityId>,
        resolved: EntityId,
    ) -> TreasuryGateEvidence {
        evidence(
            observation,
            membership_target,
            CoopResolutionObservation::ResolverOnly,
            None,
            Some(resolved),
        )
    }

    // (A) ObserveOnly (the shipped default) NEVER denies, for every observation ×
    // resolution combination — byte-identical route behavior. No route consumes the
    // enforcement decision (the active mode is ObserveOnly); this is the route-safety
    // contract for this slice.
    #[test]
    fn gate_observe_only_always_proceeds_unchanged() {
        let t = coop("food-coop");
        let other = coop("other-coop");
        for o in [allow(), deny(), indeterminate(), errored()] {
            let cases = [
                agree(o.clone(), Some(t.clone()), t.clone()),
                agree(o.clone(), None, t.clone()),
                resolver_only(o.clone(), Some(t.clone()), t.clone()),
                evidence(
                    o.clone(),
                    Some(t.clone()),
                    CoopResolutionObservation::Disagree,
                    Some(t.clone()),
                    Some(other.clone()),
                ),
                evidence(
                    o.clone(),
                    Some(t.clone()),
                    untrusted(),
                    Some(t.clone()),
                    None,
                ),
                evidence(
                    o.clone(),
                    None,
                    CoopResolutionObservation::NeitherResolved(
                        ResolutionUnavailable::NoTrustedSourceWired,
                    ),
                    None,
                    None,
                ),
            ];
            for ev in &cases {
                assert_eq!(
                    decide_treasury_gate(ObserveOnly, ev),
                    ProceedUnchanged,
                    "ObserveOnly must never deny: {ev:?}"
                );
            }
        }
    }

    // (B) Agree + verified matching membership target + affirmative member → proceed.
    #[test]
    fn gate_enforce_agree_verified_target_allows() {
        let t = coop("food-coop");
        let ev = agree(allow(), Some(t.clone()), t);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev),
            ProceedUnchanged
        );
    }

    // (B) Agree + affirmative member but MISMATCHED membership target → fail closed.
    #[test]
    fn gate_enforce_agree_mismatched_target_fails_closed() {
        let agreed = coop("food-coop");
        let other = coop("other-coop");
        let ev = agree(allow(), Some(other), agreed);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev),
            WouldDeny(AgreeTargetUnverified)
        );
    }

    // (B) Agree + affirmative member but MISSING membership target → fail closed.
    #[test]
    fn gate_enforce_agree_missing_target_fails_closed() {
        let agreed = coop("food-coop");
        let ev = agree(allow(), None, agreed);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev),
            WouldDeny(AgreeTargetUnverified)
        );
    }

    // (B) Agree + non-member → NotMember, EVEN with a verified matching target. The
    // resolver mapping verifies the target; membership is the authority.
    #[test]
    fn gate_enforce_agree_non_member_denies() {
        let t = coop("food-coop");
        let ev = agree(deny(), Some(t.clone()), t);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev),
            WouldDeny(NotMember)
        );
    }

    // (B) Agree + indeterminate / errored membership → fail closed (regardless of target).
    #[test]
    fn gate_enforce_agree_membership_gaps_fail_closed() {
        let t = coop("food-coop");
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &agree(indeterminate(), Some(t.clone()), t.clone())
            ),
            WouldDeny(IndeterminateMembership)
        );
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &agree(errored(), Some(t.clone()), t)
            ),
            WouldDeny(ObservationError)
        );
    }

    // (C) ResolverOnly + membership target equal to the resolver target + affirmative
    // member → proceed. This is the NEW capability: ResolverOnly is now enforceable when
    // the resolved target was actually the membership target.
    #[test]
    fn gate_enforce_resolver_only_verified_target_allows() {
        let t = coop("coop-legacy-abc123");
        let ev = resolver_only(allow(), Some(t.clone()), t);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev),
            ProceedUnchanged
        );
    }

    // (C) ResolverOnly + affirmative member but missing / mismatched membership target →
    // fail closed with ResolverOnlyTargetUnverified.
    #[test]
    fn gate_enforce_resolver_only_unverified_target_fails_closed() {
        let resolved = coop("coop-legacy-abc123");
        let ev_mismatch = resolver_only(allow(), Some(coop("other-coop")), resolved.clone());
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev_mismatch),
            WouldDeny(ResolverOnlyTargetUnverified)
        );
        let ev_missing = resolver_only(allow(), None, resolved);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &ev_missing),
            WouldDeny(ResolverOnlyTargetUnverified)
        );
    }

    // (C) ResolverOnly + non-member / indeterminate / errored membership → the existing
    // entity-membership signal decides (NotMember / gap), not target-unverified.
    #[test]
    fn gate_enforce_resolver_only_membership_denies_and_gaps() {
        let t = coop("coop-legacy-abc123");
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &resolver_only(deny(), Some(t.clone()), t.clone())
            ),
            WouldDeny(NotMember)
        );
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &resolver_only(indeterminate(), Some(t.clone()), t.clone())
            ),
            WouldDeny(IndeterminateMembership)
        );
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &resolver_only(errored(), Some(t.clone()), t)
            ),
            WouldDeny(ObservationError)
        );
    }

    // (D) resolver/legacy DISAGREE (migration collision) → ResolverConflict, regardless
    // of the membership signal.
    #[test]
    fn gate_enforce_disagreement_denies() {
        let legacy = coop("food-coop");
        let resolved = coop("other-coop");
        for obs in [allow(), deny(), indeterminate(), errored()] {
            let ev = evidence(
                obs.clone(),
                Some(legacy.clone()),
                CoopResolutionObservation::Disagree,
                Some(legacy.clone()),
                Some(resolved.clone()),
            );
            assert_eq!(
                decide_treasury_gate(EnforceTrustedResolver, &ev),
                WouldDeny(ResolverConflict),
                "Disagree must fail closed: {obs:?}"
            );
        }
    }

    // (E,F) Untrusted basis — UnknownLegacy / gossip / unprovenanced (UnverifiedProvenance),
    // source-unavailable / backend error, no-trusted-source-wired, for both LegacyOnly and
    // NeitherResolved carriers — fails closed with UntrustedResolution REGARDLESS of the
    // membership signal. The gate never treats a mapping as authority and never lets an
    // untrusted basis through on a member.
    #[test]
    fn gate_enforce_untrusted_resolution_denies_regardless_of_membership() {
        let t = coop("food-coop");
        let resolutions = [
            untrusted(),
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::SourceUnavailable),
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::NoTrustedSourceWired),
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::SourceUnavailable),
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::NoTrustedSourceWired),
        ];
        for r in resolutions {
            for obs in [allow(), deny(), indeterminate(), errored()] {
                let ev = evidence(obs.clone(), Some(t.clone()), r, Some(t.clone()), None);
                assert_eq!(
                    decide_treasury_gate(EnforceTrustedResolver, &ev),
                    WouldDeny(UntrustedResolution),
                    "untrusted resolution must fail closed regardless of membership: \
                     resolution={r:?} obs={obs:?}"
                );
            }
        }
    }

    // (F) The core trust invariant: a resolver mapping is a target *verifier*, never
    // authority. An untrusted basis with an affirmative member AND a matching membership
    // target still fails closed; a trusted ResolverOnly basis with a verified target but a
    // non-member is NotMember (membership remains the authority signal).
    #[test]
    fn gate_enforce_resolver_mapping_is_never_authority() {
        let t = coop("food-coop");
        let untrusted_but_matching =
            evidence(allow(), Some(t.clone()), untrusted(), Some(t.clone()), None);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &untrusted_but_matching),
            WouldDeny(UntrustedResolution)
        );
        let trusted_but_non_member = resolver_only(deny(), Some(t.clone()), t);
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &trusted_but_non_member),
            WouldDeny(NotMember)
        );
    }

    // (Logging) `ResolverConflict` is the only high-signal would-deny → `warn!`; every
    // other gate decision (ordinary would-deny, proceed) is low-noise `debug!`.
    #[test]
    fn record_treasury_gate_logs_resolver_conflict_at_warn_others_at_debug() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct BufWriter(Arc<Mutex<Vec<u8>>>);
        impl Write for BufWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for BufWriter {
            type Writer = BufWriter;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        fn capture(would_enforce: TreasuryGateDecision, ev: &TreasuryGateEvidence) -> String {
            let buf = BufWriter::default();
            let subscriber = tracing_subscriber::fmt()
                .with_writer(buf.clone())
                .with_max_level(tracing::Level::DEBUG)
                .with_ansi(false)
                .finish();
            tracing::subscriber::with_default(subscriber, || {
                record_treasury_gate(
                    &coop("food-coop"),
                    EntityAction::TreasuryWrite,
                    ev,
                    ProceedUnchanged,
                    would_enforce,
                );
            });
            let bytes = buf.0.lock().unwrap().clone();
            String::from_utf8(bytes).unwrap()
        }

        // ResolverConflict → WARN.
        let legacy = coop("food-coop");
        let resolved = coop("other-coop");
        let conflict_ev = evidence(
            allow(),
            Some(legacy.clone()),
            CoopResolutionObservation::Disagree,
            Some(legacy),
            Some(resolved),
        );
        let warn_out = capture(WouldDeny(ResolverConflict), &conflict_ev);
        assert!(
            warn_out.contains("WARN"),
            "ResolverConflict must log at WARN: {warn_out:?}"
        );
        assert!(warn_out.contains("entity_authz_gate"));

        // An ordinary would-deny → DEBUG, not WARN.
        let t = coop("food-coop");
        let ordinary_ev = resolver_only(allow(), None, t);
        let debug_out = capture(WouldDeny(ResolverOnlyTargetUnverified), &ordinary_ev);
        assert!(
            debug_out.contains("DEBUG"),
            "ordinary deny must log at DEBUG: {debug_out:?}"
        );
        assert!(
            !debug_out.contains("WARN"),
            "ordinary deny must NOT log at WARN: {debug_out:?}"
        );
    }
}
