use axum::{Router, middleware::from_fn_with_state};
use std::net::SocketAddr;
use tracing_subscriber::{FmtSubscriber, EnvFilter};
use dotenvy::dotenv;
use std::error::Error;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod routes;
mod types;
mod storage;
mod federation;
mod auth;
mod state;
mod runtime;
mod docs;
mod health;

use state::AppState;
use docs::ApiDoc;

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();
    
    // Initialize logging with environment-based filters and optional JSON format
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    
    if log_format == "json" {
        // Use JSON format for structured logging (better for automated parsing)
        FmtSubscriber::builder()
            .with_env_filter(EnvFilter::from_default_env())
            .json()
            .with_current_span(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .init();
    } else {
        // Use normal text format (better for human reading)
        FmtSubscriber::builder()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }
        
    if let Err(e) = run().await {
        tracing::error!("Application error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    // Display the database URL (with password redacted)
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let display_url = if db_url.contains('@') {
        // Simple redaction - not foolproof but good enough for logs
        let parts: Vec<&str> = db_url.splitn(2, '@').collect();
        let auth_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
        format!("{}:****@{}", auth_parts[0], parts[1])
    } else {
        "[redacted]".to_string()
    };
    tracing::info!("Using database connection: {}", display_url);

    // Create database connection pool
    tracing::info!("Creating database pool...");
    let pool = storage::create_db_pool().await?;
    
    // Run migrations if enabled
    let run_migrations = std::env::var("RUN_MIGRATIONS")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);
        
    if run_migrations {
        tracing::info!("Running database migrations...");
        sqlx::migrate!().run(&pool).await?;
        tracing::info!("Migrations completed successfully");
    }
    
    tracing::info!("Database connection pool created successfully");
    
    // Initialize federation (if enabled)
    let federation_enabled = std::env::var("ENABLE_FEDERATION")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);
    
    let federation = if federation_enabled {
        tracing::info!("Initializing federation module...");
        let fed = federation::Federation::new(pool.clone()).await?;
        
        // Start the federation
        fed.start().await?;
        tracing::info!("Federation module started");
        
        Some(Arc::new(fed))
    } else {
        tracing::info!("Federation module disabled");
        None
    };
    
    // Initialize Runtime client (if enabled)
    let runtime_enabled = std::env::var("ENABLE_RUNTIME_CLIENT")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);
    
    let mut runtime_client = if runtime_enabled {
        tracing::info!("Initializing Runtime client...");
        let mut client = runtime::RuntimeClient::new(pool.clone(), federation.clone());
        
        // Start the Runtime client
        client.start().await.map_err(|e| {
            tracing::error!("Failed to start Runtime client: {}", e);
            Box::new(e) as Box<dyn Error>
        })?;
        tracing::info!("Runtime client started");
        
        Some(client)
    } else {
        tracing::info!("Runtime client disabled");
        None
    };
    
    // Create application state
    let state = Arc::new(AppState::new(pool, federation.clone()));
    
    tracing::info!("Starting AgoraNet API...");

    // Create the Axum application with routes, passing the app state
    let app = Router::new()
        .merge(routes::threads::routes())
        .merge(routes::credentials::routes())
        .merge(routes::messages::routes())
        .merge(health::routes())
        // Add OpenAPI documentation with Swagger UI
        .merge(SwaggerUi::new("/swagger-ui")
            .url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(state);

    // Bind to the configured address and start the server
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>().unwrap_or(3001);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("API documentation available at http://{}:{}/swagger-ui/", addr.ip(), addr.port());
    tracing::info!("Listening on http://{}", addr);
    
    axum_server::bind(addr)
        .serve(app.into_make_service())
        .await?;
        
    // Shutdown the Runtime client if it was started
    if let Some(ref mut client) = runtime_client {
        tracing::info!("Stopping Runtime client...");
        if let Err(e) = client.stop().await {
            tracing::error!("Error stopping Runtime client: {}", e);
        } else {
            tracing::info!("Runtime client stopped");
        }
    }
    
    // Shutdown the federation if it was started
    if let Some(fed) = federation {
        tracing::info!("Stopping federation module...");
        fed.stop().await?;
        tracing::info!("Federation module stopped");
    }
    
    tracing::info!("Server shutdown gracefully");
    Ok(())
} 