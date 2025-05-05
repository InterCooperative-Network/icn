use chrono::{DateTime, Utc};
/**
 * Compatibility module for handling different DagNode structures
 * 
 * This module bridges the gap between different versions of the DagNode structure,
 * ensuring compatibility when working with nodes from different sources.
 */

use std::time::SystemTime;
use crate::error::SyncError;
use crate::DagNode;
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
    let payload = if let Ok(data) = serde_json::to_vec(&legacy.data) {
        data
    } else {
        // Default to empty payload if serialization fails
        vec![]
    };
    
    let metadata = if let Value::Object(obj) = &legacy.metadata {
        let mut metadata = crate::DagNodeMetadata::default();
        
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
        crate::DagNodeMetadata::default()
    };
    
    // Create current DagNode
    DagNode {
        cid: legacy.cid.clone(),
        parents: legacy.references.clone(),
        creator: "".to_string(), // No direct mapping
        timestamp: SystemTime::now(),
        signatures: Vec::new(), // No direct mapping for signature in legacy format
        content: payload,
        content_type: legacy.content_type.clone().unwrap_or("application/json".to_string()),
        metadata,
    }
}

/// Convert from the current DagNode format to the legacy format
pub fn current_to_legacy(current: &DagNode) -> LegacyDagNode {
    // Convert metadata to Value
    let mut metadata = Value::Object(serde_json::Map::new());
    
    // Add creator as issuer field to metadata
    metadata.as_object_mut().unwrap().insert("issuer".to_string(), Value::String(current.creator.clone()));
    
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
    
    // Create legacy node
    LegacyDagNode {
        cid: current.cid.clone(),
        references: current.parents.clone(),
        created_at: Utc::now(),
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
