//! # ICN Wallet Actions
//! 
//! The `icn-wallet-actions` crate provides a standardized way to define, execute and track 
//! wallet operations in the ICN ecosystem. It offers a flexible system for representing and 
//! managing different types of actions performed within the wallet.
//! 
//! ## Features
//! 
//! - **Action Types**: Predefined action types for common wallet operations like credential 
//!   issuance, proposal creation, and DAG node management.
//! - **Action Status Tracking**: Track the status of actions through their lifecycle 
//!   (Pending, Processing, Completed, Failed).
//! - **Result Storage**: Store and retrieve action results for auditing and reference.
//! - **Action History**: Maintain a history of all actions performed by the wallet for compliance
//!   and troubleshooting.
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use icn_wallet_actions::{ActionManager, ActionType, ActionStatus};
//! use icn_wallet_storage::StorageManager;
//! 
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize the storage manager
//!     let storage = StorageManager::new("wallet_data").await?;
//!     
//!     // Create an action manager
//!     let action_manager = ActionManager::new(storage);
//!     
//!     // Create a new action
//!     let action_id = action_manager.create_action(
//!         ActionType::CreateCredential,
//!         Some("Creating user credential"),
//!         None,
//!     ).await?;
//!     
//!     // Update action status
//!     action_manager.update_action_status(&action_id, ActionStatus::Processing).await?;
//!     
//!     // Store action result
//!     action_manager.complete_action(&action_id, serde_json::json!({
//!         "credential_id": "cred123",
//!         "status": "issued"
//!     })).await?;
//!     
//!     // Retrieve action history
//!     let history = action_manager.get_action_history().await?;
//!     
//!     Ok(())
//! }
//! ```

// Module imports and code
pub mod error;
pub mod types;
pub mod manager;

// Re-exports for convenience
pub use error::{ActionError, ActionResult};
pub use types::{ActionType, ActionStatus, Action, ActionRecord};
pub use manager::ActionManager; 