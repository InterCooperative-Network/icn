use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use validator::Validate;

/// Represents a discussion thread in AgoraNet
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Thread {
    /// Unique identifier for the thread
    pub id: Uuid,
    
    /// Title of the thread
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    
    /// Thread content/body text
    pub content: String,
    
    /// Author's DID
    pub author_did: String,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    
    /// Tags associated with the thread
    pub tags: Vec<String>,
    
    /// Optional ID of the proposal this thread is about
    pub proposal_id: Option<String>,
    
    /// Optional federation ID this thread belongs to
    pub federation_id: Option<String>,
    
    /// Status of the thread (open, closed, etc.)
    pub status: ThreadStatus,
    
    /// Additional metadata as key-value pairs
    pub metadata: HashMap<String, String>,
    
    /// Topic type of the thread
    pub topic_type: TopicType,
    
    /// Proposal reference
    pub proposal_ref: Option<String>,
    
    /// DAG reference
    pub dag_ref: Option<String>,
}

/// Status of a thread
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreadStatus {
    /// Thread is open for discussion
    Open,
    
    /// Thread is closed to new comments
    Closed,
    
    /// Thread is archived (read-only)
    Archived,
    
    /// Thread is hidden but not deleted
    Hidden,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TopicType {
    #[serde(rename = "proposal")]
    Proposal,
    #[serde(rename = "amendment")]
    Amendment,
    #[serde(rename = "budget")]
    Budget,
    #[serde(rename = "issue")]
    Issue,
    #[serde(rename = "announcement")]
    Announcement,
    #[serde(rename = "general")]
    General,
}

impl Default for TopicType {
    fn default() -> Self {
        TopicType::General
    }
}

impl Thread {
    /// Create a new thread
    pub fn new(
        id: Uuid,
        title: String,
        content: String,
        author_did: String,
        proposal_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        
        Self {
            id,
            title,
            content,
            author_did,
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
            proposal_id,
            federation_id: None,
            status: ThreadStatus::Open,
            metadata: HashMap::new(),
            topic_type: TopicType::General,
            proposal_ref: None,
            dag_ref: None,
        }
    }
    
    /// Add a tag to the thread
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }
    
    /// Set a metadata value
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateThreadRequest {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    pub federation_id: Option<String>,
    pub topic_type: Option<TopicType>,
    pub proposal_ref: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadResponse {
    pub id: String,
    pub title: String,
    pub creator_did: String,
    pub federation_id: Option<String>,
    pub topic_type: TopicType,
    pub proposal_ref: Option<String>,
    pub dag_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateThreadRequest {
    #[validate(length(min = 1, max = 200))]
    pub title: Option<String>,
    pub topic_type: Option<TopicType>,
    pub proposal_ref: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

impl From<Thread> for ThreadResponse {
    fn from(thread: Thread) -> Self {
        ThreadResponse {
            id: thread.id.to_string(),
            title: thread.title,
            creator_did: thread.author_did,
            federation_id: thread.federation_id,
            topic_type: thread.topic_type,
            proposal_ref: thread.proposal_ref,
            dag_ref: thread.dag_ref,
            created_at: thread.created_at,
            updated_at: thread.updated_at,
            metadata: Some(serde_json::to_value(thread.metadata).unwrap()),
        }
    }
} 