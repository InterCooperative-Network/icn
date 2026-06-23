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
use icn_governance::MembershipResolver;
use icn_identity::Did;
use tracing::warn;

use crate::events::GovernanceEventEmitter;
use crate::manager::GovernanceManager;
use crate::mandate_gate::MandateGate;

use super::handlers;

/// Deployment posture that determines how missing optional standing
/// checkers and the membership resolver are treated at HTTP context build
/// time.
///
/// The governance HTTP context exposes three optional dependencies:
/// `member_checker`, `suspension_checker`, and `membership_resolver`. When
/// any of them is `None`, governance handlers fall through to permissive
/// behavior (e.g. `TrustThreshold` domains build with
/// `excluded_delegators = None`). That is acceptable for development,
/// devnet, and bootstrap contexts; it is unsafe for production and
/// partner-bound deployments where unresolved member standing must not
/// implicitly count as standing.
///
/// `GovernanceContextBuildMode` makes that distinction legible:
///
/// * [`Bootstrap`](Self::Bootstrap) — current dev/devnet behavior.
///   Missing checkers/resolver emit explicit `tracing::warn!` warnings
///   at context-validation time. Startup is not rejected.
/// * [`Production`](Self::Production) — partner/production deployments.
///   Missing checkers/resolver are a hard configuration error: validation
///   returns a [`GovernanceContextValidationError`] naming every missing
///   dependency so the daemon fails fast before serving traffic.
/// * [`Test`](Self::Test) — explicit test-only mode. Identical permissive
///   semantics to `Bootstrap`, but distinguishable in logs so test runs
///   are not mistaken for a production misconfiguration.
///
/// **This type does not make governance production-ready.** It only makes
/// the bootstrap-vs-production distinction legible at startup. The
/// `TrustThreshold` direct-membership-mutation fail-open that issue #1870
/// tracked is now closed in the add/remove member handlers (see
/// `handlers::resolve_caller_membership`): when a `membership_resolver` is
/// configured, `TrustThreshold` callers are resolved through it and
/// unresolved standing is rejected rather than treated as standing; an
/// unconfigured resolver rejects outright in `Production`, while
/// `Bootstrap`/`Test` posture without a resolver stays permissive
/// (preserving dev/devnet behavior). Other production hardening (full
/// checker wiring, operational posture) still applies before claiming
/// readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GovernanceContextBuildMode {
    /// Permissive mode for dev, devnet, and bootstrap contexts.
    ///
    /// Missing standing checkers or membership resolver emit explicit
    /// warnings via `tracing::warn!` but do not reject startup. This is
    /// the default for backward compatibility with existing constructors
    /// that do not yet wire the mode explicitly.
    #[default]
    Bootstrap,

    /// Strict mode for partner/production deployments.
    ///
    /// Missing `MemberStandingChecker`, `SuspensionChecker`, or
    /// `membership_resolver` cause [`GovernanceContext::validate`] to
    /// return [`GovernanceContextValidationError`] naming every missing
    /// dependency. The daemon should fail to start in this case.
    Production,

    /// Explicit test mode.
    ///
    /// Functionally identical to [`Bootstrap`](Self::Bootstrap) — missing
    /// dependencies do not reject startup — but distinguishable in logs
    /// so test runs are not confused with production misconfiguration.
    Test,
}

impl GovernanceContextBuildMode {
    /// Resolve a build mode from the `ICN_GOVERNANCE_BUILD_MODE`
    /// environment variable, falling back to `Bootstrap` when unset or
    /// unrecognized. The gateway uses this to pick the mode at runtime;
    /// callers that wire a mode explicitly should use the variants
    /// directly rather than this helper.
    ///
    /// Recognized values (case-insensitive): `bootstrap`, `production`,
    /// `test`. Unrecognized values fall back to `Bootstrap` to preserve
    /// existing behavior; the gateway logs the unrecognized value.
    pub fn from_env() -> Self {
        match std::env::var("ICN_GOVERNANCE_BUILD_MODE") {
            Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
                "bootstrap" | "" => Self::Bootstrap,
                "production" => Self::Production,
                "test" => Self::Test,
                other => {
                    warn!(
                        mode = %other,
                        "ICN_GOVERNANCE_BUILD_MODE is set to an unrecognized value; falling back to Bootstrap"
                    );
                    Self::Bootstrap
                }
            },
            Err(_) => Self::Bootstrap,
        }
    }

    /// Returns `true` if this mode rejects governance contexts that are
    /// missing optional standing checkers or the membership resolver.
    pub fn rejects_unresolved_standing(self) -> bool {
        matches!(self, Self::Production)
    }
}

/// Configuration error returned by [`GovernanceContext::validate`] when
/// the context is built in [`GovernanceContextBuildMode::Production`] but
/// is missing one or more optional standing dependencies that production
/// requires.
///
/// The error names every missing dependency (not just the first) so the
/// operator can fix the full configuration in a single pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceContextValidationError {
    /// Mode the context was built in.
    pub mode: GovernanceContextBuildMode,
    /// Names of the missing optional dependencies. Stable strings,
    /// matching the field names on [`GovernanceContext`].
    pub missing: Vec<&'static str>,
}

impl std::fmt::Display for GovernanceContextValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "governance context built in {:?} mode is missing required \
             standing/resolver/authority dependencies: {}. \
             Production deployments must wire MemberStandingChecker, \
             SuspensionChecker, a MembershipResolver, and a MandateGate — \
             unresolved standing must not be treated as standing, and \
             capability scope is not a mandate.",
            self.mode,
            self.missing.join(", ")
        )
    }
}

impl std::error::Error for GovernanceContextValidationError {}

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

/// Evidence returned by a dispatcher that successfully (or unsuccessfully)
/// acted on an accepted effect. The governance app persists this as an
/// `EffectDispatchEvidence` record tied to the emitted
/// `InstitutionalEffectRecord` for the proposal.
///
/// `subsystem` is lowercase stable (`"commons"`, `"ledger"`, `"sdis"`).
/// `receipt_ref` is opaque — governance does not interpret or validate it.
/// `outcome` is the structural classification the dispatcher is willing to
/// stand behind for this result. `None` means the dispatcher has no truthful
/// outcome to report (pre-outcome-seam hook, fire-and-forget surface). See
/// [`icn_kernel_api::EffectOutcome`] for the full semantics of each variant.
#[derive(Debug, Clone)]
pub struct DispatchEvidenceSpec {
    pub subsystem: String,
    pub receipt_ref: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub outcome: Option<icn_kernel_api::EffectOutcome>,
}

/// Optional parallel hook that, when wired, returns downstream dispatch
/// evidence for an accepted effect. Runs immediately after
/// `on_proposal_accepted`. A returned `Some(spec)` is persisted against the
/// just-emitted `InstitutionalEffectRecord` (matching on `effect_kind`),
/// bringing that effect's `reconciliation_status` out of `emitted_only`.
///
/// Returning `None` is valid and means "this dispatcher does not report
/// evidence" — reconciliation stays `emitted_only` for that effect.
///
/// Async-returning: dispatchers that await real downstream work (e.g.
/// `register_steward` / `revoke_steward` in commons) can capture the
/// success/failure result before returning a `DispatchEvidenceSpec`.
/// Dispatchers that produce evidence synchronously still fit — wrap the
/// sync computation in `Box::pin(async move { ... })`.
///
/// This is additive to `ProposalAcceptedHook`. Gateways that do not
/// provide evidence-returning dispatchers can leave this `None` and keep
/// using the fire-and-forget hook.
pub type ProposalDispatchEvidenceHook = Arc<
    dyn Fn(
            &GovernanceEffect,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Option<DispatchEvidenceSpec>> + Send>,
        > + Send
        + Sync,
>;

/// Async predicate: is `did` an active steward in the commons layer?
///
/// Called by SDIS proposal handlers (AppointSteward, RemoveSteward) before
/// touching the governance manager. Returning `false` causes 403 Forbidden.
/// Steward standing is a global status — not domain-scoped — so no domain_id
/// parameter is needed. It represents a strictly higher authority level than
/// Member standing: all stewards are members, but not all members are stewards.
///
/// `None` disables the gate (useful in tests that do not wire commons).
pub type StewardStandingChecker = Arc<
    dyn Fn(Did) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync,
>;

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

/// Async predicate: is `did` currently suspended in cooperative `domain_id`?
///
/// Called by `cast_vote` before recording a vote. Returns `true` if the member
/// holds `MemberStatus::Suspended` (set by an accepted `FreezeMember` proposal).
/// A `true` result causes `403 Forbidden` — suspended members may not vote.
///
/// `None` disables the gate (useful in tests that do not wire a coop store).
///
/// The gateway wires this against `CoopManager::is_member_suspended()`.
/// The governance app never imports `icn-coop` types — the closure is the
/// only crossing point, preserving the kernel/app boundary.
pub type SuspensionChecker = Arc<
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
    /// Optional evidence-returning dispatch hook, called right after
    /// `on_proposal_accepted`. When wired, the returned `DispatchEvidenceSpec`
    /// is persisted as `EffectDispatchEvidence` against the just-emitted
    /// institutional effect record, bringing its `reconciliation_status`
    /// out of `emitted_only`. Additive to `on_proposal_accepted`.
    pub on_proposal_accepted_with_evidence: Option<ProposalDispatchEvidenceHook>,
    /// Optional gate: if set, `create_proposal` calls this before accepting the
    /// submission. Returns `true` if the caller has active Member standing in the
    /// target domain; `false` → 403 Forbidden.
    pub member_checker: Option<MemberStandingChecker>,
    /// Optional gate for SDIS proposal types (AppointSteward, RemoveSteward).
    /// Returns `true` if the caller is an active steward; `false` → 403 Forbidden.
    /// Steward standing is a strictly higher bar than Member standing.
    pub steward_checker: Option<StewardStandingChecker>,
    /// Optional gate: if set, `cast_vote` calls this before recording the vote.
    /// Returns `true` if the voter is suspended (`MemberStatus::Suspended`) in the
    /// proposal's domain — i.e., a `FreezeMember` proposal was accepted for them.
    /// A `true` result → 403 Forbidden. `None` disables the gate.
    pub suspension_checker: Option<SuspensionChecker>,
    /// Optional resolver for enumerating eligible voters in `TrustThreshold` domains.
    ///
    /// `StaticList` domains enumerate members from the source directly at close time.
    /// `TrustThreshold` domains cannot: membership is derived from a trust graph that
    /// lives outside the governance layer. When this resolver is set, `close_proposal`
    /// calls `resolve_members()` to obtain the current eligible set and checks each
    /// against `suspension_checker` to build `excluded_delegators`.
    ///
    /// If `None`, `TrustThreshold` domains fall through with `excluded_delegators: None`
    /// (fail-open — delegations from suspended members may still flow).
    pub membership_resolver: Option<Arc<dyn MembershipResolver>>,
    /// Optional SDIS service for direct steward dispatch in test environments.
    ///
    /// In daemon mode this is `None` — steward execution goes through the actor event
    /// system (`SystemEvent::ProposalAccepted` → `KernelGovernanceExecutor` → `SdisService`).
    /// Setting this in tests provides a synchronous wiring path without an actor runtime.
    pub sdis_service: Option<Arc<dyn icn_kernel_api::SdisService>>,
    /// Optional app-side, act-time authority resolver.
    ///
    /// When wired, governance handlers that perform high/medium-blast
    /// institutional acts (charter activation, federation lifecycle, steward
    /// appointment, etc.) will resolve a `MandateGrant` via this gate before
    /// proceeding. The gate sits beside capability scope checks and member
    /// standing — never replacing them — preserving the design's
    /// separate/composable-checks intent (see
    /// `docs/design/governance/mandate-gate-design.md`).
    ///
    /// `None` means no act-time mandate gating. Production deployments must
    /// wire this; [`Self::validate`] rejects a `Production` context with
    /// `mandate_gate: None`. Bootstrap/Test contexts may leave it unset and
    /// receive a startup warning. The field is **not** consumed by any
    /// handler in this PR — wiring `require()` into handlers is a separate,
    /// follow-up slice (#1868 step 7) so the act-time semantics can be
    /// reviewed independently of the startup-guard infrastructure landed
    /// here.
    pub mandate_gate: Option<Arc<dyn MandateGate + Send + Sync>>,
    /// Deployment posture for this governance HTTP context.
    ///
    /// Controls how missing optional standing checkers and the membership
    /// resolver are treated by [`Self::validate`]: warned about in
    /// `Bootstrap`/`Test`, hard-rejected in `Production`. See
    /// [`GovernanceContextBuildMode`] for the full contract.
    ///
    /// Defaults to [`GovernanceContextBuildMode::Bootstrap`] so existing
    /// dev/devnet/test constructors continue to compile and behave
    /// identically without code changes.
    pub build_mode: GovernanceContextBuildMode,
}

impl<E> GovernanceContext<E> {
    /// Return the names of optional standing/resolver dependencies that
    /// are unset on this context and that production deployments must
    /// wire. The strings are stable identifiers (matching field names)
    /// suitable for inclusion in operator-visible error messages and
    /// log output. Returns an empty vec if everything required is wired.
    ///
    /// This is purely an inspection of the field set — it does not
    /// consult [`Self::build_mode`].
    pub fn missing_production_dependencies(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.member_checker.is_none() {
            missing.push("member_checker");
        }
        if self.suspension_checker.is_none() {
            missing.push("suspension_checker");
        }
        if self.membership_resolver.is_none() {
            missing.push("membership_resolver");
        }
        if self.mandate_gate.is_none() {
            missing.push("mandate_gate");
        }
        missing
    }

    /// Validate this governance context against its declared build mode.
    ///
    /// In [`GovernanceContextBuildMode::Production`] every missing
    /// dependency listed by [`Self::missing_production_dependencies`] is
    /// a hard configuration error and this returns
    /// [`GovernanceContextValidationError`] naming all of them.
    ///
    /// In [`GovernanceContextBuildMode::Bootstrap`] and
    /// [`GovernanceContextBuildMode::Test`] this emits a `tracing::warn!`
    /// for each missing dependency (so operators can see the fall-open
    /// state in logs) but returns `Ok(())`.
    ///
    /// Callers should invoke this at daemon/gateway startup before
    /// routes are registered, not lazily inside a handler.
    pub fn validate(&self) -> Result<(), GovernanceContextValidationError> {
        let missing = self.missing_production_dependencies();
        if missing.is_empty() {
            return Ok(());
        }

        match self.build_mode {
            GovernanceContextBuildMode::Production => Err(GovernanceContextValidationError {
                mode: self.build_mode,
                missing,
            }),
            mode @ (GovernanceContextBuildMode::Bootstrap | GovernanceContextBuildMode::Test) => {
                self.warn_missing_dependencies(mode, &missing);
                Ok(())
            }
        }
    }

    fn warn_missing_dependencies(
        &self,
        mode: GovernanceContextBuildMode,
        missing: &[&'static str],
    ) {
        warn!(
            mode = ?mode,
            missing = ?missing,
            "governance context is running in {:?} mode without required \
             production standing wiring; missing: {}. \
             Production mode would reject this configuration. \
             Unresolved standing must not be treated as standing in production.",
            mode,
            missing.join(", ")
        );
        for dep in missing {
            match *dep {
                "member_checker" => warn!(
                    "governance: MemberStandingChecker is not wired; \
                     create_proposal will not enforce active Member standing"
                ),
                "suspension_checker" => warn!(
                    "governance: SuspensionChecker is not wired; \
                     cast_vote will not block suspended members and \
                     close_proposal will not exclude suspended delegators"
                ),
                "membership_resolver" => warn!(
                    "governance: membership_resolver is not wired; \
                     TrustThreshold domains will close with excluded_delegators=None \
                     (fail-open — delegations from suspended members may flow)"
                ),
                "mandate_gate" => warn!(
                    "governance: MandateGate is not wired; act-time institutional \
                     authority will not be checked when handlers begin requiring it. \
                     Production mode would reject this configuration. \
                     Capability scope is not a mandate — unresolved mandate \
                     authority must not be treated as authority in production."
                ),
                _ => {}
            }
        }
    }
}

/// Register all governance routes on `cfg`.
///
/// `E` is monomorphized at the call site (gateway), so handlers use static
/// dispatch for event emission — no `dyn Trait` overhead.
pub fn configure<E>(cfg: &mut web::ServiceConfig, ctx: GovernanceContext<E>)
where
    E: GovernanceEventEmitter + Clone + 'static,
{
    // Defensive Production-only check: callers (daemon/gateway) are expected
    // to call `ctx.validate()` themselves and propagate the error up before
    // reaching this point, so reaching here in `Production` with missing
    // standing/resolver deps means a programmer skipped that step. We do NOT
    // call `validate()` here because that would duplicate the Bootstrap/Test
    // warning logs already emitted by the caller. We only enforce the
    // Production invariant.
    if ctx.build_mode == GovernanceContextBuildMode::Production {
        let missing = ctx.missing_production_dependencies();
        if !missing.is_empty() {
            panic!(
                "{}",
                GovernanceContextValidationError {
                    mode: ctx.build_mode,
                    missing,
                }
            );
        }
    }

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
        // ── Charter activation ────────────────────────────────────────────
        .service(web::resource("/charters").route(web::post().to(handlers::activate_charter::<E>)))
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
        .service(
            web::resource("/proposals/{proposal_id}/deliberation")
                .route(web::get().to(handlers::get_proposal_deliberation::<E>)),
        )
        .service(
            web::resource("/proposals/{proposal_id}/effects")
                .route(web::get().to(handlers::list_proposal_effects::<E>)),
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
        .service(
            web::resource("/domains/{domain_id}/action-items/{item_id}/completion-receipt")
                .route(web::get().to(handlers::get_action_item_completion_receipt::<E>)),
        )
        // ── Process gate results (#2144 — first ProcessTransitionReceipt) ──
        .service(
            web::resource("/domains/{domain_id}/process-sessions/{session_id}/gate-results")
                .route(web::post().to(handlers::record_process_gate_result::<E>)),
        )
        // ── Institutional domain policy adoption (#2142 — gated adoption) ──
        .service(
            web::resource("/domains/{domain_id}/domain-policy/adopt")
                .route(web::post().to(handlers::adopt_domain_policy::<E>)),
        )
        // ── Institutional domain declaration (#2142 — gated declare) ──────
        .service(
            web::resource("/domains/{domain_id}/institutional-domain/declare")
                .route(web::post().to(handlers::declare_institutional_domain::<E>)),
        )
        // ── Structure endpoints ──────────────────────────────────────────
        .service(
            web::resource("/entities/{entity_id}/structures")
                .route(web::post().to(handlers::create_structure::<E>))
                .route(web::get().to(handlers::list_structures::<E>)),
        )
        .service(
            web::resource("/structures/{structure_id}")
                .route(web::get().to(handlers::get_structure::<E>)),
        )
        .service(
            web::resource("/structures/{structure_id}/roles")
                .route(web::post().to(handlers::assign_role::<E>))
                .route(web::get().to(handlers::list_roles::<E>)),
        )
        // ── Activity endpoints ───────────────────────────────────────────
        .service(
            web::resource("/entities/{entity_id}/activities")
                .route(web::post().to(handlers::create_activity::<E>))
                .route(web::get().to(handlers::list_activities::<E>)),
        )
        .service(
            web::resource("/activities/{activity_id}")
                .route(web::get().to(handlers::get_activity::<E>)),
        )
        // ── Program endpoints ────────────────────────────────────────────
        .service(
            web::resource("/domains/{domain_id}/programs")
                .route(web::post().to(handlers::create_program::<E>))
                .route(web::get().to(handlers::list_programs_by_domain::<E>)),
        )
        .service(
            web::resource("/programs/{program_id}")
                .route(web::get().to(handlers::get_program::<E>)),
        )
        .service(
            web::resource("/programs/{program_id}/status")
                .route(web::patch().to(handlers::update_program_status::<E>)),
        )
        .service(
            web::resource("/programs/{program_id}/dashboard")
                .route(web::get().to(handlers::get_program_dashboard::<E>)),
        )
        .service(
            web::resource("/programs/{program_id}/summary")
                .route(web::get().to(handlers::get_program_summary::<E>)),
        )
        .service(
            web::resource("/programs/{program_id}/milestones")
                .route(web::post().to(handlers::create_milestone::<E>))
                .route(web::get().to(handlers::list_milestones_by_program::<E>)),
        )
        .service(
            web::resource("/programs/{program_id}/activities/{activity_id}")
                .route(web::put().to(handlers::link_activity_to_program::<E>))
                .route(web::delete().to(handlers::unlink_activity_from_program::<E>)),
        )
        .service(
            web::resource("/milestones/{milestone_id}")
                .route(web::get().to(handlers::get_milestone::<E>))
                .route(web::patch().to(handlers::update_milestone_status::<E>)),
        )
        .service(
            web::resource("/milestones/{milestone_id}/preview")
                .route(web::get().to(handlers::preview_milestone::<E>)),
        )
        .service(
            web::resource("/milestones/{milestone_id}/history")
                .route(web::get().to(handlers::get_milestone_history::<E>)),
        )
        // ── Meeting endpoints ────────────────────────────────────────────
        .service(
            web::resource("/domains/{domain_id}/meetings")
                .route(web::post().to(handlers::create_meeting::<E>))
                .route(web::get().to(handlers::list_meetings::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}")
                .route(web::get().to(handlers::get_meeting::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}/start")
                .route(web::post().to(handlers::start_meeting::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}/end")
                .route(web::post().to(handlers::end_meeting::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}/attendees")
                .route(web::post().to(handlers::add_attendee::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}/attendance")
                .route(web::put().to(handlers::mark_attendance::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}/agenda")
                .route(web::post().to(handlers::add_agenda_item::<E>)),
        )
        .service(
            web::resource("/meetings/{meeting_id}/agenda/{item_id}")
                .route(web::put().to(handlers::update_agenda_item::<E>)),
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
        )
        // ── SDIS proposal endpoints ──────────────────────────────────────
        .service(
            web::resource("/proposals/sdis/appoint-steward")
                .route(web::post().to(handlers::create_appoint_steward_proposal::<E>)),
        )
        .service(
            web::resource("/proposals/sdis/remove-steward")
                .route(web::post().to(handlers::create_remove_steward_proposal::<E>)),
        )
        // ── Notification digest ──────────────────────────────────────────
        .service(web::resource("/digest").route(web::get().to(handlers::get_digest::<E>)))
        // ── Me (caller-scoped views) ─────────────────────────────────────
        .service(web::resource("/me/scopes").route(web::get().to(handlers::get_my_scopes::<E>)))
        .service(web::resource("/me/standing").route(web::get().to(handlers::get_my_standing::<E>)))
        .service(web::resource("/me/work").route(web::get().to(handlers::get_my_work::<E>)))
        .service(
            web::resource("/me/action-cards")
                .route(web::get().to(handlers::get_my_action_cards::<E>)),
        );
}

#[cfg(test)]
mod build_mode_tests {
    //! Unit tests for `GovernanceContextBuildMode` and the
    //! production-guard logic on `GovernanceContext::validate`.
    //!
    //! These tests construct a minimal context without exercising any
    //! HTTP/actor wiring, so they can be run in isolation under
    //! `cargo test -p icn-governance-actor`.

    use super::*;
    use crate::events::NoopEventEmitter;
    use crate::manager::GovernanceManager;

    fn ctx_with_mode(mode: GovernanceContextBuildMode) -> GovernanceContext<NoopEventEmitter> {
        GovernanceContext {
            manager: Arc::new(GovernanceManager::new()),
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: None,
            on_proposal_accepted_with_evidence: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: mode,
        }
    }

    fn dummy_member_checker() -> MemberStandingChecker {
        Arc::new(|_did, _domain| Box::pin(async { true }))
    }

    fn dummy_suspension_checker() -> SuspensionChecker {
        Arc::new(|_did, _domain| Box::pin(async { false }))
    }

    fn dummy_resolver() -> Arc<dyn MembershipResolver> {
        Arc::new(icn_governance::StaticMembershipResolver::new())
    }

    /// A test-only `MandateGate` stub that always rejects every request.
    /// Used by validate()-passes tests where only `Option::is_some()` matters;
    /// no test in this module exercises `require()` against this stub.
    struct DummyMandateGate;

    impl crate::mandate_gate::MandateGate for DummyMandateGate {
        fn require(
            &self,
            _req: &crate::mandate_gate::MandateRequest,
        ) -> Result<crate::mandate_gate::MandateGrant, crate::mandate_gate::MandateGateError>
        {
            Err(crate::mandate_gate::MandateGateError::Rejected(
                crate::mandate_gate::MandateRejection::NoMandate,
            ))
        }
    }

    fn dummy_mandate_gate() -> Arc<dyn crate::mandate_gate::MandateGate + Send + Sync> {
        Arc::new(DummyMandateGate)
    }

    #[test]
    fn default_mode_is_bootstrap() {
        // Backward-compatibility guard: existing call sites that do not
        // set `build_mode` should land on Bootstrap, not Production. If
        // this assertion ever needs to change, every downstream caller
        // must opt in to Production explicitly first.
        let mode: GovernanceContextBuildMode = Default::default();
        assert_eq!(mode, GovernanceContextBuildMode::Bootstrap);
    }

    #[test]
    fn production_only_mode_rejects_unresolved_standing() {
        assert!(GovernanceContextBuildMode::Production.rejects_unresolved_standing());
        assert!(!GovernanceContextBuildMode::Bootstrap.rejects_unresolved_standing());
        assert!(!GovernanceContextBuildMode::Test.rejects_unresolved_standing());
    }

    #[test]
    fn missing_production_dependencies_lists_unset_optional_fields() {
        // Empty context → every required dep missing, in stable order. The
        // order matches the field-declaration order on `GovernanceContext`
        // and is part of this method's stable contract — operators reading
        // a startup error and tooling parsing the message both rely on it.
        let ctx = ctx_with_mode(GovernanceContextBuildMode::Bootstrap);
        assert_eq!(
            ctx.missing_production_dependencies(),
            vec![
                "member_checker",
                "suspension_checker",
                "membership_resolver",
                "mandate_gate",
            ]
        );
    }

    #[test]
    fn missing_production_dependencies_excludes_wired_fields() {
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Bootstrap);
        ctx.member_checker = Some(dummy_member_checker());
        assert_eq!(
            ctx.missing_production_dependencies(),
            vec!["suspension_checker", "membership_resolver", "mandate_gate",]
        );
    }

    #[test]
    fn missing_production_dependencies_empty_when_all_wired() {
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Production);
        ctx.member_checker = Some(dummy_member_checker());
        ctx.suspension_checker = Some(dummy_suspension_checker());
        ctx.membership_resolver = Some(dummy_resolver());
        ctx.mandate_gate = Some(dummy_mandate_gate());
        assert!(ctx.missing_production_dependencies().is_empty());
    }

    #[test]
    fn production_all_missing_returns_error_naming_all() {
        let ctx = ctx_with_mode(GovernanceContextBuildMode::Production);
        let result = ctx.validate();
        let Err(err) = result else {
            panic!("Production with all deps missing must reject; got Ok");
        };

        assert_eq!(err.mode, GovernanceContextBuildMode::Production);
        assert_eq!(
            err.missing,
            vec![
                "member_checker",
                "suspension_checker",
                "membership_resolver",
                "mandate_gate",
            ]
        );

        let msg = err.to_string();
        assert!(
            msg.contains("Production"),
            "error must name the mode: {msg}"
        );
        assert!(
            msg.contains("member_checker"),
            "error must name member_checker: {msg}"
        );
        assert!(
            msg.contains("suspension_checker"),
            "error must name suspension_checker: {msg}"
        );
        assert!(
            msg.contains("membership_resolver"),
            "error must name membership_resolver: {msg}"
        );
        assert!(
            msg.contains("mandate_gate"),
            "error must name mandate_gate: {msg}"
        );
    }

    #[test]
    fn production_single_missing_dep_names_only_that_dep() {
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Production);
        ctx.member_checker = Some(dummy_member_checker());
        ctx.suspension_checker = Some(dummy_suspension_checker());
        ctx.mandate_gate = Some(dummy_mandate_gate());
        // membership_resolver still missing
        let result = ctx.validate();
        let Err(err) = result else {
            panic!("Production with one dep missing must reject; got Ok");
        };

        assert_eq!(err.missing, vec!["membership_resolver"]);
        let msg = err.to_string();
        assert!(
            msg.contains("membership_resolver"),
            "error must name the missing dep: {msg}"
        );
        assert!(
            !msg.contains("member_checker,"),
            "must not falsely name wired dep: {msg}"
        );
        // `err.missing == ["membership_resolver"]` above is already the strong
        // assertion that `mandate_gate` is not falsely listed. The string-level
        // check below is a belt-and-suspenders sanity guard: the snake_case
        // identifier `mandate_gate` can only enter the rendered message via
        // `missing.join(", ")`, since the doctrine sentence uses the CamelCase
        // `MandateGate`. So any `mandate_gate` substring is a falsehood
        // regardless of its position in the list (mid or trailing).
        assert!(
            !msg.contains("mandate_gate"),
            "must not falsely name wired dep: {msg}"
        );
    }

    #[test]
    fn production_all_wired_validates_ok() {
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Production);
        ctx.member_checker = Some(dummy_member_checker());
        ctx.suspension_checker = Some(dummy_suspension_checker());
        ctx.membership_resolver = Some(dummy_resolver());
        ctx.mandate_gate = Some(dummy_mandate_gate());
        assert!(ctx.validate().is_ok());
    }

    #[test]
    fn production_without_mandate_gate_rejects() {
        // All three standing/resolver deps wired; only `mandate_gate` is
        // missing. Production must fail closed and the missing-list must
        // name `mandate_gate` alone — proving the new dep is fully
        // integrated into the production guard (not merely listed).
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Production);
        ctx.member_checker = Some(dummy_member_checker());
        ctx.suspension_checker = Some(dummy_suspension_checker());
        ctx.membership_resolver = Some(dummy_resolver());
        // mandate_gate intentionally left None

        let Err(err) = ctx.validate() else {
            panic!("Production without mandate_gate must reject; got Ok");
        };
        assert_eq!(err.missing, vec!["mandate_gate"]);

        let msg = err.to_string();
        assert!(
            msg.contains("mandate_gate"),
            "error must name mandate_gate: {msg}"
        );
        assert!(
            msg.contains("MandateGate"),
            "error message must reference MandateGate by name: {msg}"
        );
        assert!(
            msg.contains("capability scope is not a mandate"),
            "error message must explain the doctrine — capability is not a mandate: {msg}"
        );
    }

    #[test]
    fn bootstrap_without_mandate_gate_does_not_reject() {
        // Bootstrap is the legacy/dev posture. Missing mandate_gate emits
        // a warning but does not fail validation, preserving dev/devnet
        // behavior.
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Bootstrap);
        ctx.member_checker = Some(dummy_member_checker());
        ctx.suspension_checker = Some(dummy_suspension_checker());
        ctx.membership_resolver = Some(dummy_resolver());
        // mandate_gate intentionally left None
        assert!(
            ctx.validate().is_ok(),
            "Bootstrap mode must not reject startup on missing mandate_gate"
        );
    }

    #[test]
    fn test_mode_without_mandate_gate_does_not_reject() {
        // Test mode is permissive, identical to Bootstrap for missing deps.
        let mut ctx = ctx_with_mode(GovernanceContextBuildMode::Test);
        ctx.member_checker = Some(dummy_member_checker());
        ctx.suspension_checker = Some(dummy_suspension_checker());
        ctx.membership_resolver = Some(dummy_resolver());
        // mandate_gate intentionally left None
        assert!(
            ctx.validate().is_ok(),
            "Test mode must not reject startup on missing mandate_gate"
        );
    }

    #[test]
    fn bootstrap_with_missing_deps_does_not_reject() {
        // Bootstrap is the legacy/dev posture. Missing deps emit warnings
        // (tracing::warn!) but do not fail validation.
        let ctx = ctx_with_mode(GovernanceContextBuildMode::Bootstrap);
        assert!(
            ctx.validate().is_ok(),
            "Bootstrap mode must not reject startup on missing deps"
        );
    }

    #[test]
    fn test_mode_with_missing_deps_does_not_reject() {
        // Test mode is functionally identical to Bootstrap — permissive,
        // but distinguishable in logs.
        let ctx = ctx_with_mode(GovernanceContextBuildMode::Test);
        assert!(
            ctx.validate().is_ok(),
            "Test mode must not reject startup on missing deps"
        );
    }

    #[test]
    fn validation_error_is_std_error() {
        // The error must implement `std::error::Error` so callers can
        // bubble it through `?` and into gateway-level error types.
        fn assert_is_error<T: std::error::Error>(_: &T) {}
        let err = GovernanceContextValidationError {
            mode: GovernanceContextBuildMode::Production,
            missing: vec!["member_checker"],
        };
        assert_is_error(&err);
    }

    /// Serialize the `from_env_*` tests against each other.
    ///
    /// Rust's test harness runs unit tests in parallel by default and these
    /// three tests mutate `ICN_GOVERNANCE_BUILD_MODE` via `set_var` /
    /// `remove_var` — without serialization they race on process-wide state
    /// and assertions become flaky depending on scheduling. A single static
    /// `Mutex` makes the env-mutating section single-threaded across the
    /// crate's lib-test process. We poison-tolerate (via `.unwrap_or_else`)
    /// so a panic in one test does not cascade into the others.
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the env-test lock and restore the previous value of
    /// `ICN_GOVERNANCE_BUILD_MODE` when the guard drops. This keeps
    /// each test hermetic regardless of order or external env state.
    struct EnvGuard {
        prev: Option<String>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn acquire() -> Self {
            let lock = ENV_TEST_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prev = std::env::var("ICN_GOVERNANCE_BUILD_MODE").ok();
            std::env::remove_var("ICN_GOVERNANCE_BUILD_MODE");
            EnvGuard { prev, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => std::env::set_var("ICN_GOVERNANCE_BUILD_MODE", v),
                None => std::env::remove_var("ICN_GOVERNANCE_BUILD_MODE"),
            }
        }
    }

    #[test]
    fn from_env_falls_back_to_bootstrap_when_unset() {
        let _g = EnvGuard::acquire();
        // EnvGuard already removed the var; assert the unset path.
        assert_eq!(
            GovernanceContextBuildMode::from_env(),
            GovernanceContextBuildMode::Bootstrap
        );
    }

    #[test]
    fn from_env_parses_production() {
        let _g = EnvGuard::acquire();
        std::env::set_var("ICN_GOVERNANCE_BUILD_MODE", "production");
        assert_eq!(
            GovernanceContextBuildMode::from_env(),
            GovernanceContextBuildMode::Production
        );
        std::env::set_var("ICN_GOVERNANCE_BUILD_MODE", "PRODUCTION");
        assert_eq!(
            GovernanceContextBuildMode::from_env(),
            GovernanceContextBuildMode::Production
        );
    }

    #[test]
    fn from_env_falls_back_on_unknown_value() {
        let _g = EnvGuard::acquire();
        std::env::set_var("ICN_GOVERNANCE_BUILD_MODE", "garbage");
        assert_eq!(
            GovernanceContextBuildMode::from_env(),
            GovernanceContextBuildMode::Bootstrap
        );
    }
}
