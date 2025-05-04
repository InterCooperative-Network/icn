//! Common types shared between wallet components

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

pub mod error;
pub mod action;
pub mod network;
pub mod dag;

/// Re-exports
pub use error::{SharedError, SharedResult};
pub use action::{ActionType, ActionStatus};
pub use network::{NetworkStatus, NodeSubmissionResponse};
pub use dag::DagNode;
pub use dag::DagThread;

/// Trust bundle for federation governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    pub id: String,
    pub epoch: u64,
    pub guardians: Vec<String>,
    pub members: Vec<String>,
    pub policies: HashMap<String, String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Bundle expiration timestamp (optional)
    pub valid_until: Option<SystemTime>,
    /// Federation ID
    pub federation_id: String,
    /// Version number
    pub version: u32,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Whether this bundle is active
    pub active: bool,
    /// Signature threshold
    pub threshold: u32,
    /// Signatures map
    pub signatures: HashMap<String, String>,
    /// Links to related resources
    #[serde(default)]
    pub links: HashMap<String, String>,
}
