use axum::{
    routing::get,
    Router,
    middleware::from_fn as middleware_fn,
};
use tower_http::cors::{CorsLayer, Any};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tokio::net::TcpListener;

mod routes;
mod models;
mod auth;
mod dag;
mod health;
mod state;
mod config;
mod services;
mod utils;
mod federation;

use crate::state::AppState;
use crate::services::ServiceRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "agoranet=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenv::dotenv().ok();

    // Database connection
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    let pool = Arc::new(pool);
    
    // Initialize services
    let dag_service = dag::service::DagService::new(Arc::clone(&pool));
    
    // Create service registry
    let service_registry = ServiceRegistry::new(dag_service);
    
    // Create app state
    let state = Arc::new(AppState {
        db_pool: Arc::clone(&pool),
        services: service_registry,
        federation: None,
    });

    // CORS configuration
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health::check_health))
        .route("/status", get(health::check_status));

    // Protected API routes (auth required)
    let api_routes = Router::new()
        .nest("/threads", routes::threads::routes())
        .nest("/messages", routes::messages::routes())
        .nest("/dag", routes::dag::routes())
        .route("/auth/verify", get(auth::verify_token))
        .layer(middleware_fn(auth::did_auth_middleware));

    // Main app with all routes
    let app = Router::new()
        .merge(public_routes)
        .nest("/api", api_routes)
        .layer(cors)
        .with_state(state.clone());

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let port = port.parse::<u16>()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("AgoraNet running on {}", addr);

    // Create a TcpListener
    let listener = TcpListener::bind(addr).await?;
    
    // Serve the app using the listener
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
} 