/*!
 * ICN Wallet Receipt Replay Verification
 *
 * Provides functionality for verifying execution receipts
 * by replaying them against the DAG.
 */

use thiserror::Error;
use chrono::{DateTime, Utc};
use crate::dag::{DagStorageManager, DagError};
use icn_wallet_agent::import::ExecutionReceipt;

/// Error types for replay verification
#[derive(Error, Debug)]
pub enum ReplayError {
    #[error("DAG error: {0}")]
    DagError(#[from] DagError),
    
    #[error("Missing DAG anchor in receipt")]
    MissingAnchor,
    
    #[error("DAG node not found: {0}")]
    NodeNotFound(String),
    
    #[error("Timestamp mismatch: receipt={0}, dag={1}")]
    TimestampMismatch(DateTime<Utc>, DateTime<Utc>),
    
    #[error("Proposal ID mismatch: receipt={0}, dag={1}")]
    ProposalIdMismatch(String, String),
    
    #[error("Outcome mismatch: receipt={0}, dag={1}")]
    OutcomeMismatch(String, String),
    
    #[error("Scope mismatch: receipt={0}, dag={1}")]
    ScopeMismatch(String, String),
    
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

/// Result of a receipt verification
pub struct VerificationResult {
    /// Status of the verification
    pub status: VerificationStatus,
    
    /// Detailed message about the verification
    pub message: String,
    
    /// Referenced DAG node CID
    pub dag_cid: Option<String>,
    
    /// Receipt proposal ID
    pub proposal_id: String,
}

/// Status of a receipt verification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Receipt verified successfully
    Verified,
    
    /// Receipt failed verification
    Failed,
    
    /// Receipt verification encountered an error
    Error,
}

/// Verify an execution receipt by replaying it against the DAG
pub async fn replay_and_verify_receipt(
    receipt: &ExecutionReceipt,
    dag_store: &impl DagStorageManager,
) -> Result<bool, ReplayError> {
    // Get the DAG anchor CID from the receipt
    let dag_cid = receipt.dag_anchor.as_ref()
        .ok_or(ReplayError::MissingAnchor)?;
    
    // Check if the DAG node exists
    if !dag_store.node_exists(dag_cid).await? {
        return Err(ReplayError::NodeNotFound(dag_cid.clone()));
    }
    
    // Get the DAG node metadata
    let metadata = dag_store.get_metadata(dag_cid).await?;
    
    // Get the DAG node
    let node = dag_store.get_node(dag_cid).await?;
    
    // Extract relevant information from the node
    let node_proposal_id = match &node.data {
        icn_dag::DagNodeData::ExecutionSummary(exec) => 
            exec.proposal_id.clone(),
        _ => return Err(ReplayError::VerificationFailed(
            "DAG node is not an execution summary".to_string()
        )),
    };
    
    // Check if the proposal ID matches
    if receipt.proposal_id != node_proposal_id {
        return Err(ReplayError::ProposalIdMismatch(
            receipt.proposal_id.clone(), 
            node_proposal_id
        ));
    }
    
    // Check if the scope matches
    if receipt.federation_scope != metadata.scope {
        return Err(ReplayError::ScopeMismatch(
            receipt.federation_scope.clone(),
            metadata.scope
        ));
    }
    
    // Check node timestamps (using a simple heuristic for now)
    if let Some(ts) = metadata.timestamp {
        if let Ok(receipt_ts) = DateTime::parse_from_rfc3339(&receipt.credential.issuance_date) {
            let receipt_utc = receipt_ts.with_timezone(&Utc);
            
            // Convert system time to UTC datetime for comparison
            let node_time = icn_wallet_sync::compat::system_time_to_datetime(ts);
            
            // This is a simplified check - in a real implementation we'd need more sophisticated logic
            // for timestamp comparison, potentially with a tolerance window
            if (receipt_utc - node_time).num_seconds().abs() > 3600 {
                return Err(ReplayError::TimestampMismatch(receipt_utc, node_time));
            }
        }
    }
    
    // Extract outcome from the node
    let node_outcome = match &node.data {
        icn_dag::DagNodeData::ExecutionSummary(exec) => {
            if exec.success { "Success" } else { "Failure" }
        },
        _ => return Err(ReplayError::VerificationFailed(
            "DAG node is not an execution summary".to_string()
        )),
    };
    
    // Check if the outcome matches
    if receipt.outcome != node_outcome {
        return Err(ReplayError::OutcomeMismatch(
            receipt.outcome.clone(),
            node_outcome.to_string()
        ));
    }
    
    // If we got here, the receipt verifies successfully
    Ok(true)
} 