//! Supervisor for managing actors

use anyhow::Result;
use tokio::select;
use tracing::info;

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
        // TODO: Identity actor
        //   Challenge: Keystore requires passphrase to unlock
        //   Options:
        //     1. Prompt for passphrase on daemon startup (secure, but UX friction)
        //     2. Load identity via API after daemon starts (deferred unlock)
        //     3. Use system keyring for passphrase storage (OS-dependent)
        //   For now: Use icnctl for identity operations (not in-daemon)
        //
        // Example code once passphrase strategy is resolved:
        //   let keypair = load_and_unlock_keystore(&self.config.keystore_path(), passphrase)?;
        //   let identity_handle = IdentityActor::spawn(
        //       self.config.keystore_path(),
        //       self.config.store_path(),
        //       keypair,
        //       self.shutdown_tx.clone()
        //   )?;

        // TODO: Spawn other actors (discovery, session, gossip, etc.)

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
