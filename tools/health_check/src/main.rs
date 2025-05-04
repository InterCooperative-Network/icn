use std::net::SocketAddr;
use std::str::FromStr;
use axum::{
    http::StatusCode,
    routing::get,
    response::Json,
    Router
};
use sqlx::postgres::PgPoolOptions;
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    database: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Set up database connection
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    println!("Connecting to database at: {}", database_url);
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Test database connection
    let result = sqlx::query("SELECT 1").execute(&pool).await;
    match result {
        Ok(_) => println!("Database connection successful!"),
        Err(e) => {
            eprintln!("Database connection error: {}", e);
            return Err(Box::new(e));
        }
    }

    // Get list of tables in the database
    let tables = sqlx::query!(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'"
    )
    .fetch_all(&pool)
    .await?;

    println!("Tables in the database:");
    for table in &tables {
        println!("- {}", table.table_name);
    }

    let app = Router::new()
        .route("/health", get(health_handler))
        .with_state(pool);

    // Get port from environment or use default
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let addr = SocketAddr::from_str(&format!("0.0.0.0:{}", port))?;
    
    println!("Server listening on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn health_handler(
    axum::extract::State(pool): axum::extract::State<sqlx::PgPool>,
) -> Result<Json<HealthResponse>, StatusCode> {
    match sqlx::query("SELECT 1").execute(&pool).await {
        Ok(_) => {
            // Get the number of tables in the database
            let tables_count = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'"
            )
            .fetch_one(&pool)
            .await
            .unwrap_or(Some(0))
            .unwrap_or(0);
            
            let threads_info = format!("Connected, {} tables in public schema", tables_count);
            
            Ok(Json(HealthResponse {
                status: "ok".to_string(),
                database: threads_info,
            }))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
} 