use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use cid::Cid;

/// Types of events that can be anchored in the DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DagEvent {
    /// Represents a reputation snapshot for mesh peers
    ReputationSnapshot {
        /// Map of peer DID to reputation score
        scores: HashMap<String, f64>,
        /// Timestamp of the snapshot
        timestamp: DateTime<Utc>,
    },
    
    /// Federation lifecycle event
    FederationLifecycle {
        /// Federation DID
        federation_did: String,
        /// Event type
        event_type: String,
        /// Additional data
        data: serde_json::Value,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Governance proposal event
    GovernanceProposal {
        /// Proposal ID
        proposal_id: String,
        /// Status
        status: String,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Mesh computation receipt anchored to the DAG
    MeshReceiptAnchored {
        /// Content ID of the execution receipt
        receipt_cid: Cid,
        /// Worker DID
        worker_did: String,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Escrow contract event
    EscrowContract {
        /// Contract ID
        contract_id: String,
        /// Event type (created, updated, etc.)
        event_type: String,
        /// Content ID of the contract data
        contract_cid: Cid,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Escrow state change event
    EscrowStateChanged {
        /// Contract ID
        contract_id: String,
        /// New status
        status: String,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Reward distribution event
    RewardDistributed {
        /// Contract ID
        contract_id: String,
        /// Worker DID
        worker_did: String,
        /// Amount distributed
        amount: u64,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Escrow dispute event
    EscrowDispute {
        /// Contract ID
        contract_id: String,
        /// Disputer DID
        disputer_did: String,
        /// Dispute reason
        reason: String,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
} 