//! Supervisor for managing actors

use anyhow::Result;
use icn_identity::KeyPair;
use tokio::select;
use tracing::{info, warn};

use crate::config::Config;
use crate::runtime::ShutdownTx;

/// Supervisor manages all actors and restarts them on failure
pub struct Supervisor {
    config: Config,
    keypair: Option<KeyPair>,
    shutdown_tx: ShutdownTx,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(config: Config, keypair: Option<KeyPair>, shutdown_tx: ShutdownTx) -> Self {
        Supervisor {
            config,
            keypair,
            shutdown_tx,
        }
    }

    /// Run the supervisor
    pub async fn run(self) -> Result<()> {
        info!("Supervisor starting");

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn actors (requires keypair from unlocked keystore)
        let network_handle = if let Some(keypair) = &self.keypair {
            info!("Keypair available - spawning actors");

            // TODO: Spawn Identity actor
            // let identity_handle = IdentityActor::spawn(
            //     self.config.keystore_path(),
            //     self.config.store_path(),
            //     keypair.clone(),
            //     self.shutdown_tx.clone()
            // )?;

            // Spawn Network actor
            let listen_addr: std::net::SocketAddr = self.config.network.listen_addr.parse()?;
            let network_handle = icn_net::NetworkActor::spawn(
                keypair,
                listen_addr,
                self.shutdown_tx.clone(),
            )
            .await?;

            info!("Network actor spawned on {}", listen_addr);
            Some(network_handle)
        } else {
            warn!("No keypair available - actors not spawned");
            warn!("Run 'icnctl id init' to create an identity");
            None
        };

        // TODO: Spawn other actors (gossip, replication, etc.)

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

        // Network actor will shut down gracefully via the shutdown signal
        // The actor's run loop listens for shutdown_rx and cleans up properly
        if network_handle.is_some() {
            info!("Network actor will shut down via shutdown signal");
        }

        // TODO: Wait for other actors to complete

        info!("Supervisor stopped");
        Ok(())
    }
}
