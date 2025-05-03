use axum::{
    Json, 
    Router, 
    routing::{get, post},
    extract::{State, Path},
    http::StatusCode,
    middleware,
};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use std::sync::Arc;
use crate::storage::ThreadRepository;
use crate::types::thread::{Thread, ThreadResponse};
use crate::auth::{did_auth_middleware, DidAuth, check_permission, Permission};
use crate::state::AppState;
use uuid::Uuid;

// Thread creation request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateThreadRequest {
    pub title: String,
    pub proposal_cid: Option<String>,
}

// Thread query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct ThreadQueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub search: Option<String>,
    pub order_by: Option<String>,
}

// Define the route handlers
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/threads", get(list_threads))
        .route("/api/threads/:id", get(get_thread))
        .route("/api/threads", post(create_thread))
        .layer(middleware::from_fn_with_state(Arc::clone, did_auth_middleware))
}

/// List all deliberation threads
/// 
/// Returns a list of threads with pagination support.
#[utoipa::path(
    get,
    path = "/api/threads",
    tag = "Threads",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of threads to return (default: 50, max: 100)"),
        ("offset" = Option<i64>, Query, description = "Number of threads to skip (default: 0)"),
        ("search" = Option<String>, Query, description = "Optional search term to filter threads by title"),
        ("order_by" = Option<String>, Query, description = "Sort order, one of: created_at_desc, created_at_asc, updated_at_desc, updated_at_asc")
    ),
    responses(
        (status = 200, description = "List of threads successfully retrieved", body = Vec<ThreadResponse>),
        (status = 401, description = "Unauthorized, authentication token missing or invalid"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn list_threads(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ThreadQueryParams>,
) -> Result<Json<Vec<ThreadResponse>>, StatusCode> {
    let thread_repo = ThreadRepository::new(state.db_pool.clone());
    
    let limit = params.limit.unwrap_or(50).max(1).min(100);
    let offset = params.offset.unwrap_or(0).max(0);
    
    let threads = if let Some(search_term) = params.search {
        // If search parameter is provided, use search function
        thread_repo.search_threads(&search_term, limit, offset).await
    } else {
        // Otherwise use paginated list with optional sorting
        let order_by = params.order_by.as_deref().unwrap_or("created_at_desc");
        thread_repo.list_threads_paginated(limit, offset, order_by).await
    };
    
    match threads {
        Ok(threads) => {
            let responses: Vec<ThreadResponse> = threads.into_iter()
                .map(|thread| thread.into())
                .collect();
            Ok(Json(responses))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get a specific thread by ID
/// 
/// Returns detailed information about a single thread.
#[utoipa::path(
    get,
    path = "/api/threads/{id}",
    tag = "Threads",
    params(
        ("id" = String, Path, description = "Thread UUID")
    ),
    responses(
        (status = 200, description = "Thread found", body = ThreadResponse),
        (status = 400, description = "Invalid UUID format"),
        (status = 401, description = "Unauthorized, authentication token missing or invalid"),
        (status = 404, description = "Thread not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn get_thread(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ThreadResponse>, StatusCode> {
    let thread_repo = ThreadRepository::new(state.db_pool.clone());
    
    let id = Uuid::parse_str(&id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    match thread_repo.get_thread(id).await {
        Ok(thread) => Ok(Json(thread.into())),
        Err(crate::storage::StorageError::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Create a new deliberation thread
/// 
/// Creates a new thread and returns its details. Requires authentication.
#[utoipa::path(
    post,
    path = "/api/threads",
    tag = "Threads",
    request_body = CreateThreadRequest,
    responses(
        (status = 201, description = "Thread created successfully", body = ThreadResponse),
        (status = 400, description = "Invalid request body"),
        (status = 401, description = "Unauthorized, authentication token missing or invalid"),
        (status = 403, description = "Forbidden, user lacks permission to create threads"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn create_thread(
    State(state): State<Arc<AppState>>,
    auth: DidAuth,
    Json(create_req): Json<CreateThreadRequest>,
) -> Result<Json<ThreadResponse>, StatusCode> {
    // Check permission
    if !check_permission(
        &auth.0,
        Permission::CreateThread,
        None,
        &state
    ).await.unwrap_or(false) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let thread_repo = ThreadRepository::new(state.db_pool.clone());
    
    let proposal_cid_ref = create_req.proposal_cid.as_deref();
    
    match thread_repo.create_thread(&create_req.title, proposal_cid_ref).await {
        Ok(thread) => {
            // Announce the new thread to the federation if enabled
            if let Some(federation) = state.federation() {
                if let Err(e) = federation.announce_thread(&thread.id.to_string()).await {
                    tracing::warn!("Failed to announce thread: {}", e);
                    // Don't return an error, as the thread was created successfully
                }
            }
            
            Ok(Json(thread.into()))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
} 