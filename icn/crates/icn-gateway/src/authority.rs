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

/// Compute, record, and return the entity-aware authorization observation for a
/// treasury request that has ALREADY passed the flat `require_coop_access` guard.
///
/// This is observation-only — it emits a metric and a log line and returns the
/// observation. It never denies the request (the caller discards the result).
/// `treasury_entity_id` is the treasury's stored `EntityId` (`treasury.entity_id()`);
/// `coop_id` is the path coop id used only as a best-effort fallback target.
pub(crate) async fn observe_treasury_entity_access(
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
    use icn_entity::{CooperativeEntity, MembershipCapability, MembershipStatus};
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
            &mgr,
            &caller,
            None,
            "treasury-coop",
            EntityAction::TreasuryWrite,
        )
        .await;
        assert_eq!(obs, EntityAccessObservation::AgreesAllow);
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
        /// canonical encoding of the record's fields. Deterministic.
        fn record_hash(&self) -> [u8; 32] {
            let mut h = blake3::Hasher::new();
            h.update(b"icn:community:action_completion:v0\x00");
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
            actor_did: steward.as_str().to_string(),
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

    /// Tampering with any bound field changes the receipt hash (integrity).
    #[tokio::test]
    async fn community_action_receipt_detects_tampering() {
        let record = |title: &str| CommunityActionRecord {
            community_id: "entity:icn:community:maple-street-mutual-aid".to_string(),
            action_id: "action-charter-ratify-0001".to_string(),
            title: title.to_string(),
            actor_did: "entity:icn:individual:zSteward".to_string(),
            authority_basis: "founder_of_community".to_string(),
            transition: "completed".to_string(),
            at: FIXTURE_AT,
        };
        let original = record("Ratify the Maple Street Mutual Aid charter").record_hash();
        let tampered = record("Ratify a DIFFERENT charter").record_hash();
        assert_ne!(
            original, tampered,
            "any change to the bound record must change the receipt hash"
        );
    }
}
