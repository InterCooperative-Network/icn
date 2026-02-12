//! Gateway server

use actix_files as fs;
use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use actix_web_prom::PrometheusMetricsBuilder;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
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
use crate::treasury_mgr::{GatewayTreasuryManager, LedgerHandle, TreasuryHandle};
use crate::trust_mgr::{TrustGraphHandle, TrustManager};
use icn_compute::ComputeHandle;
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
    data_dir: Option<std::path::PathBuf>,
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    security_config: SecurityConfig,
    rate_limit_config: Option<RateLimitConfig>,

    compute_handle: Option<ComputeHandle>,
    coop_handle: Option<icn_coop::CoopHandle>,
    /// Optional handle to daemon's TrustGraph (for actor-backed mode, deprecated)
    trust_graph_handle: Option<TrustGraphHandle>,
    /// Optional TrustService for kernel/app separation (preferred over trust_graph_handle)
    trust_service_handle: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    /// Optional handle to daemon's GovernanceActor (for actor-backed mode)
    governance_handle: Option<GovernanceHandle>,
    /// Optional handle to daemon's ContractRegistryActor (for contract management)
    contract_registry_handle: Option<icn_ccl::ContractRegistryHandle>,
    /// Optional handle to daemon's TreasuryManager (for treasury operations)
    treasury_handle: Option<TreasuryHandle>,
    /// Optional handle to daemon's Ledger (for balance queries)
    ledger_handle: Option<LedgerHandle>,
    /// Optional handle to daemon's EntityRegistry (for entity management)
    entity_handle: Option<EntityHandle>,
    /// Optional handle to daemon's CommunityActor (for civic engine)
    community_handle: Option<icn_community::CommunityHandle>,
    /// Optional handle to daemon's StewardActor (for SDIS ceremonies)
    steward_handle: Option<icn_steward::StewardHandle>,
    /// Optional handle to daemon's AgreementManager (for inter-cooperative agreements)
    agreement_manager_handle: Option<icn_federation::agreement::AgreementManagerHandle>,
    /// Optional supervisor-initialized ServiceDiscoveryManager with gossip wiring.
    /// When provided, the gateway uses this instead of creating its own (gossip-less) instance.
    service_discovery_manager: Option<Arc<crate::service_discovery_mgr::ServiceDiscoveryManager>>,
    /// Audit pruning configuration
    audit_prune_config: Option<AuditPruneConfig>,
    /// Default trust score for unknown peers (overrides DEFAULT_TRUST_SCORE)
    default_trust_score: Option<f64>,
}

impl GatewayServer {
    /// Create a new gateway server (uses temporary storage for testing)
    pub fn new(bind_addr: SocketAddr, jwt_secret: Vec<u8>) -> Self {
        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir: None,
            event_broadcaster: None,
            security_config: SecurityConfig::development(), // Permissive for tests
            rate_limit_config: None,

            compute_handle: None,
            coop_handle: None,
            trust_graph_handle: None,
            trust_service_handle: None,
            governance_handle: None,
            contract_registry_handle: None,
            treasury_handle: None,
            ledger_handle: None,
            entity_handle: None,
            community_handle: None,
            steward_handle: None,
            agreement_manager_handle: None,
            service_discovery_manager: None,
            audit_prune_config: None,
            default_trust_score: None,
        }
    }

    /// Create a new gateway server with persistent storage
    pub fn new_with_storage(
        bind_addr: SocketAddr,
        jwt_secret: Vec<u8>,
        data_dir: std::path::PathBuf,
    ) -> Self {
        // Use development security config if ICN_DEV_MODE or ICN_CORS_ORIGINS is set
        let security_config = if std::env::var("ICN_DEV_MODE").is_ok()
            || std::env::var("ICN_CORS_ORIGINS").is_ok()
        {
            SecurityConfig::development()
        } else {
            SecurityConfig::production()
        };

        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir: Some(data_dir),
            event_broadcaster: None,
            security_config,
            rate_limit_config: None,

            compute_handle: None,
            coop_handle: None,
            trust_graph_handle: None,
            trust_service_handle: None,
            governance_handle: None,
            contract_registry_handle: None,
            treasury_handle: None,
            ledger_handle: None,
            entity_handle: None,
            community_handle: None,
            steward_handle: None,
            agreement_manager_handle: None,
            service_discovery_manager: None,
            audit_prune_config: None,
            default_trust_score: None,
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
        let security_config = if std::env::var("ICN_DEV_MODE").is_ok()
            || std::env::var("ICN_CORS_ORIGINS").is_ok()
        {
            SecurityConfig::development()
        } else {
            SecurityConfig::production()
        };

        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir,
            event_broadcaster: Some(event_broadcaster),
            security_config,
            rate_limit_config: None,

            compute_handle: None,
            coop_handle: None,
            trust_graph_handle: None,
            trust_service_handle: None,
            governance_handle: None,
            contract_registry_handle: None,
            treasury_handle: None,
            ledger_handle: None,
            entity_handle: None,
            community_handle: None,
            steward_handle: None,
            agreement_manager_handle: None,
            service_discovery_manager: None,
            audit_prune_config: None,
            default_trust_score: None,
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

    /// Set trust graph handle for daemon integration (deprecated)
    ///
    /// **Deprecated**: Use `with_trust_service()` instead for proper kernel/app separation.
    /// When set, the TrustManager will delegate all operations to the daemon's
    /// TrustGraph actor, ensuring persistence and gossip synchronization.
    pub fn with_trust_handle(mut self, handle: TrustGraphHandle) -> Self {
        self.trust_graph_handle = Some(handle);
        self
    }

    /// Set trust service for daemon integration (kernel/app separated)
    ///
    /// When set, the TrustManager will use the provided TrustService for trust
    /// score queries, enabling proper kernel/app separation. This is preferred
    /// over `with_trust_handle()`.
    pub fn with_trust_service(
        mut self,
        service: Arc<dyn icn_kernel_api::services::TrustService>,
    ) -> Self {
        self.trust_service_handle = Some(service);
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

    /// Set custom rate limiting configuration
    pub fn with_rate_limit_config(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit_config = Some(config);
        self
    }

    /// Run the gateway server
    pub async fn run(self) -> Result<()> {
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

        // Create shared managers
        let auth_manager = Arc::new(AuthManager::new(self.jwt_secret));

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

        // Create governance manager with Sled-backed action items
        // (uses GovernanceActor if handle available for proposals/votes/domains)
        let governance_manager: Arc<GovernanceManager> =
            if let Some(handle) = self.governance_handle {
                info!("Governance manager connected to daemon with persistent action items");
                Arc::new(GovernanceManager::with_sled_action_items(
                    handle,
                    Arc::new(db.clone()),
                ))
            } else {
                info!("Governance manager running standalone with persistent action items");
                Arc::new(GovernanceManager::new_with_sled(Arc::new(db.clone())))
            };
        let invite_manager = Arc::new(crate::invite::InviteManager::new());
        let session_manager = Arc::new(crate::session::SessionManager::new());

        // Create trust manager (prefers TrustService, falls back to TrustGraph, then in-memory)
        let trust_manager: Arc<TrustManager> = if let Some(service) = self.trust_service_handle {
            info!("Trust manager connected to daemon (using TrustService, kernel/app separated)");
            let mut mgr = TrustManager::with_trust_service(service);
            if let Some(score) = self.default_trust_score {
                info!("Setting custom default trust score: {}", score);
                mgr = mgr.with_default_score(score);
            }
            Arc::new(mgr)
        } else if let Some(handle) = self.trust_graph_handle {
            info!("Trust manager connected to daemon (using TrustGraph actor, deprecated)");
            let mut mgr = TrustManager::with_handle(handle);
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

        let compute_manager = if let Some(handle) = self.compute_handle {
            info!("Compute manager connected to daemon");
            let service = Arc::new(icn_api::ComputeService::new(handle));
            Arc::new(ComputeManager::with_service(service))
        } else {
            info!("Compute manager running standalone (no daemon connection)");
            Arc::new(ComputeManager::new())
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

        // Start background expiry task for service endpoints (every 5 minutes)
        let _service_expiry_handle =
            crate::service_discovery_mgr::start_expiry_task(service_discovery_manager.clone(), 300);

        let federation_manager = Arc::new(FederationManager::new());
        let commons_manager = Arc::new(CommonsManager::new());

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
            Arc::new(mgr)
        } else {
            info!("Treasury manager running standalone (in-memory only)");
            Arc::new(GatewayTreasuryManager::new())
        };

        // Create SDIS state for identity verification
        let sdis_state = Arc::new(crate::api::sdis::SdisState::new());
        // Create enrollment store with persistence to CommonsManager
        let enrollment_store = Arc::new(
            crate::api::sdis::simple_enrollment::EnrollmentStore::with_persistence(
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

        // Create receipt store for economic receipts (AllocationReceipt, SettlementIntent)
        let receipt_store = Arc::new(crate::receipt_store::ReceiptStore::new(db.clone()));
        info!("Receipt store initialized");

        // Create recurring payment store
        let recurring_payment_store =
            crate::api::recurring_payments::RecurringPaymentStore::new(db.clone());
        info!("Recurring payment store initialized");

        // Create escrow store
        let escrow_store = crate::api::escrow::EscrowStore::new(db.clone());
        info!("Escrow store initialized");

        let _recurring_payments_handle = crate::api::recurring_payments::start_scheduler(
            recurring_payment_store.clone(),
            ledger_manager.clone(),
            60, // Check every minute
        );
        info!("Recurring payments scheduler started");

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
                .app_data(web::Data::new(auth_manager.clone()))
                .app_data(web::Data::new(coop_manager.clone()))
                .app_data(web::Data::new(community_manager.clone()))
                .app_data(web::Data::new(steward_manager.clone()))
                .app_data(web::Data::new(governance_manager.clone()))
                .app_data(web::Data::new(invite_manager.clone()))
                .app_data(web::Data::new(session_manager.clone()))
                .app_data(web::Data::new(trust_manager.clone()))
                .app_data(web::Data::new(trust_manager.as_oracle()))
                .app_data(web::Data::new(compute_manager.clone()))
                .app_data(web::Data::new(federation_manager.clone()))
                .app_data(web::Data::new(commons_manager.clone()))
                .app_data(web::Data::new(entity_manager.clone()))
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
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(rate_limiter.clone()))
                .app_data(web::Data::new(ip_rate_limiter.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                // Contract registry (optional - for contract management API)
                .app_data(web::Data::new(contract_registry.clone()))
                // Agreement manager (optional - for inter-cooperative agreements)
                .app_data(web::Data::new(agreement_manager.clone()))
                // Service discovery manager
                .app_data(web::Data::new(service_discovery_manager.clone()))
                // Listings manager for cooperative exchange
                .app_data(web::Data::new(listings_manager.clone()))
                // Decision registry for governance meetings and decisions
                .app_data(web::Data::new(decision_registry.clone()))
                // Economic receipts store (AllocationReceipt, SettlementIntent)
                .app_data(web::Data::new(receipt_store.clone()))
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
                        // Public cooperative statistics (no auth required)
                        .service(api::coops::get_coop_stats)
                        // Public SDIS endpoints (verification + enrollment)
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
                                .configure(api::sdis::anchor::configure),
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
                        // Protected ledger endpoints (auth + rate limiting)
                        .service(
                            web::scope("/ledger")
                                .service(api::ledger::get_balance)
                                .service(api::ledger::create_payment)
                                .service(api::ledger::get_history)
                                .service(api::ledger::get_entries_by_decision)
                                .service(api::ledger::create_cross_payment)
                                .service(api::ledger::get_cross_payment_quote)
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
                        // Protected governance endpoints (auth + rate limiting)
                        // NOTE: Must be BEFORE empty scopes for routing to work
                        .service(
                            web::scope("/gov")
                                .service(api::governance::create_domain)
                                .service(api::governance::list_domains)
                                .service(api::governance::get_domain)
                                .service(api::governance::add_domain_member)
                                .service(api::governance::create_proposal)
                                .service(api::governance::list_proposals)
                                .service(api::governance::get_proposal)
                                .service(api::governance::get_votes)
                                .service(api::governance::get_proposal_proof)
                                .service(api::governance::open_proposal)
                                .service(api::governance::close_proposal)
                                // Federation proposal endpoints
                                .service(api::governance::create_join_federation_proposal)
                                .service(api::governance::create_leave_federation_proposal)
                                .service(api::governance::create_establish_clearing_proposal)
                                .service(api::governance::create_terminate_clearing_proposal)
                                .service(api::governance::create_vouch_proposal)
                                .service(api::governance::create_revoke_vouch_proposal)
                                .service(api::governance::create_update_federation_policy_proposal)
                                // Vote endpoints
                                .service(api::governance::cast_vote)
                                // Delegation endpoints
                                .service(api::governance::create_delegation)
                                .service(api::governance::list_delegations)
                                .service(api::governance::get_delegation)
                                .service(api::governance::revoke_delegation)
                                // Deliberation endpoints
                                .service(api::governance::start_deliberation)
                                .service(api::governance::end_deliberation)
                                // Discussion endpoints
                                .service(api::governance::add_comment)
                                .service(api::governance::list_comments)
                                .service(api::governance::get_discussion)
                                .service(api::governance::edit_comment)
                                .service(api::governance::delete_comment)
                                .service(api::governance::add_reaction)
                                .service(api::governance::remove_reaction)
                                // Apply auth first, then rate limiting
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::trust_rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Recurring payments endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::recurring_payments::configure)
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
                        // Service discovery endpoints (auth + rate limiting)
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
                        ),
                )
                // Static files and root route
                .service(web::redirect("/", "/static/index.html"))
                .service(
                    fs::Files::new("/static", get_static_dir())
                        .prefer_utf8(true)
                        .use_last_modified(true),
                )
        })
        // Production-ready HTTP timeout configuration
        .keep_alive(Duration::from_secs(75)) // HTTP keep-alive timeout (75s is standard)
        .client_request_timeout(Duration::from_secs(30)) // Max time to read request headers
        .client_disconnect_timeout(Duration::from_secs(5)) // Max time waiting for client to read response
        .bind(self.bind_addr)?
        .run();

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
