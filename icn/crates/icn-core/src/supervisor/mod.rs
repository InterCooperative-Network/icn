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
pub mod decision_executor;
pub mod effect_dispatcher;
pub mod execution_store;
pub mod governance_executor;
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
pub mod init_naming;
pub mod init_network;
pub mod init_notifications;
pub mod init_resource_enforcer;
pub mod init_rpc;
pub mod init_send_callback;
pub mod init_snapshot;
pub mod init_steward;
pub mod lifecycle;
pub mod nat_dial;
pub mod shutdown;
pub mod version_tracker;

use anyhow::Result;
use icn_identity::IdentityBundle;
use icn_kernel_api::services::ServiceRegistry;

use crate::config::Config;
use crate::runtime::ShutdownTx;

pub use actors::BootstrapHandles;
pub use governance_executor::KernelGovernanceExecutor;
pub use governance_executor::KernelTreasuryExecutor;

/// Supervisor manages all actors and restarts them on failure
pub struct Supervisor {
    config: Config,
    identity_bundle: Option<IdentityBundle>,
    shutdown_tx: ShutdownTx,
    /// Optional service registry from daemon
    ///
    /// When provided, services from this registry are used instead of
    /// creating internal implementations. This enables kernel/app separation.
    service_registry: Option<ServiceRegistry>,
    /// Optional typed domain handles from daemon
    ///
    /// These carry concrete domain objects (ledger, contract runtime, etc.)
    /// that the supervisor wires into actors during initialization.
    bootstrap_handles: Option<BootstrapHandles>,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(
        config: Config,
        identity_bundle: Option<IdentityBundle>,
        shutdown_tx: ShutdownTx,
        service_registry: Option<ServiceRegistry>,
        bootstrap_handles: Option<BootstrapHandles>,
    ) -> Self {
        Supervisor {
            config,
            identity_bundle,
            shutdown_tx,
            service_registry,
            bootstrap_handles,
        }
    }

    /// Run the supervisor
    pub async fn run(self) -> Result<()> {
        lifecycle::run_supervisor(
            self.config,
            self.identity_bundle,
            self.shutdown_tx,
            self.service_registry,
            self.bootstrap_handles,
        )
        .await
    }
}
