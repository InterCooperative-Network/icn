//! Gateway server

use actix_files as fs;
use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use actix_web_prom::PrometheusMetricsBuilder;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};
use tracing_actix_web::TracingLogger;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::api;
use crate::auth::AuthManager;
use crate::commons_mgr::CommonsManager;
use crate::compute_mgr::ComputeManager;
use crate::coop::CoopManager;
use crate::entity_audit::EntityAuditManager;
use crate::entity_mgr::{EntityHandle, EntityManager};
use crate::error::{GatewayError, Result};
use crate::events::EventBroadcaster;
use crate::federation_mgr::FederationManager;
use crate::governance_adapter::GatewayEventAdapter;
use crate::governance_mgr::GovernanceHandle;
use crate::governance_mgr::GovernanceManager;
use crate::identity_mgr::IdentityManager;
use crate::ledger_mgr::LedgerManager;
use crate::listings_mgr::ListingsManager;
use crate::notification_processor::{NotificationProcessor, ProcessorConfig};
use crate::notification_queue::NotificationQueue;
use crate::notification_triggers::{GovernanceNotificationTrigger, LedgerNotificationTrigger};
use crate::notifications::NotificationService;
use crate::rate_limit::{
    IpRateLimiter, RateLimitConfig, RateLimiter, VelocityLimitConfig, VelocityLimiter,
};
use crate::security::{configure_cors, SecurityConfig, SecurityHeaders};
use crate::session_authority::{
    AuthorityProfile, InMemoryRevocationAuthority, RevocationAuthority, SessionAuthority,
    StoreRevocationAuthority, TokenLifetimePolicy,
};
use crate::treasury_mgr::{GatewayTreasuryManager, LedgerHandle, TreasuryHandle};
use crate::trust_mgr::TrustManager;
use anyhow::Context;
use icn_compute::ComputeHandle;
use icn_governance_actor::http::configure as configure_governance;
use icn_governance_actor::http::configure::GovernanceContext;
use icn_kernel_api::naming::NamingService;
use icn_store::SledStore;
use tokio::sync::RwLock;

/// Cleanup interval for rate limiter buckets (1 hour)
/// Applied to both trust-gated and regular rate limiters
const RATE_LIMITER_CLEANUP_INTERVAL_SECS: u64 = 3600;

/// Configuration for audit record pruning
#[derive(Debug, Clone)]
pub struct AuditPruneConfig {
    /// Maximum age of audit records in days
    pub retention_days: u64,
    /// Maximum records per entity
    pub max_records_per_entity: usize,
    /// Interval between prune runs in seconds
    pub prune_interval_secs: u64,
    /// Batch size for pruning
    pub batch_size: usize,
    /// Whether auto-pruning is enabled
    pub enabled: bool,
}

impl Default for AuditPruneConfig {
    fn default() -> Self {
        Self {
            retention_days: 365,
            max_records_per_entity: 10000,
            prune_interval_secs: 3600,
            batch_size: 1000,
            enabled: true,
        }
    }
}

/// Gateway server configuration
pub struct GatewayServer {
    bind_addr: SocketAddr,
    jwt_secret: Vec<u8>,
    /// Deployment profile governing which authority guarantees are REQUIRED.
    /// Defaults to the disposable-evaluator posture; the daemon sets the
    /// institutional profile when a persistent deployment is configured.
    authority_profile: AuthorityProfile,
    /// Persistent store backing session revocation. `None` yields volatile
    /// revocation, which only the evaluator profile may run (see
    /// [`AuthorityProfile::validate`]).
    revocation_store: Option<Arc<dyn icn_store::Store>>,
    /// Configured session lifetime; `None` uses the default.
    token_lifetime: Option<TokenLifetimePolicy>,
    data_dir: Option<std::path::PathBuf>,
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    security_config: SecurityConfig,
    rate_limit_config: Option<RateLimitConfig>,

    compute_handle: Option<ComputeHandle>,
    coop_handle: Option<icn_coop::CoopHandle>,
    /// Optional TrustService for kernel/app separation
    trust_service_handle: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    /// Optional LedgerService for treasury nonce queries
    ledger_service_handle: Option<Arc<dyn icn_kernel_api::services::LedgerService>>,
    /// Optional FederationService for clearing position queries.
    /// When provided, position queries use the supervisor-owned clearing state rather
    /// than the gateway's own divergent ClearingManager instance.
    federation_service_handle: Option<Arc<dyn icn_kernel_api::services::FederationService>>,
    /// Optional handle to daemon's GovernanceActor (for actor-backed mode)
    governance_handle: Option<GovernanceHandle>,
    /// Optional concrete actor handle, parallel to `governance_handle`.
    /// Used solely to install `receipt_store` onto the actor at startup so
    /// actor-path `CloseProposal::Accept` and `ForceCloseProposal::Accept`
    /// emit `InstitutionalEffectRecord` durably (closes parity gap with
    /// the gateway-path close).
    governance_actor_handle: Option<icn_governance_actor::GovernanceHandle>,
    /// Optional handle to daemon's ContractRegistryActor (for contract management)
    contract_registry_handle: Option<icn_ccl::ContractRegistryHandle>,
    /// Optional handle to daemon's TreasuryManager (for treasury operations)
    treasury_handle: Option<TreasuryHandle>,
    /// Optional handle to daemon's Ledger (for balance queries)
    ledger_handle: Option<LedgerHandle>,
    /// Optional handle to daemon's EntityRegistry (for entity management)
    entity_handle: Option<EntityHandle>,
    /// Optional handle to the daemon's canonical, provenance-aware coop_id↔EntityId
    /// name-binding store (#2082/#2190). When present, the gateway builds a trusted,
    /// fail-closed `StoreBackedCoopEntityResolver` for observe-mode treasury
    /// classification (A2c); when absent, the observe path uses the fail-closed
    /// `UnwiredCoopEntityResolver`. Observe-only — never an authorization input.
    coop_entity_map_handle: Option<icn_coop::CoopEntityMapHandle>,
    /// Optional handle to daemon's CommunityActor (for civic engine)
    community_handle: Option<icn_community::CommunityHandle>,
    /// Optional handle to daemon's StewardActor (for SDIS ceremonies)
    steward_handle: Option<icn_steward::StewardHandle>,
    /// Optional handle to daemon's AgreementManager (for inter-cooperative agreements)
    agreement_manager_handle: Option<icn_federation::agreement::AgreementManagerHandle>,
    /// Optional supervisor-initialized ServiceDiscoveryManager with gossip wiring.
    /// When provided, the gateway uses this instead of creating its own (gossip-less) instance.
    service_discovery_manager: Option<Arc<crate::service_discovery_mgr::ServiceDiscoveryManager>>,
    /// Optional naming service for `/v1/names/*` resolution.
    naming_service_handle: Option<Arc<dyn NamingService>>,
    /// Optional WASM registry for module management.
    wasm_registry_handle: Option<Arc<icn_compute::WasmRegistry>>,
    /// Audit pruning configuration
    audit_prune_config: Option<AuditPruneConfig>,
    /// Default trust score for unknown peers (overrides DEFAULT_TRUST_SCORE)
    default_trust_score: Option<f64>,
    /// Hook called when a Charter proposal is accepted: deploys the document to the oracle.
    charter_accepted_hook: Option<icn_governance_actor::http::configure::CharterAcceptedHook>,
    /// Optional pre-initialized CommonsHandle injected from the daemon/supervisor.
    /// When provided, CommonsManager uses this shared handle instead of opening its own sled store.
    /// This is the canonical path for preventing dual sled ownership over commons state.
    commons_handle: Option<icn_commons::CommonsHandle>,
    /// Settlement engine for compute audit queries (task_id → settled status).
    settlement_engine: Option<Arc<dyn icn_kernel_api::services::SettlementQueryService>>,
    /// Deferred dispatch-evidence sink: the daemon constructs this before
    /// the gateway opens the receipt store and passes it through here so
    /// the gateway can install the backend as soon as `ReceiptStore` is
    /// ready. With this installer wired, actor/internal proposal
    /// acceptances produce the same durable per-effect
    /// `EffectDispatchEvidence` as the gateway-close HTTP path.
    dispatch_evidence_sink_installer:
        Option<Arc<icn_governance_actor::DeferredDispatchEvidenceSink>>,
    /// Runtime-owned execution-record store shared from the daemon supervisor.
    /// When present, the receipt-chain audit read endpoints use it instead of
    /// re-opening the (exclusively `sled`-locked) execution path — which would
    /// otherwise fall back to an empty temporary store (Gap C).
    execution_query_store: Option<Arc<dyn icn_store::Store>>,
    /// Best-effort execution-record retention cleanup, supplied by the daemon
    /// supervisor and invoked once, right AFTER the startup dispatch-evidence
    /// backfill (Issue #1987 follow-up). Deferring cleanup until after the
    /// backfill ensures pruning cannot delete a terminal execution record whose
    /// evidence was lost in the crash window before the backfill heals it.
    post_backfill_cleanup: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Dev/demo posture: force-enable self-asserted `/auth/verify` coop issuance
    /// without relying on the `ICN_DEV_MODE` env var (issue #2075). This is the
    /// race-free channel for `icnd --insecure-gateway-no-jwt` (a loopback-only
    /// dev hatch) to opt in, instead of mutating process env after the Tokio
    /// runtime has started. `run()` OR's this with the `ICN_DEV_MODE` env read,
    /// so explicit dev deployments that set the env var keep working too.
    dev_self_serve_auth: bool,
}

/// Decide whether self-asserted `/auth/verify` coop issuance may be enabled
/// (issue #2075).
///
/// Requires BOTH an opted-in dev posture AND a loopback bind. The loopback
/// condition is the safety invariant: self-serve can never be reached by an
/// untrusted remote caller, even if a dev posture is mistakenly set on an
/// all-interfaces listener. Mirrors the `--insecure-gateway-no-jwt` loopback
/// guard. Kept as a free function so the invariant is unit-tested directly.
fn self_serve_coop_allowed(dev_opt_in: bool, bind_addr: &SocketAddr) -> bool {
    dev_opt_in && bind_addr.ip().is_loopback()
}

impl GatewayServer {
    /// Create a new gateway server (uses temporary storage for testing)
    pub fn new(bind_addr: SocketAddr, jwt_secret: Vec<u8>) -> Self {
        GatewayServer {
            bind_addr,
            jwt_secret,
            // Safe-by-default: assume the disposable posture until a deployment
            // declares itself institutional. Declaring the stronger profile
            // without the machinery to back it fails at startup rather than
            // shipping an authority claim the runtime cannot honor.
            authority_profile: AuthorityProfile::PortableEvaluator,
            revocation_store: None,
            token_lifetime: None,
            data_dir: None,
            event_broadcaster: None,
            security_config: SecurityConfig::development(), // Permissive for tests
            rate_limit_config: None,

            compute_handle: None,
            coop_handle: None,
            trust_service_handle: None,
            ledger_service_handle: None,
            federation_service_handle: None,
            governance_handle: None,
            governance_actor_handle: None,
            contract_registry_handle: None,
            treasury_handle: None,
            ledger_handle: None,
            entity_handle: None,
            coop_entity_map_handle: None,
            community_handle: None,
            steward_handle: None,
            agreement_manager_handle: None,
            service_discovery_manager: None,
            naming_service_handle: None,
            wasm_registry_handle: None,
            audit_prune_config: None,
            default_trust_score: None,
            charter_accepted_hook: None,
            commons_handle: None,
            settlement_engine: None,
            dispatch_evidence_sink_installer: None,
            execution_query_store: None,
            post_backfill_cleanup: None,
            dev_self_serve_auth: false,
        }
    }

    /// Create a new gateway server with persistent storage
    pub fn new_with_storage(
        bind_addr: SocketAddr,
        jwt_secret: Vec<u8>,
        data_dir: std::path::PathBuf,
    ) -> Self {
        // Use development security config if ICN_DEV_MODE or ICN_CORS_ORIGINS is set
        let security_config =
            if std::env::var("ICN_DEV_MODE").is_ok() || std::env::var("ICN_CORS_ORIGINS").is_ok() {
                SecurityConfig::development()
            } else {
                SecurityConfig::production()
            };

        GatewayServer {
            bind_addr,
            jwt_secret,
            authority_profile: AuthorityProfile::PortableEvaluator,
            revocation_store: None,
            token_lifetime: None,
            data_dir: Some(data_dir),
            event_broadcaster: None,
            security_config,
            rate_limit_config: None,

            compute_handle: None,
            coop_handle: None,
            trust_service_handle: None,
            ledger_service_handle: None,
            federation_service_handle: None,
            governance_handle: None,
            governance_actor_handle: None,
            contract_registry_handle: None,
            treasury_handle: None,
            ledger_handle: None,
            entity_handle: None,
            coop_entity_map_handle: None,
            community_handle: None,
            steward_handle: None,
            agreement_manager_handle: None,
            service_discovery_manager: None,
            naming_service_handle: None,
            wasm_registry_handle: None,
            audit_prune_config: None,
            default_trust_score: None,
            charter_accepted_hook: None,
            commons_handle: None,
            settlement_engine: None,
            dispatch_evidence_sink_installer: None,
            execution_query_store: None,
            post_backfill_cleanup: None,
            dev_self_serve_auth: false,
        }
    }

    /// Create a new gateway server with shared event broadcaster (for production integration)
    pub fn new_with_broadcaster(
        bind_addr: SocketAddr,
        jwt_secret: Vec<u8>,
        data_dir: Option<std::path::PathBuf>,
        event_broadcaster: Arc<EventBroadcaster>,
    ) -> Self {
        // Use development security config if ICN_DEV_MODE or ICN_CORS_ORIGINS is set
        let security_config =
            if std::env::var("ICN_DEV_MODE").is_ok() || std::env::var("ICN_CORS_ORIGINS").is_ok() {
                SecurityConfig::development()
            } else {
                SecurityConfig::production()
            };

        GatewayServer {
            bind_addr,
            jwt_secret,
            authority_profile: AuthorityProfile::PortableEvaluator,
            revocation_store: None,
            token_lifetime: None,
            data_dir,
            event_broadcaster: Some(event_broadcaster),
            security_config,
            rate_limit_config: None,

            compute_handle: None,
            coop_handle: None,
            trust_service_handle: None,
            ledger_service_handle: None,
            federation_service_handle: None,
            governance_handle: None,
            governance_actor_handle: None,
            contract_registry_handle: None,
            treasury_handle: None,
            ledger_handle: None,
            entity_handle: None,
            coop_entity_map_handle: None,
            community_handle: None,
            steward_handle: None,
            agreement_manager_handle: None,
            service_discovery_manager: None,
            naming_service_handle: None,
            wasm_registry_handle: None,
            audit_prune_config: None,
            default_trust_score: None,
            charter_accepted_hook: None,
            commons_handle: None,
            settlement_engine: None,
            dispatch_evidence_sink_installer: None,
            execution_query_store: None,
            post_backfill_cleanup: None,
            dev_self_serve_auth: false,
        }
    }

    /// Set audit pruning configuration
    pub fn with_audit_prune_config(mut self, config: AuditPruneConfig) -> Self {
        self.audit_prune_config = Some(config);
        self
    }

    /// Set default trust score
    pub fn with_default_trust_score(mut self, score: f64) -> Self {
        self.default_trust_score = Some(score);
        self
    }

    /// Force-enable self-asserted `/auth/verify` coop issuance for a dev/demo
    /// posture, independent of the `ICN_DEV_MODE` env var (issue #2075).
    ///
    /// This is the race-free way for `icnd --insecure-gateway-no-jwt` (a
    /// loopback-only local-dev hatch) to keep its challenge/verify smoke path
    /// working without mutating process environment after the Tokio runtime has
    /// started. `run()` OR's this with the `ICN_DEV_MODE` env read, so it never
    /// disables an env-configured dev posture — it only adds one. Self-serve is
    /// additionally gated on a loopback bind (see `self_serve_coop_allowed`), so
    /// setting this on an all-interfaces listener does NOT expose `/auth/verify`.
    pub fn with_dev_self_serve_auth(mut self, enabled: bool) -> Self {
        self.dev_self_serve_auth = enabled;
        self
    }

    /// Attach a charter-accepted hook so that ratified `Charter` proposals are
    /// deployed automatically when the governance vote closes with `Accepted`.
    ///
    /// The hook receives `(charter_id, charter_yaml)` and is responsible for
    /// parsing the YAML and calling `CharterPolicyOracle::deploy_charter()`.
    pub fn with_charter_accepted_hook(
        mut self,
        hook: icn_governance_actor::http::configure::CharterAcceptedHook,
    ) -> Self {
        self.charter_accepted_hook = Some(hook);
        self
    }

    /// Set custom security configuration
    pub fn with_security_config(mut self, config: SecurityConfig) -> Self {
        self.security_config = config;
        self
    }

    /// Set compute handle for daemon integration
    pub fn with_compute_handle(mut self, handle: ComputeHandle) -> Self {
        self.compute_handle = Some(handle);
        self
    }

    /// Set cooperative handle for daemon integration
    pub fn with_coop_handle(mut self, handle: icn_coop::CoopHandle) -> Self {
        self.coop_handle = Some(handle);
        self
    }

    /// Set community handle for daemon integration
    ///
    /// When set, the CommunityManager will delegate all operations to the daemon's
    /// CommunityActor, ensuring persistence and gossip synchronization.
    pub fn with_community_handle(mut self, handle: icn_community::CommunityHandle) -> Self {
        self.community_handle = Some(handle);
        self
    }

    /// Set steward handle for daemon integration
    ///
    /// When set, the StewardManager will delegate all operations to the daemon's
    /// StewardActor, enabling SDIS enrollment and recovery ceremonies with
    /// threshold PRF computation and VUI uniqueness checking.
    pub fn with_steward_handle(mut self, handle: icn_steward::StewardHandle) -> Self {
        self.steward_handle = Some(handle);
        self
    }

    /// Set trust service for daemon integration (kernel/app separated)
    ///
    /// When set, the TrustManager will use the provided TrustService for trust
    /// score queries, enabling proper kernel/app separation.
    pub fn with_trust_service(
        mut self,
        service: Arc<dyn icn_kernel_api::services::TrustService>,
    ) -> Self {
        self.trust_service_handle = Some(service);
        self
    }

    /// Set ledger service for treasury nonce queries.
    ///
    /// When set, treasury nonce endpoints query the same nonce source-of-truth
    /// used by ledger spend enforcement.
    pub fn with_ledger_service(
        mut self,
        service: Arc<dyn icn_kernel_api::services::LedgerService>,
    ) -> Self {
        self.ledger_service_handle = Some(service);
        self
    }

    /// Set federation service for clearing position queries.
    ///
    /// When set, the `GET /v1/federation/clearing/{id}/position` endpoint queries the
    /// supervisor-owned clearing state rather than the gateway's own divergent instance.
    pub fn with_federation_service(
        mut self,
        service: Arc<dyn icn_kernel_api::services::FederationService>,
    ) -> Self {
        self.federation_service_handle = Some(service);
        self
    }

    /// Set governance handle for daemon integration
    ///
    /// When set, the GovernanceManager will delegate all operations to the daemon's
    /// GovernanceActor, ensuring persistence and gossip synchronization.
    pub fn with_governance_handle(mut self, handle: GovernanceHandle) -> Self {
        self.governance_handle = Some(handle);
        self
    }

    /// Set the concrete actor-backed governance handle.
    ///
    /// This is parallel to `with_governance_handle`, which accepts the
    /// trait-object alias (`Arc<dyn GovernanceOps>`). The concrete handle is
    /// required so the gateway can install its `receipt_store` on the actor
    /// via `install_receipt_store` — without it, actor-path force-close
    /// accept and deadline auto-close do not emit `InstitutionalEffectRecord`.
    pub fn with_governance_actor_handle(
        mut self,
        handle: icn_governance_actor::GovernanceHandle,
    ) -> Self {
        self.governance_actor_handle = Some(handle);
        self
    }

    /// Set contract registry handle for daemon integration
    ///
    /// When set, the contracts API will delegate all operations to the daemon's
    /// ContractRegistryActor, enabling contract management with gossip sync.
    pub fn with_contract_registry_handle(
        mut self,
        handle: icn_ccl::ContractRegistryHandle,
    ) -> Self {
        self.contract_registry_handle = Some(handle);
        self
    }

    /// Set treasury handle for daemon integration
    ///
    /// When set, the treasury API will delegate all operations to the daemon's
    /// TreasuryManager, enabling treasury operations with governance integration.
    pub fn with_treasury_handle(mut self, handle: TreasuryHandle) -> Self {
        self.treasury_handle = Some(handle);
        self
    }

    /// Set ledger handle for balance queries
    ///
    /// When set, treasury balance endpoints query actual ledger balances
    /// instead of returning placeholders.
    pub fn with_ledger_handle(mut self, handle: LedgerHandle) -> Self {
        self.ledger_handle = Some(handle);
        self
    }

    /// Set entity handle for entity management
    ///
    /// When set, the EntityManager delegates all operations to the daemon's
    /// EntityRegistry, ensuring persistence and consistent state.
    pub fn with_entity_handle(mut self, handle: EntityHandle) -> Self {
        self.entity_handle = Some(handle);
        self
    }

    /// Set the canonical, provenance-aware coop_id↔EntityId name-binding store
    /// handle (#2082/#2190).
    ///
    /// When set, the gateway builds a trusted, fail-closed
    /// `StoreBackedCoopEntityResolver` over this store and consults it in
    /// observe-mode treasury classification (A2c). It is observe-only: it resolves
    /// only bindings with trusted provenance, never changes a route outcome, and a
    /// resolved binding grants no authority. When unset, the observe path uses the
    /// fail-closed `UnwiredCoopEntityResolver`.
    pub fn with_coop_entity_map_handle(mut self, handle: icn_coop::CoopEntityMapHandle) -> Self {
        self.coop_entity_map_handle = Some(handle);
        self
    }

    /// Set agreement manager handle for inter-cooperative agreements
    ///
    /// When set, the agreement API endpoints will use the daemon's
    /// AgreementManager for managing formal agreements between cooperatives.
    pub fn with_agreement_manager_handle(
        mut self,
        handle: icn_federation::agreement::AgreementManagerHandle,
    ) -> Self {
        self.agreement_manager_handle = Some(handle);
        self
    }

    /// Set the supervisor-initialized ServiceDiscoveryManager.
    ///
    /// When provided, the gateway uses this manager (which has gossip wiring)
    /// instead of creating its own isolated instance. This enables gossip-backed
    /// service discovery across the network.
    pub fn with_service_discovery_manager(
        mut self,
        manager: Arc<crate::service_discovery_mgr::ServiceDiscoveryManager>,
    ) -> Self {
        self.service_discovery_manager = Some(manager);
        self
    }

    /// Set naming service for daemon integration.
    ///
    /// When set, `/v1/names/*` resolution delegates to this shared service.
    pub fn with_naming_service(mut self, service: Arc<dyn NamingService>) -> Self {
        self.naming_service_handle = Some(service);
        self
    }

    /// Set WASM registry for module management.
    ///
    /// When set, distributed compute can reference WASM modules by hash.
    pub fn with_wasm_registry(mut self, registry: Arc<icn_compute::WasmRegistry>) -> Self {
        self.wasm_registry_handle = Some(registry);
        self
    }

    /// Set shared CommonsHandle from daemon/supervisor.
    ///
    /// When set, CommonsManager delegates all substrate commons operations to this
    /// shared handle instead of opening its own sled store. This is the canonical
    /// runtime path — it prevents dual sled ownership over the same commons data.
    pub fn with_commons_handle(mut self, handle: icn_commons::CommonsHandle) -> Self {
        self.commons_handle = Some(handle);
        self
    }

    /// Set custom rate limiting configuration
    pub fn with_rate_limit_config(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit_config = Some(config);
        self
    }

    /// Attach settlement engine for compute audit queries.
    pub fn with_settlement_engine(
        mut self,
        engine: Arc<dyn icn_kernel_api::services::SettlementQueryService>,
    ) -> Self {
        self.settlement_engine = Some(engine);
        self
    }

    /// Install the deferred dispatch-evidence sink installer.
    ///
    /// The daemon constructs `DeferredDispatchEvidenceSink` at bootstrap,
    /// passes a clone into the kernel as `BootstrapHandles.dispatch_evidence_sink`,
    /// and passes this clone here so the gateway — once it opens its
    /// `ReceiptStore` — can install the backend into the deferred sink.
    /// Without this wiring, actor-path acceptances that complete while
    /// the daemon is running log per-effect results but never persist
    /// `EffectDispatchEvidence` (the parity gap this closes).
    pub fn with_dispatch_evidence_sink_installer(
        mut self,
        installer: Arc<icn_governance_actor::DeferredDispatchEvidenceSink>,
    ) -> Self {
        self.dispatch_evidence_sink_installer = Some(installer);
        self
    }

    /// Share the daemon's runtime-owned execution-record store with the
    /// receipt-chain audit read endpoints.
    ///
    /// In `icnd --gateway-enable`, the decision executor holds the exclusive
    /// `sled` lock on `<data_dir>/store/execution`, so the gateway re-opening
    /// that path fails and falls back to an empty temporary store — making
    /// `/v1/receipts/chain/{decision_hash}` report no execution record. Passing
    /// the same `Arc<dyn Store>` here lets the audit reads see the executor's
    /// real records (keyed `exec:<decision_hash>`). Standalone/test gateways
    /// that have no runtime store leave this `None` and keep the path-open
    /// fallback (Gap C).
    /// Install a persistent store for session revocation.
    ///
    /// Without this, revocation is in-memory and is lost on restart — a posture
    /// only [`AuthorityProfile::PortableEvaluator`] may run. Supplying the store
    /// is what makes [`AuthorityProfile::Institutional`] assemblable.
    pub fn with_revocation_store(mut self, store: Arc<dyn icn_store::Store>) -> Self {
        self.revocation_store = Some(store);
        self
    }

    /// Declare the deployment profile whose authority guarantees must hold.
    ///
    /// A profile is a *requirement*, not a description: declaring
    /// [`AuthorityProfile::Institutional`] without durable revocation aborts
    /// startup with an actionable error rather than degrading silently.
    pub fn with_authority_profile(mut self, profile: AuthorityProfile) -> Self {
        self.authority_profile = profile;
        self
    }

    /// Apply the deployment-configured session lifetime (`token_expiry_hours`).
    pub fn with_token_lifetime(mut self, lifetime: TokenLifetimePolicy) -> Self {
        self.token_lifetime = Some(lifetime);
        self
    }

    pub fn with_execution_query_store(mut self, store: Arc<dyn icn_store::Store>) -> Self {
        self.execution_query_store = Some(store);
        self
    }

    /// Install a best-effort execution-record retention cleanup to run once,
    /// right after the startup dispatch-evidence backfill (Issue #1987
    /// follow-up). The daemon supervisor defers cleanup to here so pruning
    /// cannot delete a terminal execution record whose evidence the backfill
    /// still needs to heal.
    pub fn with_post_backfill_cleanup(mut self, cleanup: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.post_backfill_cleanup = Some(cleanup);
        self
    }

    /// Run the gateway server.
    pub async fn run(self) -> Result<()> {
        self.run_inner(None).await
    }

    /// Run the gateway and acknowledge only after initialization and socket bind.
    ///
    /// The supervisor uses this handshake so it cannot report the gateway actor
    /// active while authority assembly, storage initialization, or binding has
    /// already failed.
    pub async fn run_with_startup_signal(
        self,
        startup: tokio::sync::oneshot::Sender<()>,
        supervisor_ack: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<()> {
        self.run_inner(Some((startup, supervisor_ack))).await
    }

    async fn run_inner(
        self,
        startup: Option<(
            tokio::sync::oneshot::Sender<()>,
            tokio::sync::oneshot::Receiver<()>,
        )>,
    ) -> Result<()> {
        info!("Starting ICN Gateway on {}", self.bind_addr);

        // Initialize server start time for health check uptime reporting
        api::health::init_start_time();

        // SECURITY: Validate JWT secret is configured
        if self.jwt_secret.is_empty() {
            return Err(GatewayError::InternalError(
                "SECURITY: Gateway cannot start with empty JWT secret. \
                 Set ICN_GATEWAY_JWT_secret environment variable or provide --gateway-jwt-secret flag. \
                 The secret should be at least 32 cryptographically random bytes.".to_string()
            ));
        }

        // SECURITY: Enforce minimum JWT secret length for HS256 security
        if self.jwt_secret.len() < 32 {
            return Err(GatewayError::InternalError(format!(
                "SECURITY: JWT secret is only {} bytes. \
                 Minimum 32 bytes required for HS256 to resist brute-force attacks. \
                 Generate a secure secret with: openssl rand -base64 32",
                self.jwt_secret.len()
            )));
        }

        // Create shared managers.
        //
        // SECURITY (issue #2075): the challenge/verify flow lets a caller mint a
        // token carrying its own `coop_id`, which `require_coop_access` then
        // trusts. DID ownership alone does not authorize that coop, so this
        // self-asserted issuance is fail-closed and enabled ONLY when BOTH hold:
        //   (a) a dev posture is opted in — an explicit truthy `ICN_DEV_MODE`
        //       ("1"/"true", matching `ICN_SKIP_CORS` in `security.rs`; stricter
        //       than the CORS dev gate's bare presence check so `ICN_DEV_MODE=0`
        //       cannot open it), OR `dev_self_serve_auth` set via
        //       `with_dev_self_serve_auth` (the race-free channel for
        //       `icnd --insecure-gateway-no-jwt`); AND
        //   (b) the gateway is bound to a LOOPBACK address.
        // The loopback condition makes self-serve safe-by-construction: it can
        // never be reached by an untrusted remote caller, even if a dev posture
        // is mistakenly set on an all-interfaces listener. This matches the
        // existing `--insecure-gateway-no-jwt` loopback guard. Production (and any
        // routable-bind demo) binds coop authority through trusted issuance paths
        // (invites/sessions/enrollment) until RFC-0018's membership/entity
        // binding lands.
        let dev_mode_env = std::env::var("ICN_DEV_MODE")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        let dev_opt_in = self.dev_self_serve_auth || dev_mode_env;
        let allow_self_asserted_coop = self_serve_coop_allowed(dev_opt_in, &self.bind_addr);
        if allow_self_asserted_coop {
            warn!(
                "⚠️  self-asserted cooperative authority is ENABLED at /auth/verify on loopback \
                 bind {} — any local DID may mint a token for any coop_id. DEV/DEMO ONLY; never \
                 use a dev posture on a production gateway (see issue #2075).",
                self.bind_addr
            );
        } else if dev_opt_in {
            // Dev posture requested but the bind is not loopback: stay fail-closed
            // and say so, so the operator understands why /auth/verify returns 403.
            warn!(
                "Dev self-serve auth was requested (ICN_DEV_MODE / insecure-no-jwt) but the gateway \
                 is bound to non-loopback {} — self-asserted /auth/verify issuance stays DISABLED \
                 (fail-closed). Bind to loopback for local-dev self-serve (see issue #2075).",
                self.bind_addr
            );
        }
        // ---- Session authority composition (issues #2436, #2437) ----------
        //
        // The authority subsystem is assembled as ONE explicit value rather than
        // an implicit set of defaults, so what this deployment guarantees is
        // constructible, inspectable, and testable through the same path
        // production uses. `SessionAuthority::new` runs the profile's startup
        // invariants and refuses to assemble a deployment whose required
        // guarantees are missing.
        let lifetime = self.token_lifetime.unwrap_or_default();
        let auth_manager = Arc::new(
            AuthManager::new(self.jwt_secret)
                .with_self_asserted_coop(allow_self_asserted_coop)
                .with_token_ttl(lifetime.ttl()),
        );

        // Durable when the daemon supplied a revocation store; volatile
        // otherwise. The profile decides whether volatile is acceptable — it is
        // never silently upgraded to "revocation supported".
        let revocation: Arc<dyn RevocationAuthority> = match self.revocation_store.clone() {
            Some(store) => Arc::new(StoreRevocationAuthority::new(store)?),
            None => Arc::new(InMemoryRevocationAuthority::new()),
        };

        let session_authority = Arc::new(SessionAuthority::new(
            auth_manager.clone(),
            revocation,
            lifetime,
            self.authority_profile,
        )?);

        let authority_caps = session_authority.capabilities();
        info!(
            profile = authority_caps.profile,
            revocation = ?authority_caps.revocation,
            revocation_durability = authority_caps.revocation_durability,
            revocation_backend = authority_caps.revocation_backend,
            token_ttl_secs = authority_caps.token_ttl_secs,
            "Session authority assembled"
        );
        for note in &authority_caps.notes {
            warn!("Session authority: {}", note);
        }

        // Create cooperative manager (uses actor if handle available, otherwise in-memory)
        let coop_manager: Arc<CoopManager> = if let Some(handle) = self.coop_handle {
            info!("Cooperative manager connected to daemon (using CoopActor)");
            Arc::new(CoopManager::with_handle(handle))
        } else {
            info!("Cooperative manager running standalone (in-memory only)");
            Arc::new(CoopManager::new())
        };

        // Create community manager (requires CommunityActor handle)
        let community_manager: Option<Arc<crate::community_mgr::CommunityManager>> =
            if let Some(handle) = self.community_handle {
                info!("Community manager connected to daemon (using CommunityActor)");
                Some(Arc::new(crate::community_mgr::CommunityManager::new(
                    handle,
                )))
            } else {
                info!("Community manager not configured (community API disabled)");
                None
            };

        // Create steward manager (requires StewardActor handle)
        let steward_manager: Option<Arc<crate::steward_mgr::StewardManager>> =
            if let Some(handle) = self.steward_handle {
                info!("Steward manager connected to daemon (using StewardActor)");
                Some(Arc::new(crate::steward_mgr::StewardManager::new(handle)))
            } else {
                info!("Steward manager not configured (SDIS ceremony API disabled)");
                None
            };

        // Initialize Sled DB early for managers that need persistent storage
        // (governance action items, listings, budgets, etc.)
        // Note: sled::Db is internally Arc-wrapped, so db.clone() is cheap
        let db = if let Some(ref data_dir) = self.data_dir {
            let db_path = data_dir.join("gateway_store");
            match sled::open(&db_path) {
                Ok(db) => {
                    info!("Opened gateway storage at {:?}", db_path);
                    db
                }
                Err(e) => {
                    return Err(crate::error::GatewayError::InternalError(format!(
                        "Failed to open gateway storage: {e}"
                    )));
                }
            }
        } else {
            info!("Using temporary in-memory storage for gateway");
            match sled::Config::new().temporary(true).open() {
                Ok(db) => db,
                Err(e) => {
                    return Err(crate::error::GatewayError::InternalError(format!(
                        "Failed to open temporary storage: {e}"
                    )));
                }
            }
        };

        // Create receipt store early so governance manager can reference it
        let receipt_store = Arc::new(crate::receipt_store::ReceiptStore::new(db.clone()));
        info!("Receipt store initialized");

        // One-shot startup backfill for the ADR-0014 by-grantee index.
        // Databases written between PR #1575 (grant primary + by-decision
        // index) and PR #1579 (by-grantee index + acceptance-seam
        // revocations) hold primary grant records without a by-grantee
        // entry. Without this, `list_*_by_grantee` readers miss those
        // legacy grants and an accepted RemoveSteward/SuspendSteward/
        // RevokeAuthority lifecycle silently leaves them active after
        // acceptance. The backfill is idempotent (deterministic keys,
        // no-op when the index is already complete) and non-destructive
        // (never mutates primary records), so running on every startup
        // is safe; steady-state cost is a single prefix scan with no
        // writes.
        match receipt_store.backfill_grant_by_grantee_index() {
            Ok(0) => {
                info!("ADR-0014 by-grantee index backfill: no legacy grants found");
            }
            Ok(written) => {
                info!(
                    written,
                    "ADR-0014 by-grantee index backfill: recovered legacy grants"
                );
            }
            Err(e) => {
                // Non-fatal: startup continues. Revocation lookups for
                // legacy grants may miss until the next successful
                // backfill run, but the gateway itself is operational.
                // Surfacing the error loudly so operators can investigate
                // (and optionally re-run a manual backfill).
                tracing::error!(
                    error = %e,
                    "ADR-0014 by-grantee index backfill failed at startup; \
                     legacy grants may be invisible to by-grantee readers until next successful run"
                );
            }
        }

        // Install receipt_store on the actor so actor-path `CloseProposal::Accept`
        // and `ForceCloseProposal::Accept` emit `InstitutionalEffectRecord`
        // durably. Without this, the HTTP-close path was the sole writer and
        // force-close / deadline auto-close produced no institutional artifact.
        if let Some(ref actor_handle) = self.governance_actor_handle {
            actor_handle
                .install_receipt_store(receipt_store.clone())
                .await;
            info!("Receipt store installed on governance actor (actor-path parity)");
        }

        // Install the same receipt store into the deferred dispatch-evidence
        // sink handed to us by the daemon. From this point onward, actor/
        // internal acceptance paths flowing through the decision-executor
        // callback produce durable per-effect `EffectDispatchEvidence` —
        // the same artifact the gateway-close HTTP hook writes, just via
        // the kernel seam. Without this call the deferred sink stays a
        // no-op forwarder and the actor-path evidence gap remains open.
        if let Some(ref installer) = self.dispatch_evidence_sink_installer {
            installer.install_backend(receipt_store.clone());
            info!("Dispatch-evidence sink backend installed (actor-path evidence parity active)");
        }

        // Replay any incomplete write-ahead governance close-journal entries NOW
        // that every durable downstream sink a recovered close can feed — the
        // receipt store (installed above) AND the deferred dispatch-evidence sink
        // (installed just above) — is ready. This MUST run after the dispatch-sink
        // install: a recovered `ProposalAccepted` can drive executable effects,
        // and the deferred sink drops `EffectDispatchEvidence` batches until its
        // backend is installed, so replaying earlier could permanently lose that
        // evidence. Recovery is decoupled from `install_receipt_store` for exactly
        // this reason.
        if let Some(ref actor_handle) = self.governance_actor_handle {
            actor_handle.recover_incomplete_closes().await;
            info!("Replayed incomplete governance close-journal entries (post-sink recovery)");
        }

        // Issue #1987: backfill any EffectDispatchEvidence dropped in the async
        // execution window. Runs AFTER the dispatch-evidence sink backend is
        // installed (above) and the close-journal replay, using the same
        // execution-record store the decision executor writes to. The sink's
        // backfill path is idempotent (read-back dedup), so a clean restart
        // re-scans and writes nothing, and replay never duplicates evidence.
        if let (Some(execution_store), Some(sink)) = (
            self.execution_query_store.as_ref(),
            self.dispatch_evidence_sink_installer.as_ref(),
        ) {
            match crate::dispatch_evidence_backfill::backfill_pending_dispatch_evidence(
                execution_store,
                sink,
            ) {
                Ok(report) if report.redriven > 0 => {
                    info!(
                        scanned = report.scanned,
                        redriven = report.redriven,
                        skipped = report.skipped,
                        unparsable = report.unparsable,
                        "Backfilled dispatch evidence for recovered execution records (post-sink recovery)"
                    );
                }
                Ok(report) => {
                    debug!(
                        scanned = report.scanned,
                        "Dispatch-evidence backfill: no terminal records required healing"
                    );
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "Dispatch-evidence backfill scan failed (non-fatal; evidence may remain incomplete until next startup)"
                    );
                }
            }
        }

        // Issue #1987 follow-up: run the deferred execution-record retention
        // cleanup ONLY now — after the dispatch-evidence backfill above — so
        // pruning cannot delete a terminal record whose evidence was lost in the
        // crash window before the backfill had a chance to heal it. The daemon
        // supervisor hands us this cleanup instead of running it at its own
        // (pre-gateway) startup precisely for this ordering.
        if let Some(cleanup) = self.post_backfill_cleanup.as_ref() {
            cleanup();
        }

        // Execution record query store (read-only API surface).
        // Gap C: prefer the runtime-owned execution store shared from the daemon
        // supervisor (the same store the decision executor writes to). Only when
        // no runtime handle is present (standalone/test gateway) do we open the
        // path ourselves — which in the real daemon would hit the executor's
        // exclusive sled lock and fall back to an empty temporary store.
        let execution_query_store: Arc<dyn icn_store::Store> =
            if let Some(store) = self.execution_query_store.clone() {
                info!("Execution query store: using runtime-shared handle (Gap C)");
                store
            } else if let Some(ref data_dir) = self.data_dir {
                let exec_path = data_dir.join("store").join("execution");
                match SledStore::open(&exec_path) {
                    Ok(store) => {
                        info!("Execution query store opened at {:?}", exec_path);
                        Arc::new(store)
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            path = %exec_path.display(),
                            "Failed to open execution query store; falling back to temporary store"
                        );
                        Arc::new(SledStore::temporary().map_err(|err| {
                            GatewayError::InternalError(format!(
                                "Failed to create fallback execution query store: {err}"
                            ))
                        })?)
                    }
                }
            } else {
                Arc::new(SledStore::temporary().map_err(|e| {
                    GatewayError::InternalError(format!(
                        "Failed to create temporary execution query store: {e}"
                    ))
                })?)
            };

        let ledger_handle_for_service = self.ledger_handle.clone();

        // Initialize WASM registry for module management (Flow A)
        let wasm_registry: Option<Arc<icn_compute::WasmRegistry>> = if let Some(registry) =
            self.wasm_registry_handle
        {
            info!("WASM registry initialized (shared from supervisor)");
            Some(registry)
        } else {
            #[cfg(feature = "wasm")]
            {
                if let Some(ref data_dir) = self.data_dir {
                    let wasm_path = data_dir.join("store").join("wasm");
                    match sled::open(&wasm_path) {
                        Ok(db) => {
                            info!("WASM registry store opened at {:?}", wasm_path);
                            Some(Arc::new(icn_compute::WasmRegistry::with_store(db)))
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "Failed to open WASM registry store at {:?}; falling back to temporary storage",
                                wasm_path
                            );
                            Some(Arc::new(icn_compute::WasmRegistry::new()))
                        }
                    }
                } else {
                    info!("WASM registry initialized with temporary storage");
                    Some(Arc::new(icn_compute::WasmRegistry::new()))
                }
            }
            #[cfg(not(feature = "wasm"))]
            None
        };

        // Create governance manager with Sled-backed action items, structures, and activities.
        // (uses GovernanceActor if handle available for proposals/votes/domains)
        // Infer types from Sled*Store constructors — no explicit trait-object
        // annotations here (meaning-firewall ratchet, see icn-core).
        let sled_db_arc = Arc::new(db.clone());
        let structure_store = Arc::new(icn_governance_actor::SledStructureStore::new(
            sled_db_arc.clone(),
        ));
        let activity_store = Arc::new(icn_governance_actor::SledActivityStore::new(
            sled_db_arc.clone(),
        ));
        let meeting_store = Arc::new(icn_governance_actor::SledMeetingStore::new(
            sled_db_arc.clone(),
        ));
        let governance_manager: Arc<GovernanceManager> =
            if let Some(handle) = self.governance_handle {
                info!("Governance manager connected to daemon with persistent action items");
                Arc::new(
                    GovernanceManager::with_sled_action_items(handle, sled_db_arc)
                        .with_receipt_store(receipt_store.clone())
                        .with_structure_store(structure_store)
                        .with_activity_store(activity_store)
                        .with_meeting_store(meeting_store),
                )
            } else {
                info!("Governance manager running standalone with persistent action items");
                Arc::new(
                    GovernanceManager::new_with_sled(sled_db_arc)
                        .with_receipt_store(receipt_store.clone())
                        .with_structure_store(structure_store)
                        .with_activity_store(activity_store)
                        .with_meeting_store(meeting_store),
                )
            };
        let ledger_service: Option<Arc<icn_api::LedgerService>> =
            ledger_handle_for_service.map(|handle| Arc::new(icn_api::LedgerService::new(handle)));

        let invite_manager = Arc::new(crate::invite::InviteManager::new());
        let session_manager = Arc::new(crate::session::SessionManager::new());

        // Keep a reference for governance membership resolution before consuming.
        let trust_service_for_gov = self.trust_service_handle.clone();

        // Create trust manager (prefers TrustService, falls back to in-memory)
        let trust_manager: Arc<TrustManager> = if let Some(service) = self.trust_service_handle {
            info!("Trust manager connected to daemon (using TrustService, kernel/app separated)");
            let mut mgr = TrustManager::with_trust_service(service);
            if let Some(score) = self.default_trust_score {
                info!("Setting custom default trust score: {}", score);
                mgr = mgr.with_default_score(score);
            }
            Arc::new(mgr)
        } else {
            info!("Trust manager running standalone (in-memory only)");
            let mut mgr = TrustManager::new();
            if let Some(score) = self.default_trust_score {
                info!("Setting custom default trust score: {}", score);
                mgr = mgr.with_default_score(score);
            }
            Arc::new(mgr)
        };

        let compute_manager = {
            let mgr = if let Some(handle) = self.compute_handle {
                info!("Compute manager connected to daemon");
                let service = Arc::new(icn_api::ComputeService::new(handle));
                if let Some(registry) = wasm_registry {
                    ComputeManager::with_service_and_registry(service, registry)
                } else {
                    ComputeManager::with_service(service)
                }
            } else {
                info!("Compute manager running standalone (no daemon connection)");
                if let Some(registry) = wasm_registry {
                    ComputeManager::with_registry(registry)
                } else {
                    ComputeManager::new()
                }
            };
            let mgr = if let Some(engine) = self.settlement_engine {
                mgr.with_settlement_engine(engine)
            } else {
                mgr
            };
            Arc::new(mgr)
        };

        // Create contract registry handle for contract management API
        let contract_registry: Option<Arc<icn_ccl::ContractRegistryHandle>> =
            if let Some(handle) = self.contract_registry_handle {
                info!("Contract registry connected to daemon (using ContractRegistryActor)");
                Some(Arc::new(handle))
            } else {
                info!("Contract registry not configured (contracts API disabled)");
                None
            };

        // Use supervisor-provided service discovery manager (gossip-wired) or create a local one
        let service_discovery_manager = if let Some(mgr) = self.service_discovery_manager {
            info!("Service discovery manager initialized (gossip-wired from supervisor)");
            mgr
        } else {
            info!("Service discovery manager initialized (local, no gossip)");
            Arc::new(crate::service_discovery_mgr::ServiceDiscoveryManager::new())
        };

        // Use daemon-provided naming service or create a local sled-backed instance.
        let naming_service: Arc<dyn NamingService> =
            if let Some(service) = self.naming_service_handle {
                info!("Naming service initialized (shared from supervisor)");
                service
            } else {
                let store = if let Some(ref data_dir) = self.data_dir {
                    let store_path = data_dir.join("store").join("naming");
                    let opened = SledStore::open(&store_path).map_err(|e| {
                        GatewayError::InternalError(format!("Failed to open naming store: {e}"))
                    })?;
                    info!("Naming service initialized at {:?}", store_path);
                    Arc::new(opened)
                } else {
                    info!("Naming service initialized with temporary storage");
                    Arc::new(SledStore::temporary().map_err(|e| {
                        GatewayError::InternalError(format!(
                            "Failed to create temporary naming store: {e}"
                        ))
                    })?)
                };
                Arc::new(icn_naming::SledNamingService::new(store))
            };

        // Start background expiry task for service endpoints (every 5 minutes)
        let _service_expiry_handle =
            crate::service_discovery_mgr::start_expiry_task(service_discovery_manager.clone(), 300);

        // FederationManager: uses persistent sled storage when data_dir is available, temp
        // store otherwise (tests / no-data-dir deployments).
        //
        // ARCHITECTURE NOTE (ADR 0011):
        // This store holds federation state that originates from gateway API calls
        // (POST /clearing, POST /coops, POST /attestations, etc.).  It is intentionally
        // SEPARATE from the supervisor-owned FederationService stores (populated by
        // governance effects and compute-layer clearing callbacks).
        //
        // Production deployments should create agreements through governance (which writes
        // to the supervisor's ClearingManager).  Gateway API federation endpoints are the
        // standalone / direct-management path and serve as the truth for that origin path.
        //
        // Read paths for supervisor-owned state (e.g. clearing positions) prefer
        // federation_service_for_routes below; see get_position() handler.
        let federation_manager = if let Some(ref data_dir) = self.data_dir {
            let mgr = FederationManager::new_with_storage(data_dir.clone())?;
            info!(
                "FederationManager: using persistent sled store at {:?}",
                data_dir.join("federation_store")
            );
            Arc::new(mgr)
        } else {
            info!("FederationManager: using temporary in-memory store (no data_dir)");
            Arc::new(FederationManager::new())
        };
        // Wrap the optional FederationService handle so route handlers can get it from app_data.
        // When Some, route handlers MUST prefer this over federation_manager for reads.
        let federation_service_for_routes: Arc<
            Option<Arc<dyn icn_kernel_api::services::FederationService>>,
        > = Arc::new(self.federation_service_handle.clone());
        let commons_manager: Arc<CommonsManager> = if let Some(handle) = self.commons_handle {
            // Canonical path: daemon injected a shared CommonsHandle.
            // CommonsManager is a thin facade over this handle — no second sled store opened.
            info!("CommonsManager: using shared CommonsHandle from daemon (actor-owned state)");
            Arc::new(CommonsManager::with_handle(handle))
        } else if let Some(ref data_dir) = self.data_dir {
            // Fallback: no handle injected (gateway running standalone), open own sled store.
            let commons_path = data_dir.join("commons.sled");
            info!(
                "CommonsManager: opening standalone sled store at {:?}",
                commons_path
            );
            Arc::new(
                CommonsManager::with_sled_path(&commons_path)
                    .context("Failed to open commons sled store")?,
            )
        } else {
            warn!(
                "CommonsManager: no data_dir configured, running in-memory only — \
                 commons/personhood/charter/enrollment state will NOT survive process restart"
            );
            Arc::new(CommonsManager::new())
        };

        // Setup agreement manager if provided (for inter-cooperative agreements)
        let agreement_manager: Option<icn_federation::agreement::AgreementManagerHandle> =
            if let Some(handle) = self.agreement_manager_handle {
                info!("Agreement manager connected to daemon");
                Some(handle)
            } else {
                info!("Agreement manager not configured (agreements API will return stubs)");
                None
            };

        // Create entity manager (uses actor if handle available, otherwise in-memory)
        let entity_manager: Arc<EntityManager> = if let Some(handle) = self.entity_handle {
            info!("Entity manager wired to daemon EntityRegistry");
            Arc::new(EntityManager::with_handle(handle))
        } else {
            warn!("Entity manager running standalone (in-memory only)");
            Arc::new(EntityManager::new())
        };

        // A2c observe-mode coop_id→EntityId resolver. When the daemon wires the
        // canonical, provenance-aware CoopEntityMap, build a trusted, fail-closed
        // StoreBackedCoopEntityResolver; otherwise fall back to the fail-closed
        // UnwiredCoopEntityResolver. This resolver is consulted ONLY by observe-mode
        // treasury classification (RFC-0018, ADR-0035): it resolves only bindings with
        // trusted provenance, changes no route outcome, and grants no authority.
        let observe_coop_entity_resolver = match self.coop_entity_map_handle {
            Some(map) => {
                info!("Coop-entity resolver: store-backed (trusted, observe-only) — A2c wired");
                crate::coop_entity_resolver::ObserveCoopEntityResolver(std::sync::Arc::new(
                    crate::coop_entity_resolver::StoreBackedCoopEntityResolver::new(map),
                ))
            }
            None => {
                info!("Coop-entity resolver: unwired (fail-closed default)");
                crate::coop_entity_resolver::ObserveCoopEntityResolver::unwired()
            }
        };

        // Create treasury manager (uses handle if available, otherwise in-memory)
        let treasury_manager: Arc<GatewayTreasuryManager> = if let Some(handle) =
            self.treasury_handle
        {
            let mut mgr = GatewayTreasuryManager::with_handle(handle);
            // Wire ledger handle for balance queries
            if let Some(ledger_handle) = self.ledger_handle.clone() {
                mgr.set_ledger_handle(ledger_handle);
                info!("Treasury manager connected to daemon (TreasuryManager + Ledger handles)");
            } else {
                info!("Treasury manager connected to daemon (TreasuryManager handle only)");
            }
            if let Some(ledger_service) = self.ledger_service_handle.clone() {
                mgr.set_ledger_service_handle(ledger_service);
            }
            Arc::new(mgr)
        } else {
            info!("Treasury manager running standalone (in-memory only)");
            Arc::new(GatewayTreasuryManager::new())
        };

        // Create SDIS state for identity verification
        let sdis_state = Arc::new(crate::api::sdis::SdisState::new());
        // Create enrollment store wired to CommonsManager for write-through of completed enrollments
        let enrollment_store = Arc::new(
            crate::api::sdis::simple_enrollment::EnrollmentStore::with_commons_manager(
                commons_manager.clone(),
            ),
        );

        // Create budget store (uses db initialized earlier)
        let budget_store = crate::api::budgets::BudgetStore::new(db.clone());
        let budget_store = Arc::new(budget_store);
        info!("Budget store initialized");

        // Create entity audit manager for compliance logging
        // Uses a separate store path to avoid Sled lock conflicts with gateway_store
        let entity_audit_store: Arc<dyn icn_store::Store> =
            if let Some(ref data_dir) = self.data_dir {
                let store_path = data_dir.join("entity_audit_store");
                Arc::new(SledStore::open(&store_path).map_err(|e| {
                    GatewayError::InternalError(format!("Failed to open entity audit store: {e}"))
                })?)
            } else {
                Arc::new(SledStore::temporary().map_err(|e| {
                    GatewayError::InternalError(format!(
                        "Failed to create temporary entity audit store: {e}"
                    ))
                })?)
            };
        let entity_audit_manager = Arc::new(EntityAuditManager::new(entity_audit_store));
        info!("Entity audit manager initialized");

        // Create listings manager with Sled-backed persistent storage
        let listings_manager = Arc::new(RwLock::new(ListingsManager::with_sled(Arc::new(
            db.clone(),
        ))));
        info!("Listings manager initialized with persistent storage");

        // Create ledger manager with persistent storage if data_dir is set
        let mut ledger_manager = if let Some(ref data_dir) = self.data_dir {
            LedgerManager::new_with_storage(data_dir.clone())
        } else {
            LedgerManager::new()
        };

        // Inject budget store for enforcement
        ledger_manager.set_budget_store(budget_store.clone());

        let ledger_manager = Arc::new(ledger_manager);

        // Create identity manager for multi-device support
        let identity_manager = if let Some(ref data_dir) = self.data_dir {
            Arc::new(IdentityManager::new_with_storage(data_dir.clone())?)
        } else {
            Arc::new(IdentityManager::new())
        };

        // Create oracle state for exchange rate management
        let oracle_store: Arc<dyn icn_store::Store> = if let Some(ref data_dir) = self.data_dir {
            let oracle_path = data_dir.join("oracle_store");
            Arc::new(icn_store::SledStore::open(&oracle_path).map_err(|e| {
                crate::error::GatewayError::InternalError(format!(
                    "Failed to open oracle storage: {e}"
                ))
            })?)
        } else {
            Arc::new(icn_store::SledStore::temporary().map_err(|e| {
                crate::error::GatewayError::InternalError(format!(
                    "Failed to create temporary oracle storage: {e}"
                ))
            })?)
        };
        let oracle_state = Arc::new(api::oracle::OracleState::new(oracle_store).await);
        info!("Oracle state initialized with manual rate source");

        // Use provided event broadcaster or create a new one
        let event_broadcaster = self.event_broadcaster.unwrap_or_else(|| {
            info!("Creating new EventBroadcaster (not shared with compute actor)");
            Arc::new(EventBroadcaster::new())
        });

        // Create governance context for apps/governance HTTP handlers.
        // GatewayEventAdapter wires the domain emitter to the WebSocket broadcaster.
        //
        // If a charter_accepted_hook is provided (wired from icnd via init_gateway),
        // it is called when a Charter proposal closes with Accepted.
        //
        // on_proposal_accepted is built inline and dispatches to the appropriate
        // subsystem based on payload type (the GovernanceEffect dispatch table):
        //   FreezeMember    → ledger.freeze_member_with_metadata() + commons Suspended
        //   UnfreezeMember  → ledger.unfreeze_member_with_metadata() + commons Member
        //   DeployCharter   → commons.store_charter() (minimal stub, enables jurisdiction binding)
        //   AppointSteward  → commons.register_steward(jurisdiction = domain_id)
        //   RevokeSteward   → commons.revoke_steward()
        //   (other types)   → no-op until wired
        let ledger_mgr_for_gov = ledger_manager.clone();
        let commons_mgr_for_gov = commons_manager.clone();
        let on_proposal_accepted: icn_governance_actor::http::configure::ProposalAcceptedHook =
            std::sync::Arc::new(move |effect| {
                use icn_governance_actor::http::configure::GovernanceEffect;
                match effect {
                    GovernanceEffect::FreezeMember {
                        proposal_id,
                        domain_id,
                        member,
                        reason,
                        duration_seconds,
                    } => {
                        // Side-effect 1: freeze member in ledger journal.
                        // Applied synchronously (block_in_place) so the freeze is durable
                        // before this hook returns — prevents a race window where the
                        // member could transact between proposal close and ledger freeze.
                        let ledger_mgr = ledger_mgr_for_gov.clone();
                        let domain_id_for_ledger = domain_id.clone();
                        let member_for_ledger = member.clone();
                        let reason_for_ledger = reason.clone();
                        let proposal_id_for_ledger = proposal_id.clone();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async move {
                                match ledger_mgr.get_ledger(&domain_id_for_ledger).await {
                                    Ok(ledger_arc) => {
                                        let mut ledger = ledger_arc.write().await;
                                        ledger.freeze_member_with_metadata(
                                            member_for_ledger,
                                            reason_for_ledger,
                                            duration_seconds,
                                            Some(proposal_id_for_ledger),
                                            None,
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            coop_id = %domain_id_for_ledger,
                                            error = %e,
                                            "FreezeMember governance effect: ledger not found for domain"
                                        );
                                    }
                                }
                            })
                        });

                        // Side-effect 2: suspend commons affiliation for the frozen member.
                        // If the member has no commons holder record (not enrolled), this
                        // is a no-op — commons enrollment is not required for governance.
                        let commons_mgr = commons_mgr_for_gov.clone();
                        tokio::spawn(async move {
                            use icn_identity::{JurisdictionId, MembershipStatus};
                            let jurisdiction = JurisdictionId::new(&domain_id);
                            match commons_mgr.get_holder_by_did(&member).await {
                                Ok(Some(holder)) => {
                                    let holder_id = hex::encode(holder.id());
                                    if let Err(e) = commons_mgr
                                        .update_affiliation_status(
                                            &holder_id,
                                            &jurisdiction,
                                            MembershipStatus::Suspended,
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            did = %member,
                                            jurisdiction = %domain_id,
                                            "FreezeMember: failed to suspend commons affiliation"
                                        );
                                    } else {
                                        tracing::info!(
                                            did = %member,
                                            jurisdiction = %domain_id,
                                            "FreezeMember: commons affiliation suspended"
                                        );
                                    }
                                }
                                Ok(None) => {
                                    // Member not enrolled in commons — acceptable, no-op.
                                    tracing::debug!(
                                        did = %member,
                                        "FreezeMember: member has no commons holder record, skipping suspension"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        did = %member,
                                        "FreezeMember: commons holder lookup failed"
                                    );
                                }
                            }
                        });
                    }
                    GovernanceEffect::UnfreezeMember {
                        proposal_id,
                        domain_id,
                        member,
                        reason,
                    } => {
                        // Side-effect 1: unfreeze member in ledger journal.
                        let ledger_mgr = ledger_mgr_for_gov.clone();
                        let domain_id_for_ledger = domain_id.clone();
                        let member_for_ledger = member.clone();
                        let reason_for_ledger = reason.clone();
                        let proposal_id_for_ledger = proposal_id.clone();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async move {
                                match ledger_mgr.get_ledger(&domain_id_for_ledger).await {
                                    Ok(ledger_arc) => {
                                        let mut ledger = ledger_arc.write().await;
                                        ledger.unfreeze_member_with_metadata(
                                            &member_for_ledger,
                                            reason_for_ledger,
                                            Some(proposal_id_for_ledger),
                                            None,
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            coop_id = %domain_id_for_ledger,
                                            error = %e,
                                            "UnfreezeMember governance effect: ledger not found for domain"
                                        );
                                    }
                                }
                            })
                        });

                        // Side-effect 2: reinstate commons affiliation to Member standing.
                        let commons_mgr = commons_mgr_for_gov.clone();
                        tokio::spawn(async move {
                            use icn_identity::{JurisdictionId, MembershipStatus};
                            let jurisdiction = JurisdictionId::new(&domain_id);
                            match commons_mgr.get_holder_by_did(&member).await {
                                Ok(Some(holder)) => {
                                    let holder_id = hex::encode(holder.id());
                                    if let Err(e) = commons_mgr
                                        .update_affiliation_status(
                                            &holder_id,
                                            &jurisdiction,
                                            MembershipStatus::Member,
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            did = %member,
                                            jurisdiction = %domain_id,
                                            "UnfreezeMember: failed to reinstate commons affiliation"
                                        );
                                    } else {
                                        tracing::info!(
                                            did = %member,
                                            jurisdiction = %domain_id,
                                            "UnfreezeMember: commons affiliation reinstated to Member"
                                        );
                                    }
                                }
                                Ok(None) => {
                                    tracing::debug!(
                                        did = %member,
                                        "UnfreezeMember: member has no commons holder record, skipping reinstatement"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        did = %member,
                                        "UnfreezeMember: commons holder lookup failed"
                                    );
                                }
                            }
                        });
                    }
                    GovernanceEffect::DeployCharter {
                        proposal_id: _,
                        charter_id,
                    } => {
                        // Register a minimal Charter record in commons so the domain is
                        // known to the commons layer.  This is required before any
                        // AppointSteward effect can bind a steward to this domain as a
                        // jurisdiction.  If a charter already exists for this domain
                        // (e.g. created via the HTTP charter API), this is a no-op.
                        let commons_mgr = commons_mgr_for_gov.clone();
                        tokio::spawn(async move {
                            use icn_governance::{
                                Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType,
                            };
                            let charter = Charter::new(
                                OrgType::Cooperative,
                                charter_id.clone(),
                                charter_id.clone(),
                                GovernanceConfig::cooperative_default(),
                                MembershipPolicy::default(),
                                DisputePolicy::default(),
                            );
                            match commons_mgr.store_charter(charter).await {
                                Ok(()) => {
                                    tracing::info!(
                                        domain = %charter_id,
                                        "DeployCharter: commons charter registered for domain"
                                    );
                                }
                                Err(e) => {
                                    // "already exists" is acceptable (HTTP charter API may have
                                    // pre-registered the domain); log other errors as warnings.
                                    if e.to_string().contains("already exists") {
                                        tracing::debug!(
                                            domain = %charter_id,
                                            "DeployCharter: commons charter already exists for domain, skipping"
                                        );
                                    } else {
                                        tracing::warn!(
                                            error = %e,
                                            domain = %charter_id,
                                            "DeployCharter: failed to register commons charter"
                                        );
                                    }
                                }
                            }
                        });
                    }
                    GovernanceEffect::AppointSteward { .. }
                    | GovernanceEffect::RevokeSteward { .. } => {
                        // Owned by the kernel executor path:
                        //   actor acceptance → event_bus → create_effect_subscription
                        //   → DecisionExecutor → KernelGovernanceExecutor
                        //   → SdisServiceImpl::{appoint,revoke}_steward
                        //   → commons.{register,revoke}_steward
                        // Evidence is persisted by `GovernanceDispatchEvidenceSink`
                        // (wired via `BootstrapHandles.dispatch_evidence_sink`).
                        // We deliberately do nothing here to keep SDIS dispatch
                        // single-owner.
                    }
                    GovernanceEffect::Unhandled {
                        proposal_id,
                        payload_type,
                    } => {
                        tracing::trace!(
                            proposal_id = %proposal_id,
                            payload_type = %payload_type,
                            "Proposal accepted; no execution handler wired for this payload type"
                        );
                    }
                }
            });

        // Member-standing gate: proposer must hold active Member affiliation in the
        // target domain's jurisdiction. Wired here so the commons source of truth
        // is always consulted at submission time without the governance app importing
        // any commons types.
        let commons_mgr_for_checker = commons_manager.clone();
        let member_checker: icn_governance_actor::http::configure::MemberStandingChecker =
            std::sync::Arc::new(move |did, domain_id| {
                use icn_identity::{JurisdictionId, MembershipStatus};
                let commons = commons_mgr_for_checker.clone();
                Box::pin(async move {
                    let Ok(Some(holder)) = commons.get_holder_by_did(&did).await else {
                        return false;
                    };
                    let holder_id = hex::encode(holder.id());
                    let Ok(affiliations) = commons.list_affiliations(&holder_id).await else {
                        return false;
                    };
                    affiliations.iter().any(|a| {
                        a.jurisdiction_id == JurisdictionId::new(&domain_id)
                            && a.membership_status == MembershipStatus::Member
                    })
                })
            });

        // Steward standing gate: SDIS proposal types (AppointSteward, RemoveSteward)
        // may only be submitted by active stewards. Wired here so commons remains
        // the sole source of truth for institutional authority.
        let commons_mgr_for_steward = commons_manager.clone();
        let steward_checker: icn_governance_actor::http::configure::StewardStandingChecker =
            std::sync::Arc::new(move |did| {
                let commons = commons_mgr_for_steward.clone();
                Box::pin(async move { commons.is_active_steward(&did).await.unwrap_or(false) })
            });

        // Suspension gate: voters with MemberStatus::Suspended (set by an accepted
        // FreezeMember proposal) are blocked from casting votes. Wired here so the
        // governance app never imports icn-coop types — the closure is the only
        // crossing point, same pattern as member_checker and steward_checker.
        let coop_manager_for_suspension = coop_manager.clone();
        let suspension_checker: icn_governance_actor::http::configure::SuspensionChecker =
            std::sync::Arc::new(move |did, domain_id| {
                let mgr = coop_manager_for_suspension.clone();
                Box::pin(async move { mgr.is_member_suspended(&domain_id, &did).await })
            });

        // SDIS dispatch ownership.
        //
        // In daemon mode the kernel executor owns dispatch for AppointSteward
        // and RevokeSteward effects:
        //   actor acceptance → event_bus → create_effect_subscription
        //   → DecisionExecutor → KernelGovernanceExecutor
        //   → SdisServiceImpl::{appoint,revoke}_steward
        //   → commons.{register,revoke}_steward
        // Durable evidence is written by `GovernanceDispatchEvidenceSink`
        // (wired via `BootstrapHandles.dispatch_evidence_sink`), so the
        // gateway does NOT wire `on_proposal_accepted_with_evidence` here.
        //
        // Previously the gateway also wired an evidence-returning hook that
        // called `commons.register_steward` directly on the HTTP close path.
        // That was a second, parallel dispatch for the same accepted proposal
        // in daemon mode, and since `register_steward` is non-idempotent the
        // second attempt always failed and recorded a spurious "already
        // exists" failure evidence row alongside the kernel path's success.
        // Removing the hook establishes single dispatch ownership: one
        // acceptance, one call, one evidence row.
        //
        // The `ProposalDispatchEvidenceHook` plumbing remains in the
        // governance app so unit-level HTTP tests that simulate a hook-only
        // environment (no actor, no kernel executor) can still wire their
        // own ev_hook. Production wiring leaves it `None`.

        let gov_ctx = GovernanceContext {
            manager: governance_manager.clone(),
            emitter: GatewayEventAdapter::new(event_broadcaster.clone()),
            on_charter_accepted: self.charter_accepted_hook,
            on_proposal_accepted: Some(on_proposal_accepted),
            // See "SDIS dispatch ownership" comment above — kernel executor
            // owns SDIS dispatch; production gateway leaves this `None` to
            // prevent duplicate dispatch.
            on_proposal_accepted_with_evidence: None,
            member_checker: Some(member_checker),
            steward_checker: Some(steward_checker),
            suspension_checker: Some(suspension_checker),
            // Production TrustThreshold membership resolver.
            // Uses TrustServiceMembershipResolver (icn-governance) so this stays
            // outside the gateway's meaning-firewall domain-ref budget.
            // In TrustService mode (daemon-backed): calls get_dids_above_threshold
            // through the kernel/app boundary. In standalone mode: resolver is None,
            // so TrustThreshold domains fail-open (excluded_delegators = None).
            membership_resolver: {
                use icn_governance::{MembershipResolver, TrustServiceMembershipResolver};
                trust_service_for_gov.map(|svc| -> std::sync::Arc<dyn MembershipResolver> {
                    std::sync::Arc::new(TrustServiceMembershipResolver::new(svc))
                })
            },
            // Daemon mode: SDIS execution goes through the actor event system
            // (KernelGovernanceExecutor → SdisServiceImpl). HTTP path leaves this None.
            sdis_service: None,
            // Wire the app-side, act-time authority resolver over the gateway's
            // existing `receipt_store`, which already implements
            // `GovernanceReceiptBackend`. `DefaultMandateGate` introduces no new
            // persistence — it reuses the gateway's mandate/grant indexes.
            //
            // No handler calls `require()` yet (handler wiring is #1868 step 7);
            // populating this field now is necessary so the Production startup
            // guard does not fail closed on the gateway path. Bootstrap/Test
            // contexts may still leave this `None` and receive a warning.
            mandate_gate: Some(Arc::new(icn_governance_actor::DefaultMandateGate::new(
                receipt_store.clone(),
            ))),
            // Deployment posture for the governance HTTP context.
            //
            // Resolved from the `ICN_GOVERNANCE_BUILD_MODE` environment variable
            // (matching the existing `ICN_DEV_MODE` convention). When set to
            // `production`, missing standing checkers/membership resolver below
            // become a hard configuration error that prevents the gateway from
            // starting — see `GovernanceContext::validate`. When unset, defaults
            // to `Bootstrap` so existing dev/devnet behavior is preserved.
            //
            // A daemon-level config field is the obvious next step; see PR body
            // for follow-up notes.
            build_mode: icn_governance_actor::http::GovernanceContextBuildMode::from_env(),
        };

        // Fail fast at startup if the governance context is missing required
        // production dependencies in `Production` mode. Bootstrap/Test mode
        // logs warnings via `tracing::warn!` but does not reject startup,
        // preserving current dev/devnet behavior.
        if let Err(err) = gov_ctx.validate() {
            return Err(GatewayError::InternalError(err.to_string()));
        }

        // Create rate limiter with configured or default config
        let rate_limit_config = self.rate_limit_config.unwrap_or_default();
        info!(
            "Rate limiter: capacity={}, refill_rate={}/sec",
            rate_limit_config.capacity, rate_limit_config.refill_rate
        );
        let rate_limiter = Arc::new(RateLimiter::new(rate_limit_config));

        // Create IP-based rate limiter for auth endpoints and interest submissions
        // Used for DoS protection on sensitive endpoints
        let ip_rate_limiter = Arc::new(IpRateLimiter::new_for_auth());

        // Create trust-gated velocity limiter for transaction rate limiting
        // Limits: Isolated=10, Known=50, Partner=100, Federated=200 tx/hour
        let velocity_limiter = Arc::new(VelocityLimiter::new(VelocityLimitConfig::default()));
        info!("Velocity limiter initialized (trust-gated: 10-200 tx/hour by trust class)");

        // Create persistent notification store
        let notification_store = Arc::new(crate::notifications::NotificationStore::new(db.clone()));
        info!("Persistent notification store initialized");

        // Create notification service (FCM credentials would be loaded from config in production)
        let notification_service =
            Arc::new(NotificationService::new(notification_store.clone(), None));
        info!("Notification service initialized");

        // Create notification queue and processor
        let (notification_queue, notification_receiver) = NotificationQueue::new();
        let notification_queue = Arc::new(notification_queue);
        let notification_processor = Arc::new(NotificationProcessor::new(
            notification_queue.clone(),
            notification_store.clone(),
            notification_service.clone(),
            ProcessorConfig::from_env(),
        ));
        info!("Notification queue and processor initialized");

        // Start notification processor
        let _processor_handle = notification_processor.clone().start(notification_receiver);
        info!("Notification processor started");

        // Start ledger notification trigger (connects ledger events to notifications)
        let ledger_emitter = ledger_manager.get_event_emitter();
        let ledger_trigger = LedgerNotificationTrigger::new(
            ledger_emitter,
            notification_queue.clone(),
            "default-coop".to_string(), // Default coop ID for ledger events
        );
        let _ledger_trigger_handle = ledger_trigger.start();
        info!("Ledger notification trigger started");

        // Create governance notification trigger (for proposal/amendment/appeal notifications)
        let governance_trigger = Arc::new(GovernanceNotificationTrigger::new(
            notification_queue.clone(),
        ));

        // Create decision registry (in-memory store for meetings and decisions)
        let decision_registry = Arc::new(api::registry::DecisionRegistry::new());
        info!("Decision registry initialized");

        // receipt_store was created earlier (before governance manager) for shared use

        // Create recurring settlement store
        let recurring_payment_store =
            crate::api::recurring_settlements::RecurringPaymentStore::new(db.clone());
        info!("Recurring settlement store initialized");

        // Create escrow store
        let escrow_store = crate::api::escrow::EscrowStore::new(db.clone());
        info!("Escrow store initialized");

        let _recurring_settlements_handle = crate::api::recurring_settlements::start_scheduler(
            recurring_payment_store.clone(),
            ledger_manager.clone(),
            60, // Check every minute
        );
        info!("Recurring settlements scheduler started");

        // Start listings expiry scheduler
        let _listings_expiry_handle = crate::listings_mgr::start_expiry_scheduler(
            listings_manager.clone(),
            crate::listings_mgr::DEFAULT_EXPIRY_CHECK_INTERVAL_SECS, // 1 hour
        );
        info!("Listings expiry scheduler started (interval: 1 hour)");

        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

        // Spawn background cleanup task with graceful shutdown
        {
            let auth_manager_clone = auth_manager.clone();
            let rate_limiter_clone = rate_limiter.clone();

            let ip_rate_limiter_clone = ip_rate_limiter.clone();
            let velocity_limiter_clone = velocity_limiter.clone();
            let event_broadcaster_clone = event_broadcaster.clone();
            let coop_manager_clone = coop_manager.clone();
            let mut shutdown_signal = shutdown_tx.subscribe();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Clean up expired authentication challenges
                            if let Ok(removed) = auth_manager_clone.cleanup_expired_challenges() {
                                if removed > 0 {
                                    info!("Cleaned up {} expired authentication challenges", removed);
                                }
                            }

                            // Clean up inactive rate limiter buckets (1 hour inactivity)
                            let removed = rate_limiter_clone.cleanup_inactive_buckets(Duration::from_secs(RATE_LIMITER_CLEANUP_INTERVAL_SECS));
                            if removed > 0 {
                                info!("Cleaned up {} inactive rate limiter buckets", removed);
                            }



                            // Clean up inactive IP rate limiter buckets (10 minute inactivity for auth endpoints)
                            let removed = ip_rate_limiter_clone.cleanup_inactive_buckets(Duration::from_secs(600));
                            if removed > 0 {
                                info!("Cleaned up {} inactive IP rate limiter buckets", removed);
                            }

                            // Clean up expired velocity limiter windows (2 hour expiry for 1 hour windows)
                            let removed = velocity_limiter_clone.cleanup_inactive(Duration::from_secs(7200));
                            if removed > 0 {
                                info!("Cleaned up {} expired velocity limiter windows", removed);
                            }

                            // Clean up dead WebSocket channels for all cooperatives
                            // Get all coop IDs from coop manager
                            if let Ok(coops) = coop_manager_clone.list_all_coop_ids() {
                                for coop_id in coops {
                                    event_broadcaster_clone.cleanup(&coop_id).await;
                                }
                            }
                        }
                        _ = shutdown_signal.recv() => {
                            info!("Cleanup task received shutdown signal");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn audit record pruning background task if configured
        if let Some(ref prune_config) = self.audit_prune_config {
            if prune_config.enabled {
                let entity_audit_manager_clone = entity_audit_manager.clone();
                let retention_days = prune_config.retention_days;
                let max_records = prune_config.max_records_per_entity;
                let batch_size = prune_config.batch_size;
                let prune_interval = Duration::from_secs(prune_config.prune_interval_secs);
                let mut shutdown_signal = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(prune_interval);
                    info!(
                        "Audit prune task started: retention_days={}, max_records={}, interval={}s",
                        retention_days,
                        max_records,
                        prune_interval.as_secs()
                    );

                    loop {
                        tokio::select! {
                            _ = interval.tick() => {
                                let start = std::time::Instant::now();
                                match entity_audit_manager_clone.prune_audit_records(
                                    retention_days,
                                    max_records,
                                    batch_size,
                                ) {
                                    Ok(stats) => {
                                        let duration = start.elapsed().as_secs_f64();
                                        if stats.records_pruned > 0 {
                                            info!(
                                                "Audit prune completed: scanned={}, pruned={}, entities={}, duration={:.2}s",
                                                stats.records_scanned,
                                                stats.records_pruned,
                                                stats.entities_processed,
                                                duration
                                            );
                                        }
                                        // Record metrics
                                        icn_obs::metrics::gateway::entity_audit_pruned_inc(stats.records_pruned);
                                        icn_obs::metrics::gateway::entity_audit_prune_duration(duration);
                                    }
                                    Err(e) => {
                                        warn!("Audit prune failed: {}", e);
                                        icn_obs::metrics::gateway::entity_audit_prune_failed_inc();
                                    }
                                }
                            }
                            _ = shutdown_signal.recv() => {
                                info!("Audit prune task received shutdown signal");
                                break;
                            }
                        }
                    }
                });
                info!("Audit record pruning task started");
            }
        }

        // Clone security config for the move closure
        let security_config = self.security_config.clone();

        // Initialize Prometheus metrics
        // SAFETY: PrometheusMetricsBuilder with valid config always succeeds
        #[allow(clippy::unwrap_used)]
        let prometheus = PrometheusMetricsBuilder::new("api")
            .endpoint("/metrics")
            .build()
            .unwrap();

        // SAFETY: PrometheusMetricsBuilder with valid config always succeeds
        #[allow(clippy::unwrap_used)]
        let icn_gateway_metrics = PrometheusMetricsBuilder::new("icn_gateway")
            .endpoint("/metrics")
            .build()
            .unwrap();

        // Generate OpenAPI spec
        let openapi = crate::openapi::ApiDoc::openapi();

        let server = HttpServer::new(move || {
            // Create JWT authentication middleware
            let auth = HttpAuthentication::bearer(crate::middleware::jwt_auth);
            let auth_for_notifications = HttpAuthentication::bearer(crate::middleware::jwt_auth);

            // Configure CORS based on security config
            let cors = configure_cors(&security_config);

            App::new()
                // Swagger UI for API documentation
                .service(
                    SwaggerUi::new("/swagger-ui/{_:.*}")
                        .url("/api-docs/openapi.json", openapi.clone()),
                )
                // Shared state
                // Authority composition boundary: issuance handlers and every
                // authenticated request resolve this one object, so a router
                // cannot install a bare issuer without revocation enforcement.
                .app_data(web::Data::new(session_authority.clone()))
                .app_data(web::Data::new(coop_manager.clone()))
                .app_data(web::Data::new(community_manager.clone()))
                .app_data(web::Data::new(steward_manager.clone()))
                .app_data(web::Data::new(governance_manager.clone()))
                .app_data(web::Data::new(ledger_service.clone()))
                .app_data(web::Data::new(invite_manager.clone()))
                .app_data(web::Data::new(session_manager.clone()))
                .app_data(web::Data::new(trust_manager.clone()))
                .app_data(web::Data::new(trust_manager.as_oracle()))
                .app_data(web::Data::new(compute_manager.clone()))
                .app_data(web::Data::new(federation_manager.clone()))
                .app_data(web::Data::new(federation_service_for_routes.clone()))
                .app_data(web::Data::new(commons_manager.clone()))
                .app_data(web::Data::new(entity_manager.clone()))
                .app_data(web::Data::new(observe_coop_entity_resolver.clone()))
                .app_data(web::Data::new(entity_audit_manager.clone()))
                .app_data(web::Data::new(treasury_manager.clone()))
                .app_data(web::Data::new(ledger_manager.clone()))
                .app_data(web::Data::new(identity_manager.clone()))
                .app_data(web::Data::new(oracle_state.clone()))
                .app_data(web::Data::new(sdis_state.clone()))
                .app_data(web::Data::new(enrollment_store.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .app_data(web::Data::new(notification_queue.clone()))
                .app_data(web::Data::new(notification_processor.clone()))
                .app_data(web::Data::new(governance_trigger.clone()))
                .app_data(web::Data::new(recurring_payment_store.clone()))
                .app_data(web::Data::new(escrow_store.clone()))
                .app_data(web::Data::from(budget_store.clone()))
                .app_data(web::Data::new(rate_limiter.clone()))
                .app_data(web::Data::new(ip_rate_limiter.clone()))
                .app_data(web::Data::from(velocity_limiter.clone()))
                // Contract registry (optional - for contract management API)
                .app_data(web::Data::new(contract_registry.clone()))
                // Agreement manager (optional - for inter-cooperative agreements)
                .app_data(web::Data::new(agreement_manager.clone()))
                // Service discovery manager
                .app_data(web::Data::new(service_discovery_manager.clone()))
                .app_data(web::Data::new(naming_service.clone()))
                // Listings manager for cooperative exchange
                .app_data(web::Data::new(listings_manager.clone()))
                // Decision registry for governance meetings and decisions
                .app_data(web::Data::new(decision_registry.clone()))
                // Economic receipts store (AllocationReceipt, SettlementIntent)
                .app_data(web::Data::new(receipt_store.clone()))
                // Execution query store for /v1/execution endpoints
                .app_data(web::Data::new(execution_query_store.clone()))
                // JSON payload size limit (256KB - we're not handling file uploads)
                .app_data(web::JsonConfig::default().limit(262_144))
                // Middleware (order: last wrapped runs first for REQUEST, first runs last for RESPONSE)
                // For CORS header removal when behind reverse proxy, SecurityHeaders must run AFTER cors
                .wrap(icn_gateway_metrics.clone())
                .wrap(crate::middleware::MetricsMiddleware)
                .wrap(prometheus.clone())
                .wrap(cors)
                .wrap(SecurityHeaders::new(security_config.clone()))
                .wrap(TracingLogger::default())
                .wrap(middleware::Compress::default())
                // API v1 - single scope with public and protected routes
                .service(
                    web::scope("/v1")
                        // Public endpoints (no auth required)
                        .service(api::health::liveness)
                        .service(api::health::readiness)
                        .service(api::health::ready)
                        .service(api::health::health)
                        .service(api::health::health_detailed)
                        .service(api::health::health_full)
                        .service(api::auth::challenge)
                        .service(api::auth::verify)
                        .service(api::websocket::websocket)
                        // QR login session endpoints
                        // Note: create_session and get_session_status are public
                        // approve_session requires auth (wrapped with auth middleware)
                        .service(
                            web::scope("/sessions")
                                .service(api::sessions::create_session)
                                .service(api::sessions::get_session_status)
                                .service(
                                    web::resource("/{session_id}/approve")
                                        .route(
                                            web::post().to(api::sessions::approve_session_handler),
                                        )
                                        .wrap(auth.clone()),
                                ),
                        )
                        // Public identity resolution (for federation)
                        .service(
                            web::scope("/identity")
                                .service(api::identity::resolve_did)
                                .service(api::identity::identity_health),
                        )
                        // Public member profiles (read-only)
                        .service(api::members::get_member_profile)
                        // Protected member profile update (auth + rate limiting)
                        .service(
                            web::scope("/members")
                                .service(api::members::update_member_profile)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Public cooperative statistics (no auth required)
                        .service(api::coops::get_coop_stats)
                        // SDIS endpoints. The scope is deliberately split by
                        // authority (issue #2443): public verification and
                        // enrollment initiation stay reachable without a
                        // credential, while steward and moderation routes are
                        // mounted behind `jwt_auth` so they inherit signature
                        // validation, the configured lifetime ceiling, durable
                        // revocation, and fail-closed authority construction.
                        .service(
                            web::scope("/sdis")
                                .service(api::sdis::sdis_health)
                                .service(api::sdis::generate_ephemeral)
                                .service(api::sdis::verify_level1)
                                .service(api::sdis::verify_level2)
                                .configure(api::sdis::simple_enrollment::configure)
                                // Note: enrollment::configure disabled - using simple_enrollment as primary API
                                // .configure(api::sdis::enrollment::configure)
                                .configure(api::sdis::recovery::configure)
                                .configure(api::sdis::anchor::configure)
                                // Steward/moderation surface: same `/sdis`
                                // prefix, authenticated. Nested last so the
                                // public routes above keep their unwrapped
                                // behavior.
                                .service(
                                    web::scope("").wrap(auth.clone()).configure(
                                        api::sdis::simple_enrollment::configure_protected,
                                    ),
                                ),
                        )
                        // Protected coop endpoints (auth + rate limiting)
                        .service(
                            web::scope("/coops")
                                .service(api::coops::create_coop)
                                .service(api::coops::get_coop)
                                .service(api::coops::update_settings)
                                .service(api::coops::delete_coop)
                                .service(api::coops::add_member)
                                .service(api::coops::remove_member)
                                .service(api::coops::update_member_role)
                                // Apply auth first, then rate limiting (wrapping order: last runs first)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Flow C convenience aliases under /v1/coops
                        .service(
                            web::scope("/coops")
                                .configure(api::flow_c::configure_coops)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Flow C convenience aliases under /v1/proposals
                        .service(
                            web::scope("/proposals")
                                .configure(api::flow_c::configure_proposals)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected ledger endpoints (auth + rate limiting)
                        .service(
                            web::scope("/ledger")
                                .service(api::ledger::get_position)
                                .service(api::ledger::create_settlement)
                                .service(api::ledger::get_history)
                                .service(api::ledger::get_entries_by_decision)
                                .service(api::ledger::create_cross_settlement)
                                .service(api::ledger::get_cross_settlement_quote)
                                // Apply auth first, then rate limiting (wrapping order: last runs first)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected treasury endpoints (auth + rate limiting)
                        // Treasury operations for cooperative collective reserves
                        .service(
                            web::scope("/treasury")
                                .configure(api::treasury::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Exchange rate oracle endpoints (auth + rate limiting)
                        // Multi-currency support with cooperative-defined and federation rates
                        .service(
                            web::scope("/oracle")
                                .configure(api::oracle::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Rights summary endpoint (auth + rate limiting)
                        .service(
                            web::scope("/rights")
                                .service(api::rights::rights_summary)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected invite endpoints (auth + rate limiting)
                        .service(
                            web::scope("/invites")
                                .service(api::invites::create_invite)
                                .service(api::invites::list_invites)
                                .service(api::invites::join_via_invite)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected compute endpoints (auth + rate limiting)
                        .service(
                            web::scope("/compute")
                                .service(api::compute::submit_task)
                                .service(api::compute::get_status)
                                .service(api::compute::upload_wasm)
                                .service(api::compute::list_wasm)
                                .service(api::compute::get_wasm_metadata)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Execution record query endpoints (auth + rate limiting)
                        .service(
                            web::scope("/execution")
                                .configure(api::execution::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Service discovery endpoints (auth + rate limiting)
                        .service(
                            web::scope("/names")
                                .configure(api::names::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected services endpoints (auth + rate limiting)
                        .service(
                            web::scope("/services")
                                .configure(api::services::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected contracts endpoints (auth + rate limiting)
                        // Only available when ContractRegistryActor is configured
                        .service(
                            web::scope("/contracts")
                                .configure(api::contracts::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected entity endpoints (auth + rate limiting)
                        // Entity CRUD for cooperatives and federations
                        .service(
                            web::scope("/entities")
                                .configure(api::entity::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected listings endpoints (auth + rate limiting)
                        // Internal cooperative exchange - offers, wants, interests
                        .service(
                            web::scope("/listings")
                                .configure(api::listings::configure_routes)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected federation endpoints (auth + rate limiting)
                        .service(
                            web::scope("/federation")
                                .service(api::federation::get_status)
                                .service(api::federation::get_topology)
                                .service(api::federation::init_federation)
                                .service(api::federation::list_coops)
                                .service(api::federation::get_coop)
                                .service(api::federation::register_coop)
                                .service(api::federation::get_vouches)
                                .service(api::federation::vouch_for_coop)
                                .service(api::federation::get_attestations)
                                .service(api::federation::issue_attestation)
                                .service(api::federation::list_agreements)
                                .service(api::federation::get_agreement)
                                .service(api::federation::create_agreement)
                                .service(api::federation::propose_clearing_adoption)
                                .service(api::federation::get_position)
                                .service(api::federation::trigger_settlement)
                                .service(api::federation::process_scheduled_settlements)
                                .service(api::federation::perform_multilateral_netting)
                                .service(api::federation::apply_multilateral_netting)
                                .service(api::federation::federation_connect)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected trust endpoints (auth + rate limiting)
                        .service(
                            web::scope("/trust")
                                .service(api::trust::get_trust_score)
                                .service(api::trust::get_trust_edges)
                                .service(api::trust::create_trust_attestation)
                                .service(api::trust::revoke_trust_attestation)
                                .service(api::trust::get_trust_network)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected device management endpoints (auth + rate limiting)
                        // These enable multi-device support for mobile apps
                        // Routes: /v1/devices/{did}, /v1/devices/{did}/{device_id}
                        .service(
                            web::scope("/devices")
                                .service(api::devices::register_device)
                                .service(api::devices::list_devices)
                                .service(api::devices::get_device)
                                .service(api::devices::revoke_device)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected notification endpoints (auth required)
                        .service(
                            web::scope("/notifications")
                                .service(api::notifications::register_device)
                                .service(api::notifications::unregister_device)
                                .wrap(auth_for_notifications),
                        )
                        // Commons Evolution endpoints (auth + rate limiting)
                        .service(
                            web::scope("/commons")
                                .configure(api::commons::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Charter management endpoints (auth + rate limiting)
                        .service(
                            web::scope("/charter")
                                .configure(api::charter::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Steward management endpoints (auth + rate limiting)
                        .service(
                            web::scope("/steward")
                                .configure(api::steward::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Membership management endpoints (auth + rate limiting)
                        .service(
                            web::scope("/membership")
                                .configure(api::membership::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Constitutional governance endpoints (auth + rate limiting)
                        .service(
                            web::scope("/constitutional")
                                .configure(api::constitutional::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Decision Registry endpoints (auth + rate limiting)
                        // Governance meetings and decisions indexing
                        .service(
                            web::scope("/registry")
                                .configure(api::registry::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Economic receipts endpoints (auth + rate limiting)
                        // AllocationReceipt + SettlementIntent queries
                        .service(
                            web::scope("/receipts")
                                .configure(api::receipts::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Governance endpoints — served by apps/governance HTTP layer.
                        // Routes are registered by configure_governance() under /gov scope;
                        // auth + rate limiting applied via wrapping middleware.
                        // MUST be before empty-scope services (empty scopes shadow later ones).
                        .service(
                            web::scope("/gov")
                                .configure({
                                    let ctx = gov_ctx.clone();
                                    move |cfg| configure_governance(cfg, ctx.clone())
                                })
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // === Empty-scope services (MUST be last — empty scopes match all paths) ===
                        // Recurring payments endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::recurring_settlements::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Escrow endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::escrow::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Budget endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::budgets::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Governance dashboard (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::governance_dashboard::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Community (Civic Engine) endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::communities::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        ),
                )
                // Static files and root route
                .service(web::redirect("/", "/static/index.html"))
                .service(
                    web::scope("/static")
                        .wrap(
                            middleware::DefaultHeaders::new()
                                .add(("Cache-Control", "no-store, no-cache, must-revalidate")),
                        )
                        .service(
                            fs::Files::new("/", get_static_dir())
                                .prefer_utf8(true)
                                .use_last_modified(true),
                        ),
                )
        })
        // Production-ready HTTP timeout configuration
        .keep_alive(Duration::from_secs(75)) // HTTP keep-alive timeout (75s is standard)
        .client_request_timeout(Duration::from_secs(30)) // Max time to read request headers
        .client_disconnect_timeout(Duration::from_secs(5)) // Max time waiting for client to read response
        .bind(self.bind_addr)?
        .run();

        if let Some((startup, supervisor_ack)) = startup {
            startup.send(()).map_err(|_| {
                GatewayError::InternalError(
                    "gateway supervisor dropped startup acknowledgement channel".to_string(),
                )
            })?;
            supervisor_ack.await.map_err(|_| {
                GatewayError::InternalError(
                    "gateway supervisor did not acknowledge actor activation".to_string(),
                )
            })?;
        }

        // Wait for server to complete and then signal cleanup task to shutdown
        let result = server.await;

        // Signal cleanup task to shutdown
        info!("Server shutting down, signaling cleanup task");
        let _ = shutdown_tx.send(());

        // Give cleanup task a moment to finish
        tokio::time::sleep(Duration::from_millis(100)).await;

        result?;
        Ok(())
    }
}

/// Get the static files directory
///
/// In development, uses the source tree static directory.
/// In production, looks for static files relative to the binary.
fn get_static_dir() -> PathBuf {
    // Try environment variable first (for custom installations)
    if let Ok(static_dir) = std::env::var("ICN_STATIC_DIR") {
        return PathBuf::from(static_dir);
    }

    // Development: use source tree
    let dev_static = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    if dev_static.exists() {
        return dev_static;
    }

    // Production: relative to binary
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let prod_static = parent.join("static");
            if prod_static.exists() {
                return prod_static;
            }
        }
    }

    // Fallback to current directory
    PathBuf::from("static")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SECURITY (#2075): self-asserted coop issuance requires BOTH a dev opt-in
    /// AND a loopback bind. A dev posture on a routable/all-interfaces listener
    /// must NOT enable it.
    #[test]
    fn self_serve_coop_requires_dev_optin_and_loopback() {
        let loopback_v4: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let loopback_v6: SocketAddr = "[::1]:8080".parse().unwrap();
        let all_ifaces: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let routable: SocketAddr = "10.0.0.5:8080".parse().unwrap();

        // dev opt-in + loopback => enabled
        assert!(self_serve_coop_allowed(true, &loopback_v4));
        assert!(self_serve_coop_allowed(true, &loopback_v6));

        // dev opt-in but NOT loopback => disabled (the #2075 exposure guard)
        assert!(!self_serve_coop_allowed(true, &all_ifaces));
        assert!(!self_serve_coop_allowed(true, &routable));

        // no dev opt-in => disabled regardless of bind
        assert!(!self_serve_coop_allowed(false, &loopback_v4));
        assert!(!self_serve_coop_allowed(false, &all_ifaces));
    }

    #[tokio::test]
    async fn test_jwt_secret_empty_fails_startup() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = GatewayServer::new(addr, vec![]);

        let result = server.run().await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("empty JWT secret"),
            "Expected empty JWT secret error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn startup_signal_reports_failure_before_readiness() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = GatewayServer::new(addr, vec![]);
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        let (_supervisor_ack_tx, supervisor_ack_rx) = tokio::sync::oneshot::channel();

        let result = server
            .run_with_startup_signal(startup_tx, supervisor_ack_rx)
            .await;

        assert!(result.is_err());
        assert!(
            startup_rx.await.is_err(),
            "a failed server must never acknowledge readiness"
        );
    }

    #[tokio::test]
    async fn test_jwt_secret_too_short_fails_startup() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        // 16 bytes is too short (minimum is 32)
        let short_secret = vec![0u8; 16];
        let server = GatewayServer::new(addr, short_secret);

        let result = server.run().await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("32 bytes required") || err_msg.contains("16 bytes"),
            "Expected JWT secret length error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_jwt_secret_31_bytes_fails_startup() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        // 31 bytes is still too short (minimum is 32)
        let almost_enough = vec![0u8; 31];
        let server = GatewayServer::new(addr, almost_enough);

        let result = server.run().await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("32 bytes required") || err_msg.contains("31 bytes"),
            "Expected JWT secret length error, got: {err_msg}"
        );
    }

    #[test]
    fn test_jwt_secret_32_bytes_passes_validation() {
        // This test verifies the validation logic allows 32+ byte secrets
        // We can't fully test run() without starting a server, but we can
        // verify the constructor accepts the secret
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let valid_secret = vec![0u8; 32];
        let server = GatewayServer::new(addr, valid_secret.clone());

        // The server should be created with the secret intact
        assert_eq!(server.jwt_secret.len(), 32);
    }

    #[test]
    fn test_jwt_secret_64_bytes_passes_validation() {
        // Longer secrets should also work
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let long_secret = vec![0u8; 64];
        let server = GatewayServer::new(addr, long_secret.clone());

        assert_eq!(server.jwt_secret.len(), 64);
    }
}
