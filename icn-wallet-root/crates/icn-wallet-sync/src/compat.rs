use chrono::{DateTime, Utc};
/**
 * Compatibility module for handling different DagNode structures
 * 
 * This module bridges the gap between different versions of the DagNode structure,
 * ensuring compatibility when working with nodes from different sources.
 */

use std::time::{SystemTime, UNIX_EPOCH};
use crate::error::SyncError;
use crate::DagNode;
use crate::DagNodeMetadata;
use serde_json::Value;
use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Legacy DagNode structure for compatibility with older APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyDagNode {
    /// Node ID (CID)
    pub cid: String,
    
    /// References to other nodes (typically parent nodes)
    pub references: Vec<String>,
    
    /// Timestamp when this node was created
    pub created_at: DateTime<Utc>,
    
    /// JSON data payload
    #[serde(default)]
    pub data: Value,
    
    /// Metadata fields
    #[serde(default)]
    pub metadata: Value,
    
    /// Content type of the node
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Convert from a legacy node format to the current DagNode format
pub fn legacy_to_current(legacy: &LegacyDagNode) -> DagNode {
    let content = if let Ok(data) = serde_json::to_vec(&legacy.data) {
        data
    } else {
        // Default to empty payload if serialization fails
        vec![]
    };
    
    let metadata = if let Value::Object(obj) = &legacy.metadata {
        let mut metadata = DagNodeMetadata::default();
        
        // Try to extract sequence if it exists
        if let Some(Value::Number(num)) = obj.get("sequence") {
            if let Some(n) = num.as_u64() {
                metadata.sequence = Some(n);
            }
        }
        
        // Try to extract scope if it exists
        if let Some(Value::String(s)) = obj.get("scope") {
            metadata.scope = Some(s.clone());
        }
        
        metadata
    } else {
        DagNodeMetadata::default()
    };
    
    // Extract creator/issuer from metadata if available
    let creator = if let Value::Object(obj) = &legacy.metadata {
        if let Some(Value::String(s)) = obj.get("issuer") {
            s.clone()
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };
    
    // Convert chrono DateTime to SystemTime
    let timestamp = legacy.created_at
        .timestamp()
        .try_into()
        .ok()
        .and_then(|secs| UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs)))
        .unwrap_or_else(SystemTime::now);
    
    // Create current DagNode
    DagNode {
        cid: legacy.cid.clone(),
        parents: legacy.references.clone(),
        creator,
        timestamp,
        signatures: Vec::new(), // No direct mapping for signature in legacy format
        content,
        content_type: legacy.content_type.clone().unwrap_or("application/json".to_string()),
        metadata,
    }
}

/// Convert from the current DagNode format to the legacy format
pub fn current_to_legacy(current: &DagNode) -> LegacyDagNode {
    // Convert metadata to Value
    let mut metadata = Value::Object(serde_json::Map::new());
    
    // Add creator as issuer field to metadata
    if !current.creator.is_empty() {
        metadata.as_object_mut().unwrap().insert("issuer".to_string(), Value::String(current.creator.clone()));
    }
    
    // Add sequence if present
    if let Some(seq) = current.metadata.sequence {
        metadata.as_object_mut().unwrap().insert("sequence".to_string(), Value::Number(seq.into()));
    }
    
    // Add scope if present
    if let Some(scope) = &current.metadata.scope {
        metadata.as_object_mut().unwrap().insert("scope".to_string(), Value::String(scope.clone()));
    }
    
    // Try to parse content as JSON, fallback to string or base64
    let data = if let Ok(data) = serde_json::from_slice::<Value>(&current.content) {
        data
    } else {
        // Try as UTF-8 string
        if let Ok(s) = std::str::from_utf8(&current.content) {
            Value::String(s.to_string())
        } else {
            // Fallback to base64
            Value::String(STANDARD.encode(&current.content))
        }
    };
    
    // Convert SystemTime to chrono DateTime
    let created_at = match current.timestamp.duration_since(UNIX_EPOCH) {
        Ok(dur) => {
            let timestamp = dur.as_secs() as i64;
            DateTime::<Utc>::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now())
        },
        Err(_) => Utc::now(), // Fallback to current time
    };
    
    // Create legacy node
    LegacyDagNode {
        cid: current.cid.clone(),
        references: current.parents.clone(),
        created_at,
        data,
        metadata,
        content_type: Some(current.content_type.clone()),
    }
}

/// Try to parse a JSON value as either a current or legacy DagNode
pub fn parse_dag_node_json(json: Value) -> Result<DagNode, SyncError> {
    // First try to parse as LegacyDagNode
    if let Ok(legacy_node) = serde_json::from_value::<LegacyDagNode>(json.clone()) {
        Ok(legacy_to_current(&legacy_node))
    } else {
        // Try to parse directly as current DagNode
        serde_json::from_value(json)
            .map_err(|e| SyncError::Serialization(e))
    }
}

/// Convert a DagNode from icn-wallet-sync to icn-wallet-types format
pub fn to_wallet_types_dag_node(node: &DagNode) -> Result<icn_wallet_types::dag::DagNode, SyncError> {
    // Parse content as JSON if possible
    let content_json = if let Ok(json) = serde_json::from_slice::<Value>(&node.content) {
        json
    } else {
        // If not valid JSON, create a base64 string value
        Value::String(STANDARD.encode(&node.content))
    };
    
    // Convert signatures from Vec<String> to HashMap<String, String>
    let signatures = node.signatures.iter()
        .enumerate()
        .map(|(i, sig)| (format!("sig_{}", i), sig.clone()))
        .collect();
    
    // Convert SystemTime to Option<SystemTime>
    let created_at = Some(node.timestamp);
    
    Ok(icn_wallet_types::dag::DagNode {
        cid: node.cid.clone(),
        parents: node.parents.clone(),
        epoch: node.metadata.sequence.unwrap_or(0),
        creator: node.creator.clone(),
        timestamp: node.timestamp,
        content_type: node.content_type.clone(),
        content: content_json,
        signatures,
        data: Some(node.content.clone()),
        links: std::collections::HashMap::new(), // No equivalent in sync DagNode
        created_at,
    })
}

/// Convert a DagNode from icn-wallet-types to icn-wallet-sync format
pub fn from_wallet_types_dag_node(node: &icn_wallet_types::dag::DagNode) -> DagNode {
    // Use binary data if available, otherwise serialize the JSON content
    let content = match &node.data {
        Some(data) => data.clone(),
        None => serde_json::to_vec(&node.content).unwrap_or_default(),
    };
    
    // Extract signatures into a vector
    let signatures = node.signatures.values()
        .cloned()
        .collect();
    
    DagNode {
        cid: node.cid.clone(),
        parents: node.parents.clone(),
        timestamp: node.timestamp,
        creator: node.creator.clone(),
        content,
        content_type: node.content_type.clone(),
        signatures,
        metadata: DagNodeMetadata {
            sequence: Some(node.epoch),
            scope: None, // No direct mapping
        },
    }
} 
