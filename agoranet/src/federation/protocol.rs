use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::str::FromStr;

/// Message for a thread announcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMessage {
    /// Thread unique identifier
    pub thread_id: String,
    
    /// Thread title
    pub title: String,
    
    /// Optional proposal CID reference
    pub proposal_cid: Option<String>,
    
    /// Created timestamp
    pub created_at: i64,
    
    /// Thread author DID
    pub author_did: String,
    
    /// Message signature by author
    pub signature: Option<String>,
}

impl ThreadMessage {
    /// Create a new thread message
    pub fn new(
        thread_id: String,
        title: String,
        proposal_cid: Option<String>,
        author_did: String
    ) -> Self {
        Self {
            thread_id,
            title,
            proposal_cid,
            created_at: chrono::Utc::now().timestamp(),
            author_did,
            signature: None,
        }
    }
    
    /// Convert to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
    
    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Message for a credential link announcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialLinkMessage {
    /// Link unique identifier
    pub link_id: String,
    
    /// Thread ID this credential is linked to
    pub thread_id: String,
    
    /// Credential CID
    pub credential_cid: String,
    
    /// The DID of the entity linking the credential
    pub linked_by: String,
    
    /// Created timestamp
    pub created_at: i64,
    
    /// Message signature
    pub signature: Option<String>,
}

impl CredentialLinkMessage {
    /// Create a new credential link message
    pub fn new(
        link_id: String,
        thread_id: String,
        credential_cid: String,
        linked_by: String,
    ) -> Self {
        Self {
            link_id,
            thread_id,
            credential_cid,
            linked_by,
            created_at: chrono::Utc::now().timestamp(),
            signature: None,
        }
    }
    
    /// Convert to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
    
    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Message for requesting thread sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSyncRequestMessage {
    /// Thread ID to sync
    pub thread_id: String,
    
    /// Last known update timestamp
    pub last_update: Option<i64>,
    
    /// Requesting peer DID
    pub requester: String,
}

impl ThreadSyncRequestMessage {
    /// Create a new thread sync request message
    pub fn new(thread_id: String, last_update: Option<i64>, requester: String) -> Self {
        Self {
            thread_id,
            last_update,
            requester,
        }
    }
    
    /// Convert to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
    
    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Wrapper for all sync messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncMessage {
    #[serde(rename = "thread")]
    Thread(ThreadMessage),
    
    #[serde(rename = "credential_link")]
    CredentialLink(CredentialLinkMessage),
    
    #[serde(rename = "sync_request")]
    SyncRequest(ThreadSyncRequestMessage),
}

impl SyncMessage {
    /// Convert to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
    
    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
} 