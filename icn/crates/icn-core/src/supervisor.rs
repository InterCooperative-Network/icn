//! Supervisor for managing actors

use anyhow::Result;
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_ledger::Ledger;
use icn_rpc::RpcServer;
use icn_store::SledStore;
use icn_trust::TrustClass;
use std::sync::Arc;
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
        let (network_handle, gossip_handle, ledger_handle) = if let Some(keypair) = &self.keypair {
            info!("Keypair available - spawning actors");

            let did = keypair.did().clone();

            // Spawn Gossip actor
            let trust_lookup = Arc::new(|_did: &icn_identity::Did| Some(TrustClass::Partner));
            let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

            info!("Gossip actor spawned");

            // Spawn Ledger
            let store_path = self.config.store_path().join("ledger");
            let store = Arc::new(SledStore::open(&store_path)?);
            let mut ledger = Ledger::new(store)?;
            ledger.set_gossip(gossip_handle.clone());
            let ledger_handle = Arc::new(tokio::sync::RwLock::new(ledger));

            info!("Ledger initialized at {}", store_path.display());

            // TODO: Spawn Identity actor
            // let identity_handle = IdentityActor::spawn(
            //     self.config.keystore_path(),
            //     self.config.store_path(),
            //     keypair.clone(),
            //     self.shutdown_tx.clone()
            // )?;

            // Spawn Network actor with gossip bridge
            let listen_addr: std::net::SocketAddr = self.config.network.listen_addr.parse()?;

            // Create incoming message handler that routes to gossip
            let gossip_handle_clone = gossip_handle.clone();
            let incoming_handler: icn_net::IncomingMessageHandler = Arc::new(move |net_msg| {
                // Extract gossip message if present
                if let icn_net::MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                    // Route to gossip actor
                    let mut gossip = gossip_handle_clone.blocking_write();
                    if let Err(e) = gossip.handle_message(gossip_msg) {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                }
            });

            let network_handle = icn_net::NetworkActor::spawn(
                keypair,
                listen_addr,
                self.shutdown_tx.clone(),
                Some(incoming_handler),
            )
            .await?;

            info!("Network actor spawned on {}", listen_addr);

            // Spawn RPC server with network handle
            let rpc_addr = "127.0.0.1:5050".parse()?;
            let mut rpc_server = RpcServer::new(rpc_addr);
            rpc_server.set_network_handle(network_handle.clone());

            tokio::spawn(async move {
                if let Err(e) = rpc_server.run().await {
                    warn!("RPC server error: {}", e);
                }
            });

            info!("RPC server spawned on {}", rpc_addr);

            // Spawn anti-entropy task
            let anti_entropy_config = crate::anti_entropy::AntiEntropyConfig::default();
            let _anti_entropy_handle = crate::anti_entropy::spawn_anti_entropy_task(
                gossip_handle.clone(),
                network_handle.clone(),
                did.clone(),
                anti_entropy_config,
                self.shutdown_tx.subscribe(),
            );

            info!("Anti-entropy task spawned");

            (Some(network_handle), Some(gossip_handle), Some(ledger_handle))
        } else {
            warn!("No keypair available - actors not spawned");
            warn!("Run 'icnctl id init' to create an identity");
            (None, None, None)
        };

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

        // Gossip and Ledger are wrapped in Arc<RwLock> and will be dropped when
        // all references are released
        if gossip_handle.is_some() {
            info!("Gossip actor will be dropped when all references are released");
        }
        if ledger_handle.is_some() {
            info!("Ledger will be dropped when all references are released");
        }

        info!("Supervisor stopped");
        Ok(())
    }
}
