//! Gateway server

use actix_files as fs;
use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use actix_web_prom::PrometheusMetricsBuilder;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::api;
use crate::auth::AuthManager;
use crate::commons_mgr::CommonsManager;
use crate::compute_mgr::ComputeManager;
use crate::coop::CoopManager;
use crate::error::Result;
use crate::events::EventBroadcaster;
use crate::federation_mgr::FederationManager;
use crate::governance_mgr::GovernanceManager;
use crate::identity_mgr::IdentityManager;
use crate::ledger_mgr::LedgerManager;
use crate::notification_processor::{NotificationProcessor, ProcessorConfig};
use crate::notification_queue::NotificationQueue;
use crate::notification_triggers::{GovernanceNotificationTrigger, LedgerNotificationTrigger};
use crate::notifications::NotificationService;
use crate::rate_limit::{IpRateLimiter, RateLimitConfig, RateLimiter};
use crate::security::{configure_cors, SecurityConfig, SecurityHeaders};
use crate::trust_mgr::TrustManager;
use icn_compute::ComputeHandle;

/// Gateway server configuration
pub struct GatewayServer {
    bind_addr: SocketAddr,
    jwt_secret: Vec<u8>,
    data_dir: Option<std::path::PathBuf>,
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    security_config: SecurityConfig,
    rate_limit_config: Option<RateLimitConfig>,
    compute_handle: Option<ComputeHandle>,
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
        }
    }

    /// Create a new gateway server with persistent storage
    pub fn new_with_storage(
        bind_addr: SocketAddr,
        jwt_secret: Vec<u8>,
        data_dir: std::path::PathBuf,
    ) -> Self {
        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir: Some(data_dir),
            event_broadcaster: None,
            security_config: SecurityConfig::production(), // Strict for production
            rate_limit_config: None,
            compute_handle: None,
        }
    }

    /// Create a new gateway server with shared event broadcaster (for production integration)
    pub fn new_with_broadcaster(
        bind_addr: SocketAddr,
        jwt_secret: Vec<u8>,
        data_dir: Option<std::path::PathBuf>,
        event_broadcaster: Arc<EventBroadcaster>,
    ) -> Self {
        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir,
            event_broadcaster: Some(event_broadcaster),
            security_config: SecurityConfig::production(),
            rate_limit_config: None,
            compute_handle: None,
        }
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

    /// Set custom rate limiting configuration
    pub fn with_rate_limit_config(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit_config = Some(config);
        self
    }

    /// Run the gateway server
    pub async fn run(self) -> Result<()> {
        info!("Starting ICN Gateway on {}", self.bind_addr);

        // Create shared managers
        let auth_manager = Arc::new(AuthManager::new(self.jwt_secret));
        let coop_manager = Arc::new(CoopManager::new());
        let governance_manager = Arc::new(GovernanceManager::new());
        let invite_manager = Arc::new(crate::invite::InviteManager::new());
        let trust_manager = Arc::new(TrustManager::new());
        let compute_manager = if let Some(handle) = self.compute_handle {
            info!("Compute manager connected to daemon");
            Arc::new(ComputeManager::with_handle(handle))
        } else {
            info!("Compute manager running standalone (no daemon connection)");
            Arc::new(ComputeManager::new())
        };
        let federation_manager = Arc::new(FederationManager::new());
        let commons_manager = Arc::new(CommonsManager::new());

        // Create SDIS state for identity verification
        let sdis_state = Arc::new(crate::api::sdis::SdisState::new());
        // Create enrollment store with persistence to CommonsManager
        let enrollment_store = Arc::new(
            crate::api::sdis::simple_enrollment::EnrollmentStore::with_persistence(
                commons_manager.clone(),
            ),
        );

        // Initialize Sled DB for persistent stores (moved up for dependency injection)
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

        // Create budget store
        let budget_store = crate::api::budgets::BudgetStore::new(db.clone());
        let budget_store = Arc::new(budget_store);
        info!("Budget store initialized");

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
            Arc::new(IdentityManager::new_with_storage(data_dir.clone()))
        } else {
            Arc::new(IdentityManager::new())
        };

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

        // Create IP-based rate limiter for auth endpoints (more aggressive limits)
        let ip_rate_limiter = Arc::new(IpRateLimiter::new_for_auth());

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

        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

        // Spawn background cleanup task with graceful shutdown
        {
            let auth_manager_clone = auth_manager.clone();
            let rate_limiter_clone = rate_limiter.clone();
            let ip_rate_limiter_clone = ip_rate_limiter.clone();
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
                            let removed = rate_limiter_clone.cleanup_inactive_buckets(Duration::from_secs(3600));
                            if removed > 0 {
                                info!("Cleaned up {} inactive rate limiter buckets", removed);
                            }

                            // Clean up inactive IP rate limiter buckets (10 minute inactivity for auth endpoints)
                            let removed = ip_rate_limiter_clone.cleanup_inactive_buckets(Duration::from_secs(600));
                            if removed > 0 {
                                info!("Cleaned up {} inactive IP rate limiter buckets", removed);
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

        // Clone security config for the move closure
        let security_config = self.security_config.clone();

        // Initialize Prometheus metrics
        let prometheus = PrometheusMetricsBuilder::new("api")
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
                .app_data(web::Data::new(governance_manager.clone()))
                .app_data(web::Data::new(invite_manager.clone()))
                .app_data(web::Data::new(trust_manager.clone()))
                .app_data(web::Data::new(compute_manager.clone()))
                .app_data(web::Data::new(federation_manager.clone()))
                .app_data(web::Data::new(commons_manager.clone()))
                .app_data(web::Data::new(ledger_manager.clone()))
                .app_data(web::Data::new(identity_manager.clone()))
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
                // JSON payload size limit (256KB - we're not handling file uploads)
                .app_data(web::JsonConfig::default().limit(262_144))
                // Middleware (order: last wrapped runs first for REQUEST, first runs last for RESPONSE)
                // For CORS header removal when behind reverse proxy, SecurityHeaders must run AFTER cors
                .wrap(
                    PrometheusMetricsBuilder::new("icn_gateway")
                        .endpoint("/metrics")
                        .build()
                        .unwrap(),
                )
                .wrap(crate::middleware::MetricsMiddleware)
                .wrap(prometheus.clone())
                .wrap(cors)
                .wrap(SecurityHeaders::new(security_config.clone()))
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                // API v1 - single scope with public and protected routes
                .service(
                    web::scope("/v1")
                        // Public endpoints (no auth required)
                        .service(api::health::liveness)
                        .service(api::health::readiness)
                        .service(api::health::health)
                        .service(api::health::health_detailed)
                        .service(api::auth::challenge)
                        .service(api::auth::verify)
                        .service(api::websocket::websocket)
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
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected ledger endpoints (auth + rate limiting)
                        .service(
                            web::scope("/ledger")
                                .service(api::ledger::get_balance)
                                .service(api::ledger::create_payment)
                                .service(api::ledger::get_history)
                                // Apply auth first, then rate limiting (wrapping order: last runs first)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Recurring payments endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::recurring_payments::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Escrow endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::escrow::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Budget endpoints (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::budgets::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected governance endpoints (auth + rate limiting)
                        // NOTE: Requires GovernanceHandle from daemon supervisor
                        //       See governance integration in icnd for wiring
                        .service(
                            web::scope("/gov")
                                .service(api::governance::create_domain)
                                .service(api::governance::list_domains)
                                .service(api::governance::get_domain)
                                .service(api::governance::create_proposal)
                                .service(api::governance::list_proposals)
                                .service(api::governance::get_proposal)
                                .service(api::governance::get_votes)
                                .service(api::governance::open_proposal)
                                .service(api::governance::close_proposal)
                                .service(api::governance::cast_vote)
                                // Apply auth first, then rate limiting (wrapping order: last runs first)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
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
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Protected compute endpoints (auth + rate limiting)
                        .service(
                            web::scope("/compute")
                                .service(api::compute::submit_task)
                                .service(api::compute::get_status)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
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
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
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
                                    crate::rate_limit::rate_limit_middleware,
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
                                    crate::rate_limit::rate_limit_middleware,
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
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Charter management endpoints (auth + rate limiting)
                        .service(
                            web::scope("/charter")
                                .configure(api::charter::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Steward management endpoints (auth + rate limiting)
                        .service(
                            web::scope("/steward")
                                .configure(api::steward::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Membership management endpoints (auth + rate limiting)
                        .service(
                            web::scope("/membership")
                                .configure(api::membership::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Constitutional governance endpoints (auth + rate limiting)
                        .service(
                            web::scope("/constitutional")
                                .configure(api::constitutional::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
                                ))
                                .wrap(auth.clone()),
                        )
                        // Governance dashboard (auth + rate limiting)
                        .service(
                            web::scope("")
                                .configure(api::governance_dashboard::configure)
                                .wrap(middleware::from_fn(
                                    crate::rate_limit::rate_limit_middleware,
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
