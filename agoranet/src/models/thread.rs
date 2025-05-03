use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Represents a discussion thread in AgoraNet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Unique identifier for the thread
    pub id: String,
    
    /// Title of the thread
    pub title: String,
    
    /// Thread content/body text
    pub content: String,
    
    /// Author's DID
    pub author_did: String,
    
    /// Creation timestamp
    pub created_at: String,
    
    /// Last updated timestamp
    pub updated_at: String,
    
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

impl Thread {
    /// Create a new thread
    pub fn new(
        id: String,
        title: String,
        content: String,
        author_did: String,
        proposal_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        
        Self {
            id,
            title,
            content,
            author_did,
            created_at: now.clone(),
            updated_at: now,
            tags: Vec::new(),
            proposal_id,
            federation_id: None,
            status: ThreadStatus::Open,
            metadata: HashMap::new(),
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