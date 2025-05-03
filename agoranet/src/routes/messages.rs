use axum::{
    Json, 
    Router, 
    routing::{get, post, delete},
    extract::{Path, State, Query},
    http::StatusCode,
    middleware,
};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use crate::storage::{MessageRepository, ReactionRepository, ThreadRepository};
use crate::types::message::{Message, MessageResponse, CreateMessageRequest};
use crate::types::reaction::{ReactionRequest, ReactionResponse};
use crate::auth::{did_auth_middleware, DidAuth, check_permission, Permission};
use crate::state::AppState;
use std::sync::Arc;

// Query parameters for listing messages
#[derive(Debug, Deserialize)]
pub struct MessageQueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// Define the route handlers
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // Thread message endpoints
        .route("/api/threads/:thread_id/messages", get(list_messages))
        .route("/api/threads/:thread_id/messages", post(create_message))
        .route("/api/threads/:thread_id/messages/:message_id", get(get_message))
        .route("/api/threads/:thread_id/messages/:message_id", delete(delete_message))
        // Reaction endpoints
        .route("/api/messages/:message_id/reactions", get(list_reactions))
        .route("/api/messages/:message_id/reactions", post(add_reaction))
        .route("/api/messages/:message_id/reactions/:reaction_type", delete(remove_reaction))
        // Apply auth middleware to mutation endpoints
        .route_layer(middleware::from_fn_with_state(Arc::clone, did_auth_middleware))
}

// List messages for a thread
async fn list_messages(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(params): Query<MessageQueryParams>,
) -> Result<Json<Vec<MessageResponse>>, StatusCode> {
    let thread_uuid = Uuid::parse_str(&thread_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    // Verify thread exists
    let thread_repo = ThreadRepository::new(state.db_pool.clone());
    if thread_repo.get_thread(thread_uuid).await.is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    let message_repo = MessageRepository::new(state.db_pool.clone());
    let reaction_repo = ReactionRepository::new(state.db_pool.clone());
    
    let limit = params.limit.unwrap_or(50).max(1).min(100);
    let offset = params.offset.unwrap_or(0).max(0);
    
    match message_repo.get_messages_for_thread(thread_uuid, limit, offset).await {
        Ok(messages) => {
            let mut responses = Vec::new();
            
            for message in messages {
                let mut response = MessageResponse::from(message.clone());
                
                // Add reaction counts
                if let Ok(reaction_counts) = reaction_repo.count_reactions_by_type(message.id).await {
                    if !reaction_counts.is_empty() {
                        let reaction_summaries = reaction_counts
                            .into_iter()
                            .map(|(reaction_type, count)| crate::types::message::ReactionCount { 
                                reaction_type, 
                                count 
                            })
                            .collect();
                        
                        response.reactions = Some(reaction_summaries);
                    }
                }
                
                responses.push(response);
            }
            
            Ok(Json(responses))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Get a specific message
async fn get_message(
    State(state): State<Arc<AppState>>,
    Path((thread_id, message_id)): Path<(String, String)>,
) -> Result<Json<MessageResponse>, StatusCode> {
    let message_uuid = Uuid::parse_str(&message_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let message_repo = MessageRepository::new(state.db_pool.clone());
    let reaction_repo = ReactionRepository::new(state.db_pool.clone());
    
    match message_repo.get_message(message_uuid).await {
        Ok(message) => {
            // Verify message belongs to the specified thread
            let thread_uuid = Uuid::parse_str(&thread_id)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
                
            if message.thread_id != thread_uuid {
                return Err(StatusCode::NOT_FOUND);
            }
            
            let mut response = MessageResponse::from(message);
            
            // Add reaction counts
            if let Ok(reaction_counts) = reaction_repo.count_reactions_by_type(message_uuid).await {
                if !reaction_counts.is_empty() {
                    let reaction_summaries = reaction_counts
                        .into_iter()
                        .map(|(reaction_type, count)| crate::types::message::ReactionCount { 
                            reaction_type, 
                            count 
                        })
                        .collect();
                    
                    response.reactions = Some(reaction_summaries);
                }
            }
            
            Ok(Json(response))
        },
        Err(crate::storage::StorageError::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Create a new message in a thread
async fn create_message(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    auth: DidAuth,
    Json(create_req): Json<CreateMessageRequest>,
) -> Result<Json<MessageResponse>, StatusCode> {
    // Check permission
    if !check_permission(
        &auth.0,
        Permission::PostMessage,
        None,
        &state
    ).await.unwrap_or(false) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let thread_uuid = Uuid::parse_str(&thread_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    // Verify thread exists
    let thread_repo = ThreadRepository::new(state.db_pool.clone());
    if thread_repo.get_thread(thread_uuid).await.is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    let message_repo = MessageRepository::new(state.db_pool.clone());
    
    // Handle reply_to if present
    let reply_to = if let Some(reply_id) = &create_req.reply_to {
        match Uuid::parse_str(reply_id) {
            Ok(uuid) => {
                // Verify the referenced message exists and is in this thread
                match message_repo.get_message(uuid).await {
                    Ok(msg) if msg.thread_id == thread_uuid => Some(uuid),
                    _ => return Err(StatusCode::BAD_REQUEST), // Reply message not found or not in thread
                }
            },
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        }
    } else {
        None
    };
    
    match message_repo.create_message(
        thread_uuid, 
        &auth.0, 
        &create_req.content, 
        reply_to
    ).await {
        Ok(message) => Ok(Json(MessageResponse::from(message))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Delete a message
async fn delete_message(
    State(state): State<Arc<AppState>>,
    Path((thread_id, message_id)): Path<(String, String)>,
    auth: DidAuth,
) -> Result<StatusCode, StatusCode> {
    let message_uuid = Uuid::parse_str(&message_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let message_repo = MessageRepository::new(state.db_pool.clone());
    
    // Get the message first to verify ownership
    match message_repo.get_message(message_uuid).await {
        Ok(message) => {
            // Verify message belongs to the specified thread
            let thread_uuid = Uuid::parse_str(&thread_id)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
                
            if message.thread_id != thread_uuid {
                return Err(StatusCode::NOT_FOUND);
            }
            
            // Check if user is the author or has moderation permission
            let is_author = message.author_did.as_deref() == Some(&auth.0);
            let can_moderate = check_permission(
                &auth.0,
                Permission::ModerateContent,
                None,
                &state
            ).await.unwrap_or(false);
            
            if !is_author && !can_moderate {
                return Err(StatusCode::FORBIDDEN);
            }
            
            // Delete the message
            match message_repo.delete_message(message_uuid).await {
                Ok(_) => Ok(StatusCode::NO_CONTENT),
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        },
        Err(crate::storage::StorageError::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// List reactions for a message
async fn list_reactions(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<Vec<ReactionResponse>>, StatusCode> {
    let message_uuid = Uuid::parse_str(&message_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let reaction_repo = ReactionRepository::new(state.db_pool.clone());
    
    match reaction_repo.get_reactions_for_message(message_uuid).await {
        Ok(reactions) => {
            let responses = reactions.into_iter()
                .map(|reaction| ReactionResponse::from(reaction))
                .collect();
            
            Ok(Json(responses))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Add a reaction to a message
async fn add_reaction(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    auth: DidAuth,
    Json(req): Json<ReactionRequest>,
) -> Result<Json<ReactionResponse>, StatusCode> {
    // Check permission
    if !check_permission(
        &auth.0,
        Permission::ReactToMessage,
        None,
        &state
    ).await.unwrap_or(false) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let message_uuid = Uuid::parse_str(&message_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    // Make sure the message exists
    let message_repo = MessageRepository::new(state.db_pool.clone());
    if message_repo.get_message(message_uuid).await.is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    let reaction_repo = ReactionRepository::new(state.db_pool.clone());
    
    // Prevent duplicates
    if reaction_repo.has_user_reacted(message_uuid, &auth.0, &req.reaction_type).await.unwrap_or(false) {
        return Err(StatusCode::CONFLICT);
    }
    
    match reaction_repo.add_reaction(message_uuid, &auth.0, &req.reaction_type).await {
        Ok(reaction) => Ok(Json(ReactionResponse::from(reaction))),
        Err(crate::storage::StorageError::Other(msg)) if msg.contains("already exists") => {
            Err(StatusCode::CONFLICT)
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Remove a reaction from a message
async fn remove_reaction(
    State(state): State<Arc<AppState>>,
    Path((message_id, reaction_type)): Path<(String, String)>,
    auth: DidAuth,
) -> Result<StatusCode, StatusCode> {
    let message_uuid = Uuid::parse_str(&message_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let reaction_repo = ReactionRepository::new(state.db_pool.clone());
    
    match reaction_repo.remove_reaction(message_uuid, &auth.0, &reaction_type).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(crate::storage::StorageError::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
} 