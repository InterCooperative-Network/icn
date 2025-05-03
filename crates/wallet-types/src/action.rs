use serde::{Serialize, Deserialize};

/// The type of action being performed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionType {
    /// Create a governance proposal
    Proposal,
    /// Vote on a proposal
    Vote,
    /// Anchor data to the DAG
    Anchor,
}

/// The status of a pending action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionStatus {
    /// The action is pending and waiting to be processed
    Pending,
    /// The action is currently being processed
    Processing,
    /// The action was successfully processed
    Completed,
    /// The action failed to process
    Failed,
} 