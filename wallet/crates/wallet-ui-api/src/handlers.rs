use axum::{
    extract::{State, Path, Json, Query},
    http::StatusCode,
};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use uuid::Uuid;
use std::collections::HashMap;
use std::sync::Arc;

use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_core::credential::CredentialSigner;
use wallet_core::vc::VerifiableCredential;
use wallet_core::store::LocalWalletStore;
use wallet_agent::queue::{ActionType, ActionQueue, PendingAction};
use wallet_agent::ActionProcessor;
use wallet_agent::agoranet::{ThreadSummary, ThreadDetail, CredentialLink};
use wallet_sync::SyncManager;

use crate::error::{ApiResult, ApiError};
use crate::state::AppState;

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

#[derive(Debug, Serialize)]
pub struct DagNodeResponse {
    pub cid: String,
    pub content_type: String,
    pub creator: String,
    pub timestamp: String,
    pub content: Value,
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub id: String,
    pub action_type: String,
    pub creator_did: String,
    pub status: String,
    pub created_at: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BundleResponse {
    pub id: String,
    pub version: u64,
    pub epoch: u64,
    pub guardian_count: usize,
    pub created_at: String,
    pub valid_until: String,
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

// Health check endpoint
pub async fn health_check() -> StatusCode {
    StatusCode::OK
}

// Handler implementations for identities
pub async fn list_identities<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
) -> ApiResult<Json<Vec<IdentityResponse>>> {
    let dids = state.store.list_identities().await
        .map_err(|e| ApiError::StoreError(format!("Failed to list identities: {}", e)))?;
    
    let mut responses = Vec::new();
    
    for did in dids {
        let identity = state.store.load_identity(&did).await
            .map_err(|e| ApiError::StoreError(format!("Failed to load identity {}: {}", did, e)))?;
            
        responses.push(IdentityResponse {
            id: did.clone(),
            did: identity.did.to_string(),
            scope: format!("{:?}", identity.scope),
            document: identity.to_document(),
        });
    }
    
    Ok(Json(responses))
}

pub async fn get_identity<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(did): Path<String>,
) -> ApiResult<Json<IdentityResponse>> {
    let identity = state.store.load_identity(&did).await
        .map_err(|e| match e {
            wallet_core::WalletError::NotFound(_) => ApiError::NotFound(format!("Identity not found: {}", did)),
            _ => ApiError::StoreError(format!("Failed to load identity: {}", e)),
        })?;
    
    let response = IdentityResponse {
        id: did,
        did: identity.did.to_string(),
        scope: format!("{:?}", identity.scope),
        document: identity.to_document(),
    };
    
    Ok(Json(response))
}

pub async fn create_identity<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Json(request): Json<CreateIdentityRequest>,
) -> ApiResult<Json<IdentityResponse>> {
    let scope = match request.scope.to_lowercase().as_str() {
        "personal" => IdentityScope::Personal,
        "organization" => IdentityScope::Organization,
        "device" => IdentityScope::Device,
        "service" => IdentityScope::Service,
        _ => IdentityScope::Custom(request.scope.clone()),
    };
    
    // Create a new identity wallet with its own keypair
    let identity = IdentityWallet::new(scope, request.metadata);
    let did = identity.did.to_string();
    
    // Save the identity to the secure store
    state.store.save_identity(&identity).await
        .map_err(|e| ApiError::StoreError(format!("Failed to save identity: {}", e)))?;
    
    // Save the keypair securely too (with the same ID for simplicity)
    state.store.store_keypair(&did, &identity.keypair).await
        .map_err(|e| ApiError::StoreError(format!("Failed to save keypair: {}", e)))?;
    
    let response = IdentityResponse {
        id: did.clone(),
        did,
        scope: format!("{:?}", identity.scope),
        document: identity.to_document(),
    };
    
    Ok(Json(response))
}

// Handler implementations for credential operations
pub async fn create_credential<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(issuer_did): Path<String>,
    Json(request): Json<CreateCredentialRequest>,
) -> ApiResult<Json<CredentialResponse>> {
    // 1. Load the issuer identity
    let issuer = state.store.load_identity(&issuer_did).await
        .map_err(|e| ApiError::NotFound(format!("Issuer identity not found: {}", e)))?;
    
    // 2. Create a credential signer from the issuer
    let signer = CredentialSigner::new(issuer);
    
    // 3. Issue the credential
    let credential = signer.issue_credential(request.subject_data, request.credential_types)
        .map_err(|e| ApiError::CoreError(e))?;
    
    // 4. Generate a unique ID for the credential
    let id = Uuid::new_v4().to_string();
    
    // 5. Store the credential
    state.store.save_credential(&credential, &id).await
        .map_err(|e| ApiError::StoreError(format!("Failed to save credential: {}", e)))?;
    
    // 6. Convert the credential to JSON for the response
    let credential_json = serde_json::to_value(&credential)
        .map_err(|e| ApiError::SerializationError(format!("Failed to serialize credential: {}", e)))?;
    
    let response = CredentialResponse {
        credential: credential_json,
    };
    
    Ok(Json(response))
}

pub async fn verify_credential<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Json(credential_value): Json<Value>,
) -> ApiResult<Json<Value>> {
    // 1. Deserialize the credential
    let credential: VerifiableCredential = serde_json::from_value(credential_value)
        .map_err(|e| ApiError::InvalidRequest(format!("Invalid credential format: {}", e)))?;
    
    // 2. Get the issuer DID from the credential
    let issuer_did = credential.issuer.clone();
    
    // 3. Try to load the issuer identity
    let issuer_result = state.store.load_identity(&issuer_did).await;
    
    // 4. Validate the credential
    let is_valid = match issuer_result {
        Ok(issuer) => {
            let signer = CredentialSigner::new(issuer);
            signer.verify_credential(&credential)
                .map_err(|e| ApiError::CoreError(e))?
        },
        Err(_) => false, // If we don't have the issuer, mark as invalid
    };
    
    let response = serde_json::json!({
        "valid": is_valid,
        "issuer": issuer_did,
    });
    
    Ok(Json(response))
}

// Handler implementations for action queue operations
pub async fn queue_action<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<Value>> {
    // 1. Create an action queue
    let queue = ActionQueue::new(state.store.clone());
    
    // 2. Parse the action type from the payload
    let action_type = payload["action_type"]
        .as_str()
        .ok_or_else(|| ApiError::InvalidRequest("Missing action_type in payload".to_string()))?;
    
    let action_type = match action_type {
        "proposal" => ActionType::Proposal,
        "vote" => ActionType::Vote,
        "anchor" => ActionType::Anchor,
        _ => return Err(ApiError::InvalidRequest(format!("Invalid action type: {}", action_type))),
    };
    
    // 3. Get the creator DID from the payload
    let creator_did = payload["creator_did"]
        .as_str()
        .ok_or_else(|| ApiError::InvalidRequest("Missing creator_did in payload".to_string()))?
        .to_string();
    
    // 4. Queue the action
    let action_id = queue.queue_action(action_type, creator_did, payload.clone())
        .map_err(|e| ApiError::AgentError(e))?;
    
    let response = serde_json::json!({
        "action_id": action_id,
        "status": "queued",
    });
    
    Ok(Json(response))
}

// Handler for processing a queued action
pub async fn process_action<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(action_id): Path<String>,
) -> ApiResult<Json<DagNodeResponse>> {
    // Create an action processor
    let processor = ActionProcessor::new(state.store.clone());
    
    // Process the action
    let result = processor.process_action(&action_id).await
        .map_err(|e| ApiError::AgentError(e))?;
        
    // Convert the DAG node to a response
    let response = DagNodeResponse {
        cid: result.cid.clone(),
        content_type: result.content_type.clone(),
        creator: result.creator.clone(),
        timestamp: format!("{:?}", result.timestamp),
        content: result.content.clone(),
    };
    
    Ok(Json(response))
}

// Handler for getting an action status
pub async fn get_action<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(action_id): Path<String>,
) -> ApiResult<Json<ActionResponse>> {
    // Create an action queue
    let queue = ActionQueue::new(state.store.clone());
    
    // Get the action
    let action = queue.get_action(&action_id).await
        .map_err(|e| ApiError::AgentError(e))?;
        
    // Convert to response
    let response = ActionResponse {
        id: action.id,
        action_type: format!("{:?}", action.action_type),
        creator_did: action.creator_did,
        status: format!("{:?}", action.status),
        created_at: action.created_at.to_rfc3339(),
        error_message: action.error_message,
    };
    
    Ok(Json(response))
}

// Handler implementations for sync operations
pub async fn sync_dag<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
) -> ApiResult<Json<Value>> {
    // 1. Get the first identity to use for auth (in a real app, use the active identity)
    let identities = state.store.list_identities().await
        .map_err(|e| ApiError::StoreError(format!("Failed to list identities: {}", e)))?;
    
    if identities.is_empty() {
        return Err(ApiError::AuthError("No identities available for sync".to_string()));
    }
    
    let identity = state.store.load_identity(&identities[0]).await
        .map_err(|e| ApiError::StoreError(format!("Failed to load identity: {}", e)))?;
    
    // 2. Create a sync manager
    let sync_manager = SyncManager::new(identity, state.store.clone(), None);
    
    // 3. Perform sync
    sync_manager.sync_all().await
        .map_err(|e| ApiError::SyncError(format!("Sync failed: {}", e)))?;
    
    // 4. Get sync state
    let sync_state = sync_manager.get_sync_state("default").await;
    
    let response = match sync_state {
        Some(state) => serde_json::json!({
            "federation_id": state.federation_id,
            "last_synced_epoch": state.last_synced_epoch,
            "trust_bundles_count": state.trust_bundles_count,
            "dag_headers_count": state.dag_headers_count,
            "status": "success",
        }),
        None => serde_json::json!({
            "status": "success",
            "message": "Sync completed but no state available",
        }),
    };
    
    Ok(Json(response))
}

// Handler to trigger trust bundle synchronization
pub async fn sync_trust_bundles<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
) -> ApiResult<Json<Value>> {
    // Get the first identity to use for auth
    let identities = state.store.list_identities().await
        .map_err(|e| ApiError::StoreError(format!("Failed to list identities: {}", e)))?;
    
    if identities.is_empty() {
        return Err(ApiError::AuthError("No identities available for sync".to_string()));
    }
    
    let identity = state.store.load_identity(&identities[0]).await
        .map_err(|e| ApiError::StoreError(format!("Failed to load identity: {}", e)))?;
    
    // Create a sync manager
    let sync_manager = SyncManager::new(identity, state.store.clone(), None);
    
    // Get the federation URL from config
    let federation_url = &state.config.federation_url;
    
    // Sync trust bundles
    let bundles = sync_manager.sync_trust_bundles(federation_url).await
        .map_err(|e| ApiError::SyncError(format!("Failed to sync trust bundles: {}", e)))?;
    
    // Return summary
    let response = serde_json::json!({
        "bundles_synced": bundles.len(),
        "status": "success",
    });
    
    Ok(Json(response))
}

// Handler to list stored trust bundles
pub async fn list_trust_bundles<S: LocalWalletStore>(
    State(state): State<Arc<AppState<S>>>,
) -> ApiResult<Json<Vec<BundleResponse>>> {
    // Get the first identity to use for auth
    let identities = state.store.list_identities().await
        .map_err(|e| ApiError::StoreError(format!("Failed to list identities: {}", e)))?;
    
    if identities.is_empty() {
        return Err(ApiError::AuthError("No identities available for sync".to_string()));
    }
    
    let identity = state.store.load_identity(&identities[0]).await
        .map_err(|e| ApiError::StoreError(format!("Failed to load identity: {}", e)))?;
    
    // Create a sync manager
    let sync_manager = SyncManager::new(identity, state.store.clone(), None);
    
    // List trust bundles
    let bundles = sync_manager.list_trust_bundles().await
        .map_err(|e| ApiError::SyncError(format!("Failed to list trust bundles: {}", e)))?;
    
    // Convert to response format
    let responses = bundles.into_iter().map(|bundle| {
        BundleResponse {
            id: bundle.id,
            version: bundle.version,
            epoch: bundle.epoch,
            guardian_count: bundle.guardians.len(),
            created_at: format!("{:?}", bundle.created_at),
            valid_until: format!("{:?}", bundle.valid_until),
        }
    }).collect();
    
    Ok(Json(responses))
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