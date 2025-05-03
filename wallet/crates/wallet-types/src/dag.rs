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
        
        // Try to parse payload as JSON first
        let payload = match serde_json::from_slice::<serde_json::Value>(&node.payload) {
            Ok(json_value) => {
                // JSON parsing successful, convert to Ipld
                json_to_ipld(json_value)
            },
            Err(_) => {
                // Not valid JSON, treat as binary data
                Ipld::Bytes(node.payload.clone())
            }
        };
        
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
        
        // Convert payload based on IPLD type
        let payload_bytes = match &runtime_node.payload {
            Ipld::Bytes(bytes) => {
                // Direct binary data
                bytes.clone()
            },
            ipld => {
                // Try to convert to JSON
                ipld_to_json_bytes(ipld)
                    .map_err(|e| format!("Failed to convert IPLD to JSON: {}", e))?
            }
        };
        
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use cid::Cid;
        use libipld::Ipld;

        #[test]
        fn test_binary_roundtrip_conversion() {
            // Create a test CID
            let cid_str = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";
            let cid = Cid::try_from(cid_str).unwrap();
            
            // Create binary data that is not valid UTF-8 or JSON
            let binary_data = vec![
                0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, // JPEG header
                0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x48,
                0x00, 0x48, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 
                // Random binary data
                0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0
            ];
            
            // Create a wallet DagNode with binary payload
            let wallet_node = DagNode {
                cid: cid_str.to_string(),
                parents: vec![],
                issuer: "did:icn:test".to_string(),
                timestamp: SystemTime::now(),
                signature: vec![1, 2, 3, 4],
                payload: binary_data.clone(),
                metadata: DagNodeMetadata {
                    sequence: Some(1),
                    scope: Some("test".to_string()),
                },
            };
            
            // Convert to runtime node
            let runtime_node = to_runtime_dag_node(&wallet_node).expect("Conversion to runtime node failed");
            
            // Verify payload in runtime node is correctly stored as Ipld::Bytes
            match &runtime_node.payload {
                Ipld::Bytes(bytes) => {
                    assert_eq!(bytes, &binary_data, "Binary data should be preserved exactly");
                },
                _ => panic!("Expected Ipld::Bytes for binary data"),
            }
            
            // Convert back to wallet node
            let wallet_node2 = from_runtime_dag_node(&runtime_node, cid).expect("Conversion back to wallet node failed");
            
            // Verify roundtrip payload is preserved
            assert_eq!(wallet_node.payload, wallet_node2.payload, "Binary payload should be preserved in roundtrip");
            
            // Also verify other fields
            assert_eq!(wallet_node.issuer, wallet_node2.issuer);
            assert_eq!(wallet_node.metadata.sequence, wallet_node2.metadata.sequence);
            assert_eq!(wallet_node.metadata.scope, wallet_node2.metadata.scope);
        }

        #[test]
        fn test_non_json_handling() {
            // Create payload that looks like JSON but isn't valid
            let invalid_json = b"{not valid JSON but has curly braces}";
            
            // Create a wallet node with this payload
            let wallet_node = DagNode {
                cid: "test-invalid-json".to_string(),
                parents: vec![],
                issuer: "did:icn:test".to_string(),
                timestamp: SystemTime::now(),
                signature: vec![1, 2, 3, 4],
                payload: invalid_json.to_vec(),
                metadata: DagNodeMetadata::default(),
            };
            
            // This should not panic, but convert cleanly to Ipld::Bytes
            let runtime_node = to_runtime_dag_node(&wallet_node).expect("Should handle invalid JSON gracefully");
            
            // Verify it was treated as binary
            match &runtime_node.payload {
                Ipld::Bytes(bytes) => {
                    assert_eq!(bytes, &invalid_json.to_vec());
                },
                other => panic!("Expected Ipld::Bytes but got {:?}", other),
            }
        }

        #[test]
        fn test_ipld_bytes_to_wallet_payload() {
            // Create a runtime node with Ipld::Bytes payload
            let binary_data = vec![0x01, 0x02, 0x03, 0xF0, 0xFF];
            let runtime_payload = Ipld::Bytes(binary_data.clone());
            
            // Construct a minimal runtime node (using internals directly for test)
            let runtime_node = icn_dag::DagNode {
                issuer: icn_identity::IdentityId::new("did:icn:test"),
                parents: vec![],
                payload: runtime_payload,
                signature: vec![9, 8, 7, 6],
                metadata: icn_dag::DagNodeMetadata {
                    timestamp: 12345,
                    sequence: Some(1),
                    scope: Some("test".to_string()),
                },
            };
            
            // Convert to wallet node
            let cid = Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
            let wallet_node = from_runtime_dag_node(&runtime_node, cid).expect("Conversion failed");
            
            // Verify binary data is preserved
            assert_eq!(wallet_node.payload, binary_data);
        }
        
        #[test]
        fn test_extreme_binary_edge_cases() {
            // Test with empty payload
            let empty_payload = vec![];
            let wallet_node_empty = DagNode {
                cid: "empty-payload".to_string(),
                parents: vec![],
                issuer: "did:icn:test".to_string(),
                timestamp: SystemTime::now(),
                signature: vec![1, 2, 3, 4],
                payload: empty_payload.clone(),
                metadata: DagNodeMetadata::default(),
            };
            
            let runtime_node_empty = to_runtime_dag_node(&wallet_node_empty).expect("Empty payload conversion failed");
            match &runtime_node_empty.payload {
                Ipld::Bytes(bytes) => {
                    assert_eq!(bytes.len(), 0, "Empty payload should remain empty");
                },
                _ => panic!("Expected Ipld::Bytes for empty payload"),
            }
            
            // Test with large binary payload (1MB of random data)
            // This simulates a large file or blob
            let large_payload = vec![0xAA; 1_000_000]; // 1MB of 0xAA bytes
            let wallet_node_large = DagNode {
                cid: "large-payload".to_string(),
                parents: vec![],
                issuer: "did:icn:test".to_string(),
                timestamp: SystemTime::now(),
                signature: vec![1, 2, 3, 4],
                payload: large_payload.clone(),
                metadata: DagNodeMetadata::default(),
            };
            
            let runtime_node_large = to_runtime_dag_node(&wallet_node_large).expect("Large payload conversion failed");
            match &runtime_node_large.payload {
                Ipld::Bytes(bytes) => {
                    assert_eq!(bytes.len(), 1_000_000, "Large payload should preserve size");
                    assert_eq!(bytes[0], 0xAA);
                    assert_eq!(bytes[999_999], 0xAA);
                },
                _ => panic!("Expected Ipld::Bytes for large payload"),
            }
            
            // Test with null bytes and control characters
            let control_chars = vec![
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 
                0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
                0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
                0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F
            ];
            
            let wallet_node_control = DagNode {
                cid: "control-chars".to_string(),
                parents: vec![],
                issuer: "did:icn:test".to_string(),
                timestamp: SystemTime::now(),
                signature: vec![1, 2, 3, 4],
                payload: control_chars.clone(),
                metadata: DagNodeMetadata::default(),
            };
            
            let runtime_node_control = to_runtime_dag_node(&wallet_node_control).expect("Control chars conversion failed");
            let cid = Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
            let wallet_node_control2 = from_runtime_dag_node(&runtime_node_control, cid).expect("Round-trip conversion failed");
            
            assert_eq!(wallet_node_control.payload, wallet_node_control2.payload, 
                      "Control characters should be preserved exactly");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use serde_json::{json, Value};

    #[test]
    fn test_dag_node_creation() {
        let node = DagNode {
            cid: "test-cid".to_string(),
            parents: vec!["parent1".to_string(), "parent2".to_string()],
            issuer: "did:icn:test".to_string(),
            timestamp: SystemTime::now(),
            signature: vec![1, 2, 3, 4],
            payload: vec![10, 20, 30, 40, 50],
            metadata: DagNodeMetadata {
                sequence: Some(1),
                scope: Some("test".to_string()),
            },
        };

        assert_eq!(node.cid, "test-cid");
        assert_eq!(node.parents.len(), 2);
        assert_eq!(node.issuer, "did:icn:test");
        assert_eq!(node.metadata.sequence, Some(1));
        assert_eq!(node.payload, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_dag_node_serialization_deserialization() {
        let original = DagNode {
            cid: "test-cid".to_string(),
            parents: vec!["parent1".to_string()],
            issuer: "did:icn:test".to_string(),
            timestamp: SystemTime::now(),
            signature: vec![1, 2, 3, 4],
            payload: vec![10, 20, 30, 40, 50],
            metadata: DagNodeMetadata {
                sequence: Some(1),
                scope: Some("test".to_string()),
            },
        };

        // Serialize to JSON
        let serialized = serde_json::to_string(&original).expect("Serialization failed");
        
        // Deserialize back to DagNode
        let deserialized: DagNode = serde_json::from_str(&serialized).expect("Deserialization failed");
        
        // Assert fields match
        assert_eq!(original.cid, deserialized.cid);
        assert_eq!(original.parents, deserialized.parents);
        assert_eq!(original.issuer, deserialized.issuer);
        assert_eq!(original.signature, deserialized.signature);
        assert_eq!(original.payload, deserialized.payload);
        assert_eq!(original.metadata.sequence, deserialized.metadata.sequence);
        assert_eq!(original.metadata.scope, deserialized.metadata.scope);
    }

    #[test]
    fn test_json_serialization() {
        // Test JSON payload handling
        let json_value = json!({
            "name": "Test Node",
            "values": [1, 2, 3],
            "nested": {
                "key": "value"
            }
        });

        let json_bytes = serde_json::to_vec(&json_value).expect("JSON serialization failed");
        
        let node = DagNode {
            cid: "json-test".to_string(),
            parents: vec![],
            issuer: "did:icn:test".to_string(),
            timestamp: SystemTime::now(),
            signature: vec![1, 2, 3, 4],
            payload: json_bytes.clone(),
            metadata: DagNodeMetadata::default(),
        };

        // Verify payload can be parsed as JSON
        let payload_json = node.payload_as_json().expect("JSON parsing failed");
        assert_eq!(payload_json["name"], "Test Node");
        assert_eq!(payload_json["values"], json!([1, 2, 3]));
        assert_eq!(payload_json["nested"]["key"], "value");
        
        // Test round-trip serialization
        let serialized = serde_json::to_string(&node).expect("Serialization failed");
        let deserialized: DagNode = serde_json::from_str(&serialized).expect("Deserialization failed");
        
        assert_eq!(node.payload, deserialized.payload);
        // Verify the JSON is still valid after round trip
        let payload_json2 = deserialized.payload_as_json().expect("JSON parsing failed");
        assert_eq!(payload_json, payload_json2);
    }

    #[test]
    fn test_binary_data_handling() {
        // Test with arbitrary binary data (not valid JSON or UTF-8)
        let binary_data = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, // JPEG header
            0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x48,
            0x00, 0x48, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 
            // Random binary data follows
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0
        ];
        
        let node = DagNode {
            cid: "binary-test".to_string(),
            parents: vec![],
            issuer: "did:icn:test".to_string(),
            timestamp: SystemTime::now(),
            signature: vec![9, 8, 7, 6, 5],
            payload: binary_data.clone(),
            metadata: DagNodeMetadata {
                sequence: Some(42),
                scope: Some("binary-test".to_string()),
            },
        };
        
        // Serialize the node (with binary data)
        let serialized = serde_json::to_string(&node).expect("Serialization failed");
        
        // Deserialize
        let deserialized: DagNode = serde_json::from_str(&serialized).expect("Deserialization failed");
        
        // Verify binary content is preserved exactly
        assert_eq!(binary_data, deserialized.payload);
        
        // JSON parsing of binary data should fail
        let result = deserialized.payload_as_json();
        assert!(result.is_err(), "Binary data should not parse as valid JSON");
    }
} 