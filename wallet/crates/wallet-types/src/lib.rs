//! Common types shared between wallet components

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

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
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub valid_until: Option<DateTime<Utc>>,
    /// Federation ID
    pub federation_id: String,
    /// Version number
    pub version: u32,
    /// Creation timestamp
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
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