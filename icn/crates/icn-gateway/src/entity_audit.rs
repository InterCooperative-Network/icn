//! Entity Audit Logging
//!
//! Provides persistent audit logging for entity lifecycle operations. Every mutation
//! to an entity (registration, updates, deletion, membership changes) is recorded
//! with full context for compliance and security auditing.
//!
//! ## Design
//!
//! Follows the treasury audit pattern from `icn-ledger`:
//! - Storage prefix: `gateway:entity:audit:{entity_id}:{timestamp}:{id}`
//! - Reverse pagination for efficient "most recent first" queries
//! - Builder pattern for optional fields
//!
//! ## Example
//!
//! ```rust,ignore
//! use icn_gateway::entity_audit::{EntityAuditManager, EntityOperation};
//! use icn_entity::EntityId;
//!
//! let mut audit_mgr = EntityAuditManager::new(store);
//!
//! // Record entity registration
//! audit_mgr.record_audit(
//!     &entity_id,
//!     EntityOperation::Registered {
//!         entity_type: "cooperative".to_string(),
//!         name: "Food Co-op".to_string(),
//!     },
//!     &performer_id,
//!     None,
//!     None,
//! )?;
//!
//! // Query audit trail
//! let trail = audit_mgr.get_audit_trail(&entity_id, 50, 0)?;
//! ```

use anyhow::Result;
use icn_entity::{EntityId, MembershipRole};
use icn_obs::metrics::gateway as gateway_metrics;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// Storage key prefix for entity audit records
const ENTITY_AUDIT_PREFIX: &str = "gateway:entity:audit:";

/// Operation types for entity audit logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityOperation {
    /// Entity was registered/created
    Registered {
        /// Type of entity (cooperative, federation, individual)
        entity_type: String,
        /// Entity name
        name: String,
    },

    /// Entity metadata was updated
    Updated {
        /// List of fields that were changed
        changed_fields: Vec<String>,
    },

    /// Entity was deleted
    Deleted,

    /// Entity was suspended
    Suspended {
        /// Reason for suspension
        reason: String,
    },

    /// Entity was reactivated
    Activated,

    /// A member was added to the entity
    MemberAdded {
        /// The member that was added
        member_id: EntityId,
        /// Role assigned to the member
        role: MembershipRole,
    },

    /// A member was removed from the entity
    MemberRemoved {
        /// The member that was removed
        member_id: EntityId,
        /// Optional reason for removal
        reason: Option<String>,
    },

    /// A member's role or status was updated
    MemberUpdated {
        /// The member that was updated
        member_id: EntityId,
        /// List of changes made
        changes: Vec<String>,
    },
}

impl EntityOperation {
    /// Get a short operation name for metrics and logging
    pub fn operation_name(&self) -> &'static str {
        match self {
            Self::Registered { .. } => "registered",
            Self::Updated { .. } => "updated",
            Self::Deleted => "deleted",
            Self::Suspended { .. } => "suspended",
            Self::Activated => "activated",
            Self::MemberAdded { .. } => "member_added",
            Self::MemberRemoved { .. } => "member_removed",
            Self::MemberUpdated { .. } => "member_updated",
        }
    }
}

/// Audit record for entity operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityAuditRecord {
    /// Unique audit record ID
    pub id: String,

    /// Entity that was affected
    pub entity_id: EntityId,

    /// Operation performed
    pub operation: EntityOperation,

    /// Who performed the operation
    pub performed_by: EntityId,

    /// When performed (Unix timestamp in seconds, matching treasury audit pattern)
    pub performed_at: u64,

    /// Millisecond timestamp for storage ordering.
    /// Ensures proper chronological ordering for operations within the same second.
    /// Internal implementation detail; use `performed_at` for display.
    #[serde(default)]
    storage_order_millis: u64,

    /// Governance proposal ID (if operation required approval)
    pub proposal_id: Option<String>,

    /// Additional notes
    pub notes: Option<String>,
}

impl EntityAuditRecord {
    /// Create a new audit record
    ///
    /// Uses second-precision timestamps in `performed_at` for API consistency,
    /// but uses millisecond-precision internally for proper storage ordering.
    pub fn new(entity_id: EntityId, operation: EntityOperation, performed_by: EntityId) -> Self {
        let now_millis = icn_time::current_timestamp_millis();
        let now_secs = now_millis / 1000;
        Self {
            // Format matches treasury pattern: "audit-{timestamp}-{uuid}"
            id: format!("audit-{}-{}", now_secs, uuid::Uuid::new_v4().simple()),
            entity_id,
            operation,
            performed_by,
            performed_at: now_secs,
            storage_order_millis: now_millis,
            proposal_id: None,
            notes: None,
        }
    }

    /// Add proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    /// Add notes
    pub fn with_notes(mut self, notes: String) -> Self {
        self.notes = Some(notes);
        self
    }
}

/// Paginated audit trail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedEntityAuditTrail {
    /// Audit records for the current page
    pub records: Vec<EntityAuditRecord>,
    /// Total number of records (for pagination UI)
    pub total: usize,
    /// Current offset
    pub offset: usize,
    /// Page size limit
    pub limit: usize,
}

/// Manager for entity audit records with persistent storage
pub struct EntityAuditManager {
    store: Arc<dyn Store>,
}

impl EntityAuditManager {
    /// Create a new audit manager with the given store
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Record an audit event for an entity operation
    ///
    /// # Arguments
    /// * `entity_id` - The entity being operated on
    /// * `operation` - The operation being performed
    /// * `performed_by` - Who performed the operation
    /// * `proposal_id` - Optional governance proposal that authorized this
    /// * `notes` - Optional additional context
    ///
    /// # Returns
    /// The created audit record
    pub fn record_audit(
        &self,
        entity_id: &EntityId,
        operation: EntityOperation,
        performed_by: &EntityId,
        proposal_id: Option<String>,
        notes: Option<String>,
    ) -> Result<EntityAuditRecord> {
        let mut record = EntityAuditRecord::new(entity_id.clone(), operation, performed_by.clone());

        if let Some(proposal) = proposal_id {
            record = record.with_proposal(proposal);
        }
        if let Some(n) = notes {
            record = record.with_notes(n);
        }

        info!(
            audit_id = %record.id,
            entity_id = %entity_id,
            operation = record.operation.operation_name(),
            performed_by = %performed_by,
            "Recording entity audit"
        );

        // Record metric
        gateway_metrics::entity_audit_record_inc(record.operation.operation_name());

        // Persist the record
        self.persist_audit_record(&record)?;

        Ok(record)
    }

    /// Get audit trail for an entity with pagination
    ///
    /// Returns a `PaginatedEntityAuditTrail` containing records and total count.
    /// Uses reverse iteration for efficiency - only loads the requested records
    /// instead of loading all records into memory for sorting.
    ///
    /// Note: Records are returned most recent first.
    pub fn get_audit_trail(
        &self,
        entity_id: &EntityId,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedEntityAuditTrail> {
        let start = std::time::Instant::now();

        let prefix = format!("{ENTITY_AUDIT_PREFIX}{entity_id}");

        // Use optimized reverse pagination - only loads requested records
        let (pairs, total) = self
            .store
            .scan_reverse_paginated(prefix.as_bytes(), offset, limit)?;

        let records: Vec<EntityAuditRecord> = pairs
            .into_iter()
            .filter_map(|(key, value)| {
                serde_json::from_slice(&value)
                    .map_err(|e| {
                        // Track corruption for operational alerting
                        gateway_metrics::entity_audit_corruption_inc();
                        tracing::error!(
                            key = ?String::from_utf8_lossy(&key),
                            error = %e,
                            "Corrupted audit record detected - data integrity issue"
                        );
                        e
                    })
                    .ok()
            })
            .collect();

        // Record query duration metric
        gateway_metrics::entity_audit_query_duration(start.elapsed().as_secs_f64());

        Ok(PaginatedEntityAuditTrail {
            records,
            total,
            offset,
            limit,
        })
    }

    /// Persist an audit record to storage
    fn persist_audit_record(&self, record: &EntityAuditRecord) -> Result<()> {
        // Key uses millisecond timestamp for proper sub-second ordering.
        // Records within the same second will sort chronologically.
        let key = format!(
            "{}{}:{}:{}",
            ENTITY_AUDIT_PREFIX, record.entity_id, record.storage_order_millis, record.id
        );
        let value = serde_json::to_vec(record)?;
        self.store.put(key.as_bytes(), &value)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use tempfile::TempDir;

    fn create_test_manager() -> (EntityAuditManager, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = Arc::new(
            SledStore::open(temp_dir.path().to_str().expect("Invalid path"))
                .expect("Failed to create store"),
        );
        (EntityAuditManager::new(store), temp_dir)
    }

    #[test]
    fn test_record_and_retrieve_audit() {
        let (mgr, _temp) = create_test_manager();

        let entity_id = EntityId::cooperative("test-coop").expect("valid coop id");
        let performer_keypair = KeyPair::generate().expect("keypair");
        let performer = EntityId::from_did(performer_keypair.did());

        // Record a registration audit
        let record = mgr
            .record_audit(
                &entity_id,
                EntityOperation::Registered {
                    entity_type: "cooperative".to_string(),
                    name: "Test Co-op".to_string(),
                },
                &performer,
                None,
                Some("Initial registration".to_string()),
            )
            .expect("Failed to record audit");

        assert!(record.id.starts_with("audit-"));
        assert_eq!(record.entity_id, entity_id);
        assert_eq!(record.performed_by, performer);
        assert_eq!(record.notes.as_deref(), Some("Initial registration"));

        // Retrieve the audit trail
        let trail = mgr
            .get_audit_trail(&entity_id, 10, 0)
            .expect("Failed to get audit trail");

        assert_eq!(trail.records.len(), 1);
        assert_eq!(trail.total, 1);
        assert_eq!(trail.records[0].id, record.id);
    }

    #[test]
    fn test_audit_pagination() {
        let (mgr, _temp) = create_test_manager();

        let entity_id = EntityId::cooperative("test-coop").expect("valid coop id");
        let performer_keypair = KeyPair::generate().expect("keypair");
        let performer = EntityId::from_did(performer_keypair.did());

        // Create 5 audit records
        for i in 0..5 {
            mgr.record_audit(
                &entity_id,
                EntityOperation::Updated {
                    changed_fields: vec![format!("field{}", i)],
                },
                &performer,
                None,
                None,
            )
            .expect("Failed to record audit");
        }

        // Get first page
        let page1 = mgr
            .get_audit_trail(&entity_id, 2, 0)
            .expect("Failed to get page 1");
        assert_eq!(page1.records.len(), 2);
        assert_eq!(page1.total, 5);

        // Get second page
        let page2 = mgr
            .get_audit_trail(&entity_id, 2, 2)
            .expect("Failed to get page 2");
        assert_eq!(page2.records.len(), 2);
        assert_eq!(page2.total, 5);

        // Get last page (partial)
        let page3 = mgr
            .get_audit_trail(&entity_id, 2, 4)
            .expect("Failed to get page 3");
        assert_eq!(page3.records.len(), 1);
        assert_eq!(page3.total, 5);
    }

    #[test]
    fn test_operation_names() {
        let test_keypair = KeyPair::generate().expect("keypair");
        let test_member = EntityId::from_did(test_keypair.did());

        assert_eq!(
            EntityOperation::Registered {
                entity_type: "coop".to_string(),
                name: "Test".to_string()
            }
            .operation_name(),
            "registered"
        );
        assert_eq!(EntityOperation::Deleted.operation_name(), "deleted");
        assert_eq!(
            EntityOperation::MemberAdded {
                member_id: test_member,
                role: MembershipRole::Member,
            }
            .operation_name(),
            "member_added"
        );
    }

    #[test]
    fn test_audit_with_proposal() {
        let (mgr, _temp) = create_test_manager();

        let entity_id = EntityId::cooperative("test-coop").expect("valid coop id");
        let performer_keypair = KeyPair::generate().expect("keypair");
        let performer = EntityId::from_did(performer_keypair.did());

        let record = mgr
            .record_audit(
                &entity_id,
                EntityOperation::Deleted,
                &performer,
                Some("prop-123".to_string()),
                Some("Governance approved deletion".to_string()),
            )
            .expect("Failed to record audit");

        assert_eq!(record.proposal_id.as_deref(), Some("prop-123"));

        // Verify persisted
        let trail = mgr
            .get_audit_trail(&entity_id, 10, 0)
            .expect("Failed to get audit trail");
        assert_eq!(trail.records[0].proposal_id.as_deref(), Some("prop-123"));
    }

    #[test]
    fn test_audit_record_ordering() {
        let (mgr, _temp) = create_test_manager();

        let entity_id = EntityId::cooperative("test-coop").expect("valid coop id");
        let performer_keypair = KeyPair::generate().expect("keypair");
        let performer = EntityId::from_did(performer_keypair.did());

        // Create 3 records with small delays to ensure distinct timestamps
        let mut ids = Vec::new();
        for i in 0..3 {
            // Sleep to ensure distinct timestamps (seconds precision)
            std::thread::sleep(std::time::Duration::from_millis(1100));
            let record = mgr
                .record_audit(
                    &entity_id,
                    EntityOperation::Updated {
                        changed_fields: vec![format!("field{}", i)],
                    },
                    &performer,
                    None,
                    None,
                )
                .expect("Failed to record audit");
            ids.push(record.id.clone());
        }

        // Verify reverse chronological order (most recent first)
        let trail = mgr
            .get_audit_trail(&entity_id, 10, 0)
            .expect("Failed to get audit trail");
        assert_eq!(trail.records.len(), 3, "Should have 3 records");
        assert_eq!(trail.records[0].id, ids[2], "Most recent should be first");
        assert_eq!(trail.records[1].id, ids[1], "Second most recent");
        assert_eq!(trail.records[2].id, ids[0], "Oldest should be last");

        // Verify timestamps are descending (most recent first)
        assert!(
            trail.records[0].performed_at >= trail.records[1].performed_at,
            "Timestamps should be descending"
        );
        assert!(
            trail.records[1].performed_at >= trail.records[2].performed_at,
            "Timestamps should be descending"
        );
    }
}
