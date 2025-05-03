//! DAG-related data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// DAG node structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// CID of the node
    pub cid: String,
    
    /// Parent CIDs
    pub parents: Vec<String>,
    
    /// Epoch number
    pub epoch: u64,
    
    /// Creator DID
    pub creator: String,
    
    /// Timestamp
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
    
    /// Content type
    pub content_type: String,
    
    /// Node content (JSON)
    pub content: serde_json::Value,
    
    /// Signatures map
    pub signatures: HashMap<String, String>,
    
    /// Binary data for the node (if applicable)
    pub data: Option<Vec<u8>>,
    
    /// Node links (for IPLD compatibility) - map of name to CID
    pub links: HashMap<String, String>,
    
    /// Created time for the node
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub created_at: Option<DateTime<Utc>>,
}

/// DAG Thread structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagThread {
    /// Thread ID
    pub id: String,
    
    /// Thread type
    pub thread_type: String,
    
    /// Root node CID
    pub root_cid: String,
    
    /// List of node CIDs in this thread
    pub nodes: Vec<String>,
    
    /// Last updated timestamp
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_updated: DateTime<Utc>,
    
    /// The latest CID in the thread
    pub latest_cid: String,
} 