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

use crate::events::GovernanceEventEmitter;
use crate::manager::GovernanceManager;

use super::handlers;

/// Shared application context for governance HTTP handlers.
///
/// Stored as `web::Data<GovernanceContext<E>>`. Using a single struct keeps
/// the handler signatures thin and avoids multiple `Data<>` extractions.
#[derive(Clone)]
pub struct GovernanceContext<E> {
    pub manager: Arc<GovernanceManager>,
    pub emitter: E,
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
