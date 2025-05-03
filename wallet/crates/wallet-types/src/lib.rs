/*!
# Wallet Types

This crate provides shared types between the ICN Wallet and Runtime.
It ensures compatibility and consistency across components.
*/

mod dag;
mod error;
mod time;
mod network;

// Export key structures
pub use dag::{DagNode, DagNodeMetadata, DagThread};
pub use error::{WalletError, WalletResult};
pub use network::NodeSubmissionResponse;

// Export time utilities
pub use time::{system_time_to_datetime, datetime_to_system_time};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use chrono::{DateTime, Utc};
    use cid::Cid;
    use serde_json::json;

    #[test]
    fn test_time_conversions() {
        let now_system = SystemTime::now();
        let now_dt = system_time_to_datetime(now_system);
        let back_to_system = datetime_to_system_time(now_dt);

        // Allow for minor precision loss in conversion
        let diff = now_system.duration_since(back_to_system)
            .or_else(|_| back_to_system.duration_since(now_system))
            .unwrap_or_default();
        
        assert!(diff.as_millis() < 2, "Time conversion should be nearly lossless");
    }

    #[test]
    fn test_dag_node_serialization() {
        let cid = Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
        
        let node = DagNode {
            cid: cid.to_string(),
            parents: vec![],
            issuer: "did:icn:test".to_string(),
            timestamp: SystemTime::now(),
            signature: vec![1, 2, 3, 4],
            payload: json!({"test": "value"}).to_string().into_bytes(),
            metadata: DagNodeMetadata {
                sequence: Some(1),
                scope: Some("test".to_string()),
            },
        };

        let serialized = serde_json::to_string(&node).unwrap();
        let deserialized: DagNode = serde_json::from_str(&serialized).unwrap();

        assert_eq!(node.cid, deserialized.cid);
        assert_eq!(node.issuer, deserialized.issuer);
        assert_eq!(node.metadata.sequence, deserialized.metadata.sequence);
    }
} 