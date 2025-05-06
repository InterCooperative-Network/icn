use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::State,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone)]
struct AppState {
    version: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

#[derive(Deserialize)]
struct Message {
    thread_id: String,
    content: String,
}

#[derive(Serialize)]
struct MessageResponse {
    id: String,
    status: String,
}

#[tokio::main]
async fn main() {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");
    
    info!("Starting AgoraNet API");
    
    // App state
    let state = Arc::new(AppState {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });
    
    // Create router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/messages", post(post_message))
        .with_state(state);
    
    // Start server
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("AgoraNet API listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: state.version.clone(),
    })
}

async fn post_message(
    State(_state): State<Arc<AppState>>,
    Json(message): Json<Message>,
) -> Json<MessageResponse> {
    // This is a placeholder implementation
    // In a real implementation, this would store the message in a database
    info!("Received message for thread {}: {}", message.thread_id, message.content);
    
    Json(MessageResponse {
        id: uuid::Uuid::new_v4().to_string(),
        status: "accepted".to_string(),
    })
}
