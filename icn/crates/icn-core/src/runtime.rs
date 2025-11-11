//! Tokio-based actor runtime

use anyhow::Result;
use icn_identity::KeyPair;
use tokio::sync::broadcast;
use tracing::info;

use crate::config::Config;

/// Shutdown signal broadcaster
pub type ShutdownTx = broadcast::Sender<()>;
pub type ShutdownRx = broadcast::Receiver<()>;

/// ICNd runtime handle
pub struct Runtime {
    config: Config,
    keypair: Option<KeyPair>,
    shutdown_tx: ShutdownTx,
}

impl Runtime {
    /// Create a new runtime with the given configuration and optional keypair
    pub fn new(config: Config, keypair: Option<KeyPair>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Runtime {
            config,
            keypair,
            shutdown_tx,
        }
    }

    /// Get a shutdown receiver for actors
    pub fn shutdown_rx(&self) -> ShutdownRx {
        self.shutdown_tx.subscribe()
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Run the runtime until shutdown
    pub async fn run(self) -> Result<()> {
        info!("ICNd runtime starting");

        // Create supervisor
        let supervisor = crate::supervisor::Supervisor::new(
            self.config.clone(),
            self.keypair,
            self.shutdown_tx.clone(),
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
