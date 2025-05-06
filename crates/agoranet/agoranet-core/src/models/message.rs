use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_did: String,
    pub content: String,
    pub reply_to: Option<Uuid>,
    pub signature: Option<String>,
    pub dag_ref: Option<String>,
    pub dag_anchored: bool,
    pub credential_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateMessageRequest {
    #[validate(length(min = 1, max = 10000))]
    pub content: String,
    pub reply_to: Option<String>,
    pub signature: Option<String>,
    pub anchor_to_dag: Option<bool>,
    pub credential_refs: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageResponse {
    pub id: String,
    pub thread_id: String,
    pub author_did: String,
    pub content: String,
    pub reply_to: Option<String>,
    pub signature: Option<String>,
    pub dag_ref: Option<String>,
    pub dag_anchored: bool,
    pub credential_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
    pub reactions: Option<Vec<ReactionCount>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReactionCount {
    pub reaction_type: String,
    pub count: i64,
}

impl From<Message> for MessageResponse {
    fn from(message: Message) -> Self {
        MessageResponse {
            id: message.id.to_string(),
            thread_id: message.thread_id.to_string(),
            author_did: message.author_did,
            content: message.content,
            reply_to: message.reply_to.map(|id| id.to_string()),
            signature: message.signature,
            dag_ref: message.dag_ref,
            dag_anchored: message.dag_anchored,
            credential_refs: message.credential_refs,
            created_at: message.created_at,
            metadata: message.metadata,
            reactions: None,
        }
    }
} 