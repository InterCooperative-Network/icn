//! Integration tests for contract deployment and execution across nodes

use icn_ccl::{BinOp, Capability, Contract, ContractActor, ContractExecutionRequest, ContractInstallation, ContractRuntime, Expr, Rule, Stmt, Value};
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_ledger::{ContentHash, Ledger};
use icn_net::NetworkActor;
use icn_store::SledStore;
use icn_trust::TrustGraph;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Helper to create a test node with full actor stack
struct TestNode {
    did: icn_identity::Did,
    keypair: KeyPair,
    _network_handle: icn_net::NetworkHandle,
    _gossip_handle: Arc<RwLock<GossipActor>>,
    contract_actor: Arc<RwLock<ContractActor>>,
    contract_runtime: Arc<RwLock<ContractRuntime>>,
    trust_graph: Arc<RwLock<TrustGraph>>,
    _temp_dir: TempDir,
    _shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl TestNode {
    async fn new(port: u16) -> anyhow::Result<Self> {
        // Initialize Rustls crypto provider (required for TLS)
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        let temp_dir = TempDir::new()?;
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        // Initialize trust graph
        let trust_store_path = temp_dir.path().join("trust");
        let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&trust_store_path)?);
        let trust_graph = TrustGraph::new(trust_store, did.clone());
        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create trust lookup for gossip
        let trust_graph_for_gossip = trust_graph_handle.clone();
        let trust_lookup = Arc::new(move |peer_did: &icn_identity::Did| {
            let graph = trust_graph_for_gossip.clone();
            let peer = peer_did.clone();
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let graph = graph.read().await;
                    graph.trust_class(&peer).ok()
                })
            })
        });

        // Spawn gossip actor
        let gossip_handle = GossipActor::spawn_with_trust_graph(
            did.clone(),
            trust_lookup,
            Some(trust_graph_handle.clone()),
        );

        // Initialize ledger
        let ledger_store_path = temp_dir.path().join("ledger");
        let ledger_store = Arc::new(SledStore::open(&ledger_store_path)?);
        let mut ledger = Ledger::new(ledger_store)?;
        ledger.set_gossip(gossip_handle.clone());
        let ledger_handle = Arc::new(RwLock::new(ledger));

        // Initialize contract runtime
        let contract_runtime = ContractRuntime::new(ledger_handle.clone());
        let contract_runtime_handle = Arc::new(RwLock::new(contract_runtime));

        // Create contract actor
        let contract_actor = ContractActor::new(
            did.clone(),
            contract_runtime_handle.clone(),
            Some(trust_graph_handle.clone()),
        );
        let contract_actor_handle = Arc::new(RwLock::new(contract_actor));

        // Spawn network actor
        let listen_addr: std::net::SocketAddr = format!("127.0.0.1:{}", port).parse()?;

        let gossip_for_incoming = gossip_handle.clone();
        let incoming_handler: icn_net::IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender_did = net_msg.from.clone();

            if let icn_net::MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                let gossip_handle = gossip_for_incoming.clone();
                tokio::spawn(async move {
                    let mut gossip = gossip_handle.write().await;
                    let _ = gossip.handle_message(&sender_did, gossip_msg);
                });
            }
        });

        let network_handle = NetworkActor::spawn(
            &keypair,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No rate limiting for tests
            None,
            None,
        )
        .await?;

        // Set gossip send callback
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();
            let own_did_clone = did.clone();

            let send_callback: icn_gossip::SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
                let net_handle = network_handle_clone.clone();
                let from_did = own_did_clone.clone();

                tokio::spawn(async move {
                    let result = if let Some(target_did) = recipient {
                        let net_msg = icn_net::NetworkMessage::gossip(from_did, Some(target_did.clone()), gossip_msg);
                        net_handle.send_message(target_did, net_msg).await
                    } else {
                        let net_msg = icn_net::NetworkMessage::gossip(from_did, None, gossip_msg);
                        net_handle.broadcast(net_msg).await
                    };
                    if let Err(e) = result {
                        eprintln!("Failed to send gossip message: {}", e);
                    }
                });
            });

            gossip.set_send_callback(send_callback);

            // Set up contract deployment notification callback
            let contract_actor_for_notifications = contract_actor_handle.clone();
            let notification_callback: icn_gossip::EntryNotificationCallback = Arc::new(move |topic, entry, _subscriber_did| {
                if topic == "contracts:deploy" {
                    let contract_actor = contract_actor_for_notifications.clone();
                    let entry_data = entry.data.clone();

                    tokio::spawn(async move {
                        match serde_json::from_slice::<icn_ccl::ContractDeploymentMessage>(&entry_data) {
                            Ok(deployment_msg) => {
                                let mut actor = contract_actor.write().await;
                                if let Err(e) = actor.handle_deployment_message(deployment_msg).await {
                                    eprintln!("Failed to handle contract deployment: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to deserialize contract deployment: {}", e);
                            }
                        }
                    });
                }
            });

            gossip.set_notification_callback(notification_callback);

            // Create and subscribe to contracts:deploy topic
            use icn_gossip::{AccessControl, Topic};
            let contracts_topic = Topic::new("contracts:deploy".to_string(), AccessControl::Public)
                .with_max_entries(100);
            gossip.create_topic(contracts_topic);
            gossip.subscribe("contracts:deploy", did.clone())?;
        }

        Ok(TestNode {
            did,
            keypair,
            _network_handle: network_handle,
            _gossip_handle: gossip_handle,
            contract_actor: contract_actor_handle,
            contract_runtime: contract_runtime_handle,
            trust_graph: trust_graph_handle,
            _temp_dir: temp_dir,
            _shutdown_tx: shutdown_tx,
        })
    }

    /// Establish trust with another node
    async fn trust_peer(&self, peer_did: &icn_identity::Did, score: f64) -> anyhow::Result<()> {
        let mut graph = self.trust_graph.write().await;
        let edge = icn_trust::TrustEdge::new(self.did.clone(), peer_did.clone(), score);
        graph.add_edge(edge)?;
        Ok(())
    }

    /// Deploy a contract from this node
    async fn deploy_contract(
        &self,
        contract: Contract,
        capabilities: Vec<Capability>,
    ) -> anyhow::Result<ContentHash> {
        // Compute code hash
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(contract.name.as_bytes());
        for participant in &contract.participants {
            hasher.update(participant.as_str().as_bytes());
        }
        for rule in &contract.rules {
            hasher.update(rule.name.as_bytes());
        }
        let code_hash = ContentHash::from_bytes(hasher.finalize().into());

        let installed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // Generate deployer signature
        let signing_bytes = icn_ccl::ContractDeploymentMessage::compute_signing_bytes(
            &code_hash,
            installed_at,
        );
        let deployer_signature = self.keypair.sign(&signing_bytes);

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: self.did.clone(),
            capabilities,
            participants: contract.participants.clone(),
            signatures: vec![(self.did.clone(), deployer_signature.to_bytes().to_vec())],
            installed_at,
            min_caller_trust: None,
        };

        let actor = self.contract_actor.read().await;
        actor.deploy_contract(contract, installation, deployer_signature.to_bytes().to_vec()).await
    }

    /// Execute a contract rule
    async fn execute_contract(
        &self,
        code_hash: ContentHash,
        rule_name: String,
        args: std::collections::HashMap<String, Value>,
    ) -> anyhow::Result<icn_ccl::ExecutionResult> {
        let request = ContractExecutionRequest {
            code_hash,
            rule_name,
            args,
            caller: self.did.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        };

        let actor = self.contract_actor.read().await;
        actor.execute_rule(request).await
    }

    /// List installed contracts
    async fn list_contracts(&self) -> Vec<icn_ccl::ContractInfo> {
        let actor = self.contract_actor.read().await;
        actor.list_contracts().await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_two_node_contract_deployment() {
    // Create two nodes
    let node_a = TestNode::new(19001).await.expect("Failed to create node A");
    let node_b = TestNode::new(19002).await.expect("Failed to create node B");

    // Establish mutual trust (score 0.5 > MIN_DEPLOYER_TRUST 0.4)
    node_a.trust_peer(&node_b.did, 0.5).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.5).await.expect("Failed to trust A from B");

    // Connect nodes
    let addr_b: std::net::SocketAddr = "127.0.0.1:19002".parse().unwrap();
    node_a._network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    // Give connection time to establish
    sleep(Duration::from_millis(100)).await;

    // Create a simple contract
    let contract = Contract::new("TestContract".to_string())
        .add_participant(node_a.did.clone())
        .add_participant(node_b.did.clone())
        .add_rule(
            Rule::new("noop".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Bool(true)),
                }),
        );

    // Deploy from node A
    let code_hash = node_a
        .deploy_contract(contract, vec![])
        .await
        .expect("Failed to deploy contract");

    println!("Contract deployed from node A: {}", code_hash);

    // Wait for gossip propagation
    sleep(Duration::from_millis(500)).await;

    // Verify node A has the contract
    let contracts_a = node_a.list_contracts().await;
    assert_eq!(contracts_a.len(), 1, "Node A should have 1 contract");
    assert_eq!(contracts_a[0].name, "TestContract");

    // Verify node B received the contract
    let contracts_b = node_b.list_contracts().await;
    assert_eq!(contracts_b.len(), 1, "Node B should have received the contract");
    assert_eq!(contracts_b[0].name, "TestContract");

    println!("✓ Contract successfully deployed and replicated to both nodes");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_contract_execution_after_deployment() {
    // Create two nodes
    let node_a = TestNode::new(19003).await.expect("Failed to create node A");
    let node_b = TestNode::new(19004).await.expect("Failed to create node B");

    // Establish mutual trust
    node_a.trust_peer(&node_b.did, 0.5).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.5).await.expect("Failed to trust A from B");

    // Connect nodes
    let addr_b: std::net::SocketAddr = "127.0.0.1:19004".parse().unwrap();
    node_a._network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    sleep(Duration::from_millis(100)).await;

    // Create contract with a rule
    let contract = Contract::new("Calculator".to_string())
        .add_participant(node_a.did.clone())
        .add_participant(node_b.did.clone())
        .add_rule(
            Rule::new("add".to_string())
                .add_param("a".to_string())
                .add_param("b".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Var("a".to_string())),
                        right: Box::new(Expr::Var("b".to_string())),
                    },
                }),
        );

    // Deploy from node A
    let code_hash = node_a
        .deploy_contract(contract, vec![])
        .await
        .expect("Failed to deploy contract");

    // Wait for propagation
    sleep(Duration::from_millis(500)).await;

    // Execute on node A
    let mut args_a = std::collections::HashMap::new();
    args_a.insert("a".to_string(), Value::Int(5));
    args_a.insert("b".to_string(), Value::Int(3));

    let result_a = node_a
        .execute_contract(code_hash.clone(), "add".to_string(), args_a)
        .await
        .expect("Failed to execute on node A");

    assert_eq!(result_a.value, Value::Int(8), "Node A execution result incorrect");

    // Execute on node B (should also work after receiving deployment)
    let mut args_b = std::collections::HashMap::new();
    args_b.insert("a".to_string(), Value::Int(10));
    args_b.insert("b".to_string(), Value::Int(7));

    let result_b = node_b
        .execute_contract(code_hash, "add".to_string(), args_b)
        .await
        .expect("Failed to execute on node B");

    assert_eq!(result_b.value, Value::Int(17), "Node B execution result incorrect");

    println!("✓ Contract executed successfully on both nodes");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_untrusted_deployer_rejected() {
    // Create two nodes
    let node_a = TestNode::new(19005).await.expect("Failed to create node A");
    let node_b = TestNode::new(19006).await.expect("Failed to create node B");

    // Node B trusts A with LOW score (0.2 < MIN_DEPLOYER_TRUST 0.4)
    node_b.trust_peer(&node_a.did, 0.2).await.expect("Failed to set low trust");

    // Connect nodes
    let addr_b: std::net::SocketAddr = "127.0.0.1:19006".parse().unwrap();
    node_a._network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    sleep(Duration::from_millis(100)).await;

    // Create contract
    let contract = Contract::new("UntrustedContract".to_string())
        .add_participant(node_a.did.clone())
        .add_rule(
            Rule::new("test".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Bool(true)),
                }),
        );

    // Deploy from node A (trust score 0.2 < 0.4, should be rejected by B)
    let _code_hash = node_a
        .deploy_contract(contract, vec![])
        .await
        .expect("Node A should deploy locally");

    // Wait for potential propagation
    sleep(Duration::from_millis(500)).await;

    // Verify node A has it
    let contracts_a = node_a.list_contracts().await;
    assert_eq!(contracts_a.len(), 1, "Node A should have the contract");

    // Verify node B rejected it (insufficient trust)
    let contracts_b = node_b.list_contracts().await;
    assert_eq!(
        contracts_b.len(),
        0,
        "Node B should have rejected the contract due to insufficient trust"
    );

    println!("✓ Untrusted deployer correctly rejected");
}
