//! Identity actor - manages node identity, signing, and trust graph

use anyhow::{Context, Result};
use icn_identity::{AgeKeyStore, Did, KeyPair};
use icn_store::{SledStore, Store};
use icn_trust::TrustGraph;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::runtime::ShutdownTx;

/// Messages that can be sent to the identity actor
#[derive(Debug)]
pub enum IdentityMsg {
    /// Get the node's DID
    GetDid(oneshot::Sender<Did>),

    /// Sign a message
    Sign {
        message: Vec<u8>,
        response: oneshot::Sender<ed25519_dalek::Signature>,
    },

    /// Get trust score for a DID
    GetTrustScore {
        did: Did,
        response: oneshot::Sender<Result<f64>>,
    },

    /// Add a trust edge
    AddTrustEdge {
        target: Did,
        score: f64,
        labels: Vec<String>,
        response: oneshot::Sender<Result<()>>,
    },
}

/// Handle to interact with the identity actor
#[derive(Clone)]
pub struct IdentityHandle {
    tx: mpsc::Sender<IdentityMsg>,
}

impl IdentityHandle {
    /// Get the node's DID
    pub async fn get_did(&self) -> Result<Did> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(IdentityMsg::GetDid(tx))
            .await
            .context("Identity actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Sign a message
    pub async fn sign(&self, message: Vec<u8>) -> Result<ed25519_dalek::Signature> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(IdentityMsg::Sign {
                message,
                response: tx,
            })
            .await
            .context("Identity actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Get trust score for a DID
    pub async fn get_trust_score(&self, did: Did) -> Result<f64> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(IdentityMsg::GetTrustScore { did, response: tx })
            .await
            .context("Identity actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Add a trust edge
    pub async fn add_trust_edge(&self, target: Did, score: f64, labels: Vec<String>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(IdentityMsg::AddTrustEdge {
                target,
                score,
                labels,
                response: tx,
            })
            .await
            .context("Identity actor closed")?;
        rx.await.context("Response channel closed")?
    }
}

/// Identity actor state
pub struct IdentityActor {
    keypair: KeyPair,
    trust_graph: TrustGraph,
    #[allow(dead_code)]
    keystore: AgeKeyStore,
    rx: mpsc::Receiver<IdentityMsg>,
}

impl IdentityActor {
    /// Start the identity actor
    ///
    /// Opens the keystore - it must already be unlocked with the passphrase
    pub fn spawn(
        keystore_path: impl AsRef<Path>,
        store_path: impl AsRef<Path>,
        keypair: KeyPair,
        shutdown_tx: ShutdownTx,
    ) -> Result<IdentityHandle> {
        let keystore_path = keystore_path.as_ref().to_path_buf();
        let store_path = store_path.as_ref();

        // Open keystore (already unlocked)
        let keystore = AgeKeyStore::open(&keystore_path)?;

        // Open store and create trust graph
        let store: Arc<dyn Store> = Arc::new(SledStore::open(store_path)?);
        let own_did = keypair.did().clone();
        let trust_graph = TrustGraph::new(store, own_did);

        info!("Identity actor initializing with DID: {}", keypair.did());

        // Create channel
        let (tx, rx) = mpsc::channel(32);

        // Create actor
        let actor = IdentityActor {
            keypair,
            trust_graph,
            keystore,
            rx,
        };

        // Spawn task
        tokio::spawn(async move {
            if let Err(e) = actor.run(shutdown_tx).await {
                warn!("Identity actor error: {}", e);
            }
        });

        Ok(IdentityHandle { tx })
    }

    /// Run the identity actor event loop
    async fn run(mut self, shutdown_tx: ShutdownTx) -> Result<()> {
        info!("Identity actor running");

        let mut shutdown_rx = shutdown_tx.subscribe();

        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(msg) => self.handle_message(msg).await,
                        None => {
                            info!("Identity actor channel closed");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Identity actor received shutdown signal");
                    break;
                }
            }
        }

        info!("Identity actor stopped");
        Ok(())
    }

    /// Handle a single message
    async fn handle_message(&mut self, msg: IdentityMsg) {
        match msg {
            IdentityMsg::GetDid(tx) => {
                let _ = tx.send(self.keypair.did().clone());
            }

            IdentityMsg::Sign { message, response } => {
                let signature = self.keypair.sign(&message);
                let _ = response.send(signature);
            }

            IdentityMsg::GetTrustScore { did, response } => {
                let result = self.trust_graph.compute_trust_score(&did);
                let _ = response.send(result);
            }

            IdentityMsg::AddTrustEdge {
                target,
                score,
                labels,
                response,
            } => {
                let mut edge = icn_trust::TrustEdge::new(self.keypair.did().clone(), target, score);

                for label in labels {
                    edge = edge.with_label(label);
                }

                let result = self.trust_graph.add_edge(edge);
                let _ = response.send(result);
            }
        }
    }
}
