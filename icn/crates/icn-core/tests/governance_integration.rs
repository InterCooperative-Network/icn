//! Integration test for governance over gossip protocol
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! This test validates end-to-end governance flow:
//! 1. Multi-node governance domain setup
//! 2. Proposal creation and gossip propagation
//! 3. Distributed voting
//! 4. Outcome convergence across all nodes

use anyhow::{bail, Result};
use icn_gossip::GossipActor;
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceMessage, GovernanceParams,
    GovernanceProfile, GovernanceProfileId, MembershipConfig, MembershipResolver, Proposal,
    ProposalId, ProposalOutcome, ProposalPayload, ProposalState, StaticMembershipResolver,
    TallySnapshot, Vote, VoteChoice, VoteTally,
};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor};
use icn_trust::TrustClass;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Get an available port for testing
fn get_available_port() -> u16 {
    portpicker::pick_unused_port().expect("No available ports")
}

const GOVERNANCE_TOPIC: &str = "governance:proposal";

/// Helper to create a test node with governance support
struct TestNode {
    _keypair: KeyPair,
    did: Did,
    network_handle: icn_net::NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    listen_addr: SocketAddr,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    // Local storage for governance state
    domains: Arc<RwLock<HashMap<GovernanceDomainId, GovernanceDomain>>>,
    proposals: Arc<RwLock<HashMap<ProposalId, Proposal>>>,
    votes: Arc<RwLock<HashMap<(ProposalId, Did), Vote>>>,
}

impl TestNode {
    async fn spawn(port: u16) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning test node with DID: {}", did);

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(16);

        // Spawn gossip actor
        let trust_lookup = Arc::new(|_did: &Did| Some(TrustClass::Partner));
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        info!("Gossip actor spawned");

        // Local governance state
        let domains: Arc<RwLock<HashMap<GovernanceDomainId, GovernanceDomain>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let proposals: Arc<RwLock<HashMap<ProposalId, Proposal>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let votes: Arc<RwLock<HashMap<(ProposalId, Did), Vote>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Create incoming message handler that processes both Gossip and governance messages
        let gossip_handle_clone = gossip_handle.clone();

        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender_did = net_msg.from.clone();
            let gossip_clone = gossip_handle_clone.clone();

            if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
                // Log the gossip message type for debugging
                info!(
                    "Incoming gossip message: {} from {}",
                    gossip_msg.variant_name(),
                    sender_did
                );

                // Spawn async task to avoid blocking in callback
                tokio::spawn(async move {
                    let mut gossip = gossip_clone.write().await;
                    if let Err(e) = gossip.handle_message(&sender_did, gossip_msg).await {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                });
            }
        });

        // Set up notification callback for governance messages
        let domains_notify = domains.clone();
        let proposals_notify = proposals.clone();
        let votes_notify = votes.clone();
        let did_notify = did.clone();

        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_notification_callback(Arc::new(move |topic, entry, _subscriber_did| {
                if topic != GOVERNANCE_TOPIC {
                    return;
                }

                // Deserialize governance message
                let gov_msg = match GovernanceMessage::from_bytes(&entry.data) {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!("Failed to deserialize governance message: {}", e);
                        return;
                    }
                };

                info!("[{}] Received {}", did_notify, gov_msg.message_type());

                // Clone Arc references for the spawned task
                let domains_clone = domains_notify.clone();
                let proposals_clone = proposals_notify.clone();
                let votes_clone = votes_notify.clone();

                // Spawn async task to handle the message (avoid blocking in callback)
                tokio::spawn(async move {
                    match gov_msg {
                        GovernanceMessage::DomainCreated { domain } => {
                            let domain_id = domain.id.clone();
                            info!("Storing domain with ID: {:?}", domain_id);
                            domains_clone
                                .write()
                                .await
                                .insert(domain_id.clone(), domain);
                            info!(
                                "Domain stored, total domains: {}",
                                domains_clone.read().await.len()
                            );
                        }
                        GovernanceMessage::ProposalCreated { proposal } => {
                            let proposal_id = proposal.id.clone();
                            proposals_clone.write().await.insert(proposal_id, proposal);
                        }
                        GovernanceMessage::ProposalOpened {
                            id,
                            opened_at,
                            closes_at,
                        } => {
                            if let Some(proposal) = proposals_clone.write().await.get_mut(&id) {
                                // Calculate duration from opened_at and closes_at
                                let duration = closes_at.saturating_sub(opened_at);
                                let _ = proposal.open(duration);
                            }
                        }
                        GovernanceMessage::VoteCast { vote, .. } => {
                            let key = (vote.proposal_id.clone(), vote.voter.clone());
                            votes_clone.write().await.insert(key, vote);
                        }
                        GovernanceMessage::ProposalClosed {
                            id,
                            outcome,
                            closed_at,
                            tally: _,
                        } => {
                            if let Some(proposal) = proposals_clone.write().await.get_mut(&id) {
                                // Update proposal state based on outcome
                                let new_state = match outcome {
                                    ProposalOutcome::Accepted => {
                                        ProposalState::Accepted { closed_at }
                                    }
                                    ProposalOutcome::Rejected => {
                                        ProposalState::Rejected { closed_at }
                                    }
                                    ProposalOutcome::NoQuorum => {
                                        ProposalState::NoQuorum { closed_at }
                                    }
                                };
                                // Use close() method to update state properly
                                let _ = proposal.close(new_state);
                            }
                        }
                        _ => {}
                    }
                });
            }));
        }

        // Spawn network actor
        let listen_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone())?;
        let network_handle = NetworkActor::spawn(
            identity_bundle,
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No trust graph for tests
            None, // No trust-gated config for tests
            None, // No fallback config for tests
            None, // No topology config
            None, // No STUN servers
            None, // No TURN config
            None, // No misbehavior detector for tests
            None, // No store for tests
        )
        .await?;

        info!("Network actor spawned on {}", listen_addr);

        // Wire up gossip send callback to route messages over network
        {
            let mut gossip = gossip_handle.write().await;
            let network_handle_clone = network_handle.clone();
            let from_did = did.clone();

            let send_callback = Arc::new(
                move |recipient: Option<icn_identity::Did>,
                      gossip_msg: icn_gossip::GossipMessage| {
                    let net_handle = network_handle_clone.clone();
                    let from = from_did.clone();
                    let msg_type = gossip_msg.variant_name();

                    info!("Send callback: sending {} to {:?}", msg_type, recipient);

                    tokio::spawn(async move {
                        let net_msg =
                            icn_net::NetworkMessage::gossip(from, recipient.clone(), gossip_msg);

                        let result = if let Some(to_did) = recipient {
                            net_handle.send_message(to_did, net_msg).await
                        } else {
                            net_handle.broadcast(net_msg).await
                        };

                        if let Err(e) = result {
                            warn!("Failed to send gossip message: {}", e);
                        }
                    });
                },
            );

            gossip.set_send_callback(send_callback);
        }

        Ok(TestNode {
            _keypair: keypair,
            did,
            network_handle,
            gossip_handle,
            listen_addr,
            shutdown_tx,
            domains,
            proposals,
            votes,
        })
    }

    /// Subscribe to governance topic
    async fn subscribe_governance(&self) -> Result<()> {
        let mut gossip = self.gossip_handle.write().await;

        // Create the topic if it doesn't exist
        let topic = icn_gossip::Topic::new(
            GOVERNANCE_TOPIC.to_string(),
            icn_gossip::AccessControl::Public,
        );
        gossip.create_topic(topic);

        gossip.subscribe(GOVERNANCE_TOPIC, self.did.clone()).await?;
        Ok(())
    }

    /// Publish a governance message to gossip and broadcast Announce to peers
    async fn publish_governance(&self, msg: GovernanceMessage) -> Result<[u8; 32]> {
        let bytes = msg.to_bytes()?;

        // Publish to local gossip store
        let (hash, clock) = {
            let mut gossip = self.gossip_handle.write().await;
            let hash = gossip.publish(GOVERNANCE_TOPIC, bytes).await?;
            let entry = gossip
                .get_entry(GOVERNANCE_TOPIC, &hash)
                .expect("Entry should exist");
            (hash, entry.clock.clone())
        };

        // Broadcast Announce message to all peers
        let announce_msg = icn_gossip::GossipMessage::Announce {
            hash,
            author: self.did.clone(),
            clock,
            topic: GOVERNANCE_TOPIC.to_string(),
        };

        let net_msg = icn_net::NetworkMessage::gossip(self.did.clone(), None, announce_msg);
        self.network_handle.broadcast(net_msg).await?;

        Ok(hash)
    }

    /// Create a governance domain locally and broadcast it
    async fn create_domain(
        &self,
        _domain_id: String,
        name: String,
        members: Vec<Did>,
    ) -> Result<GovernanceDomain> {
        let config = GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative"),
            MembershipConfig::static_list(members),
            GovernanceParams::new(50, 50, 604800), // 7 days
        );

        let domain = GovernanceDomain::new(name, config);

        // Store locally using the domain's actual ID (not the parameter)
        self.domains
            .write()
            .await
            .insert(domain.id.clone(), domain.clone());

        // Broadcast
        let msg = GovernanceMessage::domain_created(domain.clone());
        self.publish_governance(msg).await?;

        Ok(domain)
    }

    /// Create a proposal locally and broadcast it
    async fn create_proposal(
        &self,
        domain_id: GovernanceDomainId,
        title: String,
        description: String,
        payload: ProposalPayload,
    ) -> Result<Proposal> {
        let proposal = Proposal::new(domain_id, self.did.clone(), title, description, payload);

        // Store locally
        self.proposals
            .write()
            .await
            .insert(proposal.id.clone(), proposal.clone());

        // Broadcast
        let msg = GovernanceMessage::proposal_created(proposal.clone());
        self.publish_governance(msg).await?;

        Ok(proposal)
    }

    /// Open a proposal for voting and broadcast it
    async fn open_proposal(&self, proposal_id: ProposalId, duration_secs: u64) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let closes_at = now + duration_secs;

        // Update locally
        if let Some(proposal) = self.proposals.write().await.get_mut(&proposal_id) {
            proposal.open(duration_secs)?;
        } else {
            bail!("Proposal not found");
        }

        // Broadcast
        let msg = GovernanceMessage::proposal_opened(proposal_id, now, closes_at);
        self.publish_governance(msg).await?;

        Ok(())
    }

    /// Cast a vote and broadcast it
    async fn cast_vote(&self, proposal_id: ProposalId, choice: VoteChoice) -> Result<Vote> {
        let vote = Vote::new(proposal_id.clone(), self.did.clone(), choice);

        // Store locally
        let key = (proposal_id, self.did.clone());
        self.votes.write().await.insert(key, vote.clone());

        // Broadcast
        let msg = GovernanceMessage::vote_cast(vote.clone(), None);
        self.publish_governance(msg).await?;

        Ok(vote)
    }

    /// Close a proposal with tallied votes and broadcast the outcome
    async fn close_proposal(&self, proposal_id: ProposalId) -> Result<ProposalOutcome> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // Get proposal and domain
        let proposal = self
            .proposals
            .read()
            .await
            .get(&proposal_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Proposal not found"))?;

        let domain = self
            .domains
            .read()
            .await
            .get(&proposal.domain_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Domain not found"))?;

        // Tally votes
        let votes: Vec<Vote> = self
            .votes
            .read()
            .await
            .iter()
            .filter(|((pid, _), _)| pid == &proposal_id)
            .map(|(_, v)| v.clone())
            .collect();

        let tally = VoteTally::from(votes);

        // Resolve membership
        let resolver = StaticMembershipResolver::new();
        let eligible_count = resolver.member_count(&domain)?;

        // Evaluate outcome using proposal-type-specific thresholds (Issue #477)
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = domain.config.thresholds_for_proposal(&proposal.payload);
        let outcome_result =
            profile.evaluate_with_thresholds(&tally, thresholds, eligible_count)?;

        let outcome = match outcome_result {
            icn_governance::DecisionOutcome::Accepted => ProposalOutcome::Accepted,
            icn_governance::DecisionOutcome::Rejected => ProposalOutcome::Rejected,
            icn_governance::DecisionOutcome::NoQuorum => ProposalOutcome::NoQuorum,
        };

        // Update local state
        if let Some(prop) = self.proposals.write().await.get_mut(&proposal_id) {
            let new_state = match outcome {
                ProposalOutcome::Accepted => ProposalState::Accepted { closed_at: now },
                ProposalOutcome::Rejected => ProposalState::Rejected { closed_at: now },
                ProposalOutcome::NoQuorum => ProposalState::NoQuorum { closed_at: now },
            };
            prop.close(new_state)?;
        }

        // Broadcast outcome
        let tally_snapshot = TallySnapshot::new(
            tally.for_votes,
            tally.against_votes,
            tally.abstain_votes,
            eligible_count,
        );

        let msg =
            GovernanceMessage::proposal_closed(proposal_id, outcome.clone(), now, tally_snapshot);
        self.publish_governance(msg).await?;

        Ok(outcome)
    }

    async fn shutdown(self) {
        info!("Shutting down test node");
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Helper to wait for a condition with retry logic
async fn wait_for_condition<F, Fut>(
    condition: F,
    description: &str,
    max_retries: usize,
    retry_delay: Duration,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for attempt in 0..max_retries {
        if condition().await {
            info!("✓ {} (took {} attempts)", description, attempt + 1);
            return Ok(());
        }

        if attempt == max_retries - 1 {
            bail!(
                "{} timeout after {}ms",
                description,
                max_retries * retry_delay.as_millis() as usize
            );
        }

        tokio::time::sleep(retry_delay).await;
    }
    Ok(())
}

#[tokio::test]
#[ignore] // Flaky: Domain propagation timeout in CI - needs investigation
async fn test_governance_proposal_lifecycle() -> Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Starting governance integration test ===");

    // Spawn three nodes
    let node1 = TestNode::spawn(get_available_port()).await?;
    let node2 = TestNode::spawn(get_available_port()).await?;
    let node3 = TestNode::spawn(get_available_port()).await?;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);
    info!("Node 3 DID: {}", node3.did);

    // Subscribe all nodes to governance topic
    node1.subscribe_governance().await?;
    node2.subscribe_governance().await?;
    node3.subscribe_governance().await?;

    info!("✓ All nodes subscribed to governance topic");

    // Connect nodes in a full mesh topology
    // Node 1 → Node 2 and Node 3
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    node1
        .network_handle
        .dial(node3.listen_addr, node3.did.clone())
        .await?;

    // Node 2 → Node 1 and Node 3
    node2
        .network_handle
        .dial(node1.listen_addr, node1.did.clone())
        .await?;
    node2
        .network_handle
        .dial(node3.listen_addr, node3.did.clone())
        .await?;

    // Node 3 → Node 1 and Node 2
    node3
        .network_handle
        .dial(node1.listen_addr, node1.did.clone())
        .await?;
    node3
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    info!("✓ Network connections established");

    // Node 1 creates a governance domain with all three members
    let members = vec![node1.did.clone(), node2.did.clone(), node3.did.clone()];

    let domain = node1
        .create_domain(
            "tech-coop".to_string(), // Parameter is currently unused, domain gets auto-generated ID
            "Tech Cooperative".to_string(),
            members.clone(),
        )
        .await?;

    let domain_id = domain.id.clone();
    info!(
        "✓ Node 1 created governance domain with ID: {:?}",
        domain_id
    );

    // Wait for domain propagation (increase timeout for Request/Response cycle)
    let domain_id_check = domain_id.clone();
    let node2_domains = node2.domains.clone();
    let node3_domains = node3.domains.clone();
    wait_for_condition(
        || async {
            let has_node2 = node2_domains.read().await.contains_key(&domain_id_check);
            let has_node3 = node3_domains.read().await.contains_key(&domain_id_check);
            if !has_node2 {
                info!("Node 2 still waiting for domain...");
            }
            if !has_node3 {
                info!("Node 3 still waiting for domain...");
            }
            has_node2 && has_node3
        },
        "Domain propagated to all nodes",
        50, // Increased from 20
        Duration::from_millis(200),
    )
    .await?;

    // Node 1 creates a proposal
    let proposal = node1
        .create_proposal(
            domain_id.clone(),
            "Approve new supplier".to_string(),
            "We should work with Acme Corp for office supplies".to_string(),
            ProposalPayload::Text {
                body: "Proposal details...".to_string(),
            },
        )
        .await?;

    let proposal_id = proposal.id.clone();
    info!("✓ Node 1 created proposal: {:?}", proposal_id);

    // Wait for proposal propagation
    let proposal_id_check = proposal_id.clone();
    let node2_proposals = node2.proposals.clone();
    let node3_proposals = node3.proposals.clone();
    wait_for_condition(
        || async {
            node2_proposals
                .read()
                .await
                .contains_key(&proposal_id_check)
                && node3_proposals
                    .read()
                    .await
                    .contains_key(&proposal_id_check)
        },
        "Proposal propagated to all nodes",
        20,
        Duration::from_millis(200),
    )
    .await?;

    // Node 1 opens the proposal for voting (7 days)
    node1.open_proposal(proposal_id.clone(), 604800).await?;
    info!("✓ Node 1 opened proposal for voting");

    // Wait for proposal open state propagation
    let proposal_id_open = proposal_id.clone();
    let node2_proposals_open = node2.proposals.clone();
    let node3_proposals_open = node3.proposals.clone();
    wait_for_condition(
        || async {
            let p2 = node2_proposals_open.read().await;
            let p3 = node3_proposals_open.read().await;
            p2.get(&proposal_id_open).is_some_and(|p| p.state.is_open())
                && p3.get(&proposal_id_open).is_some_and(|p| p.state.is_open())
        },
        "Proposal opened on all nodes",
        20,
        Duration::from_millis(200),
    )
    .await?;

    // All three nodes cast votes
    node1
        .cast_vote(proposal_id.clone(), VoteChoice::For)
        .await?;
    node2
        .cast_vote(proposal_id.clone(), VoteChoice::For)
        .await?;
    node3
        .cast_vote(proposal_id.clone(), VoteChoice::Against)
        .await?;

    info!("✓ All nodes cast votes (2 For, 1 Against)");

    // Wait for vote propagation (each node should have 3 votes)
    let proposal_id_votes = proposal_id.clone();
    let node1_votes = node1.votes.clone();
    let node2_votes = node2.votes.clone();
    let node3_votes = node3.votes.clone();
    wait_for_condition(
        || async {
            let v1 = node1_votes.read().await;
            let v2 = node2_votes.read().await;
            let v3 = node3_votes.read().await;

            let count1 = v1
                .iter()
                .filter(|((pid, _), _)| pid == &proposal_id_votes)
                .count();
            let count2 = v2
                .iter()
                .filter(|((pid, _), _)| pid == &proposal_id_votes)
                .count();
            let count3 = v3
                .iter()
                .filter(|((pid, _), _)| pid == &proposal_id_votes)
                .count();

            count1 == 3 && count2 == 3 && count3 == 3
        },
        "Votes propagated to all nodes",
        20,
        Duration::from_millis(200),
    )
    .await?;

    // Node 1 closes the proposal and evaluates outcome
    let outcome = node1.close_proposal(proposal_id.clone()).await?;
    info!("✓ Node 1 closed proposal with outcome: {:?}", outcome);

    // Wait for outcome propagation (all nodes should show Accepted state)
    let proposal_id_outcome = proposal_id.clone();
    let node1_proposals_outcome = node1.proposals.clone();
    let node2_proposals_outcome = node2.proposals.clone();
    let node3_proposals_outcome = node3.proposals.clone();
    wait_for_condition(
        || async {
            let p1 = node1_proposals_outcome.read().await;
            let p2 = node2_proposals_outcome.read().await;
            let p3 = node3_proposals_outcome.read().await;

            // Expected: 2 For, 1 Against = 66% approval = Accepted (>50% threshold)
            p1.get(&proposal_id_outcome)
                .is_some_and(|p| matches!(p.state, ProposalState::Accepted { .. }))
                && p2
                    .get(&proposal_id_outcome)
                    .is_some_and(|p| matches!(p.state, ProposalState::Accepted { .. }))
                && p3
                    .get(&proposal_id_outcome)
                    .is_some_and(|p| matches!(p.state, ProposalState::Accepted { .. }))
        },
        "All nodes converged on proposal outcome: Accepted",
        20,
        Duration::from_millis(200),
    )
    .await?;
    info!("=== Governance integration test completed successfully ===");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
    node3.shutdown().await;

    Ok(())
}

/// Test that emergency proposals require higher quorum thresholds (Issue #477)
///
/// This is an end-to-end test that verifies emergency proposals (freeze, veto, rollback)
/// require 67%+ quorum vs the standard 50% for normal proposals.
///
/// Note: Ignored in CI due to intermittent QUIC stream failures in containerized environment.
/// Run manually with: cargo test -p icn-core --test governance_integration test_emergency_proposal_requires_supermajority_quorum -- --ignored
#[tokio::test]
#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]
async fn test_emergency_proposal_requires_supermajority_quorum() -> Result<()> {
    // Initialize test environment
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Starting emergency quorum enforcement test ===");

    // Setup: Create a governance domain with 10 members
    // We'll use static membership with 10 DIDs
    let mut members = Vec::new();
    for _ in 0..10 {
        let kp = KeyPair::generate()?;
        members.push(kp.did().clone());
    }

    // Create domain with default cooperative config (50% quorum for normal, 67% for freeze)
    let config = GovernanceConfig::new(
        GovernanceProfileId::builtin("cooperative"),
        MembershipConfig::static_list(members.clone()),
        GovernanceParams::new(50, 50, 604800),
    );
    let domain = GovernanceDomain::new("Test Cooperative".to_string(), config);
    let domain_id = domain.id.clone();

    // Verify the thresholds for freeze vs normal proposals
    let normal_payload = ProposalPayload::Text {
        body: "Normal proposal".to_string(),
    };
    let freeze_payload = ProposalPayload::FreezeMember {
        member: members[0].clone(),
        reason: "Security concern".to_string(),
        duration_seconds: Some(86400),
    };

    let normal_thresholds = domain.config.thresholds_for_proposal(&normal_payload);
    let freeze_thresholds = domain.config.thresholds_for_proposal(&freeze_payload);

    info!(
        "Normal proposal thresholds: {}% quorum, {}% approval",
        normal_thresholds.quorum_percentage, normal_thresholds.approval_percentage
    );
    info!(
        "Freeze proposal thresholds: {}% quorum, {}% approval",
        freeze_thresholds.quorum_percentage, freeze_thresholds.approval_percentage
    );

    // Verify emergency thresholds are higher
    assert!(
        freeze_thresholds.quorum_percentage > normal_thresholds.quorum_percentage,
        "Freeze proposal should require higher quorum than normal"
    );

    // Create test proposals
    let proposer_did = members[1].clone();

    let normal_proposal = Proposal::new(
        domain_id.clone(),
        proposer_did.clone(),
        "Normal Text Proposal".to_string(),
        "A routine proposal".to_string(),
        normal_payload,
    );

    let freeze_proposal = Proposal::new(
        domain_id.clone(),
        proposer_did,
        "Freeze Member".to_string(),
        "Emergency action".to_string(),
        freeze_payload,
    );

    // Scenario: 5 out of 10 members vote (50% turnout)
    // For integer division: (10 * 50) / 100 = 5 required for normal
    //                       (10 * 67) / 100 = 6 required for freeze
    let mut normal_votes = Vec::new();
    let mut freeze_votes = Vec::new();
    for member in members.iter().take(5) {
        normal_votes.push(Vote::new(
            normal_proposal.id.clone(),
            member.clone(),
            VoteChoice::For,
        ));
        freeze_votes.push(Vote::new(
            freeze_proposal.id.clone(),
            member.clone(),
            VoteChoice::For,
        ));
    }

    let normal_tally = VoteTally::from(normal_votes);
    let freeze_tally = VoteTally::from(freeze_votes);

    // Evaluate outcomes
    let profile = GovernanceProfile::cooperative_default();
    let resolver = StaticMembershipResolver::new();
    let eligible_count = resolver.member_count(&domain)?;

    let normal_outcome =
        profile.evaluate_with_thresholds(&normal_tally, normal_thresholds, eligible_count)?;
    let freeze_outcome =
        profile.evaluate_with_thresholds(&freeze_tally, freeze_thresholds, eligible_count)?;

    info!(
        "Normal proposal outcome with 50% turnout: {:?}",
        normal_outcome
    );
    info!(
        "Freeze proposal outcome with 50% turnout: {:?}",
        freeze_outcome
    );

    // Assert: Normal proposal passes (50% turnout meets 50% quorum)
    assert!(
        matches!(normal_outcome, icn_governance::DecisionOutcome::Accepted),
        "Normal proposal with 50% turnout should pass 50% quorum"
    );

    // Assert: Freeze proposal fails quorum (50% turnout < 67% required)
    assert!(
        matches!(freeze_outcome, icn_governance::DecisionOutcome::NoQuorum),
        "Freeze proposal with 50% turnout should fail 67% quorum (need 6 votes, got 5)"
    );

    // Scenario 2: 7 out of 10 members vote (70% turnout) - should pass for freeze
    let mut freeze_votes_7 = Vec::new();
    for member in members.iter().take(7) {
        freeze_votes_7.push(Vote::new(
            freeze_proposal.id.clone(),
            member.clone(),
            VoteChoice::For,
        ));
    }
    let freeze_tally_7 = VoteTally::from(freeze_votes_7);
    let freeze_outcome_7 =
        profile.evaluate_with_thresholds(&freeze_tally_7, freeze_thresholds, eligible_count)?;

    info!(
        "Freeze proposal outcome with 70% turnout: {:?}",
        freeze_outcome_7
    );

    // Assert: Freeze proposal passes with 70% turnout (> 67% required)
    assert!(
        matches!(freeze_outcome_7, icn_governance::DecisionOutcome::Accepted),
        "Freeze proposal with 70% turnout should pass 67% quorum"
    );

    info!("=== Emergency quorum enforcement test completed successfully ===");
    Ok(())
}
