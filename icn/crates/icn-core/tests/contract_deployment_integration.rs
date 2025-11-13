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
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
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
        // This is needed for the pull protocol (Pull requests and Responses)
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();
            let own_did_clone = did.clone();

            let send_callback: icn_gossip::SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
                let net_handle = network_handle_clone.clone();
                let from_did = own_did_clone.clone();

                tokio::spawn(async move {
                    let msg_type = match &gossip_msg {
                        icn_gossip::GossipMessage::Announce { .. } => "Announce",
                        icn_gossip::GossipMessage::Request { .. } => "Request",
                        icn_gossip::GossipMessage::Response { .. } => "Response",
                        icn_gossip::GossipMessage::RequestBloomFilter { .. } => "RequestBloomFilter",
                        icn_gossip::GossipMessage::SendBloomFilter { .. } => "SendBloomFilter",
                        icn_gossip::GossipMessage::RequestMissing { .. } => "RequestMissing",
                        icn_gossip::GossipMessage::Digest { .. } => "Digest",
                        icn_gossip::GossipMessage::PullRequest { .. } => "PullRequest",
                        icn_gossip::GossipMessage::PullResponse { .. } => "PullResponse",
                    };

                    let result = if let Some(target_did) = recipient {
                        eprintln!("Sending {} from {} to {}", msg_type, from_did, target_did);
                        let net_msg = icn_net::NetworkMessage::gossip(from_did.clone(), Some(target_did.clone()), gossip_msg);
                        net_handle.send_message(target_did, net_msg).await
                    } else {
                        // Skip broadcast in tests - we don't have peer tracking
                        eprintln!("Skipping broadcast {} from {}", msg_type, from_did);
                        return;
                    };
                    match result {
                        Ok(_) => eprintln!("✓ {} sent successfully", msg_type),
                        Err(e) => eprintln!("✗ Failed to send {}: {}", msg_type, e),
                    }
                });
            });

            gossip.set_send_callback(send_callback);
        }

        // Set up contract deployment notification callback
        {
            let mut gossip = gossip_handle.write().await;
            let contract_actor_for_notifications = contract_actor_handle.clone();
            let notification_callback: icn_gossip::EntryNotificationCallback = Arc::new(move |topic, entry, _subscriber_did| {
                if topic == "contracts:deploy" {
                    let contract_actor = contract_actor_for_notifications.clone();
                    // Use get_data() to handle decompression if needed
                    let entry_data = match entry.get_data() {
                        Ok(data) => data,
                        Err(e) => {
                            eprintln!("Failed to get entry data: {}", e);
                            return;
                        }
                    };

                    tokio::spawn(async move {
                        match serde_json::from_slice::<icn_ccl::ContractDeploymentMessage>(&entry_data) {
                            Ok(deployment_msg) => {
                                let actor = contract_actor.write().await;
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
            network_handle,
            gossip_handle,
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

    /// Deploy a contract from this node (Phase 10C: Multi-party signature collection)
    async fn deploy_contract(
        &self,
        contract: Contract,
        capabilities: Vec<Capability>,
        other_participants: Vec<&TestNode>,  // Other participant nodes to collect signatures from
        announce_to: Vec<&icn_identity::Did>,
    ) -> anyhow::Result<ContentHash> {
        // Compute code hash (must match ContractActor::compute_code_hash)
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(contract.name.as_bytes());
        for participant in &contract.participants {
            hasher.update(format!("{:?}", participant).as_bytes());
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

        // Collect signatures from all participants (Phase 10C)
        let mut signatures = vec![(self.did.clone(), deployer_signature.to_bytes().to_vec())];

        // Add signatures from other participants
        for participant_node in &other_participants {
            let participant_signature = participant_node.keypair.sign(&signing_bytes);
            signatures.push((participant_node.did.clone(), participant_signature.to_bytes().to_vec()));
        }

        let installation = ContractInstallation {
            code_hash: code_hash.clone(),
            installed_by: self.did.clone(),
            capabilities,
            participants: contract.participants.clone(),
            signatures,  // Now contains all participant signatures
            installed_at,
            min_caller_trust: None,
        };

        // Deploy locally first
        let actor = self.contract_actor.read().await;
        let result_hash = actor.deploy_contract(contract.clone(), installation.clone(), deployer_signature.to_bytes().to_vec()).await?;

        // Publish deployment message to gossip for distribution
        let deployment_msg = icn_ccl::ContractDeploymentMessage {
            code_hash: code_hash.clone(),
            contract,
            installation,
            deployer_signature: deployer_signature.to_bytes().to_vec(),
        };

        let message_bytes = serde_json::to_vec(&deployment_msg)?;

        // Publish locally and get the entry to announce
        let (hash, clock) = {
            let mut gossip = self.gossip_handle.write().await;
            let hash = gossip.publish("contracts:deploy", message_bytes)?;
            let entry = gossip.get_entry("contracts:deploy", &hash)
                .ok_or_else(|| anyhow::anyhow!("Published entry not found"))?;
            (hash, entry.clock)
        };

        // Send Announce message to each connected peer
        for peer_did in announce_to {
            eprintln!("Sending Announce for contract {} to {}", hash.iter().take(8).map(|b| format!("{:02x}", b)).collect::<String>(), peer_did);
            let announce_msg = icn_gossip::GossipMessage::Announce {
                hash,
                author: self.did.clone(),
                clock: clock.clone(),
                topic: "contracts:deploy".to_string(),
            };
            let net_msg = icn_net::NetworkMessage::gossip(
                self.did.clone(),
                Some(peer_did.clone()),
                announce_msg
            );
            match self.network_handle.send_message(peer_did.clone(), net_msg).await {
                Ok(_) => eprintln!("✓ Announce sent successfully to {}", peer_did),
                Err(e) => {
                    eprintln!("✗ Failed to send Announce to {}: {}", peer_did, e);
                    return Err(e.into());
                }
            }
        }

        Ok(result_hash)
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

    // Establish mutual trust (score 0.6 * 0.7 = 0.42 > MIN_DEPLOYER_TRUST 0.4)
    // Note: Trust graph applies 70% direct + 30% transitive weighting
    node_a.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from B");

    // Connect nodes bidirectionally
    // Node A dials Node B
    let addr_b: std::net::SocketAddr = "127.0.0.1:19002".parse().unwrap();
    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    // Node B dials Node A
    let addr_a: std::net::SocketAddr = "127.0.0.1:19001".parse().unwrap();
    node_b.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial node A");

    // Give connections time to establish
    sleep(Duration::from_millis(300)).await;

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

    // Deploy from node A (with node B's signature collected)
    let code_hash = node_a
        .deploy_contract(contract, vec![], vec![&node_b], vec![&node_b.did])
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

    // Establish mutual trust (score 0.6 * 0.7 = 0.42 > MIN_DEPLOYER_TRUST 0.4)
    // Note: Trust graph applies 70% direct + 30% transitive weighting
    node_a.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from B");

    // Connect nodes bidirectionally
    // Node A dials Node B
    let addr_b: std::net::SocketAddr = "127.0.0.1:19004".parse().unwrap();
    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    // Node B dials Node A
    let addr_a: std::net::SocketAddr = "127.0.0.1:19003".parse().unwrap();
    node_b.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial node A");

    sleep(Duration::from_millis(300)).await;

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

    // Deploy from node A (with node B's signature collected)
    let code_hash = node_a
        .deploy_contract(contract, vec![], vec![&node_b], vec![&node_b.did])
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
    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    sleep(Duration::from_millis(200)).await;

    // Create contract
    let contract = Contract::new("UntrustedContract".to_string())
        .add_participant(node_a.did.clone())
        .add_rule(
            Rule::new("test".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Bool(true)),
                }),
        );

    // Deploy from node A and announce to node B (trust score 0.2 < 0.4, should be rejected by B)
    // Single-participant contract, no other signatures needed
    let _code_hash = node_a
        .deploy_contract(contract, vec![], vec![], vec![&node_b.did])
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
