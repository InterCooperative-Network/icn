//! Integration test for governance over gossip protocol
//!
//! This test validates end-to-end governance flow:
//! 1. Multi-node governance domain setup
//! 2. Proposal creation and gossip propagation
//! 3. Distributed voting
//! 4. Outcome convergence across all nodes

use anyhow::{bail, Result};
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceMessage, GovernanceParams,
    GovernanceProfile, GovernanceProfileId, GovernanceRule, MembershipConfig, MembershipResolver,
    Proposal, ProposalId, ProposalOutcome, ProposalPayload, ProposalState,
    StaticMembershipResolver, TallySnapshot, Vote, VoteChoice, VoteTally,
};
use icn_gossip::GossipActor;
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_net::{IncomingMessageHandler, MessagePayload, NetworkActor};
use icn_trust::TrustClass;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

const GOVERNANCE_TOPIC: &str = "governance:proposal";

/// Helper to create a test node with governance support
struct TestNode {
    keypair: KeyPair,
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

            match net_msg.payload {
                MessagePayload::Gossip(gossip_msg) => {
                    let mut gossip = gossip_handle_clone.blocking_write();
                    if let Err(e) = gossip.handle_message(&sender_did, gossip_msg) {
                        warn!("Failed to handle gossip message: {}", e);
                    }
                }
                _ => {}
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
                            domains_clone.write().await.insert(domain_id, domain);
                        }
                        GovernanceMessage::ProposalCreated { proposal } => {
                            let proposal_id = proposal.id.clone();
                            proposals_clone
                                .write()
                                .await
                                .insert(proposal_id, proposal);
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
                                    ProposalOutcome::Accepted => ProposalState::Accepted { closed_at },
                                    ProposalOutcome::Rejected => ProposalState::Rejected { closed_at },
                                    ProposalOutcome::NoQuorum => ProposalState::NoQuorum { closed_at },
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
        let listen_addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
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
        )
        .await?;

        info!("Network actor spawned on {}", listen_addr);

        Ok(TestNode {
            keypair,
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

        gossip.subscribe(GOVERNANCE_TOPIC, self.did.clone())?;
        Ok(())
    }

    /// Publish a governance message to gossip
    async fn publish_governance(&self, msg: GovernanceMessage) -> Result<[u8; 32]> {
        let bytes = msg.to_bytes()?;
        let mut gossip = self.gossip_handle.write().await;
        let hash = gossip.publish(GOVERNANCE_TOPIC, bytes)?;
        Ok(hash)
    }

    /// Create a governance domain locally and broadcast it
    async fn create_domain(
        &self,
        domain_id: String,
        name: String,
        members: Vec<Did>,
    ) -> Result<GovernanceDomain> {
        let config = GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative"),
            MembershipConfig::static_list(members),
            GovernanceParams::new(50, 50, 604800), // 7 days
        );

        let domain = GovernanceDomain::new(name, config);
        let domain_id_obj = GovernanceDomainId(domain_id);

        // Store locally
        self.domains
            .write()
            .await
            .insert(domain_id_obj.clone(), domain.clone());

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
        let proposal = Proposal::new(
            domain_id,
            self.did.clone(),
            title,
            description,
            payload,
        );

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
    async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        choice: VoteChoice,
    ) -> Result<Vote> {
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

        // Evaluate outcome
        let profile = GovernanceProfile::cooperative_default();
        let outcome_result = profile.evaluate(&tally, &domain.config.params, eligible_count)?;

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

        let msg = GovernanceMessage::proposal_closed(proposal_id, outcome.clone(), now, tally_snapshot);
        self.publish_governance(msg).await?;

        Ok(outcome)
    }

    async fn shutdown(self) {
        info!("Shutting down test node");
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore] // Requires network interfaces and QUIC handshake
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
    let node1 = TestNode::spawn(16001).await?;
    let node2 = TestNode::spawn(16002).await?;
    let node3 = TestNode::spawn(16003).await?;

    info!("Node 1 DID: {}", node1.did);
    info!("Node 2 DID: {}", node2.did);
    info!("Node 3 DID: {}", node3.did);

    // Subscribe all nodes to governance topic
    node1.subscribe_governance().await?;
    node2.subscribe_governance().await?;
    node3.subscribe_governance().await?;

    info!("✓ All nodes subscribed to governance topic");

    // Connect nodes in a triangle: 1-2, 2-3, 1-3
    node1
        .network_handle
        .dial(node2.listen_addr, node2.did.clone())
        .await?;
    node2
        .network_handle
        .dial(node3.listen_addr, node3.did.clone())
        .await?;
    node1
        .network_handle
        .dial(node3.listen_addr, node3.did.clone())
        .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    info!("✓ Network connections established");

    // Node 1 creates a governance domain with all three members
    let domain_id = GovernanceDomainId("tech-coop".to_string());
    let members = vec![node1.did.clone(), node2.did.clone(), node3.did.clone()];

    let _domain = node1
        .create_domain(
            domain_id.0.clone(),
            "Tech Cooperative".to_string(),
            members.clone(),
        )
        .await?;

    info!("✓ Node 1 created governance domain: {}", domain_id.0);

    // Wait for gossip propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes received the domain
    {
        let domains2 = node2.domains.read().await;
        let domains3 = node3.domains.read().await;

        assert!(
            domains2.contains_key(&domain_id),
            "Node 2 should have received domain"
        );
        assert!(
            domains3.contains_key(&domain_id),
            "Node 3 should have received domain"
        );
    }
    info!("✓ Domain propagated to all nodes");

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

    // Wait for gossip propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes received the proposal
    {
        let proposals2 = node2.proposals.read().await;
        let proposals3 = node3.proposals.read().await;

        assert!(
            proposals2.contains_key(&proposal_id),
            "Node 2 should have received proposal"
        );
        assert!(
            proposals3.contains_key(&proposal_id),
            "Node 3 should have received proposal"
        );
    }
    info!("✓ Proposal propagated to all nodes");

    // Node 1 opens the proposal for voting (7 days)
    node1.open_proposal(proposal_id.clone(), 604800).await?;
    info!("✓ Node 1 opened proposal for voting");

    // Wait for gossip propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes see proposal as open
    {
        let proposals2 = node2.proposals.read().await;
        let proposals3 = node3.proposals.read().await;

        assert!(
            proposals2.get(&proposal_id).unwrap().state.is_open(),
            "Node 2 proposal should be open"
        );
        assert!(
            proposals3.get(&proposal_id).unwrap().state.is_open(),
            "Node 3 proposal should be open"
        );
    }
    info!("✓ Proposal opened on all nodes");

    // All three nodes cast votes
    node1.cast_vote(proposal_id.clone(), VoteChoice::For).await?;
    node2.cast_vote(proposal_id.clone(), VoteChoice::For).await?;
    node3.cast_vote(proposal_id.clone(), VoteChoice::Against).await?;

    info!("✓ All nodes cast votes (2 For, 1 Against)");

    // Wait for vote propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes received all votes
    {
        let votes1 = node1.votes.read().await;
        let votes2 = node2.votes.read().await;
        let votes3 = node3.votes.read().await;

        // Each node should have 3 votes
        let proposal_votes1: Vec<_> = votes1
            .iter()
            .filter(|((pid, _), _)| pid == &proposal_id)
            .collect();
        let proposal_votes2: Vec<_> = votes2
            .iter()
            .filter(|((pid, _), _)| pid == &proposal_id)
            .collect();
        let proposal_votes3: Vec<_> = votes3
            .iter()
            .filter(|((pid, _), _)| pid == &proposal_id)
            .collect();

        assert_eq!(proposal_votes1.len(), 3, "Node 1 should have 3 votes");
        assert_eq!(proposal_votes2.len(), 3, "Node 2 should have 3 votes");
        assert_eq!(proposal_votes3.len(), 3, "Node 3 should have 3 votes");
    }
    info!("✓ Votes propagated to all nodes");

    // Node 1 closes the proposal and evaluates outcome
    let outcome = node1.close_proposal(proposal_id.clone()).await?;
    info!("✓ Node 1 closed proposal with outcome: {:?}", outcome);

    // Wait for outcome propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes converged on the same outcome
    {
        let proposals1 = node1.proposals.read().await;
        let proposals2 = node2.proposals.read().await;
        let proposals3 = node3.proposals.read().await;

        let state1 = &proposals1.get(&proposal_id).unwrap().state;
        let state2 = &proposals2.get(&proposal_id).unwrap().state;
        let state3 = &proposals3.get(&proposal_id).unwrap().state;

        // Expected: 2 For, 1 Against = 66% approval = Accepted (>50% threshold)
        assert!(
            matches!(state1, ProposalState::Accepted { .. }),
            "Node 1: Proposal should be accepted"
        );
        assert!(
            matches!(state2, ProposalState::Accepted { .. }),
            "Node 2: Proposal should be accepted"
        );
        assert!(
            matches!(state3, ProposalState::Accepted { .. }),
            "Node 3: Proposal should be accepted"
        );
    }

    info!("✓ All nodes converged on proposal outcome: Accepted");
    info!("=== Governance integration test completed successfully ===");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
    node3.shutdown().await;

    Ok(())
}
