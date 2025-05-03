use axum::{
    routing::get,
    Router,
    middleware::from_fn as middleware_fn,
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;

mod routes;
mod auth;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    let pool = Arc::new(pool);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(|| async { "ok" }));

    // Protected API routes (auth required)
    let api_routes = Router::new()
        .nest("/threads", routes::threads::routes())
        .route("/auth/verify", get(auth::verify_token))
        .layer(middleware_fn(auth::auth_middleware));

    // Main app with all routes
    let app = Router::new()
        .merge(public_routes)
        .nest("/api", api_routes)
        .with_state(pool);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let port = port.parse::<u16>()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("AgoraNet running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
} 