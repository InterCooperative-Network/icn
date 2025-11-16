//! Gateway server

use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use crate::api;
use crate::auth::AuthManager;
use crate::coop::CoopManager;
use crate::events::EventBroadcaster;
use crate::ledger_mgr::LedgerManager;
use crate::rate_limit::{RateLimitConfig, RateLimiter};
use crate::error::Result;

/// Gateway server configuration
pub struct GatewayServer {
    bind_addr: SocketAddr,
    jwt_secret: Vec<u8>,
    data_dir: Option<std::path::PathBuf>,
}

impl GatewayServer {
    /// Create a new gateway server (uses temporary storage for testing)
    pub fn new(bind_addr: SocketAddr, jwt_secret: Vec<u8>) -> Self {
        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir: None,
        }
    }

    /// Create a new gateway server with persistent storage
    pub fn new_with_storage(bind_addr: SocketAddr, jwt_secret: Vec<u8>, data_dir: std::path::PathBuf) -> Self {
        GatewayServer {
            bind_addr,
            jwt_secret,
            data_dir: Some(data_dir),
        }
    }

    /// Run the gateway server
    pub async fn run(self) -> Result<()> {
        info!("Starting ICN Gateway on {}", self.bind_addr);

        // Create shared managers
        let auth_manager = Arc::new(AuthManager::new(self.jwt_secret));
        let coop_manager = Arc::new(CoopManager::new());

        // Create ledger manager with persistent storage if data_dir is set
        let ledger_manager = if let Some(data_dir) = self.data_dir {
            Arc::new(LedgerManager::new_with_storage(data_dir))
        } else {
            Arc::new(LedgerManager::new())
        };

        let event_broadcaster = Arc::new(EventBroadcaster::new());

        // Create rate limiter with default config
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

        // Spawn background cleanup task
        {
            let auth_manager_clone = auth_manager.clone();
            let rate_limiter_clone = rate_limiter.clone();
            let event_broadcaster_clone = event_broadcaster.clone();
            let coop_manager_clone = coop_manager.clone();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                loop {
                    interval.tick().await;

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

                    // Clean up dead WebSocket channels for all cooperatives
                    // Get all coop IDs from coop manager
                    if let Ok(coops) = coop_manager_clone.list_all_coop_ids() {
                        for coop_id in coops {
                            event_broadcaster_clone.cleanup(&coop_id).await;
                        }
                    }
                }
            });
        }

        HttpServer::new(move || {
            // Create JWT authentication middleware
            let auth = HttpAuthentication::bearer(crate::middleware::jwt_auth);

            App::new()
                // Shared state
                .app_data(web::Data::new(auth_manager.clone()))
                .app_data(web::Data::new(coop_manager.clone()))
                .app_data(web::Data::new(ledger_manager.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .app_data(web::Data::new(rate_limiter.clone()))
                // Middleware (order: last wrapped runs first, so metrics wraps everything)
                .wrap(crate::middleware::MetricsMiddleware)
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                // API v1 - single scope with public and protected routes
                .service(
                    web::scope("/v1")
                        // Public endpoints (no auth required)
                        .service(api::health::health)
                        .service(api::auth::challenge)
                        .service(api::auth::verify)
                        .service(api::websocket::websocket)
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
                                .wrap(middleware::from_fn(crate::rate_limit::rate_limit_middleware))
                                .wrap(auth.clone())
                        )
                        // Protected ledger endpoints (auth + rate limiting)
                        .service(
                            web::scope("/ledger")
                                .service(api::ledger::get_balance)
                                .service(api::ledger::create_payment)
                                .service(api::ledger::get_history)
                                // Apply auth first, then rate limiting (wrapping order: last runs first)
                                .wrap(middleware::from_fn(crate::rate_limit::rate_limit_middleware))
                                .wrap(auth)
                        )
                )
        })
        .bind(self.bind_addr)?
        .run()
        .await?;

        Ok(())
    }
}
