//! Gateway server

use actix_web::{middleware, web, App, HttpServer};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

use crate::api;
use crate::auth::AuthManager;
use crate::coop::CoopManager;
use crate::ledger_mgr::LedgerManager;
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

        HttpServer::new(move || {
            App::new()
                // Shared state
                .app_data(web::Data::new(auth_manager.clone()))
                .app_data(web::Data::new(coop_manager.clone()))
                .app_data(web::Data::new(ledger_manager.clone()))
                // Middleware
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                // Health endpoint
                .service(api::health::health)
                // Auth endpoints
                .service(api::auth::challenge)
                .service(api::auth::verify)
                // Coop endpoints
                .service(api::coops::create_coop)
                .service(api::coops::get_coop)
                .service(api::coops::update_settings)
                .service(api::coops::delete_coop)
                .service(api::coops::add_member)
                .service(api::coops::remove_member)
                .service(api::coops::update_member_role)
                // Ledger endpoints
                .service(api::ledger::get_balance)
                .service(api::ledger::create_payment)
                .service(api::ledger::get_history)
        })
        .bind(self.bind_addr)?
        .run()
        .await?;

        Ok(())
    }
}
