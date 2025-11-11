//! Supervisor for managing actors

use anyhow::Result;
use tokio::select;
use tracing::{info, error};

use crate::config::Config;
use crate::runtime::ShutdownTx;

/// Supervisor manages all actors and restarts them on failure
pub struct Supervisor {
    config: Config,
    shutdown_tx: ShutdownTx,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(config: Config, shutdown_tx: ShutdownTx) -> Self {
        Supervisor {
            config,
            shutdown_tx,
        }
    }

    /// Run the supervisor
    pub async fn run(self) -> Result<()> {
        info!("Supervisor starting");

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn actors
        // TODO: Spawn actual actors (identity, discovery, session, etc.)

        // Wait for shutdown signal
        select! {
            _ = shutdown_rx.recv() => {
                info!("Supervisor received shutdown signal");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Supervisor received Ctrl+C");
                let _ = self.shutdown_tx.send(());
            }
        }

        // Graceful shutdown of actors
        info!("Supervisor shutting down actors");

        // TODO: Wait for actors to complete

        info!("Supervisor stopped");
        Ok(())
    }
}
