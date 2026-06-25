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
            TreasuryGateDenyReason::IndeterminateMembership => "indeterminate_membership",
            TreasuryGateDenyReason::ObservationError => "observation_error",
        }
    }
}

/// Pure A2d gate decision for a treasury request (no side effects).
///
/// Inputs are the two already-computed observe-mode signals — the entity-membership
/// `observation` and the `coop_id → EntityId` `resolution` classification — plus the
/// `mode`. The function NEVER consults a mapping as authority: under enforcement the
/// `resolution` is used only as a *trust qualifier* (is there a trusted basis at all?),
/// and the actual allow/deny signal is the entity-membership `observation`.
///
/// - [`TreasuryEntityAuthMode::ObserveOnly`] (the shipped default) ALWAYS returns
///   [`TreasuryGateDecision::ProceedUnchanged`]: byte-identical to the flat guard,
///   regardless of the observation or resolution.
/// - [`TreasuryEntityAuthMode::EnforceTrustedResolver`] (decision-only; not wired to any
///   route here) requires a trusted basis first, then defers to membership:
///   - `Agree` / `ResolverOnly` → trusted basis established;
///   - `Disagree` → `WouldDeny(ResolverConflict)` (migration collision; fail closed);
///   - `LegacyOnly(_)` / `NeitherResolved(_)` → `WouldDeny(UntrustedResolution)` — covers
///     `UnknownLegacy`, gossip-originated/unprovenanced rows, ambiguous bindings,
///     entity-type mismatch, and backend/source errors (all fail closed);
///   - with a trusted basis: `AgreesAllow` → `ProceedUnchanged`; `EntityDeny` →
///     `WouldDeny(NotMember)`; `Indeterminate` → `WouldDeny(IndeterminateMembership)`;
///     `Error` → `WouldDeny(ObservationError)`.
pub(crate) fn decide_treasury_gate(
    mode: TreasuryEntityAuthMode,
    observation: &EntityAccessObservation,
    resolution: &CoopResolutionObservation,
) -> TreasuryGateDecision {
    match mode {
        // Default, shipped behavior: never alter the route.
        TreasuryEntityAuthMode::ObserveOnly => TreasuryGateDecision::ProceedUnchanged,
        TreasuryEntityAuthMode::EnforceTrustedResolver => {
            // (1) A trusted coop→EntityId basis is required before any enforcement.
            //     An untrusted/unavailable/ambiguous mapping fails closed BEFORE the
            //     membership signal is even consulted — you cannot trust the membership
            //     target if you cannot trust the coop→entity mapping that named it.
            match resolution {
                CoopResolutionObservation::Agree | CoopResolutionObservation::ResolverOnly => {}
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
            }
            // (2) With a trusted basis, the entity-membership observation decides.
            match observation {
                EntityAccessObservation::AgreesAllow => TreasuryGateDecision::ProceedUnchanged,
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
/// decision. Logging only — this denies nothing and changes no route outcome.
fn record_treasury_gate(
    caller: &EntityId,
    action: EntityAction,
    active: TreasuryGateDecision,
    would_enforce: TreasuryGateDecision,
) {
    match would_enforce {
        TreasuryGateDecision::WouldDeny(reason) => warn!(
            target: "entity_authz_gate",
            caller = %caller,
            action = action.metric_label(),
            active = active.outcome_label(),
            would_enforce = would_enforce.outcome_label(),
            reason = reason.label(),
            "treasury entity gate WOULD DENY under enforcement (observe-only; flat coop guard remains authoritative)"
        ),
        TreasuryGateDecision::ProceedUnchanged => debug!(
            target: "entity_authz_gate",
            caller = %caller,
            action = action.metric_label(),
            active = active.outcome_label(),
            would_enforce = would_enforce.outcome_label(),
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
    let observation =
        compute_treasury_observation(entity_mgr, caller, treasury_entity_id, coop_id, action).await;

    gateway_metrics::entity_authz_observation_inc(
        "treasury",
        action.metric_label(),
        observation.result_label(),
        observation.reason_label(),
    );

    match &observation {
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
    let active_gate =
        decide_treasury_gate(ACTIVE_TREASURY_ENTITY_AUTH_MODE, &observation, &resolution);
    let would_enforce_gate = decide_treasury_gate(
        TreasuryEntityAuthMode::EnforceTrustedResolver,
        &observation,
        &resolution,
    );
    record_treasury_gate(caller, action, active_gate, would_enforce_gate);

    observation
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
/// between the legacy projection and the governed resolver seam.
///
/// Returns the classification for testability; production callers discard it. It
/// performs no authorization, denies nothing, and never fabricates an `EntityId`.
pub(crate) async fn observe_coop_entity_resolution(
    resolver: &dyn CoopEntityResolver,
    coop_id: &str,
) -> CoopResolutionObservation {
    let resolver_outcome = resolver.resolve_coop_entity(coop_id).await;
    let legacy_target = legacy_coop_id_to_entity_id_fallback(coop_id).ok();
    let observation = classify_coop_resolution(legacy_target.as_ref(), &resolver_outcome);

    match observation {
        CoopResolutionObservation::Disagree => {
            // Log both identifiers so the collision is actionable for diagnosing
            // provenance/mapping issues once A2c wires a real resolver. An `EntityId`
            // is an institutional identifier (slug), not a secret.
            warn!(
                target: "coop_resolution_observe",
                coop_id = %coop_id,
                result = observation.result_label(),
                legacy_entity_id = legacy_target.as_ref().map_or("<none>", |e| e.as_str()),
                resolver_entity_id =
                    resolver_outcome.resolved_entity_id().map_or("<none>", |e| e.as_str()),
                "legacy projection and resolver DISAGREE on coop_id -> EntityId (observe-only; flat coop guard remains authoritative)"
            );
        }
        _ => {
            debug!(
                target: "coop_resolution_observe",
                coop_id = %coop_id,
                result = observation.result_label(),
                reason = observation.reason_label(),
                "coop_id -> EntityId resolution observation (observe-only)"
            );
        }
    }

    observation
}

/// Pure decision half of [`observe_treasury_entity_access`] (no side effects).
async fn compute_treasury_observation(
    entity_mgr: &EntityManager,
    caller: &EntityId,
    treasury_entity_id: Option<&EntityId>,
    coop_id: &str,
    action: EntityAction,
) -> EntityAccessObservation {
    // Resolve the target entity: prefer the treasury's stored EntityId, else a
    // best-effort (reject-not-normalize) projection of the flat coop_id.
    let target = match treasury_entity_id {
        Some(id) => id.clone(),
        None => match legacy_coop_id_to_entity_id_fallback(coop_id) {
            Ok(id) => id,
            Err(_) => {
                return EntityAccessObservation::Indeterminate(
                    IndeterminateReason::MissingTreasuryEntityId,
                );
            }
        },
    };

    match entity_mgr.get(&target).await {
        Err(e) => return EntityAccessObservation::Error(e.to_string()),
        Ok(None) => {
            return EntityAccessObservation::Indeterminate(
                IndeterminateReason::MissingEntityRecord,
            );
        }
        Ok(Some(_)) => {}
    }

    let members = match entity_mgr.get_members(&target).await {
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
            out,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::NoTrustedSourceWired)
        );
    }

    #[tokio::test]
    async fn default_resolver_observe_unmappable_coop_is_neither_resolved() {
        // `coop_A` is not a valid cooperative slug (uppercase), so the legacy
        // projection rejects it and the unwired resolver is unavailable.
        let out = observe_coop_entity_resolution(&UnwiredCoopEntityResolver, "coop_A").await;
        assert_eq!(
            out,
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::NoTrustedSourceWired)
        );
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
                    out,
                    CoopResolutionObservation::Agree
                        | CoopResolutionObservation::ResolverOnly
                        | CoopResolutionObservation::Disagree
                ),
                "unwired resolver must never yield a resolver-trusting outcome for {coop_id:?}, got {out:?}"
            );
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
        assert_eq!(out, CoopResolutionObservation::Agree);
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
        assert_eq!(out, CoopResolutionObservation::ResolverOnly);
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
            out,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::UnverifiedProvenance)
        );
        assert!(!matches!(out, CoopResolutionObservation::Agree));
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
            out,
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::SourceUnavailable)
        );
    }

    // ----------------------------------------------------------------------
    // A2d: treasury entity-auth gate decision (pure). The shipped default
    // (ObserveOnly) never alters a route; EnforceTrustedResolver fails closed unless a
    // TRUSTED coop→EntityId basis AND an affirmative membership both hold. Route safety
    // (observe_treasury_entity_access returns the unchanged observation) is proven by
    // `entity_access_tests::store_backed_resolver_does_not_change_treasury_observation`.
    // ----------------------------------------------------------------------
    use TreasuryEntityAuthMode::{EnforceTrustedResolver, ObserveOnly};
    use TreasuryGateDecision::{ProceedUnchanged, WouldDeny};
    use TreasuryGateDenyReason::{
        IndeterminateMembership, NotMember, ObservationError, ResolverConflict, UntrustedResolution,
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

    // (A) ObserveOnly (the shipped default) NEVER denies, for every observation ×
    // resolution combination — byte-identical route behavior.
    #[test]
    fn gate_observe_only_always_proceeds_unchanged() {
        let observations = [allow(), deny(), indeterminate(), errored()];
        let resolutions = [
            CoopResolutionObservation::Agree,
            CoopResolutionObservation::Disagree,
            CoopResolutionObservation::ResolverOnly,
            untrusted(),
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::SourceUnavailable),
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::NoTrustedSourceWired),
        ];
        for o in &observations {
            for r in &resolutions {
                assert_eq!(
                    decide_treasury_gate(ObserveOnly, o, r),
                    ProceedUnchanged,
                    "ObserveOnly must never deny: observation={o:?} resolution={r:?}"
                );
            }
        }
    }

    // (B) Enforcement: trusted agreement allows.
    #[test]
    fn gate_enforce_trusted_agreement_allows() {
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &allow(),
                &CoopResolutionObservation::Agree
            ),
            ProceedUnchanged
        );
    }

    // (B) Enforcement: a trusted resolver-only basis (an activation/backfill surrogate
    // the legacy projection can't produce) still allows an affirmative member —
    // intentionally-documented policy.
    #[test]
    fn gate_enforce_trusted_resolver_only_allows_member() {
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &allow(),
                &CoopResolutionObservation::ResolverOnly
            ),
            ProceedUnchanged
        );
    }

    // (B,D) Enforcement: legacy-only / untrusted provenance (UnknownLegacy, gossip rows)
    // is NOT a trusted basis — fail closed even for an affirmative member.
    #[test]
    fn gate_enforce_untrusted_resolution_denies_even_for_member() {
        assert_eq!(
            decide_treasury_gate(EnforceTrustedResolver, &allow(), &untrusted()),
            WouldDeny(UntrustedResolution)
        );
    }

    // (B) Enforcement: resolver/legacy DISAGREE (migration collision) fails closed.
    #[test]
    fn gate_enforce_disagreement_denies() {
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &allow(),
                &CoopResolutionObservation::Disagree
            ),
            WouldDeny(ResolverConflict)
        );
    }

    // (B,E) Enforcement: source-unavailable / backend-error fails closed (both the
    // LegacyOnly and NeitherResolved carriers).
    #[test]
    fn gate_enforce_source_unavailable_denies() {
        for r in [
            CoopResolutionObservation::LegacyOnly(ResolutionUnavailable::SourceUnavailable),
            CoopResolutionObservation::NeitherResolved(ResolutionUnavailable::SourceUnavailable),
        ] {
            assert_eq!(
                decide_treasury_gate(EnforceTrustedResolver, &allow(), &r),
                WouldDeny(UntrustedResolution),
                "source-unavailable must fail closed under enforcement: {r:?}"
            );
        }
    }

    // (C) Enforcement with a trusted basis: a non-member is denied via the EXISTING
    // entity-membership signal (the gate adds no new allow path).
    #[test]
    fn gate_enforce_trusted_basis_non_member_denies() {
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &deny(),
                &CoopResolutionObservation::Agree
            ),
            WouldDeny(NotMember)
        );
    }

    // (C) Enforcement with a trusted basis: indeterminate membership (data gap) and a
    // membership error both fail closed.
    #[test]
    fn gate_enforce_trusted_basis_membership_gaps_fail_closed() {
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &indeterminate(),
                &CoopResolutionObservation::Agree
            ),
            WouldDeny(IndeterminateMembership)
        );
        assert_eq!(
            decide_treasury_gate(
                EnforceTrustedResolver,
                &errored(),
                &CoopResolutionObservation::ResolverOnly
            ),
            WouldDeny(ObservationError)
        );
    }

    // (D) Trust precedence / no mapping-as-authority: an untrusted resolution fails
    // closed REGARDLESS of the membership signal — the gate never treats a mapping as
    // authority, and never lets an untrusted basis through on a member.
    #[test]
    fn gate_enforce_untrusted_resolution_precedes_membership() {
        for obs in [allow(), deny(), indeterminate(), errored()] {
            assert_eq!(
                decide_treasury_gate(EnforceTrustedResolver, &obs, &untrusted()),
                WouldDeny(UntrustedResolution),
                "untrusted resolution must fail closed before membership: {obs:?}"
            );
        }
    }
}
