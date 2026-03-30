//! Actix-web configuration for governance HTTP routes.
//!
//! Call [`configure`] from the gateway server setup, providing a concrete
//! `GovernanceContext<E>` where `E` implements [`GovernanceEventEmitter`].
//!
//! ```ignore
//! let ctx = GovernanceContext {
//!     manager: governance_manager,
//!     emitter: GatewayEventAdapter::new(event_broadcaster),
//! };
//! app.configure(|cfg| icn_governance_actor::http::configure(cfg, ctx.clone()));
//! ```

use std::sync::Arc;

use actix_web::web;
use icn_identity::Did;

use crate::events::GovernanceEventEmitter;
use crate::manager::GovernanceManager;

use super::handlers;

/// Callback invoked when a `Charter` proposal is accepted.
///
/// Arguments: `(charter_id, charter_yaml)`.
///
/// The gateway wires this to `CharterPolicyOracle::deploy_charter()`.
/// Errors are non-fatal to the proposal-close operation — implementations
/// should log warnings internally rather than panicking.
pub type CharterAcceptedHook = Arc<dyn Fn(String, String) + Send + Sync>;

/// A governance acceptance translated into a dispatchable institutional effect.
///
/// ## Role in the architecture
///
/// `GovernanceEffect` is the **institutional effects boundary** between the
/// governance app and gateway-layer subsystems (ledger, commons, etc.). The
/// governance app translates an accepted `Proposal`'s domain `ProposalPayload`
/// into a `GovernanceEffect` before invoking `ProposalAcceptedHook`. The
/// gateway therefore never needs to import `icn_governance` domain types —
/// satisfying the meaning-firewall constraint.
///
/// Adding a new governance-driven institutional action means:
/// 1. Add a variant here describing the effect (gateway-level semantics only).
/// 2. Map the `ProposalPayload` → this variant in `handlers::close_proposal`.
/// 3. Dispatch it in the `on_proposal_accepted` hook in `server.rs`.
///
/// When CCL-governed institutional logic is introduced, `GovernanceEffect`
/// variants will be the translation target from CCL execution results —
/// keeping the dispatch table here and the wiring in `server.rs` unchanged.
///
/// | Proposal payload            | GovernanceEffect      | Ledger          | Commons                    |
/// |-----------------------------|-----------------------|-----------------|----------------------------|
/// | `FreezeMember`              | `FreezeMember`        | freeze          | Suspended                  |
/// | `UnfreezeMember`            | `UnfreezeMember`      | unfreeze        | Member                     |
/// | `Charter`                   | `DeployCharter`       | —               | store_charter (stub)       |
/// | `Sdis::AppointSteward`      | `AppointSteward`      | —               | register_steward (scoped)  |
/// | `Sdis::RemoveSteward`       | `RevokeSteward`       | —               | revoke_steward             |
#[derive(Debug, Clone)]
pub enum GovernanceEffect {
    /// A member should be frozen: block ledger transactions and suspend commons
    /// affiliation in the proposal's jurisdiction.
    FreezeMember {
        proposal_id: String,
        domain_id: String,
        member: Did,
        reason: String,
        duration_seconds: Option<u64>,
    },
    /// A previously frozen member should be unfrozen: unblock ledger and
    /// reinstate commons affiliation to `Member` standing.
    UnfreezeMember {
        proposal_id: String,
        domain_id: String,
        member: Did,
        reason: String,
    },
    /// A governance-ratified charter should be registered in the commons charter store.
    ///
    /// This creates a minimal `Charter` record keyed by `domain_id`, making the
    /// domain known to the commons layer. Required before steward appointment can
    /// bind to this domain as a jurisdiction.
    ///
    /// `domain_id` equals `charter_id` per governance convention (the charter ID
    /// is the stable domain identifier, e.g. the cooperative's name or DID).
    DeployCharter {
        proposal_id: String,
        /// Governance `charter_id` — used as both the charter identifier and domain key.
        charter_id: String,
    },
    /// A candidate should be registered as a steward in commons, scoped to a
    /// governance domain that must already have a ratified charter.
    ///
    /// The candidate must already have a Commons Holder record with Strong POP level.
    AppointSteward {
        proposal_id: String,
        /// Governance domain the appointment was approved in; used as jurisdiction.
        domain_id: String,
        candidate: Did,
        region: String,
        bond_amount: i64,
        term_length_seconds: u64,
    },
    /// A steward's appointment should be revoked for cause.
    RevokeSteward {
        proposal_id: String,
        steward: Did,
        reason: String,
    },
    /// Accepted but no gateway execution handler is wired for this payload type.
    Unhandled {
        proposal_id: String,
        payload_type: String,
    },
}

/// Callback invoked when any proposal is accepted, receiving a translated effect.
///
/// Called after the charter-specific hook. The governance app translates
/// the accepted `Proposal` into a `GovernanceEffect` before invoking this hook,
/// so the gateway never needs to import `icn_governance` domain types.
///
/// Errors are non-fatal. Implementations should log internally and not panic.
pub type ProposalAcceptedHook = Arc<dyn Fn(GovernanceEffect) + Send + Sync>;

/// Async predicate: does `did` hold active Member standing in `domain_id`?
///
/// Called by `create_proposal` before touching the governance manager. Returning
/// `false` causes the handler to return `403 Forbidden`. `None` disables the
/// gate (useful in tests that do not wire commons).
///
/// The gateway wires this to `CommonsManager::list_affiliations()` filtered by
/// the proposal's target domain. The governance app never imports commons types —
/// the closure is the only crossing point.
pub type MemberStandingChecker = Arc<
    dyn Fn(Did, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>
        + Send
        + Sync,
>;

/// Shared application context for governance HTTP handlers.
///
/// Stored as `web::Data<GovernanceContext<E>>`. Using a single struct keeps
/// the handler signatures thin and avoids multiple `Data<>` extractions.
#[derive(Clone)]
pub struct GovernanceContext<E> {
    pub manager: Arc<GovernanceManager>,
    pub emitter: E,
    /// Optional hook called when a `Charter` proposal is accepted.
    pub on_charter_accepted: Option<CharterAcceptedHook>,
    /// Optional hook called when any proposal is accepted.
    ///
    /// Receives the translated [`GovernanceEffect`] for the accepted proposal,
    /// allowing gateway dispatch without importing governance domain types.
    pub on_proposal_accepted: Option<ProposalAcceptedHook>,
    /// Optional gate: if set, `create_proposal` calls this before accepting the
    /// submission. Returns `true` if the caller has active Member standing in the
    /// target domain; `false` → 403 Forbidden.
    pub member_checker: Option<MemberStandingChecker>,
}

/// Register all governance routes on `cfg`.
///
/// `E` is monomorphized at the call site (gateway), so handlers use static
/// dispatch for event emission — no `dyn Trait` overhead.
pub fn configure<E>(cfg: &mut web::ServiceConfig, ctx: GovernanceContext<E>)
where
    E: GovernanceEventEmitter + Clone + 'static,
{
    let data = web::Data::new(ctx);

    cfg.app_data(data.clone())
        // ── Domain endpoints ─────────────────────────────────────────────
        .service(
            web::resource("/domains")
                .route(web::post().to(handlers::create_domain::<E>))
                .route(web::get().to(handlers::list_domains::<E>)),
        )
        .service(
            web::resource("/domains/{domain_id}").route(web::get().to(handlers::get_domain::<E>)),
        )
        .service(
            web::resource("/domains/{domain_id}/members")
                .route(web::post().to(handlers::add_domain_member::<E>))
                .route(web::delete().to(handlers::remove_domain_member::<E>)),
        )
        // ── Proposal endpoints ────────────────────────────────────────────
        .service(
            web::resource("/proposals")
                .route(web::post().to(handlers::create_proposal::<E>))
                .route(web::get().to(handlers::list_proposals::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}")
                .route(web::get().to(handlers::get_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/open")
                .route(web::post().to(handlers::open_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/close")
                .route(web::post().to(handlers::close_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/vote")
                .route(web::post().to(handlers::cast_vote::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/tally")
                .route(web::get().to(handlers::get_vote_tally::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/proof")
                .route(web::get().to(handlers::get_proof::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/chain")
                .route(web::get().to(handlers::get_chain::<E>)),
        )
        // ── Discussion endpoints ─────────────────────────────────────────
        .service(
            web::resource("/proposals/{proposal_id}/discussion")
                .route(web::get().to(handlers::get_discussion::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/discussion/comments")
                .route(web::post().to(handlers::add_comment::<E>))
                .route(web::get().to(handlers::list_comments::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/discussion/comments/{comment_id}")
                .route(web::put().to(handlers::edit_comment::<E>))
                .route(web::delete().to(handlers::delete_comment::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions")
                .route(web::post().to(handlers::add_reaction::<E>))
                .route(web::delete().to(handlers::remove_reaction::<E>)),
        )
        // ── Delegation endpoints ─────────────────────────────────────────
        .service(
            web::resource("/delegations")
                .route(web::post().to(handlers::create_delegation::<E>))
                .route(web::get().to(handlers::list_delegations::<E>)),
        )
        .service(
            web::resource("/delegations/{delegation_id}")
                .route(web::delete().to(handlers::revoke_delegation::<E>)),
        )
        // ── Action item endpoints ────────────────────────────────────────
        .service(
            web::resource("/domains/{domain_id}/action-items")
                .route(web::post().to(handlers::create_action_item::<E>))
                .route(web::get().to(handlers::list_action_items::<E>)),
        )
        .service(
            web::resource("/domains/{domain_id}/action-items/{item_id}")
                .route(web::get().to(handlers::get_action_item::<E>))
                .route(web::put().to(handlers::update_action_item::<E>))
                .route(web::delete().to(handlers::delete_action_item::<E>)),
        )
        .service(
            web::resource("/domains/{domain_id}/action-items/{item_id}/status")
                .route(web::put().to(handlers::update_action_item_status::<E>)),
        )
        .service(
            web::resource("/domains/{domain_id}/action-items/{item_id}/notes")
                .route(web::post().to(handlers::add_action_item_note::<E>)),
        )
        // ── Federation proposal endpoints ────────────────────────────────
        .service(
            web::resource("/proposals/federation/join")
                .route(web::post().to(handlers::create_join_federation_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/federation/leave")
                .route(web::post().to(handlers::create_leave_federation_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/federation/clearing/establish")
                .route(web::post().to(handlers::create_establish_clearing_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/federation/clearing/terminate")
                .route(web::post().to(handlers::create_terminate_clearing_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/federation/vouch")
                .route(web::post().to(handlers::create_vouch_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/federation/vouch/revoke")
                .route(web::post().to(handlers::create_revoke_vouch_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/federation/policy")
                .route(web::post().to(handlers::create_update_federation_policy_proposal::<E>)),
        );
}
