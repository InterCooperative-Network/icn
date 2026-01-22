//! Supervisor for managing actors
//!
//! This module has been refactored into focused submodules:
//! - `actors` - Actor handle types and coordination
//! - `bridge` - Gossip/network bridging and bootstrap utilities
//! - `lifecycle` - Supervisor startup and shutdown orchestration
//! - `shutdown` - Graceful shutdown and state snapshot management
//! - `version_tracker` - Protocol version tracking
//!
//! The initialization modules (`init_*`) handle spawning specific actors.

pub mod actors;
pub mod background_tasks;
pub mod bridge;
pub mod governance_handlers;
pub mod init_bootstrap;
pub mod init_community;
pub mod init_compute;
pub mod init_contract_registry;
pub mod init_coop;
pub mod init_entity;
pub mod init_federation;
pub mod init_gateway;
pub mod init_gossip;
pub mod init_governance;
pub mod init_ledger;
pub mod init_network;
pub mod init_notifications;
pub mod init_rpc;
pub mod init_send_callback;
pub mod init_snapshot;
pub mod init_steward;
pub mod init_trust;
pub mod lifecycle;
pub mod nat_dial;
pub mod registry;
pub mod shutdown;
pub mod version_tracker;

use anyhow::Result;
use icn_identity::IdentityBundle;

use crate::config::Config;
use crate::runtime::ShutdownTx;

/// Supervisor manages all actors and restarts them on failure
pub struct Supervisor {
    config: Config,
    identity_bundle: Option<IdentityBundle>,
    shutdown_tx: ShutdownTx,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(
        config: Config,
        identity_bundle: Option<IdentityBundle>,
        shutdown_tx: ShutdownTx,
    ) -> Self {
        Supervisor {
            config,
            identity_bundle,
            shutdown_tx,
        }
    }

    /// Run the supervisor
    pub async fn run(self) -> Result<()> {
        lifecycle::run_supervisor(self.config, self.identity_bundle, self.shutdown_tx).await
    }
}
