use axum::{
    extract::{State, Path, Json},
    http::StatusCode,
};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_agent::queue::ActionType;
use crate::error::{ApiResult, ApiError};
use crate::state::SharedState;
use uuid::Uuid;

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