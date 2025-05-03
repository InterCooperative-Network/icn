use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DagNode {
    pub id: String,
    pub node_type: DagNodeType,
    pub content_hash: String,
    pub signature: String,
    pub signer_did: String,
    pub parent_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DagNodeType {
    Thread,
    Message,
    ThreadSummary,
    ProposalExecution,
    Federation,
    Credential,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnchorRequest {
    pub content: String,
    pub node_type: DagNodeType,
    pub parent_refs: Vec<String>,
    pub signer_did: String,
    pub signature: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnchorResponse {
    pub dag_ref: String,
    pub content_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadAnchorRequest {
    pub thread_id: Uuid,
    pub signer_did: String,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageAnchorRequest {
    pub message_id: Uuid,
    pub thread_id: Uuid,
    pub signer_did: String,
    pub signature: String,
} 