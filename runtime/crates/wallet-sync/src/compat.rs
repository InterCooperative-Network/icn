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
    pub sequence: u64,
    
    /// Scope of the node (cooperative, community, etc.)
    pub scope: Option<String>,
    
    /// Content type/format
    pub content_type: Option<String>,
    
    /// Additional tags
    pub tags: Vec<String>,
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
    
    // Extract scope from tags if present (scope:xxx tag)
    let scope = runtime_node.metadata.tags.iter()
        .find(|tag| tag.starts_with("scope:"))
        .map(|tag| tag[6..].to_string());
    
    // Create wallet metadata from runtime metadata
    let metadata = WalletDagNodeMetadata {
        sequence: runtime_node.metadata.sequence,
        scope,
        content_type: runtime_node.metadata.content_type.clone(),
        tags: runtime_node.metadata.tags.clone(),
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
        
    // Collect tags
    let mut tags = wallet_node.metadata.tags.clone();
    
    // Add scope as a tag if it exists
    if let Some(scope) = &wallet_node.metadata.scope {
        // Add a special tag for scope if it doesn't already exist
        if !tags.iter().any(|tag| tag.starts_with("scope:")) {
            tags.push(format!("scope:{}", scope));
        }
    }
    
    // Create the metadata
    let metadata = DagNodeMetadata {
        timestamp,
        sequence: wallet_node.metadata.sequence,
        content_type: wallet_node.metadata.content_type.clone(),
        tags,
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
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
        
    let scope = legacy.metadata.get("scope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
        
    let content_type = legacy.metadata.get("content_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract tags if they exist in metadata
    let tags = legacy.metadata.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(Vec::new);
        
    let metadata = WalletDagNodeMetadata {
        sequence,
        scope,
        content_type,
        tags,
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
    
    // Add sequence
    metadata.insert("sequence".to_string(), Value::Number(wallet.metadata.sequence.into()));
    
    // Add scope if present
    if let Some(scope) = &wallet.metadata.scope {
        metadata.insert("scope".to_string(), Value::String(scope.clone()));
    }

    // Add content_type if present
    if let Some(content_type) = &wallet.metadata.content_type {
        metadata.insert("content_type".to_string(), Value::String(content_type.clone()));
    }
    
    // Add tags if present
    if !wallet.metadata.tags.is_empty() {
        let tags_array = wallet.metadata.tags.iter()
            .map(|tag| Value::String(tag.clone()))
            .collect::<Vec<_>>();
        metadata.insert("tags".to_string(), Value::Array(tags_array));
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
            sequence: 42,
            content_type: Some("application/json".to_string()),
            tags: vec!["test".to_string(), "example".to_string(), "scope:test-scope".to_string()],
        };
        
        let mut map = BTreeMap::new();
        map.insert("key".to_string(), Ipld::String("value".to_string()));
        
        RuntimeDagNode {
            cid,
            parents: vec![parent_cid],
            issuer: IdentityId::new("did:icn:test123".to_string()),
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
            issuer: "did:icn:test123".to_string(),
            timestamp: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1683123456),
            signature: vec![1, 2, 3, 4],
            payload: b"{\"key\":\"value\"}".to_vec(),
            metadata: WalletDagNodeMetadata {
                sequence: 42,
                scope: Some("test-scope".to_string()),
                content_type: Some("application/json".to_string()),
                tags: vec!["test".to_string(), "example".to_string()],
            },
        }
    }

    #[test]
    fn test_runtime_to_wallet_conversion() {
        let runtime_node = create_test_runtime_node();
        let wallet_node = runtime_to_wallet(&runtime_node).unwrap();
        
        assert_eq!(wallet_node.cid, runtime_node.cid.to_string());
        assert_eq!(wallet_node.issuer, runtime_node.issuer.to_string());
        assert_eq!(wallet_node.metadata.sequence, runtime_node.metadata.sequence);
        assert_eq!(wallet_node.metadata.scope, Some("test-scope".to_string()));
        assert_eq!(wallet_node.metadata.content_type, runtime_node.metadata.content_type);
        assert!(wallet_node.metadata.tags.contains(&"test".to_string()));
        assert!(wallet_node.metadata.tags.contains(&"example".to_string()));
        assert!(wallet_node.metadata.tags.contains(&"scope:test-scope".to_string()));
    }

    #[test]
    fn test_wallet_to_runtime_conversion() {
        let wallet_node = create_test_wallet_node();
        let runtime_node = wallet_to_runtime(&wallet_node).unwrap();
        
        assert_eq!(runtime_node.cid.to_string(), wallet_node.cid);
        assert_eq!(runtime_node.issuer.to_string(), wallet_node.issuer);
        assert_eq!(runtime_node.metadata.sequence, wallet_node.metadata.sequence);
        assert_eq!(runtime_node.metadata.content_type, wallet_node.metadata.content_type);
        
        // Verify that the scope was added as a tag
        assert!(runtime_node.metadata.tags.contains(&"scope:test-scope".to_string()));
        assert!(runtime_node.metadata.tags.contains(&"test".to_string()));
        assert!(runtime_node.metadata.tags.contains(&"example".to_string()));
    }

    #[test]
    fn test_legacy_conversions() {
        // Create a legacy node
        let mut metadata = serde_json::Map::new();
        metadata.insert("issuer".to_string(), Value::String("did:icn:legacy123".to_string()));
        metadata.insert("sequence".to_string(), Value::Number(100.into()));
        metadata.insert("scope".to_string(), Value::String("legacy-scope".to_string()));
        metadata.insert("content_type".to_string(), Value::String("text/plain".to_string()));
        let tags = vec![
            Value::String("legacy".to_string()),
            Value::String("old-format".to_string())
        ];
        metadata.insert("tags".to_string(), Value::Array(tags));
        
        let legacy_node = LegacyWalletDagNode {
            id: "legacy-cid-123".to_string(),
            data: b"legacy data".to_vec(),
            created_at: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1600000000),
            refs: vec!["parent-cid-1".to_string(), "parent-cid-2".to_string()],
            metadata,
        };
        
        // Convert legacy -> wallet -> legacy
        let wallet_node = legacy_to_wallet(&legacy_node).unwrap();
        let round_trip = wallet_to_legacy(&wallet_node).unwrap();
        
        // Check original conversions
        assert_eq!(wallet_node.cid, legacy_node.id);
        assert_eq!(wallet_node.payload, legacy_node.data);
        assert_eq!(wallet_node.timestamp, legacy_node.created_at);
        assert_eq!(wallet_node.metadata.sequence, 100);
        assert_eq!(wallet_node.metadata.scope.as_ref().unwrap(), "legacy-scope");
        assert_eq!(wallet_node.metadata.content_type.as_ref().unwrap(), "text/plain");
        assert_eq!(wallet_node.metadata.tags.len(), 2);
        assert!(wallet_node.metadata.tags.contains(&"legacy".to_string()));
        assert!(wallet_node.metadata.tags.contains(&"old-format".to_string()));
        
        // Check round trip conversions
        assert_eq!(round_trip.id, legacy_node.id);
        assert_eq!(round_trip.data, legacy_node.data);
        assert_eq!(round_trip.created_at, legacy_node.created_at);
        
        let seq_val = round_trip.metadata.get("sequence").unwrap().as_u64().unwrap();
        assert_eq!(seq_val, 100);
        
        let scope_val = round_trip.metadata.get("scope").unwrap().as_str().unwrap();
        assert_eq!(scope_val, "legacy-scope");
        
        let content_type_val = round_trip.metadata.get("content_type").unwrap().as_str().unwrap();
        assert_eq!(content_type_val, "text/plain");
        
        let tags_arr = round_trip.metadata.get("tags").unwrap().as_array().unwrap();
        assert_eq!(tags_arr.len(), 2);
        assert!(tags_arr.iter().any(|v| v.as_str().unwrap() == "legacy"));
        assert!(tags_arr.iter().any(|v| v.as_str().unwrap() == "old-format"));
    }

    #[test]
    fn test_datetime_conversions() {
        // Create a test time
        let now = SystemTime::now();
        
        // Convert to DateTime
        let dt = system_time_to_datetime(now).unwrap();
        
        // Convert back to SystemTime
        let st = datetime_to_system_time(dt);
        
        // Duration between original and round-trip times should be less than 1 second
        // (due to nanosecond precision loss)
        let duration = match st.duration_since(now) {
            Ok(d) => d,
            Err(e) => e.duration(),
        };
        
        assert!(duration.as_secs() < 1);
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    // Helper function to create a test RuntimeDagNode metadata
    fn create_runtime_metadata() -> DagNodeMetadata {
        DagNodeMetadata {
            timestamp: 1683123456,
            sequence: 42,
            content_type: Some("text/plain".to_string()),
            tags: vec!["test".to_string(), "example".to_string(), "scope:test-scope".to_string()],
        }
    }

    // Helper function to create a test WalletDagNodeMetadata
    fn create_wallet_metadata() -> WalletDagNodeMetadata {
        WalletDagNodeMetadata {
            sequence: 42,
            scope: Some("test-scope".to_string()),
            content_type: Some("text/plain".to_string()),
            tags: vec!["test".to_string(), "example".to_string()],
        }
    }

    // Helper functions to simulate the metadata conversion logic
    fn runtime_to_wallet_metadata(runtime_metadata: &DagNodeMetadata) -> WalletDagNodeMetadata {
        // Extract scope from tags if present (scope:xxx tag)
        let scope = runtime_metadata.tags.iter()
            .find(|tag| tag.starts_with("scope:"))
            .map(|tag| tag[6..].to_string());
        
        WalletDagNodeMetadata {
            sequence: runtime_metadata.sequence,
            scope,
            content_type: runtime_metadata.content_type.clone(),
            tags: runtime_metadata.tags.clone(),
        }
    }

    fn wallet_to_runtime_metadata(wallet_metadata: &WalletDagNodeMetadata) -> DagNodeMetadata {
        // Collect tags
        let mut tags = wallet_metadata.tags.clone();
        
        // Add scope as a tag if it exists
        if let Some(scope) = &wallet_metadata.scope {
            // Add a special tag for scope if it doesn't already exist
            if !tags.iter().any(|tag| tag.starts_with("scope:")) {
                tags.push(format!("scope:{}", scope));
            }
        }
        
        DagNodeMetadata {
            timestamp: 0, // Not relevant for these tests
            sequence: wallet_metadata.sequence,
            content_type: wallet_metadata.content_type.clone(),
            tags,
        }
    }

    #[test]
    fn test_wallet_to_runtime_to_wallet_roundtrip() {
        let original_wallet_metadata = create_wallet_metadata();
        
        // Convert wallet -> runtime
        let runtime_metadata = wallet_to_runtime_metadata(&original_wallet_metadata);
        
        // Convert runtime -> wallet
        let round_trip_wallet_metadata = runtime_to_wallet_metadata(&runtime_metadata);
        
        // Verify sequence
        assert_eq!(round_trip_wallet_metadata.sequence, original_wallet_metadata.sequence);
        
        // Verify scope
        assert_eq!(round_trip_wallet_metadata.scope, original_wallet_metadata.scope);
        
        // Verify content_type
        assert_eq!(round_trip_wallet_metadata.content_type, original_wallet_metadata.content_type);
        
        // Verify tags (excluding scope tag)
        for tag in &original_wallet_metadata.tags {
            assert!(round_trip_wallet_metadata.tags.contains(tag));
        }
    }

    #[test]
    fn test_runtime_to_wallet_to_runtime_roundtrip() {
        let original_runtime_metadata = create_runtime_metadata();
        
        // Convert runtime -> wallet
        let wallet_metadata = runtime_to_wallet_metadata(&original_runtime_metadata);
        
        // Convert wallet -> runtime
        let round_trip_runtime_metadata = wallet_to_runtime_metadata(&wallet_metadata);
        
        // Verify sequence
        assert_eq!(round_trip_runtime_metadata.sequence, original_runtime_metadata.sequence);
        
        // Verify content_type
        assert_eq!(round_trip_runtime_metadata.content_type, original_runtime_metadata.content_type);
        
        // Verify that all original tags are present in round trip
        for tag in &original_runtime_metadata.tags {
            assert!(round_trip_runtime_metadata.tags.contains(tag));
        }
        
        // Verify that scope was preserved
        let original_scope_tag = original_runtime_metadata.tags.iter()
            .find(|tag| tag.starts_with("scope:"));
        let round_trip_scope_tag = round_trip_runtime_metadata.tags.iter()
            .find(|tag| tag.starts_with("scope:"));
        
        assert_eq!(original_scope_tag, round_trip_scope_tag);
    }

    #[test]
    fn test_multiple_unrelated_tags() {
        let mut runtime_metadata = create_runtime_metadata();
        runtime_metadata.tags.push("unrelated1".to_string());
        runtime_metadata.tags.push("unrelated2".to_string());
        runtime_metadata.tags.push("another:tag".to_string());
        
        // Convert runtime -> wallet -> runtime
        let wallet_metadata = runtime_to_wallet_metadata(&runtime_metadata);
        let round_trip_metadata = wallet_to_runtime_metadata(&wallet_metadata);
        
        // Verify all tags are preserved
        for tag in &runtime_metadata.tags {
            assert!(round_trip_metadata.tags.contains(tag));
        }
        
        // Verify the scope was correctly extracted
        assert_eq!(wallet_metadata.scope, Some("test-scope".to_string()));
    }

    #[test]
    fn test_missing_scope() {
        let mut runtime_metadata = create_runtime_metadata();
        // Remove the scope tag
        runtime_metadata.tags.retain(|tag| !tag.starts_with("scope:"));
        
        // Convert runtime -> wallet
        let wallet_metadata = runtime_to_wallet_metadata(&runtime_metadata);
        
        // Verify scope is None
        assert_eq!(wallet_metadata.scope, None);
        
        // Convert wallet -> runtime
        let round_trip_metadata = wallet_to_runtime_metadata(&wallet_metadata);
        
        // Verify no scope tag was added
        assert!(!round_trip_metadata.tags.iter().any(|tag| tag.starts_with("scope:")));
    }

    #[test]
    fn test_malformed_scope() {
        let mut runtime_metadata = create_runtime_metadata();
        // Remove the correct scope tag and add a malformed one
        runtime_metadata.tags.retain(|tag| !tag.starts_with("scope:"));
        runtime_metadata.tags.push("scope:".to_string());  // Empty scope value
        
        // Convert runtime -> wallet
        let wallet_metadata = runtime_to_wallet_metadata(&runtime_metadata);
        
        // Verify scope is empty string
        assert_eq!(wallet_metadata.scope, Some("".to_string()));
        
        // Convert wallet -> runtime
        let round_trip_metadata = wallet_to_runtime_metadata(&wallet_metadata);
        
        // Verify the scope tag was preserved
        assert!(round_trip_metadata.tags.contains(&"scope:".to_string()));
    }

    #[test]
    fn test_duplicate_scope_tags() {
        let mut runtime_metadata = create_runtime_metadata();
        // Add a second scope tag
        runtime_metadata.tags.push("scope:another-scope".to_string());
        
        // Convert runtime -> wallet
        let wallet_metadata = runtime_to_wallet_metadata(&runtime_metadata);
        
        // The first scope tag should be used
        assert_eq!(wallet_metadata.scope, Some("test-scope".to_string()));
        
        // Convert wallet -> runtime
        let round_trip_metadata = wallet_to_runtime_metadata(&wallet_metadata);
        
        // Both scope tags should be preserved (although this is potentially ambiguous)
        assert!(round_trip_metadata.tags.contains(&"scope:test-scope".to_string()));
        assert!(round_trip_metadata.tags.contains(&"scope:another-scope".to_string()));
    }

    #[test]
    fn test_legacy_metadata_conversion() {
        // Create legacy metadata
        let mut legacy_metadata = serde_json::Map::new();
        legacy_metadata.insert("sequence".to_string(), Value::Number(100.into()));
        legacy_metadata.insert("scope".to_string(), Value::String("legacy-scope".to_string()));
        legacy_metadata.insert("content_type".to_string(), Value::String("text/plain".to_string()));
        let tags = vec![
            Value::String("legacy".to_string()),
            Value::String("old-format".to_string())
        ];
        legacy_metadata.insert("tags".to_string(), Value::Array(tags));
        
        // Extract metadata using the legacy_to_wallet logic
        let sequence = legacy_metadata.get("sequence")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
            
        let scope = legacy_metadata.get("scope")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
            
        let content_type = legacy_metadata.get("content_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    
        let tags = legacy_metadata.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(Vec::new);
            
        let wallet_metadata = WalletDagNodeMetadata {
            sequence,
            scope,
            content_type,
            tags,
        };
        
        // Verify extracted values
        assert_eq!(wallet_metadata.sequence, 100);
        assert_eq!(wallet_metadata.scope, Some("legacy-scope".to_string()));
        assert_eq!(wallet_metadata.content_type, Some("text/plain".to_string()));
        assert_eq!(wallet_metadata.tags.len(), 2);
        assert!(wallet_metadata.tags.contains(&"legacy".to_string()));
        assert!(wallet_metadata.tags.contains(&"old-format".to_string()));
        
        // Convert to runtime metadata
        let runtime_metadata = wallet_to_runtime_metadata(&wallet_metadata);
        
        // Verify runtime metadata
        assert_eq!(runtime_metadata.sequence, 100);
        assert_eq!(runtime_metadata.content_type, Some("text/plain".to_string()));
        assert!(runtime_metadata.tags.contains(&"legacy".to_string()));
        assert!(runtime_metadata.tags.contains(&"old-format".to_string()));
        assert!(runtime_metadata.tags.contains(&"scope:legacy-scope".to_string()));
        
        // Convert back to wallet metadata
        let round_trip_wallet_metadata = runtime_to_wallet_metadata(&runtime_metadata);
        
        // Verify round-trip values
        assert_eq!(round_trip_wallet_metadata.sequence, 100);
        assert_eq!(round_trip_wallet_metadata.scope, Some("legacy-scope".to_string()));
        assert_eq!(round_trip_wallet_metadata.content_type, Some("text/plain".to_string()));
        assert!(round_trip_wallet_metadata.tags.contains(&"legacy".to_string()));
        assert!(round_trip_wallet_metadata.tags.contains(&"old-format".to_string()));
    }
} 