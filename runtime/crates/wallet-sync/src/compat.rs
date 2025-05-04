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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Helper function to create a test RuntimeDagNode
    fn create_test_runtime_node() -> RuntimeDagNode {
        let cid = cid::Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
        let parent_cid = cid::Cid::try_from("bafkreiaxnnnb7qz6drrbababuirxx54hlzkrl2yxekizxr6gpceiqdu4i").unwrap();
        
        let metadata = DagNodeMetadata {
            timestamp: 1683123456,
            sequence: Some(42),
            scope: Some("test-scope".to_string()),
        };
        
        let mut map = BTreeMap::new();
        map.insert("key".to_string(), Ipld::String("value".to_string()));
        
        RuntimeDagNode {
            cid,
            parents: vec![parent_cid],
            issuer: IdentityId::new("did:icn:test".to_string()),
            signature: vec![1, 2, 3, 4],
            payload: Ipld::Map(map),
            metadata,
        }
    }

    // Helper function to create a test WalletDagNode
    fn create_test_wallet_node() -> WalletDagNode {
        WalletDagNode {
            cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
            parents: vec!["bafkreiaxnnnb7qz6drrbababuirxx54hlzkrl2yxekizxr6gpceiqdu4i".to_string()],
            issuer: "did:icn:test".to_string(),
            timestamp: UNIX_EPOCH + std::time::Duration::from_secs(1683123456),
            signature: vec![1, 2, 3, 4],
            payload: r#"{"key":"value"}"#.as_bytes().to_vec(),
            metadata: WalletDagNodeMetadata {
                sequence: Some(42),
                scope: Some("test-scope".to_string()),
            },
        }
    }

    #[test]
    fn test_runtime_to_wallet_conversion() {
        let runtime_node = create_test_runtime_node();
        let wallet_node = runtime_to_wallet(&runtime_node).unwrap();
        
        assert_eq!(wallet_node.cid, runtime_node.cid.to_string());
        assert_eq!(wallet_node.parents.len(), runtime_node.parents.len());
        assert_eq!(wallet_node.parents[0], runtime_node.parents[0].to_string());
        assert_eq!(wallet_node.issuer, runtime_node.issuer.to_string());
        assert_eq!(wallet_node.signature, runtime_node.signature);
        assert_eq!(wallet_node.metadata.sequence, runtime_node.metadata.sequence);
        assert_eq!(wallet_node.metadata.scope, runtime_node.metadata.scope);
    }

    #[test]
    fn test_wallet_to_runtime_conversion() {
        let wallet_node = create_test_wallet_node();
        let runtime_node = wallet_to_runtime(&wallet_node).unwrap();
        
        assert_eq!(runtime_node.cid.to_string(), wallet_node.cid);
        assert_eq!(runtime_node.parents.len(), wallet_node.parents.len());
        assert_eq!(runtime_node.parents[0].to_string(), wallet_node.parents[0]);
        assert_eq!(runtime_node.issuer.to_string(), wallet_node.issuer);
        assert_eq!(runtime_node.signature, wallet_node.signature);
        assert_eq!(runtime_node.metadata.sequence, wallet_node.metadata.sequence);
        assert_eq!(runtime_node.metadata.scope, wallet_node.metadata.scope);
    }

    #[test]
    fn test_legacy_conversions() {
        let wallet_node = create_test_wallet_node();
        
        // Convert to legacy format
        let legacy_node = wallet_to_legacy(&wallet_node).unwrap();
        
        // Then back to wallet format
        let converted_wallet_node = legacy_to_wallet(&legacy_node).unwrap();
        
        // Verify key properties were preserved
        assert_eq!(converted_wallet_node.cid, wallet_node.cid);
        assert_eq!(converted_wallet_node.parents, wallet_node.parents);
        assert_eq!(converted_wallet_node.issuer, wallet_node.issuer);
        assert_eq!(converted_wallet_node.timestamp, wallet_node.timestamp);
        assert_eq!(converted_wallet_node.payload, wallet_node.payload);
        assert_eq!(converted_wallet_node.metadata.sequence, wallet_node.metadata.sequence);
        assert_eq!(converted_wallet_node.metadata.scope, wallet_node.metadata.scope);
        // Note: signature is not preserved in legacy format
    }

    #[test]
    fn test_datetime_conversions() {
        let now = SystemTime::now();
        let datetime = system_time_to_datetime(now).unwrap();
        let converted_time = datetime_to_system_time(datetime);
        
        // Due to precision loss, we compare with a small epsilon
        let diff = now.duration_since(converted_time).unwrap_or_else(|_| {
            converted_time.duration_since(now).unwrap()
        });
        
        // Allow 1 second difference due to precision issues
        assert!(diff.as_secs() <= 1, "Time conversion difference too large: {:?}", diff);
    }
} 