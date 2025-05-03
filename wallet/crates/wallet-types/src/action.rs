use serde::{Deserialize, Serialize};

/// Types of actions that can be performed by wallet components
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// Create a new item
    Create,
    
    /// Update an existing item
    Update,
    
    /// Delete an item
    Delete,
    
    /// Submit data to the network
    Submit,
    
    /// Query or fetch data
    Query,
    
    /// Synchronize data
    Sync,
    
    /// Import data
    Import,
    
    /// Export data
    Export,
    
    /// Sign data
    Sign,
    
    /// Verify signature
    Verify,
    
    /// Approve an action
    Approve,
    
    /// Reject an action
    Reject,
    
    /// Proposal action for governance
    Proposal,
    
    /// Vote action for governance
    Vote,
    
    /// Anchor action for data anchoring
    Anchor,
    
    /// Custom action type
    Custom,
}

/// Status of an action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    /// Action is pending
    Pending,
    
    /// Action is in progress
    InProgress,
    
    /// Action completed successfully
    Completed,
    
    /// Action failed
    Failed,
    
    /// Action was cancelled
    Cancelled,
    
    /// Action requires approval
    RequiresApproval,
    
    /// Action is expired
    Expired,
    
    /// Action is being processed
    Processing,
} 