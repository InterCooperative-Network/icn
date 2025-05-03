pub mod api;
pub mod state;
pub mod error;
pub mod handlers;

use std::net::SocketAddr;
use std::sync::Arc;
use std::path::Path;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use wallet_core::store::{LocalWalletStore, FileStore, SecurePlatform, create_mock_secure_store};
use state::{AppState, AppConfig};

/// Start the wallet API server with a custom store
pub async fn start_server<S: LocalWalletStore + 'static>(
    store: S,
    config: AppConfig,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create the application state
    let state = Arc::new(AppState::new(store, config));
    
    // Create the API router
    let api_router = api::create_api_router(state.clone());
    
    // Add CORS middleware
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    // Build the application
    let app = Router::new()
        .nest("/api", api_router)
        .layer(cors);
    
    // Start the server
    let listener = TcpListener::bind(addr).await?;
    println!("🚀 Wallet API server listening on http://{}", addr);
    axum::serve(listener, app).await?;
    
    Ok(())
}

/// Start the wallet API server with a file-based store
pub async fn start_file_server(
    data_dir: &str,
    federation_url: &str,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create the file store
    let store = FileStore::new(data_dir);
    
    // Initialize the store
    store.init().await
        .map_err(|e| format!("Failed to initialize store: {}", e))?;
    
    // Create the config
    let config = AppConfig {
        federation_url: federation_url.to_string(),
        data_dir: data_dir.to_string(),
        auto_sync: true,
        sync_interval: 60,
    };
    
    // Start the server
    start_server(store, config, addr).await
}

/// Start the wallet API server with a secure store
pub async fn start_secure_server(
    data_dir: &str,
    federation_url: &str,
    platform: SecurePlatform,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create the secure store
    let store = create_mock_secure_store(platform, data_dir);
    
    // Initialize the store
    store.init().await
        .map_err(|e| format!("Failed to initialize store: {}", e))?;
    
    // Create the config
    let config = AppConfig {
        federation_url: federation_url.to_string(),
        data_dir: data_dir.to_string(),
        auto_sync: true,
        sync_interval: 60,
    };
    
    // Start the server
    start_server(store, config, addr).await
}
