//! ContractActor - manages contract lifecycle and distribution

use crate::ast::Contract;
use crate::messages::{ContractDeploymentMessage, ContractExecutionRequest};
use crate::runtime::ContractRuntime;
use crate::types::{ContractInstallation, ExecutionContext, ExecutionResult};
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use icn_ledger::ContentHash;
use icn_trust::TrustGraph;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Gossip topic for contract deployment announcements
pub const CONTRACTS_DEPLOY_TOPIC: &str = "contracts:deploy";

/// Callback for publishing contract deployment messages via gossip
pub type GossipCallback = Arc<dyn Fn(ContractDeploymentMessage) + Send + Sync>;

/// Minimum trust score required to deploy contracts to the network
pub const MIN_DEPLOYER_TRUST: f64 = 0.4;

/// Default fuel for contract execution
pub const DEFAULT_FUEL: u64 = 100_000;

/// Maximum fuel for contract execution
pub const MAX_FUEL: u64 = 1_000_000;

/// ContractActor manages contract lifecycle and distribution
pub struct ContractActor {
    /// This node's DID (currently unused, reserved for future signing)
    #[allow(dead_code)]
    did: Did,

    /// Contract runtime (manages installed contracts)
    runtime: Arc<RwLock<ContractRuntime>>,

    /// Trust graph for authorization
    trust_graph: Option<Arc<RwLock<TrustGraph>>>,

    /// Gossip callback for publishing contract deployments
    gossip_callback: Option<GossipCallback>,
}

impl ContractActor {
    /// Create a new ContractActor
    pub fn new(
        did: Did,
        runtime: Arc<RwLock<ContractRuntime>>,
        trust_graph: Option<Arc<RwLock<TrustGraph>>>,
    ) -> Self {
        info!("Creating ContractActor for {}", did);
        ContractActor {
            did,
            runtime,
            trust_graph,
            gossip_callback: None,
        }
    }

    /// Set the gossip callback for publishing contract deployments
    ///
    /// When set, deployed contracts will be broadcast to the network via the
    /// `contracts:deploy` gossip topic.
    pub fn set_gossip_callback(&mut self, callback: GossipCallback) {
        self.gossip_callback = Some(callback);
        debug!("Gossip callback configured for contract deployments");
    }

    /// Deploy a contract to the network
    ///
    /// This validates the contract, checks deployer authorization, and publishes
    /// the contract to the gossip network for distribution.
    pub async fn deploy_contract(
        &self,
        contract: Contract,
        installation: ContractInstallation,
        deployer_signature: Vec<u8>,
    ) -> Result<ContentHash> {
        info!("Deploying contract: {}", contract.name);

        // Validate contract structure
        contract.validate().context("Contract validation failed")?;

        // Check deployer authorization via trust graph
        // Skip trust check if deploying locally (deployer is self)
        if installation.installed_by == self.did {
            debug!("Local deployment by {}, skipping trust check", self.did);
        } else if let Some(ref trust_graph) = self.trust_graph {
            let trust_score = {
                let graph = trust_graph.read().await;
                graph
                    .compute_trust_score(&installation.installed_by)
                    .unwrap_or(0.0)
            };

            if trust_score < MIN_DEPLOYER_TRUST {
                bail!(
                    "Deployer {} has insufficient trust: {:.2} < {:.2}",
                    installation.installed_by,
                    trust_score,
                    MIN_DEPLOYER_TRUST
                );
            }

            debug!(
                "Deployer {} authorized with trust score {:.2}",
                installation.installed_by, trust_score
            );
        } else {
            warn!("No trust graph available; skipping deployer authorization");
        }

        // Generate code hash (in real implementation, hash the contract bytecode)
        // For now, use a deterministic hash based on contract name + participants
        let code_hash = self.compute_code_hash(&contract);

        // Install contract locally with metadata
        {
            let mut runtime = self.runtime.write().await;
            runtime.install_contract_with_metadata(
                code_hash.clone(),
                contract.clone(),
                installation.clone(),
            )?;
        }

        info!("Contract installed locally: {}", code_hash);

        // Create deployment message
        let deployment_msg = ContractDeploymentMessage::new(
            code_hash.clone(),
            contract,
            installation,
            deployer_signature,
        );

        // Verify the message before publishing
        deployment_msg
            .verify()
            .context("Deployment message verification failed")?;

        // Publish to gossip topic `contracts:deploy` if callback is configured
        if let Some(ref callback) = self.gossip_callback {
            info!(
                "Publishing contract deployment to gossip: {} ({})",
                deployment_msg.contract.name, code_hash
            );
            callback(deployment_msg);
            icn_obs::metrics::contract::deployments_inc();
        } else {
            debug!(
                "No gossip callback configured; contract {} deployed locally only",
                code_hash
            );
        }

        Ok(code_hash)
    }

    /// Execute a contract rule
    ///
    /// This checks authorization (caller must be participant or meet trust threshold),
    /// executes the rule, and applies ledger operations.
    pub async fn execute_rule(&self, request: ContractExecutionRequest) -> Result<ExecutionResult> {
        debug!(
            "Executing contract {} rule {}",
            request.code_hash, request.rule_name
        );

        // Get contract installation metadata
        let installation = {
            let runtime = self.runtime.read().await;
            runtime
                .get_installation(&request.code_hash)
                .context("Contract installation not found")?
                .clone()
        };

        // Check caller authorization
        self.check_execution_authorization(&request.caller, &installation)
            .await?;

        // Build execution context
        let context = ExecutionContext::new(
            request.caller.clone(),
            request.timestamp,
            DEFAULT_FUEL,
            installation.capabilities.clone(),
            installation.participants.clone(),
        );

        // Execute the rule
        let result = {
            let mut runtime = self.runtime.write().await;
            runtime
                .execute_rule(
                    &request.code_hash,
                    &request.rule_name,
                    context,
                    request.args,
                )
                .await?
        };

        info!(
            "Contract execution completed: {} fuel used",
            result.fuel_consumed
        );

        Ok(result)
    }

    /// Handle incoming contract deployment message from gossip
    pub async fn handle_deployment_message(&self, msg: ContractDeploymentMessage) -> Result<()> {
        info!(
            "Received contract deployment: {} from {}",
            msg.contract.name, msg.installation.installed_by
        );

        // Verify the deployment message
        msg.verify().context("Invalid deployment message")?;

        // Check deployer trust
        // Skip trust check if deployment is from local node (self-deployment via gossip)
        if msg.installation.installed_by == self.did {
            debug!(
                "Deployment from local node {}, skipping trust check",
                self.did
            );
        } else if let Some(ref trust_graph) = self.trust_graph {
            let trust_score = {
                let graph = trust_graph.read().await;
                graph
                    .compute_trust_score(&msg.installation.installed_by)
                    .unwrap_or(0.0)
            };

            if trust_score < MIN_DEPLOYER_TRUST {
                bail!(
                    "Deployer {} has insufficient trust: {:.2} < {:.2}",
                    msg.installation.installed_by,
                    trust_score,
                    MIN_DEPLOYER_TRUST
                );
            }
        }

        // Install contract locally with metadata
        let mut runtime = self.runtime.write().await;
        runtime.install_contract_with_metadata(
            msg.code_hash.clone(),
            msg.contract.clone(),
            msg.installation.clone(),
        )?;

        info!("Contract installed: {}", msg.code_hash);
        icn_obs::metrics::contract::deployments_received_inc();

        Ok(())
    }

    /// Check if caller is authorized to execute a contract
    async fn check_execution_authorization(
        &self,
        caller: &Did,
        installation: &ContractInstallation,
    ) -> Result<()> {
        // Check if caller is a participant
        if installation.participants.contains(caller) {
            debug!("Caller {} is a participant", caller);
            return Ok(());
        }

        // Check if contract allows non-participants
        if let Some(min_trust) = installation.min_caller_trust {
            if let Some(ref trust_graph) = self.trust_graph {
                let trust_score = {
                    let graph = trust_graph.read().await;
                    graph.compute_trust_score(caller).unwrap_or(0.0)
                };

                if trust_score >= min_trust {
                    debug!(
                        "Caller {} authorized with trust score {:.2} >= {:.2}",
                        caller, trust_score, min_trust
                    );
                    Ok(())
                } else {
                    bail!(
                        "Caller {caller} has insufficient trust: {trust_score:.2} < {min_trust:.2}"
                    );
                }
            } else {
                warn!("No trust graph available; denying non-participant caller");
                bail!("Trust graph unavailable; only participants can execute");
            }
        } else {
            bail!("Contract is participant-only; caller {caller} is not a participant");
        }
    }

    /// Compute deterministic hash for contract code
    fn compute_code_hash(&self, contract: &Contract) -> ContentHash {
        // In a real implementation, this would hash the contract bytecode
        // For now, use a simple hash based on contract name + participants
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(contract.name.as_bytes());
        for participant in &contract.participants {
            hasher.update(format!("{participant:?}").as_bytes());
        }
        let hash_bytes = hasher.finalize();
        ContentHash::from_bytes(hash_bytes.into())
    }

    /// List all installed contracts
    pub async fn list_contracts(&self) -> Vec<crate::runtime::ContractInfo> {
        let runtime = self.runtime.read().await;
        runtime.list_contracts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Rule, Stmt};
    use crate::messages::ContractDeploymentMessage;
    use crate::types::{Capability, Value};
    use icn_identity::KeyPair;
    use icn_ledger::Ledger;
    use icn_store::SledStore;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_actor() -> (ContractActor, Arc<RwLock<ContractRuntime>>) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(store).unwrap()));
        let runtime = Arc::new(RwLock::new(ContractRuntime::new(ledger)));

        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let actor = ContractActor::new(did, runtime.clone(), None);
        (actor, runtime)
    }

    /// Helper to compute code hash (same algorithm as ContractActor)
    fn compute_code_hash(contract: &Contract) -> ContentHash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(contract.name.as_bytes());
        for participant in &contract.participants {
            hasher.update(format!("{participant:?}").as_bytes());
        }
        ContentHash::from_bytes(hasher.finalize().into())
    }

    /// Helper to create valid signatures for contract deployment
    fn create_signatures(
        code_hash: &ContentHash,
        installed_at: u64,
        keypairs: &[&KeyPair],
    ) -> Vec<(icn_identity::Did, Vec<u8>)> {
        let signing_bytes =
            ContractDeploymentMessage::compute_signing_bytes(code_hash, installed_at);

        keypairs
            .iter()
            .map(|kp| {
                let signature = kp.sign(&signing_bytes);
                (kp.did().clone(), signature.to_bytes().to_vec())
            })
            .collect()
    }

    #[tokio::test]
    async fn test_deploy_contract_without_trust_graph() {
        let (actor, _runtime) = create_test_actor();

        let alice_kp = KeyPair::generate().unwrap();
        let bob_kp = KeyPair::generate().unwrap();
        let alice = alice_kp.did().clone();
        let bob = bob_kp.did().clone();

        let contract = Contract::new("test".to_string())
            .add_participant(alice.clone())
            .add_participant(bob.clone())
            .with_currency("hours".to_string());

        let code_hash = compute_code_hash(&contract);
        let installed_at = 1234567890u64;
        let signatures = create_signatures(&code_hash, installed_at, &[&alice_kp, &bob_kp]);

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: alice.clone(),
            capabilities: vec![],
            participants: vec![alice.clone(), bob.clone()],
            signatures,
            installed_at,
            min_caller_trust: None,
        };

        let signing_bytes =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
        let deployer_signature = alice_kp.sign(&signing_bytes).to_bytes().to_vec();

        let result = actor
            .deploy_contract(contract, installation, deployer_signature)
            .await;

        // Should succeed even without trust graph (warning logged)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_rule_as_participant() {
        let (actor, _runtime) = create_test_actor();

        let alice_kp = KeyPair::generate().unwrap();
        let bob_kp = KeyPair::generate().unwrap();
        let alice = alice_kp.did().clone();
        let bob = bob_kp.did().clone();

        // Create and deploy a simple contract
        let contract = Contract::new("test".to_string())
            .add_participant(alice.clone())
            .add_participant(bob.clone())
            .with_currency("hours".to_string())
            .add_rule(
                Rule::new("transfer".to_string())
                    .add_param("recipient".to_string())
                    .add_param("amount".to_string())
                    .add_stmt(Stmt::LedgerTransfer {
                        from: Expr::Var("sender".to_string()),
                        to: Expr::Var("recipient".to_string()),
                        amount: Expr::Var("amount".to_string()),
                        currency: Expr::Literal(Value::String("hours".to_string())),
                    }),
            );

        let code_hash = compute_code_hash(&contract);
        let installed_at = 1234567890u64;
        let signatures = create_signatures(&code_hash, installed_at, &[&alice_kp, &bob_kp]);

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: alice.clone(),
            capabilities: vec![Capability::WriteLedger {
                accounts: vec![alice.clone(), bob.clone()],
            }],
            participants: vec![alice.clone(), bob.clone()],
            signatures,
            installed_at,
            min_caller_trust: None,
        };

        let signing_bytes =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
        let deployer_signature = alice_kp.sign(&signing_bytes).to_bytes().to_vec();

        let returned_code_hash = actor
            .deploy_contract(contract, installation, deployer_signature)
            .await
            .unwrap();

        // Execute as participant Alice
        let mut args = HashMap::new();
        args.insert("recipient".to_string(), Value::Did(bob.clone()));
        args.insert("amount".to_string(), Value::Int(10));

        let request = ContractExecutionRequest {
            code_hash: returned_code_hash,
            rule_name: "transfer".to_string(),
            args,
            caller: alice.clone(),
            timestamp: 1234567890,
        };

        let result = actor.execute_rule(request).await;
        if let Err(ref e) = result {
            eprintln!("Execution failed: {e:?}");
        }
        assert!(result.is_ok(), "Execution failed: {:?}", result.err());

        let execution_result = result.unwrap();
        assert_eq!(execution_result.ledger_ops.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_rule_as_non_participant_fails() {
        let (actor, _runtime) = create_test_actor();

        let alice_kp = KeyPair::generate().unwrap();
        let bob_kp = KeyPair::generate().unwrap();
        let charlie = KeyPair::generate().unwrap().did().clone(); // Not a participant
        let alice = alice_kp.did().clone();
        let bob = bob_kp.did().clone();

        // Create and deploy contract
        let contract = Contract::new("test".to_string())
            .add_participant(alice.clone())
            .add_participant(bob.clone())
            .with_currency("hours".to_string())
            .add_rule(Rule::new("noop".to_string()).add_stmt(Stmt::Return {
                value: Expr::Literal(Value::Bool(true)),
            }));

        let code_hash = compute_code_hash(&contract);
        let installed_at = 1234567890u64;
        let signatures = create_signatures(&code_hash, installed_at, &[&alice_kp, &bob_kp]);

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: alice.clone(),
            capabilities: vec![],
            participants: vec![alice.clone(), bob.clone()],
            signatures,
            installed_at,
            min_caller_trust: None, // Participant-only
        };

        let signing_bytes =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
        let deployer_signature = alice_kp.sign(&signing_bytes).to_bytes().to_vec();

        let returned_code_hash = actor
            .deploy_contract(contract, installation, deployer_signature)
            .await
            .unwrap();

        // Try to execute as non-participant Charlie
        let request = ContractExecutionRequest {
            code_hash: returned_code_hash,
            rule_name: "noop".to_string(),
            args: HashMap::new(),
            caller: charlie.clone(),
            timestamp: 1234567890,
        };

        let result = actor.execute_rule(request).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a participant"));
    }

    #[tokio::test]
    async fn test_list_contracts() {
        let (actor, _runtime) = create_test_actor();

        let alice_kp = KeyPair::generate().unwrap();
        let alice = alice_kp.did().clone();

        // Deploy two contracts
        let contract1 = Contract::new("contract1".to_string()).add_participant(alice.clone());

        let code_hash1 = compute_code_hash(&contract1);
        let installed_at1 = 1234567890u64;
        let signatures1 = create_signatures(&code_hash1, installed_at1, &[&alice_kp]);

        let installation1 = ContractInstallation {
            code_hash: code_hash1.clone(),
            installed_by: alice.clone(),
            capabilities: vec![],
            participants: vec![alice.clone()],
            signatures: signatures1,
            installed_at: installed_at1,
            min_caller_trust: None,
        };

        let signing_bytes1 =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash1, installed_at1);
        let deployer_signature1 = alice_kp.sign(&signing_bytes1).to_bytes().to_vec();

        actor
            .deploy_contract(contract1, installation1, deployer_signature1)
            .await
            .unwrap();

        let contract2 = Contract::new("contract2".to_string()).add_participant(alice.clone());

        let code_hash2 = compute_code_hash(&contract2);
        let installed_at2 = 1234567891u64;
        let signatures2 = create_signatures(&code_hash2, installed_at2, &[&alice_kp]);

        let installation2 = ContractInstallation {
            code_hash: code_hash2.clone(),
            installed_by: alice.clone(),
            capabilities: vec![],
            participants: vec![alice.clone()],
            signatures: signatures2,
            installed_at: installed_at2,
            min_caller_trust: None,
        };

        let signing_bytes2 =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash2, installed_at2);
        let deployer_signature2 = alice_kp.sign(&signing_bytes2).to_bytes().to_vec();

        actor
            .deploy_contract(contract2, installation2, deployer_signature2)
            .await
            .unwrap();

        // List contracts
        let contracts = actor.list_contracts().await;
        assert_eq!(contracts.len(), 2);
        assert!(contracts.iter().any(|c| c.name == "contract1"));
        assert!(contracts.iter().any(|c| c.name == "contract2"));
    }

    #[tokio::test]
    async fn test_gossip_callback_invoked_on_deploy() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;

        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(store).unwrap()));
        let runtime = Arc::new(RwLock::new(ContractRuntime::new(ledger)));

        let alice_kp = KeyPair::generate().unwrap();
        let alice = alice_kp.did().clone();

        let mut actor = ContractActor::new(alice.clone(), runtime.clone(), None);

        // Set up callback to track invocations (use std Mutex to avoid blocking issues)
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_clone = callback_count.clone();
        let received_msg = Arc::new(Mutex::new(None::<ContractDeploymentMessage>));
        let received_msg_clone = received_msg.clone();

        actor.set_gossip_callback(Arc::new(move |msg| {
            callback_count_clone.fetch_add(1, Ordering::SeqCst);
            *received_msg_clone.lock().unwrap() = Some(msg);
        }));

        // Deploy a contract
        let contract = Contract::new("gossip_test".to_string()).add_participant(alice.clone());

        let code_hash = compute_code_hash(&contract);
        let installed_at = 1234567890u64;
        let signatures = create_signatures(&code_hash, installed_at, &[&alice_kp]);

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: alice.clone(),
            capabilities: vec![],
            participants: vec![alice.clone()],
            signatures,
            installed_at,
            min_caller_trust: None,
        };

        let signing_bytes =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
        let deployer_signature = alice_kp.sign(&signing_bytes).to_bytes().to_vec();

        actor
            .deploy_contract(contract, installation, deployer_signature)
            .await
            .unwrap();

        // Verify callback was invoked
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);

        // Verify message content
        let msg = received_msg.lock().unwrap().clone().unwrap();
        assert_eq!(msg.contract.name, "gossip_test");
        assert_eq!(msg.code_hash, code_hash);
    }

    #[tokio::test]
    async fn test_handle_deployment_message() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(store).unwrap()));
        let runtime = Arc::new(RwLock::new(ContractRuntime::new(ledger)));

        let alice_kp = KeyPair::generate().unwrap();
        let alice = alice_kp.did().clone();

        let actor = ContractActor::new(alice.clone(), runtime.clone(), None);

        // Create a deployment message
        let contract = Contract::new("remote_deploy".to_string()).add_participant(alice.clone());

        let code_hash = compute_code_hash(&contract);
        let installed_at = 1234567890u64;
        let signatures = create_signatures(&code_hash, installed_at, &[&alice_kp]);

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: alice.clone(),
            capabilities: vec![],
            participants: vec![alice.clone()],
            signatures,
            installed_at,
            min_caller_trust: None,
        };

        let signing_bytes =
            ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
        let deployer_signature = alice_kp.sign(&signing_bytes).to_bytes().to_vec();

        let msg =
            ContractDeploymentMessage::new(code_hash.clone(), contract, installation, deployer_signature);

        // Handle the message
        actor.handle_deployment_message(msg).await.unwrap();

        // Verify contract was installed
        let contracts = actor.list_contracts().await;
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].name, "remote_deploy");
    }
}
