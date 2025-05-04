/*!
 * Compatibility module for wallet-runtime integration
 *
 * This module handles conversion between wallet and runtime data structures
 * to ensure proper interoperability between the two systems.
 */

use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use thiserror::Error;
use libipld::Ipld;

use icn_dag::{DagNode as RuntimeDagNode, DagNodeMetadata};
use icn_identity::IdentityId;

/// Error types for compatibility operations
#[derive(Error, Debug)]
pub enum CompatError {
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Format error: {0}")]
    FormatError(String),
    
    #[error("Conversion error: {0}")]
    ConversionError(String),
}

/// Result type for compatibility operations
pub type CompatResult<T> = std::result::Result<T, CompatError>;

/// Wallet-side DAG node structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletDagNode {
    /// CID of the node
    pub cid: String,
    
    /// Parent CIDs
    pub parents: Vec<String>,
    
    /// Issuer DID
    pub issuer: String,
    
    /// Timestamp when this node was created
    pub timestamp: SystemTime,
    
    /// Signature bytes
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    
    /// Binary payload
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    
    /// Metadata for the node
    pub metadata: WalletDagNodeMetadata,
}

/// Wallet-side DAG node metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WalletDagNodeMetadata {
    /// Sequence number within the DAG
    pub sequence: Option<u64>,
    
    /// Scope of the node (cooperative, community, etc.)
    pub scope: Option<String>,
}

/// Legacy wallet DAG node format (for compatibility with older wallets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyWalletDagNode {
    /// Node ID (CID)
    pub id: String,
    
    /// Binary data payload
    pub data: Vec<u8>,
    
    /// Timestamp when this node was created
    pub created_at: SystemTime,
    
    /// References to other nodes (typically parent nodes)
    pub refs: Vec<String>,
    
    /// Metadata fields
    pub metadata: serde_json::Map<String, Value>,
}

/// Convert from a runtime DAG node to a wallet DAG node
pub fn runtime_to_wallet(runtime_node: &RuntimeDagNode) -> CompatResult<WalletDagNode> {
    // Extract the necessary fields from the runtime node
    let cid = runtime_node.cid.to_string();
    
    // Convert parents from Cid to String
    let parents = runtime_node.parents.iter()
        .map(|cid| cid.to_string())
        .collect();
        
    // Convert payload to Vec<u8>
    let payload = match runtime_node.payload {
        Ipld::Bytes(ref bytes) => bytes.clone(),
        _ => {
            // If it's not bytes, try to serialize it to JSON and then to bytes
            let json = serde_json::to_string(&runtime_node.payload)?;
            json.as_bytes().to_vec()
        }
    };
    
    // Create wallet metadata from runtime metadata
    let metadata = WalletDagNodeMetadata {
        sequence: runtime_node.metadata.sequence,
        scope: runtime_node.metadata.scope.clone(),
    };
    
    Ok(WalletDagNode {
        cid,
        parents,
        issuer: runtime_node.issuer.to_string(),
        timestamp: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(runtime_node.metadata.timestamp),
        signature: runtime_node.signature.clone(),
        payload,
        metadata,
    })
}

/// Convert from a wallet DAG node to a runtime DAG node
pub fn wallet_to_runtime(wallet_node: &WalletDagNode) -> CompatResult<RuntimeDagNode> {
    // Parse the CID string
    let cid = cid::Cid::try_from(&wallet_node.cid)
        .map_err(|e| CompatError::ConversionError(format!("Invalid CID: {}", e)))?;
        
    // Convert parent strings to Cid objects
    let parents = wallet_node.parents.iter()
        .map(|s| cid::Cid::try_from(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CompatError::ConversionError(format!("Invalid parent CID: {}", e)))?;
        
    // Create Ipld payload from binary data
    // First try to parse as JSON, if that fails use as raw bytes
    let payload = match serde_json::from_slice::<Value>(&wallet_node.payload) {
        Ok(json) => {
            // Successfully parsed as JSON, convert to IPLD
            json_to_ipld(json)
        },
        Err(_) => {
            // Not valid JSON, use as raw bytes
            Ipld::Bytes(wallet_node.payload.clone())
        }
    };
    
    // Convert timestamp to seconds since epoch
    let timestamp = wallet_node.timestamp
        .duration_since(UNIX_EPOCH)
        .map_err(|e| CompatError::ConversionError(format!("Invalid timestamp: {}", e)))?
        .as_secs();
        
    // Create the metadata
    let metadata = DagNodeMetadata {
        timestamp,
        sequence: wallet_node.metadata.sequence,
        scope: wallet_node.metadata.scope.clone(),
    };
    
    // Create the runtime node
    Ok(RuntimeDagNode {
        cid,
        parents,
        issuer: IdentityId::new(wallet_node.issuer.clone()),
        signature: wallet_node.signature.clone(),
        payload,
        metadata,
    })
}

/// Convert from a legacy wallet node to the current wallet node format
pub fn legacy_to_wallet(legacy: &LegacyWalletDagNode) -> CompatResult<WalletDagNode> {
    let issuer = legacy.metadata.get("issuer")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    
    let sequence = legacy.metadata.get("sequence")
        .and_then(|v| v.as_u64());
        
    let scope = legacy.metadata.get("scope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
        
    let metadata = WalletDagNodeMetadata {
        sequence,
        scope,
    };
    
    Ok(WalletDagNode {
        cid: legacy.id.clone(),
        parents: legacy.refs.clone(),
        issuer,
        timestamp: legacy.created_at,
        signature: Vec::new(), // No direct mapping for signature in legacy format
        payload: legacy.data.clone(),
        metadata,
    })
}

/// Convert wallet node to legacy format
pub fn wallet_to_legacy(wallet: &WalletDagNode) -> CompatResult<LegacyWalletDagNode> {
    let mut metadata = serde_json::Map::new();
    
    // Add issuer to metadata
    metadata.insert("issuer".to_string(), Value::String(wallet.issuer.clone()));
    
    // Add sequence if present
    if let Some(seq) = wallet.metadata.sequence {
        metadata.insert("sequence".to_string(), Value::Number(seq.into()));
    }
    
    // Add scope if present
    if let Some(scope) = &wallet.metadata.scope {
        metadata.insert("scope".to_string(), Value::String(scope.clone()));
    }
    
    Ok(LegacyWalletDagNode {
        id: wallet.cid.clone(),
        data: wallet.payload.clone(),
        created_at: wallet.timestamp,
        refs: wallet.parents.clone(),
        metadata,
    })
}

/// Helper function to convert JSON Value to IPLD
fn json_to_ipld(json: Value) -> Ipld {
    match json {
        Value::Null => Ipld::Null,
        Value::Bool(b) => Ipld::Bool(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ipld::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Ipld::Float(f)
            } else {
                // Default to string if number can't be represented as i64 or f64
                Ipld::String(n.to_string())
            }
        },
        Value::String(s) => Ipld::String(s),
        Value::Array(arr) => {
            Ipld::List(arr.into_iter().map(json_to_ipld).collect())
        },
        Value::Object(obj) => {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_ipld(v));
            }
            Ipld::Map(map)
        }
    }
}

/// Helper function to convert timestamp between formats
pub fn system_time_to_datetime(time: SystemTime) -> CompatResult<DateTime<Utc>> {
    let since_epoch = time.duration_since(UNIX_EPOCH)
        .map_err(|e| CompatError::ConversionError(format!("Invalid SystemTime: {}", e)))?;
    
    let datetime = DateTime::<Utc>::from_timestamp(
        since_epoch.as_secs() as i64,
        since_epoch.subsec_nanos()
    ).ok_or_else(|| CompatError::ConversionError("Invalid timestamp for DateTime".to_string()))?;
    
    Ok(datetime)
}

/// Helper function to convert datetime to system time
pub fn datetime_to_system_time(dt: DateTime<Utc>) -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64)
} 