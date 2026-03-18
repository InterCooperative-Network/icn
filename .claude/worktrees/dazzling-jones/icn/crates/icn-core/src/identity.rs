//! Identity actor - manages node identity and signing
//!
//! The IdentityActor provides a centralized interface for:
//! - Node identity (DID and signing operations)
//! - Trust queries (via TrustService)
//!
//! Trust operations are delegated to the TrustService trait, maintaining
//! kernel/app separation (no direct TrustGraph access).

use anyhow::{Context, Result};
use icn_identity::{Did, KeyPair};
use icn_kernel_api::services::TrustService;
use icn_kernel_api::TrustClass;
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

    /// Remove a trust edge
    RemoveTrustEdge {
        target: Did,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get trust class for a DID
    GetTrustClass {
        did: Did,
        response: oneshot::Sender<Result<TrustClass>>,
    },
}

/// Handle to interact with the identity actor
#[derive(Clone)]
pub struct IdentityHandle {
    tx: mpsc::Sender<IdentityMsg>,
    /// Cached DID for synchronous access
    did: Did,
}

impl IdentityHandle {
    /// Get the node's DID (synchronous - cached)
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Get the node's DID (async - queries actor)
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

    /// Get trust class for a DID
    pub async fn get_trust_class(&self, did: Did) -> Result<TrustClass> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(IdentityMsg::GetTrustClass { did, response: tx })
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

    /// Remove a trust edge
    pub async fn remove_trust_edge(&self, target: Did) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(IdentityMsg::RemoveTrustEdge {
                target,
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
    trust_service: Option<Arc<dyn TrustService>>,
    rx: mpsc::Receiver<IdentityMsg>,
}

impl IdentityActor {
    /// Start the identity actor with an optional trust service
    ///
    /// # Arguments
    /// - `keypair`: The node's Ed25519 keypair
    /// - `trust_service`: Optional trust service for trust queries/mutations
    /// - `shutdown_tx`: Broadcast channel for shutdown coordination
    pub fn spawn(
        keypair: KeyPair,
        trust_service: Option<Arc<dyn TrustService>>,
        shutdown_tx: ShutdownTx,
    ) -> IdentityHandle {
        let did = keypair.did().clone();
        info!("Identity actor spawning with DID: {}", did);

        // Create channel
        let (tx, rx) = mpsc::channel(32);

        // Create actor
        let actor = IdentityActor {
            keypair,
            trust_service,
            rx,
        };

        // Spawn task
        tokio::spawn(async move {
            if let Err(e) = actor.run(shutdown_tx).await {
                warn!("Identity actor error: {}", e);
            }
        });

        IdentityHandle { tx, did }
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
                let result = match &self.trust_service {
                    Some(svc) => Ok(svc.trust_score(&did.to_string())),
                    None => Err(anyhow::anyhow!("Trust service not available")),
                };
                let _ = response.send(result);
            }

            IdentityMsg::GetTrustClass { did, response } => {
                let result = match &self.trust_service {
                    Some(svc) => {
                        let score = svc.trust_score(&did.to_string());
                        Ok(TrustClass::from_score(score))
                    }
                    None => Err(anyhow::anyhow!("Trust service not available")),
                };
                let _ = response.send(result);
            }

            IdentityMsg::AddTrustEdge {
                target,
                score,
                labels,
                response,
            } => {
                let result = match &self.trust_service {
                    Some(svc) => svc
                        .submit_attestation(&target.to_string(), score, labels)
                        .map(|_bytes| ())
                        .map_err(|e| anyhow::anyhow!("{e}")),
                    None => Err(anyhow::anyhow!("Trust service not available")),
                };
                let _ = response.send(result);
            }

            IdentityMsg::RemoveTrustEdge { target, response } => {
                let result = match &self.trust_service {
                    Some(svc) => svc
                        .revoke_trust(&target.to_string())
                        .map(|_bytes| ())
                        .map_err(|e| anyhow::anyhow!("{e}")),
                    None => Err(anyhow::anyhow!("Trust service not available")),
                };
                let _ = response.send(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::authz::{
        ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest,
    };
    use tokio::sync::broadcast;

    /// Mock trust service for tests
    struct MockTrustService;

    impl TrustService for MockTrustService {
        fn oracle(&self) -> Arc<dyn PolicyOracle> {
            struct MockOracle;
            impl PolicyOracle for MockOracle {
                fn evaluate(&self, _req: &PolicyRequest) -> PolicyDecision {
                    PolicyDecision::Allow {
                        constraints: ConstraintSet::default(),
                        policy_domain: None,
                        evaluated_at: None,
                    }
                }
                fn domain(&self) -> Domain {
                    Domain::trust()
                }
            }
            Arc::new(MockOracle)
        }

        fn trust_score(&self, _actor: &icn_kernel_api::types::Did) -> f64 {
            0.56 // Simulates 0.8 direct * 0.7 weight
        }

        fn record_event(
            &self,
            _actor: &icn_kernel_api::types::Did,
            _event: icn_kernel_api::services::TrustEvent,
        ) {
        }

        fn submit_attestation(
            &self,
            _target: &icn_kernel_api::types::Did,
            _score: f64,
            _labels: Vec<String>,
        ) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }

        fn revoke_trust(&self, _target: &icn_kernel_api::types::Did) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_identity_actor_get_did() {
        let keypair = KeyPair::generate().unwrap();
        let expected_did = keypair.did().clone();

        let (shutdown_tx, _) = broadcast::channel(1);
        let handle = IdentityActor::spawn(keypair, None, shutdown_tx);

        // Test cached DID access
        assert_eq!(handle.did(), &expected_did);

        // Test async DID access
        let did = handle.get_did().await.unwrap();
        assert_eq!(did, expected_did);
    }

    #[tokio::test]
    async fn test_identity_actor_sign() {
        let keypair = KeyPair::generate().unwrap();

        let (shutdown_tx, _) = broadcast::channel(1);
        let handle = IdentityActor::spawn(keypair.clone(), None, shutdown_tx);

        let message = b"test message".to_vec();
        let signature = handle.sign(message.clone()).await.unwrap();

        // Verify signature
        use ed25519_dalek::Verifier;
        keypair
            .verifying_key()
            .verify(&message, &signature)
            .unwrap();
    }

    #[tokio::test]
    async fn test_identity_actor_trust_operations() {
        let keypair = KeyPair::generate().unwrap();

        let trust_service: Arc<dyn TrustService> = Arc::new(MockTrustService);

        let (shutdown_tx, _) = broadcast::channel(1);
        let handle = IdentityActor::spawn(keypair.clone(), Some(trust_service), shutdown_tx);

        // Create a target DID
        let target_keypair = KeyPair::generate().unwrap();
        let target_did = target_keypair.did().clone();

        // Add trust edge (via TrustService)
        handle
            .add_trust_edge(target_did.clone(), 0.8, vec!["test".to_string()])
            .await
            .unwrap();

        // Get trust score (mock returns 0.56)
        let score = handle.get_trust_score(target_did.clone()).await.unwrap();
        assert!((score - 0.56).abs() < 0.01, "Expected 0.56, got {score}");

        // Get trust class (0.56 is in Partner range: 0.4-0.7)
        let class = handle.get_trust_class(target_did.clone()).await.unwrap();
        assert_eq!(class, TrustClass::Partner);

        // Remove trust edge (via TrustService)
        handle.remove_trust_edge(target_did).await.unwrap();
    }
}
