//! Snapshot coordinator initialization

use anyhow::Result;
use icn_identity::Did;
use icn_snapshot::{SnapshotConfig, SnapshotCoordinator};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Initialize snapshot coordinator
pub async fn init_snapshot_coordinator(did: Did) -> Result<Arc<RwLock<SnapshotCoordinator>>> {
    let config = SnapshotConfig::default();
    let coordinator = SnapshotCoordinator::new(did.clone(), config);

    info!("Snapshot coordinator initialized for DID: {}", did);

    Ok(Arc::new(RwLock::new(coordinator)))
}
