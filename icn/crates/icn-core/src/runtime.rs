//! Tokio-based actor runtime

use anyhow::Result;
use icn_identity::IdentityBundle;
use icn_kernel_api::services::ServiceRegistry;
use tokio::sync::broadcast;
use tracing::info;

use crate::config::Config;

/// Shutdown signal broadcaster
pub type ShutdownTx = broadcast::Sender<()>;
pub type ShutdownRx = broadcast::Receiver<()>;

/// ICNd runtime handle
pub struct Runtime {
    config: Config,
    identity_bundle: Option<IdentityBundle>,
    shutdown_tx: ShutdownTx,
    /// Optional pre-configured service registry from the daemon
    ///
    /// When provided, the supervisor will use these services instead of
    /// creating its own. This enables proper kernel/app separation where
    /// the daemon constructs domain services from app crates.
    service_registry: Option<ServiceRegistry>,
}

impl Runtime {
    /// Create a new runtime with the given configuration and optional identity bundle
    pub fn new(config: Config, identity_bundle: Option<IdentityBundle>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Runtime {
            config,
            identity_bundle,
            shutdown_tx,
            service_registry: None,
        }
    }

    /// Set a pre-configured service registry
    ///
    /// When provided, the supervisor will use these services instead of
    /// creating its own internal implementations. This enables proper
    /// kernel/app separation:
    ///
    /// ```ignore
    /// // In icnd (daemon):
    /// let trust_service = apps_trust::create_service(graph);
    /// let registry = ServiceRegistry::new()
    ///     .with_trust(trust_service);
    /// let runtime = Runtime::new(config, identity)
    ///     .with_services(registry);
    /// ```
    pub fn with_services(mut self, registry: ServiceRegistry) -> Self {
        self.service_registry = Some(registry);
        self
    }

    /// Get a shutdown receiver for actors
    pub fn shutdown_rx(&self) -> ShutdownRx {
        self.shutdown_tx.subscribe()
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get a clone of the shutdown sender
    pub fn shutdown_tx(&self) -> ShutdownTx {
        self.shutdown_tx.clone()
    }

    /// Run the runtime until shutdown
    pub async fn run(self) -> Result<()> {
        info!("ICNd runtime starting");

        // Create supervisor
        let supervisor = crate::supervisor::Supervisor::new(
            self.config.clone(),
            self.identity_bundle,
            self.shutdown_tx.clone(),
            self.service_registry,
        );

        // Run supervisor
        supervisor.run().await?;

        info!("ICNd runtime stopped");
        Ok(())
    }

    /// Trigger shutdown
    pub fn shutdown(&self) {
        info!("Triggering shutdown");
        let _ = self.shutdown_tx.send(());
    }
}
