use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::info;
use std::time::Duration;

pub async fn create_db_pool() -> Result<PgPool, sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment variables");
    
    // Get max connections from environment, default to 20 if not set
    let max_connections = std::env::var("DB_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "20".to_string())
        .parse::<u32>()
        .unwrap_or(20);
    
    info!("Connecting to database with {max_connections} max connections...");
    
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .connect(&database_url)
        .await?;
    
    info!("Database connection established");
    
    Ok(pool)
} 