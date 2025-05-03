use serde::{Serialize, Deserialize};
use serde_json::Value;
use sha2::{Sha256, Digest};
use multihash::Multihash;
use cid::Cid;
use std::collections::HashMap;
use crate::error::{WalletResult, WalletError};
use chrono::{DateTime, Utc};

/// Content-addressed data in a DAG structure with signatures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// The content of this node
    pub data: Value,
    /// Links to other DAG nodes (name -> CID)
    pub links: HashMap<String, String>,
    /// Signatures by DIDs (did -> base64 signature)
    pub signatures: HashMap<String, String>,
    /// Timestamp when this node was created
    pub created_at: DateTime<Utc>,
}

/// The type of DAG thread this node belongs to
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThreadType {
    /// Governance proposal thread
    Proposal,
    /// Vote on a proposal
    Vote,
    /// Data anchor (e.g., credential issuance)
    Anchor,
    /// Generic thread type
    Custom(String),
}

/// A DAG thread is a linked list of DAG nodes with a specific purpose
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagThread {
    /// The type of this thread
    pub thread_type: ThreadType,
    /// The creator of this thread
    pub creator: String,
    /// The root CID of this thread
    pub root_cid: String,
    /// The latest CID of this thread
    pub latest_cid: String,
    /// The title of this thread (if any)
    pub title: Option<String>,
    /// The description of this thread (if any)
    pub description: Option<String>,
    /// When this thread was created
    pub created_at: DateTime<Utc>,
    /// When this thread was last updated
    pub updated_at: DateTime<Utc>,
}

/// Cached information about a DAG thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDagThreadInfo {
    /// The thread ID
    pub thread_id: String,
    /// The thread type
    pub thread_type: ThreadType,
    /// The list of node CIDs in the thread, in order from oldest to newest
    pub node_cids: Vec<String>,
    /// Head CID (root of the thread)
    pub head_cid: String,
    /// Tail CID (latest node in the thread)
    pub tail_cid: String,
    /// When this cache was last updated
    pub last_updated: DateTime<Utc>,
    /// Additional metadata about the thread
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct DagOperations {
    // In a real implementation, this might store crypto keys, trusted CIDs, etc.
}

impl DagNode {
    pub fn new(data: Value) -> Self {
        Self {
            data,
            links: HashMap::new(),
            signatures: HashMap::new(),
            created_at: Utc::now(),
        }
    }
    
    pub fn add_link(&mut self, name: &str, cid: &str) {
        self.links.insert(name.to_string(), cid.to_string());
    }
    
    pub fn add_signature(&mut self, did: &str, signature: &str) {
        self.signatures.insert(did.to_string(), signature.to_string());
    }
}

impl DagThread {
    pub fn new(thread_type: ThreadType, creator: String, root_cid: String, title: Option<String>, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            thread_type,
            creator,
            root_cid: root_cid.clone(),
            latest_cid: root_cid,
            title,
            description,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn update_latest_cid(&mut self, cid: String) {
        self.latest_cid = cid;
        self.updated_at = Utc::now();
    }
}

impl CachedDagThreadInfo {
    /// Create a new cached thread info
    pub fn new(thread_id: &str, thread_type: ThreadType, head_cid: &str) -> Self {
        let now = Utc::now();
        Self {
            thread_id: thread_id.to_string(),
            thread_type,
            node_cids: vec![head_cid.to_string()],
            head_cid: head_cid.to_string(),
            tail_cid: head_cid.to_string(),
            last_updated: now,
            metadata: HashMap::new(),
        }
    }
    
    /// Add a node CID to the thread cache
    pub fn add_node(&mut self, cid: &str) {
        if !self.node_cids.contains(&cid.to_string()) {
            self.node_cids.push(cid.to_string());
            self.tail_cid = cid.to_string();
            self.last_updated = Utc::now();
        }
    }
    
    /// Add metadata to the thread cache
    pub fn add_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
        self.last_updated = Utc::now();
    }
}

impl DagOperations {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn compute_cid(&self, node: &DagNode) -> WalletResult<String> {
        // Create a canonical representation for hashing
        let mut canonical = node.clone();
        canonical.signatures = HashMap::new(); // Remove signatures from CID calculation
        
        let json = serde_json::to_string(&canonical)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize for CID: {}", e)))?;
            
        // Hash the content
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash_result = hasher.finalize();
        
        // Create a multihash
        let multihash = Multihash::wrap(0x12, &hash_result)
            .map_err(|e| WalletError::CryptoError(format!("Failed to create multihash: {}", e)))?;
            
        // Create a CID
        let cid = Cid::new_v1(0x71, multihash); // 0x71 is the codec for DAG-CBOR
        
        Ok(cid.to_string())
    }
    
    pub fn verify_node(&self, node: &DagNode, expected_cid: &str) -> WalletResult<bool> {
        // First, check if the CID of the object matches the expected CID
        let actual_cid = self.compute_cid(node)?;
        
        if actual_cid != expected_cid {
            return Ok(false);
        }
        
        // In a full implementation, we would also verify the signatures
        // For this example, we'll just check that there's at least one signature
        if node.signatures.is_empty() {
            return Ok(false);
        }
        
        Ok(true)
    }
    
    pub fn validate_thread_path(&self, nodes: &[DagNode], path: &[String]) -> WalletResult<bool> {
        // For this example, we'll just check that each path segment exists in the links
        if nodes.len() != path.len() {
            return Ok(false);
        }
        
        for (i, segment) in path.iter().enumerate() {
            if i + 1 < nodes.len() {
                let node = &nodes[i];
                let next_cid = self.compute_cid(&nodes[i + 1])?;
                
                if let Some(link_cid) = node.links.get(segment) {
                    if link_cid != &next_cid {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
        }
        
        Ok(true)
    }
    
    /// Create a new thread with a single root node
    pub fn create_thread(&self, thread_type: ThreadType, creator: &str, data: Value, title: Option<String>, description: Option<String>) -> WalletResult<(DagThread, DagNode)> {
        let mut root_node = DagNode::new(data);
        root_node.created_at = Utc::now();
        
        let root_cid = self.compute_cid(&root_node)?;
        let thread = DagThread::new(
            thread_type,
            creator.to_string(),
            root_cid.clone(),
            title,
            description,
        );
        
        Ok((thread, root_node))
    }
    
    /// Append a new node to an existing thread
    pub fn append_to_thread(&self, thread: &mut DagThread, parent_cid: &str, data: Value) -> WalletResult<DagNode> {
        let mut new_node = DagNode::new(data);
        new_node.add_link("parent", parent_cid);
        new_node.created_at = Utc::now();
        
        let new_cid = self.compute_cid(&new_node)?;
        thread.update_latest_cid(new_cid);
        
        Ok(new_node)
    }
} 