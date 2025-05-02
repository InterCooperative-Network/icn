use axum::{
    extract::{State, Path, Json, Query},
    http::StatusCode,
};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_core::credential::{VerifiableCredential, CredentialSigner};
use wallet_agent::queue::ActionType;
use wallet_agent::agoranet::{ThreadSummary, ThreadDetail, CredentialLink};
use crate::error::{ApiResult, ApiError};
use crate::state::SharedState;
use uuid::Uuid;
use std::collections::HashMap;

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct CreateIdentityRequest {
    pub scope: String,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct IdentityResponse {
    pub id: String,
    pub did: String,
    pub scope: String,
    pub document: Value,
}

#[derive(Debug, Deserialize)]
pub struct SignProposalRequest {
    pub proposal_type: String,
    pub content: Value,
}

#[derive(Debug, Serialize)]
pub struct SignProposalResponse {
    pub action_id: String,
    pub signed: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateCredentialRequest {
    pub subject_data: Value,
    pub credential_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CredentialResponse {
    pub credential: Value,
}

#[derive(Debug, Deserialize)]
pub struct GetThreadsQuery {
    pub proposal_id: Option<String>,
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinkCredentialRequest {
    pub thread_id: String,
    pub credential_id: String,
}

// Handler implementations
pub async fn list_identities(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<IdentityResponse>>> {
    let identities = state.identities.read().await;
    
    let response: Vec<IdentityResponse> = identities
        .iter()
        .map(|(id, wallet)| {
            IdentityResponse {
                id: id.clone(),
                did: wallet.did.to_string(),
                scope: format!("{:?}", wallet.scope),
                document: wallet.to_document(),
            }
        })
        .collect();
    
    Ok(Json(response))
}

pub async fn get_identity(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<IdentityResponse>> {
    let identities = state.identities.read().await;
    
    let wallet = identities.get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("Identity not found: {}", id)))?;
    
    let response = IdentityResponse {
        id,
        did: wallet.did.to_string(),
        scope: format!("{:?}", wallet.scope),
        document: wallet.to_document(),
    };
    
    Ok(Json(response))
}

pub async fn create_identity(
    State(state): State<SharedState>,
    Json(request): Json<CreateIdentityRequest>,
) -> ApiResult<Json<IdentityResponse>> {
    let scope = match request.scope.to_lowercase().as_str() {
        "personal" => IdentityScope::Personal,
        "organization" => IdentityScope::Organization,
        "device" => IdentityScope::Device,
        "service" => IdentityScope::Service,
        _ => IdentityScope::Custom(request.scope.clone()),
    };
    
    let wallet = IdentityWallet::new(scope, request.metadata);
    let id = Uuid::new_v4().to_string();
    
    let response = IdentityResponse {
        id: id.clone(),
        did: wallet.did.to_string(),
        scope: format!("{:?}", wallet.scope),
        document: wallet.to_document(),
    };
    
    let mut identities = state.identities.write().await;
    identities.insert(id, wallet);
    
    Ok(Json(response))
}

pub async fn set_active_identity(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.set_active_identity(&id).await?;
    Ok(StatusCode::OK)
}

pub async fn sign_proposal(
    State(state): State<SharedState>,
    Json(request): Json<SignProposalRequest>,
) -> ApiResult<Json<SignProposalResponse>> {
    let guardian = state.create_guardian().await?;
    
    let action_id = guardian.create_proposal(&request.proposal_type, request.content)?;
    
    let response = SignProposalResponse {
        action_id,
        signed: true,
    };
    
    Ok(Json(response))
}

pub async fn sync_dag(
    State(state): State<SharedState>,
) -> ApiResult<Json<Value>> {
    let client = state.create_sync_client().await?;
    
    let bundles = client.sync_trust_bundles().await
        .map_err(ApiError::SyncError)?;
        
    let response = serde_json::json!({
        "synced_bundles": bundles.len(),
        "status": "success",
    });
    
    Ok(Json(response))
}

pub async fn verify_credential(
    State(state): State<SharedState>,
    Json(credential): Json<Value>,
) -> ApiResult<Json<Value>> {
    let identity = state.get_active_identity().await?;
    
    // In a real implementation, this would create a CredentialSigner and verify
    // the credential cryptographically
    
    let response = serde_json::json!({
        "valid": true,
        "issuer": credential.get("issuer").and_then(|v| v.as_str()).unwrap_or("unknown"),
        "verified_by": identity.did.to_string(),
    });
    
    Ok(Json(response))
}

pub async fn appeal_mandate(
    State(state): State<SharedState>,
    Path(mandate_id): Path<String>,
    Json(reason): Json<String>,
) -> ApiResult<Json<Value>> {
    let guardian = state.create_guardian().await?;
    
    let action_id = guardian.appeal_mandate(&mandate_id, reason)?;
    
    let response = serde_json::json!({
        "action_id": action_id,
        "status": "appeal_queued",
    });
    
    Ok(Json(response))
}

pub async fn list_actions(
    State(state): State<SharedState>,
    Path(action_type): Path<String>,
) -> ApiResult<Json<Value>> {
    let queue = state.create_proposal_queue().await?;
    
    let action_type = match action_type.to_lowercase().as_str() {
        "proposal" => Some(ActionType::Proposal),
        "vote" => Some(ActionType::Vote),
        "appeal" => Some(ActionType::Appeal),
        "credential" => Some(ActionType::Credential),
        "verification" => Some(ActionType::Verification),
        "all" => None,
        _ => return Err(ApiError::InvalidRequest(format!("Unknown action type: {}", action_type))),
    };
    
    let actions = queue.list_actions(action_type)?;
    
    let response = serde_json::json!({
        "actions": actions,
    });
    
    Ok(Json(response))
}

// AgoraNet API handlers
pub async fn get_threads(
    State(state): State<SharedState>,
    Query(query): Query<GetThreadsQuery>,
) -> ApiResult<Json<Vec<ThreadSummary>>> {
    let client = state.create_agoranet_client().await?;
    
    let threads = client.get_threads(
        query.proposal_id.as_deref(),
        query.topic.as_deref()
    ).await?;
    
    Ok(Json(threads))
}

pub async fn get_thread(
    State(state): State<SharedState>,
    Path(thread_id): Path<String>,
) -> ApiResult<Json<ThreadDetail>> {
    let client = state.create_agoranet_client().await?;
    
    let thread = client.get_thread(&thread_id).await?;
    
    Ok(Json(thread))
}

pub async fn get_credential_links(
    State(state): State<SharedState>,
    Path(thread_id): Path<String>,
) -> ApiResult<Json<Vec<CredentialLink>>> {
    let client = state.create_agoranet_client().await?;
    
    let links = client.get_credential_links(&thread_id).await?;
    
    Ok(Json(links))
}

pub async fn link_credential(
    State(state): State<SharedState>,
    Json(request): Json<LinkCredentialRequest>,
) -> ApiResult<Json<CredentialLink>> {
    let client = state.create_agoranet_client().await?;
    let identity = state.get_active_identity().await?;
    
    // Retrieve the credential from storage
    // In a real implementation, this would fetch from a credential store
    // For now, we'll create a dummy credential
    let signer = CredentialSigner::new(identity);
    let credential = signer.issue_credential(
        serde_json::json!({
            "id": request.credential_id,
            "name": "Sample Credential",
            "type": "MembershipCredential"
        }),
        vec!["MembershipCredential".to_string()]
    ).map_err(|e| ApiError::InternalError(format!("Failed to create credential: {}", e)))?;
    
    // Link the credential to the thread
    let link = client.link_credential(&request.thread_id, &credential).await?;
    
    Ok(Json(link))
}

pub async fn notify_proposal_event(
    State(state): State<SharedState>,
    Path(proposal_id): Path<String>,
    Json(details): Json<Value>,
) -> ApiResult<StatusCode> {
    let client = state.create_agoranet_client().await?;
    
    client.notify_proposal_event(&proposal_id, "status_update", details).await?;
    
    Ok(StatusCode::OK)
}

// Trust Bundle handlers
pub async fn list_trust_bundles(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<wallet_agent::governance::TrustBundle>>> {
    let guardian = state.create_guardian().await?;
    
    let bundles = guardian.list_trust_bundles().await?;
    
    Ok(Json(bundles))
}

pub async fn check_guardian_status(
    State(state): State<SharedState>,
) -> ApiResult<Json<Value>> {
    let guardian = state.create_guardian().await?;
    
    let is_guardian = guardian.is_active_guardian().await?;
    
    let response = serde_json::json!({
        "is_active_guardian": is_guardian,
    });
    
    Ok(Json(response))
}

pub async fn create_execution_receipt(
    State(state): State<SharedState>,
    Path(proposal_id): Path<String>,
    Json(result): Json<Value>,
) -> ApiResult<Json<wallet_agent::governance::ExecutionReceipt>> {
    let guardian = state.create_guardian().await?;
    
    let receipt = guardian.create_execution_receipt(&proposal_id, result)?;
    
    // Optionally notify AgoraNet
    let agoranet = state.create_agoranet_client().await?;
    
    // This is intentionally not using ? to avoid failing if AgoraNet notification fails
    let _ = guardian.notify_agoranet(
        &agoranet, 
        &proposal_id, 
        "executed", 
        serde_json::json!({
            "receipt_id": receipt.proposal_id,
            "executor": receipt.executed_by,
        })
    ).await;
    
    Ok(Json(receipt))
}

// Enhanced sync endpoint that loads from disk and syncs from network
pub async fn sync_trust_bundles(
    State(state): State<SharedState>,
) -> ApiResult<Json<Value>> {
    // 1. First load any local bundles from disk
    let guardian = state.create_guardian().await?;
    let local_count = guardian.load_trust_bundles_from_disk().await?;
    
    // 2. Then sync from the network
    let client = state.create_sync_client().await?;
    let network_bundles = client.sync_trust_bundles().await
        .map_err(ApiError::SyncError)?;
    
    // Store the length before we start consuming bundles
    let network_bundles_count = network_bundles.len();
    
    // 3. Store the network bundles
    let mut stored_count = 0;
    for bundle in network_bundles {
        // Store returns an error for invalid bundles, which we'll ignore
        if guardian.store_trust_bundle(bundle).await.is_ok() {
            stored_count += 1;
        }
    }
    
    let response = serde_json::json!({
        "local_bundles_loaded": local_count,
        "network_bundles_synced": network_bundles_count,
        "network_bundles_stored": stored_count,
        "status": "success",
    });
    
    Ok(Json(response))
}

/// Health check endpoint
pub async fn health_check() -> StatusCode {
    StatusCode::OK
} 