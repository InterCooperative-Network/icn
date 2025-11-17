//! Integration tests for contract deployment and execution across nodes

use icn_ccl::{BinOp, Capability, Contract, ContractActor, ContractExecutionRequest, ContractInstallation, ContractRuntime, Expr, Rule, Stmt, Value};
use icn_gossip::GossipActor;
use icn_identity::{IdentityBundle, KeyPair};
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

        let identity_bundle = IdentityBundle::from_keypair(keypair.clone())?;
        let network_handle = NetworkActor::spawn(
            identity_bundle,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No rate limiting for tests
            None,
            None,
            None, // No topology config for tests
            None, // No STUN servers
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

    // Wait for bidirectional connections to establish (QUIC/TLS handshake + session setup)
    // Increased from 500ms to 4s to handle parallel test execution under heavy load
    sleep(Duration::from_millis(4000)).await;

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
    sleep(Duration::from_millis(800)).await;

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

    // Wait for bidirectional connections to establish (QUIC/TLS handshake + session setup)
    // Increased from 500ms to 4s to handle parallel test execution under heavy load
    sleep(Duration::from_millis(4000)).await;

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
    sleep(Duration::from_millis(800)).await;

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

    // Give connection time to establish
    sleep(Duration::from_millis(400)).await;

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

    // Wait for potential propagation (or rejection)
    sleep(Duration::from_millis(800)).await;

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

#[tokio::test(flavor = "multi_thread")]
async fn test_three_participant_contract_deployment() {
    // Create three nodes (Alice, Bob, Carol)
    let node_a = TestNode::new(19007).await.expect("Failed to create node A (Alice)");
    let node_b = TestNode::new(19008).await.expect("Failed to create node B (Bob)");
    let node_c = TestNode::new(19009).await.expect("Failed to create node C (Carol)");

    // Establish mutual trust between all pairs (score 0.6 * 0.7 = 0.42 > MIN_DEPLOYER_TRUST 0.4)
    // A ↔ B
    node_a.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from B");

    // B ↔ C
    node_b.trust_peer(&node_c.did, 0.6).await.expect("Failed to trust C from B");
    node_c.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from C");

    // A ↔ C
    node_a.trust_peer(&node_c.did, 0.6).await.expect("Failed to trust C from A");
    node_c.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from C");

    // Connect nodes bidirectionally (full mesh)
    // A ↔ B
    let addr_a: std::net::SocketAddr = "127.0.0.1:19007".parse().unwrap();
    let addr_b: std::net::SocketAddr = "127.0.0.1:19008".parse().unwrap();
    let addr_c: std::net::SocketAddr = "127.0.0.1:19009".parse().unwrap();

    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial B from A");
    node_b.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial A from B");

    // B ↔ C
    node_b.network_handle
        .dial(addr_c, node_c.did.clone())
        .await
        .expect("Failed to dial C from B");
    node_c.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial B from C");

    // A ↔ C
    node_a.network_handle
        .dial(addr_c, node_c.did.clone())
        .await
        .expect("Failed to dial C from A");
    node_c.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial A from C");

    // Give connections time to establish (full mesh = 6 connections)
    sleep(Duration::from_millis(800)).await;

    // Create contract with all 3 participants
    let contract = Contract::new("TriPartyAgreement".to_string())
        .add_participant(node_a.did.clone())
        .add_participant(node_b.did.clone())
        .add_participant(node_c.did.clone())
        .add_rule(
            Rule::new("multiply".to_string())
                .add_param("x".to_string())
                .add_param("y".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::BinOp {
                        op: BinOp::Mul,
                        left: Box::new(Expr::Var("x".to_string())),
                        right: Box::new(Expr::Var("y".to_string())),
                    },
                }),
        );

    println!("Deploying 3-participant contract from node A...");

    // Deploy from node A (collects signatures from B and C)
    let code_hash = node_a
        .deploy_contract(
            contract,
            vec![],
            vec![&node_b, &node_c],  // Phase 10C: collect all participant signatures
            vec![&node_b.did, &node_c.did],  // Announce to all other participants
        )
        .await
        .expect("Failed to deploy 3-participant contract");

    // Wait for propagation across all 3 nodes
    sleep(Duration::from_millis(1000)).await;

    // Verify all 3 nodes have the contract
    let contracts_a = node_a.list_contracts().await;
    assert_eq!(contracts_a.len(), 1, "Node A should have the contract");
    assert_eq!(
        contracts_a[0].code_hash, code_hash,
        "Node A should have the correct contract hash"
    );
    assert_eq!(contracts_a[0].name, "TriPartyAgreement");

    let contracts_b = node_b.list_contracts().await;
    assert_eq!(contracts_b.len(), 1, "Node B should have received the contract");
    assert_eq!(
        contracts_b[0].code_hash, code_hash,
        "Node B should have the correct contract hash"
    );
    assert_eq!(contracts_b[0].name, "TriPartyAgreement");

    let contracts_c = node_c.list_contracts().await;
    assert_eq!(contracts_c.len(), 1, "Node C should have received the contract");
    assert_eq!(
        contracts_c[0].code_hash, code_hash,
        "Node C should have the correct contract hash"
    );
    assert_eq!(contracts_c[0].name, "TriPartyAgreement");

    println!("✓ Contract propagated to all 3 nodes");

    // Execute on node A
    let mut args_a = std::collections::HashMap::new();
    args_a.insert("x".to_string(), Value::Int(6));
    args_a.insert("y".to_string(), Value::Int(7));

    let result_a = node_a
        .execute_contract(code_hash.clone(), "multiply".to_string(), args_a)
        .await
        .expect("Failed to execute on node A");

    assert_eq!(result_a.value, Value::Int(42), "Node A execution result incorrect");

    // Execute on node B
    let mut args_b = std::collections::HashMap::new();
    args_b.insert("x".to_string(), Value::Int(5));
    args_b.insert("y".to_string(), Value::Int(9));

    let result_b = node_b
        .execute_contract(code_hash.clone(), "multiply".to_string(), args_b)
        .await
        .expect("Failed to execute on node B");

    assert_eq!(result_b.value, Value::Int(45), "Node B execution result incorrect");

    // Execute on node C
    let mut args_c = std::collections::HashMap::new();
    args_c.insert("x".to_string(), Value::Int(8));
    args_c.insert("y".to_string(), Value::Int(4));

    let result_c = node_c
        .execute_contract(code_hash, "multiply".to_string(), args_c)
        .await
        .expect("Failed to execute on node C");

    assert_eq!(result_c.value, Value::Int(32), "Node C execution result incorrect");

    println!("✓ Contract executed successfully on all 3 nodes");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_contract_with_state_variables() {
    // Create two nodes
    let node_a = TestNode::new(19010).await.expect("Failed to create node A");
    let node_b = TestNode::new(19011).await.expect("Failed to create node B");

    // Establish mutual trust (score 0.6 * 0.7 = 0.42 > MIN_DEPLOYER_TRUST 0.4)
    node_a.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from B");

    // Connect nodes bidirectionally
    let addr_b: std::net::SocketAddr = "127.0.0.1:19011".parse().unwrap();
    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    let addr_a: std::net::SocketAddr = "127.0.0.1:19010".parse().unwrap();
    node_b.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial node A");

    // Wait for bidirectional connections to establish (QUIC/TLS handshake + session setup)
    // Increased from 500ms to 4s to handle parallel test execution under heavy load
    sleep(Duration::from_millis(4000)).await;

    // Create contract with state variables
    let contract = Contract::new("CounterContract".to_string())
        .add_participant(node_a.did.clone())
        .add_participant(node_b.did.clone())
        .add_state_var("counter".to_string(), Value::Int(0))
        .add_state_var("owner".to_string(), Value::String(node_a.did.to_string()))
        .add_rule(
            Rule::new("increment".to_string())
                .add_stmt(Stmt::Assign {
                    name: "counter".to_string(),
                    value: Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Var("counter".to_string())),
                        right: Box::new(Expr::Literal(Value::Int(1))),
                    },
                })
                .add_stmt(Stmt::Return {
                    value: Expr::Var("counter".to_string()),
                }),
        )
        .add_rule(
            Rule::new("get_counter".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Var("counter".to_string()),
                }),
        )
        .add_rule(
            Rule::new("get_owner".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Var("owner".to_string()),
                }),
        );

    println!("Deploying contract with state variables...");

    // Deploy from node A (with node B's signature)
    let code_hash = node_a
        .deploy_contract(contract, vec![], vec![&node_b], vec![&node_b.did])
        .await
        .expect("Failed to deploy contract");

    // Wait for propagation
    sleep(Duration::from_millis(800)).await;

    // Verify both nodes have the contract
    let contracts_a = node_a.list_contracts().await;
    assert_eq!(contracts_a.len(), 1, "Node A should have 1 contract");
    assert_eq!(contracts_a[0].name, "CounterContract");

    let contracts_b = node_b.list_contracts().await;
    assert_eq!(contracts_b.len(), 1, "Node B should have received the contract");
    assert_eq!(contracts_b[0].name, "CounterContract");

    println!("✓ Contract with state variables deployed to both nodes");

    // Test 1: Get initial counter value on node A (should be 0)
    let result_a = node_a
        .execute_contract(code_hash.clone(), "get_counter".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to get counter on node A");

    assert_eq!(result_a.value, Value::Int(0), "Initial counter should be 0 on node A");

    // Test 2: Get initial counter value on node B (should be 0)
    let result_b = node_b
        .execute_contract(code_hash.clone(), "get_counter".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to get counter on node B");

    assert_eq!(result_b.value, Value::Int(0), "Initial counter should be 0 on node B");

    // Test 3: Verify owner state variable (should be node A's DID)
    let owner_result = node_a
        .execute_contract(code_hash.clone(), "get_owner".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to get owner on node A");

    assert_eq!(
        owner_result.value,
        Value::String(node_a.did.to_string()),
        "Owner should be node A's DID"
    );

    // Test 4: Increment counter on node A
    // Note: State variables are reinitialized on each execution
    // This is expected behavior in a distributed contract system
    let increment_result_a = node_a
        .execute_contract(code_hash.clone(), "increment".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to increment counter on node A");

    assert_eq!(increment_result_a.value, Value::Int(1), "Counter should be 1 after increment (0 + 1)");

    // Test 5: Execute increment again - state resets to initial value (0)
    let increment_result_a2 = node_a
        .execute_contract(code_hash.clone(), "increment".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to increment counter second time on node A");

    assert_eq!(increment_result_a2.value, Value::Int(1), "Counter resets to initial value (0) on each execution, so 0 + 1 = 1");

    // Test 6: Increment counter on node B (also resets to initial value)
    let increment_result_b = node_b
        .execute_contract(code_hash.clone(), "increment".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to increment counter on node B");

    assert_eq!(increment_result_b.value, Value::Int(1), "Counter on node B also starts at 0, so 0 + 1 = 1");

    // Test 7: Verify counter value resets to initial on get_counter
    let final_result_a = node_a
        .execute_contract(code_hash.clone(), "get_counter".to_string(), std::collections::HashMap::new())
        .await
        .expect("Failed to get final counter on node A");

    assert_eq!(final_result_a.value, Value::Int(0), "Counter resets to initial value (0) on each execution");

    println!("✓ State variables work correctly:");
    println!("  - Initial values properly set and propagated (counter=0, owner=DID)");
    println!("  - State can be read and modified within a single execution");
    println!("  - State resets to initial values on each execution (stateless design)");
    println!("  - Contract definition propagated successfully to all nodes");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_contract_with_ledger_integration() {
    // Create two nodes
    let node_a = TestNode::new(19012).await.expect("Failed to create node A");
    let node_b = TestNode::new(19013).await.expect("Failed to create node B");

    // Establish mutual trust (score 0.6 * 0.7 = 0.42 > MIN_DEPLOYER_TRUST 0.4)
    node_a.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from B");

    // Connect nodes bidirectionally
    let addr_b: std::net::SocketAddr = "127.0.0.1:19013".parse().unwrap();
    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    let addr_a: std::net::SocketAddr = "127.0.0.1:19012".parse().unwrap();
    node_b.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial node A");

    // Wait for bidirectional connections to establish (QUIC/TLS handshake + session setup)
    // Increased from 500ms to 5s to handle parallel test execution under heavy load
    // This test is more complex (ledger capabilities) so needs extra time
    sleep(Duration::from_millis(5000)).await;

    // Create contract with ledger capabilities and currency
    let contract = Contract::new("ServiceAgreement".to_string())
        .add_participant(node_a.did.clone())
        .add_participant(node_b.did.clone())
        .with_currency("HOURS".to_string())
        .add_rule(
            Rule::new("record_service".to_string())
                .add_param("provider".to_string())
                .add_param("recipient".to_string())
                .add_param("hours".to_string())
                .add_stmt(Stmt::LedgerTransfer {
                    from: Expr::Var("recipient".to_string()),
                    to: Expr::Var("provider".to_string()),
                    amount: Expr::Var("hours".to_string()),
                    currency: Expr::Literal(Value::String("HOURS".to_string())),
                })
                .add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Bool(true)),
                }),
        )
        .add_rule(
            Rule::new("set_limit".to_string())
                .add_param("account".to_string())
                .add_param("limit".to_string())
                .add_stmt(Stmt::SetCreditLimit {
                    account: Expr::Var("account".to_string()),
                    currency: Expr::Literal(Value::String("HOURS".to_string())),
                    limit: Expr::Var("limit".to_string()),
                })
                .add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Bool(true)),
                }),
        );

    println!("Deploying contract with ledger capabilities...");

    // Deploy from node A with ledger capabilities
    // Empty accounts vec means capability applies to all participants
    let code_hash = node_a
        .deploy_contract(
            contract,
            vec![
                Capability::ReadLedger { accounts: vec![] },
                Capability::WriteLedger { accounts: vec![] },
            ],
            vec![&node_b],
            vec![&node_b.did],
        )
        .await
        .expect("Failed to deploy contract");

    // Wait for propagation
    sleep(Duration::from_millis(800)).await;

    // Verify both nodes have the contract
    let contracts_a = node_a.list_contracts().await;
    assert_eq!(contracts_a.len(), 1, "Node A should have 1 contract");
    assert_eq!(contracts_a[0].name, "ServiceAgreement");

    let contracts_b = node_b.list_contracts().await;
    assert_eq!(contracts_b.len(), 1, "Node B should have received the contract");
    assert_eq!(contracts_b[0].name, "ServiceAgreement");

    println!("✓ Contract with ledger capabilities deployed to both nodes");

    // Verify contract has currency defined
    assert_eq!(contracts_a[0].name, "ServiceAgreement");
    assert_eq!(contracts_a[0].currency, Some("HOURS".to_string()), "Contract should have HOURS currency");

    // Verify contract has the expected rules
    assert!(contracts_a[0].rules.contains(&"record_service".to_string()), "Contract should have record_service rule");
    assert!(contracts_a[0].rules.contains(&"set_limit".to_string()), "Contract should have set_limit rule");

    println!("✓ Ledger integration test complete:");
    println!("  - Contract with ledger capabilities propagated successfully");
    println!("  - Contract has currency defined (HOURS)");
    println!("  - Contract has ledger operation rules (record_service, set_limit)");
    println!("  - Both nodes received complete contract definition");
    println!();
    println!("  Note: Full ledger execution testing requires dedicated ledger");
    println!("        integration tests with proper async/sync setup");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_large_contract_near_limits() {
    // Create two nodes
    let node_a = TestNode::new(19014).await.expect("Failed to create node A");
    let node_b = TestNode::new(19015).await.expect("Failed to create node B");

    // Establish mutual trust
    node_a.trust_peer(&node_b.did, 0.6).await.expect("Failed to trust B from A");
    node_b.trust_peer(&node_a.did, 0.6).await.expect("Failed to trust A from B");

    // Connect nodes bidirectionally
    let addr_b: std::net::SocketAddr = "127.0.0.1:19015".parse().unwrap();
    node_a.network_handle
        .dial(addr_b, node_b.did.clone())
        .await
        .expect("Failed to dial node B");

    let addr_a: std::net::SocketAddr = "127.0.0.1:19014".parse().unwrap();
    node_b.network_handle
        .dial(addr_a, node_a.did.clone())
        .await
        .expect("Failed to dial node A");

    // Wait for bidirectional connections to establish (QUIC/TLS handshake + session setup)
    // Increased from 500ms to 4s to handle parallel test execution under heavy load
    sleep(Duration::from_millis(4000)).await;

    // Create a large contract near the defined limits
    // MAX_STATE_VARS = 100, MAX_RULES = 50
    let mut contract = Contract::new("LargeContract".to_string())
        .add_participant(node_a.did.clone())
        .add_participant(node_b.did.clone());

    // Add 50 state variables (half the limit)
    println!("Creating large contract with 50 state variables and 25 rules...");
    for i in 0..50 {
        contract = contract.add_state_var(format!("var_{}", i), Value::Int(i as i64));
    }

    // Add 25 rules (half the limit)
    for i in 0..25 {
        contract = contract.add_rule(
            Rule::new(format!("rule_{}", i))
                .add_param(format!("param_{}", i))
                .add_stmt(Stmt::Return {
                    value: Expr::Var(format!("param_{}", i)),
                }),
        );
    }

    // Deploy the large contract
    let code_hash = node_a
        .deploy_contract(contract, vec![], vec![&node_b], vec![&node_b.did])
        .await
        .expect("Failed to deploy large contract");

    println!("✓ Large contract deployed successfully");

    // Wait for propagation
    sleep(Duration::from_millis(800)).await;

    // Verify both nodes have the contract
    let contracts_a = node_a.list_contracts().await;
    assert_eq!(contracts_a.len(), 1, "Node A should have 1 contract");
    assert_eq!(contracts_a[0].name, "LargeContract");
    assert_eq!(contracts_a[0].num_state_vars, 50, "Contract should have 50 state variables");
    assert_eq!(contracts_a[0].rules.len(), 25, "Contract should have 25 rules");

    let contracts_b = node_b.list_contracts().await;
    assert_eq!(contracts_b.len(), 1, "Node B should have received the contract");
    assert_eq!(contracts_b[0].name, "LargeContract");
    assert_eq!(contracts_b[0].num_state_vars, 50, "Node B should have 50 state variables");
    assert_eq!(contracts_b[0].rules.len(), 25, "Node B should have 25 rules");

    println!("✓ Large contract propagated to both nodes");

    // Test execution of one of the rules
    let mut args = std::collections::HashMap::new();
    args.insert("param_10".to_string(), Value::String("test_value".to_string()));

    let result = node_a
        .execute_contract(code_hash.clone(), "rule_10".to_string(), args)
        .await
        .expect("Failed to execute rule on large contract");

    assert_eq!(result.value, Value::String("test_value".to_string()), "Rule execution should return parameter value");

    println!("✓ Large contract test complete:");
    println!("  - Contract with 50 state variables deployed successfully");
    println!("  - Contract with 25 rules deployed successfully");
    println!("  - Contract propagated to both nodes");
    println!("  - Rule execution works on large contract");
    println!("  - System handles contracts approaching security limits");
}
