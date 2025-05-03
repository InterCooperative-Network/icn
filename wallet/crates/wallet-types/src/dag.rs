use serde::{Serialize, Deserialize};
use std::time::SystemTime;
use serde_bytes::Bytes;

/// Metadata for a DAG node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagNodeMetadata {
    /// Sequence number of this node (optional)
    pub sequence: Option<u64>,
    
    /// Scope of this node (optional)
    pub scope: Option<String>,
}

impl Default for DagNodeMetadata {
    fn default() -> Self {
        Self {
            sequence: None,
            scope: None,
        }
    }
}

/// Represents a node in the DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// CID of the node
    pub cid: String,
    
    /// Parent CIDs
    pub parents: Vec<String>,
    
    /// Identity (DID) that issued/signed this node
    pub issuer: String,
    
    /// Timestamp when this node was created
    pub timestamp: SystemTime,
    
    /// Signature over the canonicalized representation of the node
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    
    /// Binary data payload of this node
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    
    /// Metadata associated with this node
    #[serde(default)]
    pub metadata: DagNodeMetadata,
}

impl DagNode {
    /// Create a new DAG node
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cid: String,
        parents: Vec<String>,
        issuer: String,
        timestamp: SystemTime,
        signature: Vec<u8>,
        payload: Vec<u8>,
        metadata: Option<DagNodeMetadata>,
    ) -> Self {
        Self {
            cid,
            parents,
            issuer,
            timestamp,
            signature,
            payload,
            metadata: metadata.unwrap_or_default(),
        }
    }
    
    /// Get the parent CIDs
    pub fn parents(&self) -> &[String] {
        &self.parents
    }
    
    /// Get the timestamp
    pub fn timestamp(&self) -> u64 {
        // Convert SystemTime to seconds since UNIX epoch
        self.timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    /// Try to parse the payload as JSON
    pub fn payload_as_json(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::from_slice(&self.payload)
    }
    
    /// Set a new payload from a JSON value
    pub fn set_payload_from_json(&mut self, value: &serde_json::Value) -> serde_json::Result<()> {
        self.payload = serde_json::to_vec(value)?;
        Ok(())
    }
}

/// DAG Thread structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagThread {
    /// Thread ID
    pub id: String,
    
    /// Thread type (e.g., "proposal", "credential", etc.)
    pub thread_type: String,
    
    /// Root node CID
    pub root_cid: String,
    
    /// List of node CIDs in this thread
    pub nodes: Vec<String>,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
    
    /// The latest CID in the thread
    pub latest_cid: String,
}

impl DagThread {
    /// Create a new DAG thread
    pub fn new(
        id: String,
        thread_type: String,
        root_cid: String,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            thread_type,
            root_cid: root_cid.clone(),
            nodes: vec![root_cid.clone()],
            last_updated: now,
            latest_cid: root_cid,
        }
    }
    
    /// Add a node to the thread
    pub fn add_node(&mut self, cid: String) {
        self.nodes.push(cid.clone());
        self.latest_cid = cid;
        self.last_updated = SystemTime::now();
    }
    
    /// Check if the thread contains a node
    pub fn contains_node(&self, cid: &str) -> bool {
        self.nodes.iter().any(|node_cid| node_cid == cid)
    }
}

// Conversion functions between wallet and runtime DagNode types will be added
// when the runtime-compat feature is enabled.
#[cfg(feature = "runtime-compat")]
pub mod runtime_compat {
    use super::*;
    use cid::Cid;
    use libipld::Ipld;

    /// Convert from wallet DagNode to runtime DagNode
    pub fn to_runtime_dag_node(node: &DagNode) -> Result<icn_dag::DagNode, String> {
        // Parse CIDs
        let parents: Result<Vec<Cid>, _> = node.parents
            .iter()
            .map(|cid_str| Cid::try_from(cid_str.as_str())
                .map_err(|e| format!("Invalid parent CID: {}", e)))
            .collect();
        
        let parents = parents?;
        
        // Parse payload as JSON for now (runtime uses Ipld)
        let payload_json = serde_json::from_slice::<serde_json::Value>(&node.payload)
            .map_err(|e| format!("Invalid payload JSON: {}", e))?;
        
        // Convert JSON to Ipld
        let payload = json_to_ipld(payload_json);
        
        // Create runtime metadata
        let metadata = icn_dag::DagNodeMetadata {
            timestamp: node.timestamp(),
            sequence: node.metadata.sequence,
            scope: node.metadata.scope.clone(),
        };
        
        // Create runtime node using DagNodeBuilder
        let runtime_node = icn_dag::DagNodeBuilder::new()
            .issuer(icn_identity::IdentityId::new(&node.issuer))
            .parents(parents)
            .payload(payload)
            .signature(node.signature.clone())
            .metadata(metadata)
            .build()
            .map_err(|e| format!("Failed to build runtime node: {}", e))?;
        
        Ok(runtime_node)
    }
    
    /// Convert runtime DagNode to wallet DagNode
    pub fn from_runtime_dag_node(runtime_node: &icn_dag::DagNode, cid: Cid) -> Result<DagNode, String> {
        // Convert parents to strings
        let parents: Vec<String> = runtime_node.parents
            .iter()
            .map(|cid| cid.to_string())
            .collect();
        
        // Convert payload to JSON bytes
        let payload_bytes = ipld_to_json_bytes(&runtime_node.payload)
            .map_err(|e| format!("Failed to convert IPLD to JSON: {}", e))?;
        
        // Create wallet metadata
        let metadata = DagNodeMetadata {
            sequence: runtime_node.metadata.sequence,
            scope: runtime_node.metadata.scope.clone(),
        };
        
        // Get timestamp as SystemTime
        let timestamp_secs = runtime_node.timestamp();
        let timestamp = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs);
        
        // Create wallet node
        let wallet_node = DagNode {
            cid: cid.to_string(),
            parents,
            issuer: runtime_node.issuer.0.clone(),
            timestamp,
            signature: runtime_node.signature.clone(),
            payload: payload_bytes,
            metadata,
        };
        
        Ok(wallet_node)
    }
    
    // Helper to convert JSON to IPLD
    fn json_to_ipld(value: serde_json::Value) -> Ipld {
        match value {
            serde_json::Value::Null => Ipld::Null,
            serde_json::Value::Bool(b) => Ipld::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ipld::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Ipld::Float(f)
                } else {
                    // Fallback for unsupported number types
                    Ipld::String(n.to_string())
                }
            },
            serde_json::Value::String(s) => Ipld::String(s),
            serde_json::Value::Array(a) => {
                Ipld::List(a.into_iter().map(json_to_ipld).collect())
            },
            serde_json::Value::Object(o) => {
                let map = o.into_iter()
                    .map(|(k, v)| (k, json_to_ipld(v)))
                    .collect();
                Ipld::Map(map)
            },
        }
    }
    
    // Helper to convert IPLD to JSON bytes
    fn ipld_to_json_bytes(ipld: &Ipld) -> serde_json::Result<Vec<u8>> {
        let json_value = ipld_to_json(ipld);
        serde_json::to_vec(&json_value)
    }
    
    // Helper to convert IPLD to JSON
    fn ipld_to_json(ipld: &Ipld) -> serde_json::Value {
        match ipld {
            Ipld::Null => serde_json::Value::Null,
            Ipld::Bool(b) => serde_json::Value::Bool(*b),
            Ipld::Integer(i) => serde_json::Value::Number((*i).into()),
            Ipld::Float(f) => {
                if let Some(n) = serde_json::Number::from_f64(*f) {
                    serde_json::Value::Number(n)
                } else {
                    // Fallback for NaN/infinity
                    serde_json::Value::String(f.to_string())
                }
            },
            Ipld::String(s) => serde_json::Value::String(s.clone()),
            Ipld::Bytes(b) => {
                // Convert bytes to base64 string
                let b64 = base64::encode(b);
                serde_json::Value::String(b64)
            },
            Ipld::List(l) => {
                serde_json::Value::Array(
                    l.iter().map(ipld_to_json).collect()
                )
            },
            Ipld::Map(m) => {
                let mut map = serde_json::Map::new();
                for (k, v) in m {
                    map.insert(k.clone(), ipld_to_json(v));
                }
                serde_json::Value::Object(map)
            },
            Ipld::Link(l) => {
                // Convert CID to string
                serde_json::Value::String(l.to_string())
            },
        }
    }
} 