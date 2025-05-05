/*!
# DAG Audit System

Provides audit logging for DAG operations to ensure traceability and security.
*/

use crate::DagNode;
use cid::Cid;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, VecDeque};
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{info, warn, debug, error};

/// Maximum number of audit records to keep in memory
const MAX_AUDIT_RECORDS: usize = 10000;

/// Error types for audit operations
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("Failed to record audit event: {0}")]
    RecordingFailed(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Result type for audit operations
pub type AuditResult<T> = Result<T, AuditError>;

/// Describes the type of operation being audited
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    /// Node creation or addition
    NodeCreated,
    
    /// Node content verified (e.g., signature check)
    NodeVerified,
    
    /// Node read operation
    NodeRead,
    
    /// Node data or metadata queried
    NodeQueried,
    
    /// Anchor to DAG root
    DagAnchor,
    
    /// Merkle proof verification
    MerkleVerification,
    
    /// Lineage attestation creation
    AttestationCreated,
    
    /// Federation sync operation
    FederationSync,
    
    /// Security-related operation
    SecurityEvent,
    
    /// Custom event type with string descriptor
    Custom(String),
}

/// Detailed audit record for a DAG operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// Unique identifier for this audit record
    pub id: String,
    
    /// Timestamp of when this operation occurred
    pub timestamp: DateTime<Utc>,
    
    /// The DID that performed the operation
    pub actor_did: String,
    
    /// The action performed
    pub action: AuditAction,
    
    /// CID of the node affected, if applicable
    pub node_cid: Option<Cid>,
    
    /// Entity DID this operation applies to
    pub entity_did: Option<String>,
    
    /// Success/failure status
    pub success: bool,
    
    /// Error message if the operation failed
    pub error_message: Option<String>,
    
    /// Additional context as JSON string
    pub context: Option<String>,
    
    /// Source information (e.g., IP address, client info)
    pub source_info: Option<String>,
    
    /// Request ID for correlation
    pub request_id: Option<String>,
}

impl AuditRecord {
    /// Create a new audit record builder
    pub fn builder() -> AuditRecordBuilder {
        AuditRecordBuilder::new()
    }
}

/// Builder for creating AuditRecord instances
pub struct AuditRecordBuilder {
    actor_did: Option<String>,
    action: Option<AuditAction>,
    node_cid: Option<Cid>,
    entity_did: Option<String>,
    success: Option<bool>,
    error_message: Option<String>,
    context: Option<String>,
    source_info: Option<String>,
    request_id: Option<String>,
}

impl AuditRecordBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            actor_did: None,
            action: None,
            node_cid: None,
            entity_did: None,
            success: Some(true), // Default to success
            error_message: None,
            context: None,
            source_info: None,
            request_id: None,
        }
    }
    
    /// Set the actor DID
    pub fn actor(mut self, actor_did: impl Into<String>) -> Self {
        self.actor_did = Some(actor_did.into());
        self
    }
    
    /// Set the action
    pub fn action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }
    
    /// Set the node CID
    pub fn node(mut self, cid: Cid) -> Self {
        self.node_cid = Some(cid);
        self
    }
    
    /// Set the entity DID
    pub fn entity(mut self, entity_did: impl Into<String>) -> Self {
        self.entity_did = Some(entity_did.into());
        self
    }
    
    /// Set success status
    pub fn success(mut self, success: bool) -> Self {
        self.success = Some(success);
        self
    }
    
    /// Set error message
    pub fn error(mut self, error_message: impl Into<String>) -> Self {
        self.error_message = Some(error_message.into());
        self.success = Some(false); // Setting error implies failure
        self
    }
    
    /// Set context
    pub fn context<T: Serialize>(mut self, context: &T) -> Self {
        if let Ok(json) = serde_json::to_string(context) {
            self.context = Some(json);
        }
        self
    }
    
    /// Set source info
    pub fn source(mut self, source_info: impl Into<String>) -> Self {
        self.source_info = Some(source_info.into());
        self
    }
    
    /// Set request ID
    pub fn request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
    
    /// Build the audit record
    pub fn build(self) -> AuditResult<AuditRecord> {
        let id = uuid::Uuid::new_v4().to_string();
        
        Ok(AuditRecord {
            id,
            timestamp: Utc::now(),
            actor_did: self.actor_did
                .ok_or_else(|| AuditError::RecordingFailed("Actor DID is required".to_string()))?,
            action: self.action
                .ok_or_else(|| AuditError::RecordingFailed("Action is required".to_string()))?,
            node_cid: self.node_cid,
            entity_did: self.entity_did,
            success: self.success.unwrap_or(true),
            error_message: self.error_message,
            context: self.context,
            source_info: self.source_info,
            request_id: self.request_id,
        })
    }
}

/// Interface for audit logging systems
#[async_trait::async_trait]
pub trait AuditLogger: Send + Sync {
    /// Record an audit event
    async fn record(&self, record: AuditRecord) -> AuditResult<()>;
    
    /// Get audit records for a specific entity
    async fn get_records_for_entity(&self, entity_did: &str, limit: usize) -> AuditResult<Vec<AuditRecord>>;
    
    /// Get audit records for a specific node
    async fn get_records_for_node(&self, node_cid: &Cid, limit: usize) -> AuditResult<Vec<AuditRecord>>;
    
    /// Get audit records for a specific actor
    async fn get_records_for_actor(&self, actor_did: &str, limit: usize) -> AuditResult<Vec<AuditRecord>>;
    
    /// Get all audit records
    async fn get_all_records(&self, limit: usize) -> AuditResult<Vec<AuditRecord>>;
    
    /// Subscribe to audit events
    fn subscribe(&self) -> AuditResult<broadcast::Receiver<AuditRecord>>;
}

/// In-memory implementation of AuditLogger
pub struct InMemoryAuditLogger {
    /// In-memory store of all audit records
    records: Mutex<VecDeque<AuditRecord>>,
    
    /// Index by entity DID
    entity_index: Mutex<HashMap<String, Vec<usize>>>,
    
    /// Index by node CID
    node_index: Mutex<HashMap<Cid, Vec<usize>>>,
    
    /// Index by actor DID
    actor_index: Mutex<HashMap<String, Vec<usize>>>,
    
    /// Broadcast channel for subscribers
    event_sender: broadcast::Sender<AuditRecord>,
}

impl InMemoryAuditLogger {
    /// Create a new in-memory audit logger
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        
        Self {
            records: Mutex::new(VecDeque::with_capacity(MAX_AUDIT_RECORDS)),
            entity_index: Mutex::new(HashMap::new()),
            node_index: Mutex::new(HashMap::new()),
            actor_index: Mutex::new(HashMap::new()),
            event_sender: sender,
        }
    }
}

#[async_trait::async_trait]
impl AuditLogger for InMemoryAuditLogger {
    async fn record(&self, record: AuditRecord) -> AuditResult<()> {
        // Clone for logging and broadcasting
        let record_clone = record.clone();
        
        // Log the event using tracing
        match &record.action {
            AuditAction::SecurityEvent => {
                if record.success {
                    info!(
                        actor = %record.actor_did,
                        node = ?record.node_cid,
                        entity = ?record.entity_did,
                        request_id = ?record.request_id,
                        "Security event: success"
                    );
                } else {
                    warn!(
                        actor = %record.actor_did,
                        node = ?record.node_cid,
                        entity = ?record.entity_did,
                        request_id = ?record.request_id,
                        error = ?record.error_message,
                        "Security event: failure"
                    );
                }
            },
            _ => {
                if record.success {
                    debug!(
                        action = ?record.action,
                        actor = %record.actor_did,
                        node = ?record.node_cid,
                        entity = ?record.entity_did,
                        "DAG operation: success"
                    );
                } else {
                    warn!(
                        action = ?record.action,
                        actor = %record.actor_did,
                        node = ?record.node_cid,
                        entity = ?record.entity_did,
                        error = ?record.error_message,
                        "DAG operation: failure"
                    );
                }
            }
        }
        
        // Store the record
        let mut records = self.records.lock().unwrap();
        
        // Add to indices
        let index = records.len();
        
        if let Some(entity_did) = &record.entity_did {
            let mut entity_index = self.entity_index.lock().unwrap();
            entity_index.entry(entity_did.clone()).or_default().push(index);
        }
        
        if let Some(node_cid) = &record.node_cid {
            let mut node_index = self.node_index.lock().unwrap();
            node_index.entry(*node_cid).or_default().push(index);
        }
        
        let mut actor_index = self.actor_index.lock().unwrap();
        actor_index.entry(record.actor_did.clone()).or_default().push(index);
        
        // Add to records
        records.push_back(record);
        
        // Keep records within size limit
        if records.len() > MAX_AUDIT_RECORDS {
            records.pop_front();
            
            // Adjust indices (this is inefficient but simple; a more complex solution would use a different data structure)
            let mut entity_index = self.entity_index.lock().unwrap();
            let mut node_index = self.node_index.lock().unwrap();
            let mut actor_index = self.actor_index.lock().unwrap();
            
            for indices in entity_index.values_mut() {
                *indices = indices.iter().filter_map(|&i| if i > 0 { Some(i - 1) } else { None }).collect();
            }
            
            for indices in node_index.values_mut() {
                *indices = indices.iter().filter_map(|&i| if i > 0 { Some(i - 1) } else { None }).collect();
            }
            
            for indices in actor_index.values_mut() {
                *indices = indices.iter().filter_map(|&i| if i > 0 { Some(i - 1) } else { None }).collect();
            }
        }
        
        // Broadcast the event to subscribers
        let _ = self.event_sender.send(record_clone); // Ignore errors if no receivers
        
        Ok(())
    }
    
    async fn get_records_for_entity(&self, entity_did: &str, limit: usize) -> AuditResult<Vec<AuditRecord>> {
        let entity_index = self.entity_index.lock().unwrap();
        let records = self.records.lock().unwrap();
        
        let indices = match entity_index.get(entity_did) {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };
        
        let result = indices.iter()
            .filter_map(|&i| records.get(i).cloned())
            .take(limit)
            .collect();
        
        Ok(result)
    }
    
    async fn get_records_for_node(&self, node_cid: &Cid, limit: usize) -> AuditResult<Vec<AuditRecord>> {
        let node_index = self.node_index.lock().unwrap();
        let records = self.records.lock().unwrap();
        
        let indices = match node_index.get(node_cid) {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };
        
        let result = indices.iter()
            .filter_map(|&i| records.get(i).cloned())
            .take(limit)
            .collect();
        
        Ok(result)
    }
    
    async fn get_records_for_actor(&self, actor_did: &str, limit: usize) -> AuditResult<Vec<AuditRecord>> {
        let actor_index = self.actor_index.lock().unwrap();
        let records = self.records.lock().unwrap();
        
        let indices = match actor_index.get(actor_did) {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };
        
        let result = indices.iter()
            .filter_map(|&i| records.get(i).cloned())
            .take(limit)
            .collect();
        
        Ok(result)
    }
    
    async fn get_all_records(&self, limit: usize) -> AuditResult<Vec<AuditRecord>> {
        let records = self.records.lock().unwrap();
        let result = records.iter().take(limit).cloned().collect();
        Ok(result)
    }
    
    fn subscribe(&self) -> AuditResult<broadcast::Receiver<AuditRecord>> {
        Ok(self.event_sender.subscribe())
    }
}

/// Audit log wrapper for DAG operations
/// This provides a convenient way to log DAG operations with proper context
pub struct AuditedOperation<'a, T: AuditLogger> {
    logger: &'a T,
    builder: AuditRecordBuilder,
}

impl<'a, T: AuditLogger> AuditedOperation<'a, T> {
    /// Create a new audited operation
    pub fn new(logger: &'a T, action: AuditAction, actor_did: impl Into<String>) -> Self {
        Self {
            logger,
            builder: AuditRecord::builder()
                .action(action)
                .actor(actor_did),
        }
    }
    
    /// Set the node CID
    pub fn with_node(mut self, cid: Cid) -> Self {
        self.builder = self.builder.node(cid);
        self
    }
    
    /// Set the entity DID
    pub fn with_entity(mut self, entity_did: impl Into<String>) -> Self {
        self.builder = self.builder.entity(entity_did);
        self
    }
    
    /// Set context
    pub fn with_context<C: Serialize>(mut self, context: &C) -> Self {
        self.builder = self.builder.context(context);
        self
    }
    
    /// Set source info
    pub fn with_source(mut self, source_info: impl Into<String>) -> Self {
        self.builder = self.builder.source(source_info);
        self
    }
    
    /// Set request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.builder = self.builder.request_id(request_id);
        self
    }
    
    /// Execute the operation and record the result
    pub async fn execute<F, R, E>(self, operation: F) -> Result<R, E>
    where
        F: FnOnce() -> Result<R, E>,
        E: std::fmt::Display,
    {
        // Execute the operation
        let result = operation();
        
        // Build the audit record based on the result
        let record = match &result {
            Ok(_) => self.builder.success(true).build(),
            Err(e) => self.builder.success(false).error(e.to_string()).build(),
        };
        
        // Record the audit event
        if let Ok(record) = record {
            let _ = self.logger.record(record).await; // Ignore errors in audit logging
        }
        
        // Return the original result
        result
    }
    
    /// Execute an async operation and record the result
    pub async fn execute_async<F, R, E>(self, operation: F) -> Result<R, E>
    where
        F: std::future::Future<Output = Result<R, E>>,
        E: std::fmt::Display,
    {
        // Execute the operation
        let result = operation.await;
        
        // Build the audit record based on the result
        let record = match &result {
            Ok(_) => self.builder.success(true).build(),
            Err(e) => self.builder.success(false).error(e.to_string()).build(),
        };
        
        // Record the audit event
        if let Ok(record) = record {
            let _ = self.logger.record(record).await; // Ignore errors in audit logging
        }
        
        // Return the original result
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;
    
    #[test]
    fn test_audit_record_builder() {
        let record = AuditRecord::builder()
            .actor("did:icn:alice")
            .action(AuditAction::NodeCreated)
            .entity("did:icn:coop1")
            .build()
            .unwrap();
        
        assert_eq!(record.actor_did, "did:icn:alice");
        assert!(matches!(record.action, AuditAction::NodeCreated));
        assert_eq!(record.entity_did, Some("did:icn:coop1".to_string()));
        assert!(record.success);
        assert!(record.error_message.is_none());
    }
    
    #[test]
    fn test_audit_record_with_error() {
        let record = AuditRecord::builder()
            .actor("did:icn:alice")
            .action(AuditAction::NodeVerified)
            .error("Signature verification failed")
            .build()
            .unwrap();
        
        assert_eq!(record.actor_did, "did:icn:alice");
        assert!(matches!(record.action, AuditAction::NodeVerified));
        assert!(!record.success);
        assert_eq!(record.error_message, Some("Signature verification failed".to_string()));
    }
    
    #[test]
    fn test_inmemory_audit_logger() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let logger = InMemoryAuditLogger::new();
            
            // Record some events
            let record1 = AuditRecord::builder()
                .actor("did:icn:alice")
                .action(AuditAction::NodeCreated)
                .entity("did:icn:coop1")
                .node(Cid::new_v1(0x71, cid::multihash::Code::Sha2_256.digest(b"node1")))
                .build()
                .unwrap();
                
            let record2 = AuditRecord::builder()
                .actor("did:icn:bob")
                .action(AuditAction::NodeRead)
                .entity("did:icn:coop1")
                .node(Cid::new_v1(0x71, cid::multihash::Code::Sha2_256.digest(b"node1")))
                .build()
                .unwrap();
                
            let record3 = AuditRecord::builder()
                .actor("did:icn:alice")
                .action(AuditAction::NodeCreated)
                .entity("did:icn:coop2")
                .node(Cid::new_v1(0x71, cid::multihash::Code::Sha2_256.digest(b"node2")))
                .build()
                .unwrap();
            
            // Store records
            logger.record(record1).await.unwrap();
            logger.record(record2).await.unwrap();
            logger.record(record3).await.unwrap();
            
            // Test queries
            let all_records = logger.get_all_records(10).await.unwrap();
            assert_eq!(all_records.len(), 3);
            
            let alice_records = logger.get_records_for_actor("did:icn:alice", 10).await.unwrap();
            assert_eq!(alice_records.len(), 2);
            
            let coop1_records = logger.get_records_for_entity("did:icn:coop1", 10).await.unwrap();
            assert_eq!(coop1_records.len(), 2);
            
            let node1_cid = Cid::new_v1(0x71, cid::multihash::Code::Sha2_256.digest(b"node1"));
            let node1_records = logger.get_records_for_node(&node1_cid, 10).await.unwrap();
            assert_eq!(node1_records.len(), 2);
        });
    }
    
    #[test]
    fn test_audited_operation() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let logger = InMemoryAuditLogger::new();
            
            // Test successful operation
            let result: Result<i32, String> = AuditedOperation::new(
                &logger, 
                AuditAction::NodeCreated,
                "did:icn:alice"
            )
            .with_entity("did:icn:coop1")
            .execute(|| Ok(42))
            .await;
            
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 42);
            
            // Test failed operation
            let result: Result<i32, String> = AuditedOperation::new(
                &logger, 
                AuditAction::NodeVerified,
                "did:icn:alice"
            )
            .with_entity("did:icn:coop1")
            .execute(|| Err("Verification failed".to_string()))
            .await;
            
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), "Verification failed");
            
            // Check that both operations were recorded
            let all_records = logger.get_all_records(10).await.unwrap();
            assert_eq!(all_records.len(), 2);
            
            // First record should be success
            assert!(all_records[0].success);
            assert!(matches!(all_records[0].action, AuditAction::NodeCreated));
            
            // Second record should be failure
            assert!(!all_records[1].success);
            assert!(matches!(all_records[1].action, AuditAction::NodeVerified));
            assert_eq!(all_records[1].error_message, Some("Verification failed".to_string()));
        });
    }
} 