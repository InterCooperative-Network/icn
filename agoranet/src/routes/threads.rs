use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};

// Thread model
#[derive(Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub proposal_cid: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Request models
#[derive(Debug, Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    pub proposal_cid: Option<String>,
}

// Response models
#[derive(Debug, Serialize)]
pub struct ThreadResponse {
    pub id: String,
    pub title: String,
    pub proposal_cid: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Setup routes
pub fn routes() -> Router<Arc<PgPool>> {
    Router::new()
        .route("/api/threads", get(list_threads))
        .route("/api/threads", post(create_thread))
        .route("/api/threads/:id", get(get_thread))
}

// Route handlers
async fn list_threads(
    State(pool): State<Arc<PgPool>>,
) -> Result<Json<Vec<ThreadResponse>>, StatusCode> {
    let threads = sqlx::query_as!(
        Thread,
        "SELECT id, title, proposal_cid, created_at, updated_at FROM threads ORDER BY created_at DESC"
    )
    .fetch_all(pool.as_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let response = threads
        .into_iter()
        .map(|thread| ThreadResponse {
            id: thread.id,
            title: thread.title,
            proposal_cid: thread.proposal_cid,
            created_at: thread.created_at,
            updated_at: thread.updated_at,
        })
        .collect();

    Ok(Json(response))
}

async fn create_thread(
    State(pool): State<Arc<PgPool>>,
    Json(request): Json<CreateThreadRequest>,
) -> Result<Json<ThreadResponse>, StatusCode> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let proposal_cid = request.proposal_cid.unwrap_or_else(|| "".to_string());

    let thread = sqlx::query_as!(
        Thread,
        "INSERT INTO threads (id, title, proposal_cid, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5) 
         RETURNING id, title, proposal_cid, created_at, updated_at",
        id,
        request.title,
        proposal_cid,
        now,
        now
    )
    .fetch_one(pool.as_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ThreadResponse {
        id: thread.id,
        title: thread.title,
        proposal_cid: thread.proposal_cid,
        created_at: thread.created_at,
        updated_at: thread.updated_at,
    }))
}

async fn get_thread(
    State(pool): State<Arc<PgPool>>,
    Path(id): Path<String>,
) -> Result<Json<ThreadResponse>, StatusCode> {
    let thread = sqlx::query_as!(
        Thread,
        "SELECT id, title, proposal_cid, created_at, updated_at FROM threads WHERE id = $1",
        id
    )
    .fetch_optional(pool.as_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ThreadResponse {
        id: thread.id,
        title: thread.title,
        proposal_cid: thread.proposal_cid,
        created_at: thread.created_at,
        updated_at: thread.updated_at,
    }))
} 