//! Gateway server

use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use std::net::SocketAddr;
use std::sync::Arc;
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
}

impl GatewayServer {
    /// Create a new gateway server
    pub fn new(bind_addr: SocketAddr, jwt_secret: Vec<u8>) -> Self {
        GatewayServer { bind_addr, jwt_secret }
    }

    /// Run the gateway server
    pub async fn run(self) -> Result<()> {
        info!("Starting ICN Gateway on {}", self.bind_addr);

        // Create shared managers
        let auth_manager = Arc::new(AuthManager::new(self.jwt_secret));
        let coop_manager = Arc::new(CoopManager::new());
        let ledger_manager = Arc::new(LedgerManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());

        // Create rate limiter with default config
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

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
