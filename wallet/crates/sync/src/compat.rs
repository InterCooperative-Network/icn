/*!
 * Compatibility module for handling different DagNode structures
 * 
 * This module bridges the gap between different versions of the DagNode structure,
 * ensuring compatibility when working with nodes from different sources.
 */

use crate::error::SyncError;
use crate::DagNode;
use serde_json::Value;
use std::collections::HashMap;
use std::time::SystemTime;

/// Legacy DagNode structure for compatibility with older APIs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LegacyDagNode {
    /// Node ID (CID)
    pub id: String,
    
    /// Binary data payload (base64 encoded in JSON)
    #[serde(default)]
    pub data: Vec<u8>,
    
    /// Timestamp when this node was created
    #[serde(default)]
    pub created_at: SystemTime,
    
    /// References to other nodes (typically parent nodes)
    #[serde(default)]
    pub refs: Vec<String>,
    
    /// Metadata fields
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

/// Convert from a legacy node format to the current DagNode format
pub fn legacy_to_current(legacy: &LegacyDagNode) -> DagNode {
    let issuer = legacy.metadata.get("issuer")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    
    let mut metadata = wallet_types::DagNodeMetadata::default();
    
    if let Some(Value::Number(seq)) = legacy.metadata.get("sequence") {
        if let Some(seq_u64) = seq.as_u64() {
            metadata.sequence = Some(seq_u64);
        }
    }
    
    if let Some(Value::String(scope)) = legacy.metadata.get("scope") {
        metadata.scope = Some(scope.clone());
    }
    
    // Create current DagNode
    DagNode {
        cid: legacy.id.clone(),
        parents: legacy.refs.clone(),
        issuer,
        timestamp: legacy.created_at,
        signature: Vec::new(), // No direct mapping for signature in legacy format
        payload: legacy.data.clone(),
        metadata,
    }
}

/// Convert from the current DagNode format to the legacy format
pub fn current_to_legacy(current: &DagNode) -> LegacyDagNode {
    let mut metadata = HashMap::new();
    
    // Add issuer to metadata
    metadata.insert("issuer".to_string(), Value::String(current.issuer.clone()));
    
    // Add sequence if present
    if let Some(seq) = current.metadata.sequence {
        metadata.insert("sequence".to_string(), Value::Number(seq.into()));
    }
    
    // Add scope if present
    if let Some(scope) = &current.metadata.scope {
        metadata.insert("scope".to_string(), Value::String(scope.clone()));
    }
    
    // Create legacy node
    LegacyDagNode {
        id: current.cid.clone(),
        data: current.payload.clone(),
        created_at: current.timestamp,
        refs: current.parents.clone(),
        metadata,
    }
}

/// Try to parse a JSON value as either a current or legacy DagNode
pub fn parse_dag_node_json(value: Value) -> Result<DagNode, SyncError> {
    // First try parsing as current format
    match serde_json::from_value::<DagNode>(value.clone()) {
        Ok(node) => Ok(node),
        Err(_) => {
            // Try parsing as legacy format
            match serde_json::from_value::<LegacyDagNode>(value) {
                Ok(legacy_node) => Ok(legacy_to_current(&legacy_node)),
                Err(e) => Err(SyncError::Serialization(e)),
            }
        }
    }
} 