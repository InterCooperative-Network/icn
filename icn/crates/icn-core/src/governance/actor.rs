//! GovernanceActor implementation
//!
//! This actor manages governance state and coordinates distributed decision-making
//! across the ICN network.

use anyhow::{bail, Result};
use async_trait::async_trait;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_store::Store;

use icn_governance::{
    DecisionOutcome, Delegation, DelegationId, GovernanceConfig, GovernanceDomain,
    GovernanceDomainId, GovernanceMessage, GovernanceParams, GovernanceProfile,
    GovernanceProfileId, GovernanceRule, MembershipAction, MembershipConfig, MembershipResolver,
    MembershipSource, Proposal, ProposalId, ProposalOutcome, ProposalPayload, ProposalState,
    TallySnapshot, Timestamp, Vote, VoteChoice, VoteTally,
};

use crate::events::{EventBus, SystemEvent};

/// Gossip topic for governance messages
const GOVERNANCE_TOPIC: &str = "governance:proposal";

/// Interval for checking proposal expiration
const SCHEDULER_INTERVAL: Duration = Duration::from_secs(10);

/// Scheduled proposal close event
#[derive(Clone, Debug, Eq, PartialEq)]
struct ScheduledClose {
    closes_at: Instant,
    proposal_id: ProposalId,
}

impl Ord for ScheduledClose {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Earlier times have higher priority
        self.closes_at.cmp(&other.closes_at)
    }
}

impl PartialOrd for ScheduledClose {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Lightweight config for creating domains
#[derive(Clone, Debug)]
pub struct GovernanceConfigLite {
    /// Governance profile name (e.g., "consensus", "majority")
    pub profile: String,
    /// Core governance parameters
    pub params: GovernanceParams,
    /// Membership configuration
    pub membership: MembershipConfig,
}

/// Commands that can be submitted to the governance actor
#[derive(Debug)]
pub enum GovernanceCommand {
    /// Create a new governance domain
    CreateDomain {
        /// Unique identifier for the domain
        domain_id: GovernanceDomainId,
        /// Human-readable name
        name: String,
        /// Domain configuration
        config: GovernanceConfigLite,
    },
    /// Create a new proposal
    CreateProposal {
        /// Unique proposal identifier
        proposal_id: ProposalId,
        /// Domain in which to create the proposal
        domain_id: GovernanceDomainId,
        /// Proposal title
        title: String,
        /// Detailed description
        description: String,
        /// Proposal action payload
        payload: ProposalPayload,
    },
    /// Open a proposal for voting
    OpenProposal {
        /// Proposal to open
        proposal_id: ProposalId,
        /// Duration of voting period in seconds
        voting_period_seconds: u64,
    },
    /// Cast a vote on a proposal
    CastVote {
        /// Proposal to vote on
        proposal_id: ProposalId,
        /// Vote choice
        choice: VoteChoice,
        /// Optional comment explaining the vote
        comment: Option<String>,
    },
    /// Close voting on a proposal
    CloseProposal {
        /// Proposal to close
        proposal_id: ProposalId,
    },
    /// Emergency veto - marks a proposal as vetoed
    VetoProposal {
        /// Proposal to veto
        proposal_id: ProposalId,
        /// Reason for veto
        reason: String,
    },
    /// Emergency force close - closes a proposal with a forced outcome
    ForceCloseProposal {
        /// Proposal to force close
        proposal_id: ProposalId,
        /// Outcome to force
        forced_outcome: icn_governance::ForcedOutcome,
        /// Reason for forcing
        reason: String,
    },
    /// Update domain configuration (from accepted ConfigChange proposal)
    UpdateDomainConfig {
        /// Domain to update
        domain_id: GovernanceDomainId,
        /// New configuration
        new_config: GovernanceConfig,
    },
    /// Update domain membership (from accepted Membership proposal)
    UpdateMembership {
        /// Domain to update
        domain_id: GovernanceDomainId,
        /// Membership action (add/remove/etc.)
        action: MembershipAction,
        /// Member DID to act upon
        member: Did,
    },
}

/// Handle for interacting with the governance actor
#[derive(Clone)]
pub struct GovernanceHandle {
    inner: Arc<RwLock<GovernanceActor>>,
}

impl GovernanceHandle {
    /// Submit a command to the governance actor
    pub async fn submit(&self, cmd: GovernanceCommand) -> Result<()> {
        self.inner.write().await.handle(cmd).await
    }

    /// List all governance domains
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        self.inner.read().await.list_domains()
    }

    /// List all proposals
    pub async fn list_proposals(&self) -> Result<Vec<Proposal>> {
        self.inner.read().await.list_proposals()
    }

    /// Get a specific domain
    pub async fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        self.inner.read().await.load_domain(id)
    }

    /// Get a specific proposal
    pub async fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        self.inner.read().await.load_proposal(id)
    }

    /// Create a new delegation
    pub async fn create_delegation(&self, delegation: Delegation) -> Result<()> {
        self.inner.write().await.create_delegation(delegation)
    }

    /// Get a delegation by ID
    pub async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        self.inner.read().await.load_delegation(id)
    }

    /// Get all delegations from a delegator
    pub async fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        self.inner.read().await.list_delegations_from(delegator)
    }

    /// Get all delegations to a delegate
    pub async fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        self.inner.read().await.list_delegations_to(delegate)
    }

    /// Revoke a delegation
    pub async fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        self.inner.write().await.revoke_delegation(id, revoked_at)
    }
}

/// Implement GovernanceOps trait to allow RPC integration without circular dependencies
#[async_trait]
impl icn_governance::GovernanceOps for GovernanceHandle {
    // Read operations

    async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        Self::list_domains(self).await
    }

    async fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        Self::get_domain(self, id).await
    }

    async fn list_proposals(&self) -> Result<Vec<Proposal>> {
        Self::list_proposals(self).await
    }

    async fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        Self::get_proposal(self, id).await
    }

    // Write operations

    async fn create_domain(
        &self,
        domain_id: GovernanceDomainId,
        name: String,
        profile: String,
        params: icn_governance::GovernanceParams,
        membership: icn_governance::MembershipConfig,
    ) -> Result<()> {
        let config = GovernanceConfigLite {
            profile,
            params,
            membership,
        };

        self.submit(GovernanceCommand::CreateDomain {
            domain_id,
            name,
            config,
        })
        .await
    }

    async fn create_proposal(
        &self,
        domain_id: GovernanceDomainId,
        title: String,
        description: String,
        payload: icn_governance::ProposalPayload,
    ) -> Result<ProposalId> {
        // Generate a new proposal ID
        let proposal_id = ProposalId::generate();

        self.submit(GovernanceCommand::CreateProposal {
            proposal_id: proposal_id.clone(),
            domain_id,
            title,
            description,
            payload,
        })
        .await?;

        Ok(proposal_id)
    }

    async fn open_proposal(
        &self,
        proposal_id: ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        self.submit(GovernanceCommand::OpenProposal {
            proposal_id,
            voting_period_seconds,
        })
        .await
    }

    async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        choice: icn_governance::VoteChoice,
        comment: Option<String>,
    ) -> Result<()> {
        self.submit(GovernanceCommand::CastVote {
            proposal_id,
            choice,
            comment,
        })
        .await
    }

    async fn close_proposal(&self, proposal_id: ProposalId) -> Result<()> {
        self.submit(GovernanceCommand::CloseProposal { proposal_id })
            .await
    }

    // Delegation operations

    async fn create_delegation(&self, delegation: Delegation) -> Result<()> {
        Self::create_delegation(self, delegation).await
    }

    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        Self::get_delegation(self, id).await
    }

    async fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        Self::get_delegations_from(self, delegator).await
    }

    async fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        Self::get_delegations_to(self, delegate).await
    }

    async fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        Self::revoke_delegation(self, id, revoked_at).await
    }
}

/// The governance actor
pub struct GovernanceActor {
    did: Did,
    store: Arc<dyn Store>,
    gossip: Arc<RwLock<GossipActor>>,
    resolver: Arc<dyn MembershipResolver + Send + Sync>,
    profile: GovernanceProfile,
    close_scheduler: Arc<RwLock<BinaryHeap<Reverse<ScheduledClose>>>>,
    close_tx: mpsc::UnboundedSender<ProposalId>,
    event_bus: Option<Arc<EventBus>>,
}

impl GovernanceActor {
    /// Spawn a new governance actor
    pub async fn spawn(
        did: Did,
        store: Arc<dyn Store>,
        gossip: Arc<RwLock<GossipActor>>,
        resolver: Arc<dyn MembershipResolver + Send + Sync>,
        event_bus: Option<Arc<EventBus>>,
    ) -> Result<GovernanceHandle> {
        info!("Spawning GovernanceActor for DID: {}", did);

        // Subscribe to governance topic
        {
            let mut g = gossip.write().await;
            g.subscribe(GOVERNANCE_TOPIC, did.clone())?;
        }

        // Set up notification callback for incoming messages
        let store_notify = store.clone();
        let did_notify = did.clone();

        {
            let mut g = gossip.write().await;
            g.set_notification_callback(Arc::new(move |topic, entry, _subscriber_did| {
                if topic != GOVERNANCE_TOPIC {
                    return;
                }

                match GovernanceMessage::from_bytes(&entry.data) {
                    Ok(msg) => {
                        info!(
                            "[{}] Received governance message: {}",
                            did_notify,
                            msg.message_type()
                        );
                        if let Err(e) = handle_incoming(store_notify.as_ref(), msg) {
                            warn!("Failed to handle incoming governance message: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to deserialize governance message: {}", e);
                    }
                }
            }));
        }

        // Create scheduler and channel for auto-closing proposals
        let close_scheduler = Arc::new(RwLock::new(BinaryHeap::new()));
        let (close_tx, mut close_rx) = mpsc::unbounded_channel();

        let actor = GovernanceActor {
            did: did.clone(),
            store: store.clone(),
            gossip: gossip.clone(),
            resolver: resolver.clone(),
            profile: GovernanceProfile::cooperative_default(),
            close_scheduler: close_scheduler.clone(),
            close_tx,
            event_bus,
        };

        let handle = GovernanceHandle {
            inner: Arc::new(RwLock::new(actor)),
        };

        // Spawn background timer task for auto-closing proposals
        let handle_clone = handle.clone();
        let scheduler_clone = close_scheduler.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SCHEDULER_INTERVAL);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check for expired proposals
                        let now = Instant::now();
                        let mut expired = Vec::new();

                        {
                            let mut scheduler = scheduler_clone.write().await;
                            while let Some(Reverse(scheduled)) = scheduler.peek() {
                                if scheduled.closes_at <= now {
                                    // SAFETY: We just peeked and confirmed an element exists
                                    #[allow(clippy::unwrap_used)]
                                    expired.push(scheduler.pop().unwrap().0.proposal_id.clone());
                                } else {
                                    break;
                                }
                            }
                        }

                        // Auto-close expired proposals
                        for proposal_id in expired {
                            info!("Auto-closing expired proposal: {}", proposal_id.0);
                            if let Err(e) = handle_clone.submit(GovernanceCommand::CloseProposal {
                                proposal_id: proposal_id.clone(),
                            }).await {
                                warn!("Failed to auto-close proposal {}: {}", proposal_id.0, e);
                            }
                        }
                    }

                    Some(proposal_id) = close_rx.recv() => {
                        // Manual close - remove from scheduler if present
                        let mut scheduler = scheduler_clone.write().await;
                        scheduler.retain(|Reverse(sc)| sc.proposal_id != proposal_id);
                    }
                }
            }
        });

        info!(
            "✓ Governance scheduler started (checking every {}s)",
            SCHEDULER_INTERVAL.as_secs()
        );

        Ok(handle)
    }

    /// Handle a governance command
    async fn handle(&mut self, cmd: GovernanceCommand) -> Result<()> {
        match cmd {
            GovernanceCommand::CreateDomain {
                domain_id,
                name,
                config,
            } => {
                info!("Creating governance domain: {}", domain_id.0);

                let profile_id = GovernanceProfileId::builtin(&config.profile);
                let domain = GovernanceDomain::new(
                    name,
                    GovernanceConfig::new(profile_id, config.membership, config.params),
                );

                // Persist locally
                self.store
                    .put(&domain_key(&domain_id), &serde_json::to_vec(&domain)?)?;

                // Broadcast to network
                self.publish(GovernanceMessage::domain_created(domain))
                    .await?;

                info!("✓ Domain created: {}", domain_id.0);
            }

            GovernanceCommand::CreateProposal {
                proposal_id,
                domain_id,
                title,
                description,
                payload,
            } => {
                info!("Creating proposal: {}", title);

                let mut proposal = Proposal::new(
                    domain_id,
                    self.did.clone(),
                    title.clone(),
                    description,
                    payload,
                );

                // Use the provided proposal ID instead of the generated one
                proposal.id = proposal_id.clone();

                // Persist locally
                self.store
                    .put(&proposal_key(&proposal_id), &serde_json::to_vec(&proposal)?)?;

                // Broadcast to network
                self.publish(GovernanceMessage::proposal_created(proposal))
                    .await?;

                info!("✓ Proposal created: {} (ID: {})", title, proposal_id.0);
            }

            GovernanceCommand::OpenProposal {
                proposal_id,
                voting_period_seconds,
            } => {
                info!("Opening proposal: {}", proposal_id.0);

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Open it
                proposal.open(voting_period_seconds)?;

                // Extract timestamps from state
                let (opened_at, closes_at) = match &proposal.state {
                    ProposalState::Open {
                        opened_at,
                        closes_at,
                    } => (*opened_at, *closes_at),
                    _ => bail!("Proposal failed to transition to Open state"),
                };

                // Persist updated state
                self.store
                    .put(&proposal_key(&proposal_id), &serde_json::to_vec(&proposal)?)?;

                // Schedule auto-close
                let closes_at_instant = Instant::now() + Duration::from_secs(voting_period_seconds);
                let scheduled = ScheduledClose {
                    closes_at: closes_at_instant,
                    proposal_id: proposal_id.clone(),
                };
                self.close_scheduler.write().await.push(Reverse(scheduled));

                // Broadcast to network
                self.publish(GovernanceMessage::proposal_opened(
                    proposal_id.clone(),
                    opened_at,
                    closes_at,
                ))
                .await?;

                info!(
                    "✓ Proposal opened: {} (auto-close scheduled for {}s)",
                    proposal_id.0, voting_period_seconds
                );
            }

            GovernanceCommand::CastVote {
                proposal_id,
                choice,
                comment,
            } => {
                info!("Casting vote on proposal: {}", proposal_id.0);

                let mut vote = Vote::new(proposal_id.clone(), self.did.clone(), choice);
                if let Some(c) = comment {
                    vote = vote.with_comment(c);
                }

                // Persist locally
                self.store.put(
                    &vote_key(&proposal_id, &self.did),
                    &serde_json::to_vec(&vote)?,
                )?;

                // Broadcast to network
                self.publish(GovernanceMessage::vote_cast(vote, None))
                    .await?;

                info!("✓ Vote cast: {:?}", choice);
            }

            GovernanceCommand::CloseProposal { proposal_id } => {
                info!("Closing proposal: {}", proposal_id.0);

                // Notify scheduler to cancel auto-close (if scheduled)
                let _ = self.close_tx.send(proposal_id.clone());

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Load domain
                let domain = self
                    .load_domain(&proposal.domain_id)?
                    .ok_or_else(|| anyhow::anyhow!("Domain not found: {}", proposal.domain_id.0))?;

                // Load and tally votes
                let votes = self.load_votes(&proposal_id)?;
                let tally = VoteTally::from(votes);

                // Resolve eligible membership
                let eligible_count = self.resolver.member_count(&domain)?;

                // Evaluate outcome
                let outcome_result =
                    self.profile
                        .evaluate(&tally, &domain.config.params, eligible_count)?;

                // Map to proposal state
                let now = now_seconds();
                let new_state = match outcome_result {
                    DecisionOutcome::Accepted => ProposalState::Accepted { closed_at: now },
                    DecisionOutcome::Rejected => ProposalState::Rejected { closed_at: now },
                    DecisionOutcome::NoQuorum => ProposalState::NoQuorum { closed_at: now },
                };

                // Update proposal
                proposal.close(new_state)?;

                // Persist updated state
                self.store
                    .put(&proposal_key(&proposal_id), &serde_json::to_vec(&proposal)?)?;

                // Create tally snapshot for broadcast
                let tally_snapshot = TallySnapshot::new(
                    tally.for_votes,
                    tally.against_votes,
                    tally.abstain_votes,
                    eligible_count,
                );

                // Map outcome for message
                let outcome_msg = match outcome_result {
                    DecisionOutcome::Accepted => ProposalOutcome::Accepted,
                    DecisionOutcome::Rejected => ProposalOutcome::Rejected,
                    DecisionOutcome::NoQuorum => ProposalOutcome::NoQuorum,
                };

                // Broadcast to network
                self.publish(GovernanceMessage::proposal_closed(
                    proposal_id.clone(),
                    outcome_msg,
                    now,
                    tally_snapshot,
                ))
                .await?;

                // Emit event for downstream processing (e.g., ledger transactions)
                if let Some(ref event_bus) = self.event_bus {
                    let event = match outcome_result {
                        DecisionOutcome::Accepted => SystemEvent::ProposalAccepted {
                            proposal_id: proposal_id.clone(),
                            domain_id: proposal.domain_id.0.clone(),
                            payload: proposal.payload.clone(),
                            decided_at: now,
                        },
                        _ => SystemEvent::ProposalRejected {
                            proposal_id: proposal_id.clone(),
                            domain_id: proposal.domain_id.0.clone(),
                            decided_at: now,
                        },
                    };

                    event_bus.emit(event).await;
                }

                info!(
                    "✓ Proposal closed: {} ({:?})",
                    proposal_id.0, outcome_result
                );
            }

            GovernanceCommand::VetoProposal {
                proposal_id,
                reason,
            } => {
                info!(
                    "🚫 Vetoing proposal: {} (reason: {})",
                    proposal_id.0, reason
                );

                // Notify scheduler to cancel auto-close (if scheduled)
                let _ = self.close_tx.send(proposal_id.clone());

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Veto the proposal
                proposal.veto(reason.clone())?;

                // Persist updated state
                self.store
                    .put(&proposal_key(&proposal_id), &serde_json::to_vec(&proposal)?)?;

                // Emit event for downstream processing
                if let Some(ref event_bus) = self.event_bus {
                    let now = now_seconds();
                    event_bus
                        .emit(SystemEvent::ProposalRejected {
                            proposal_id: proposal_id.clone(),
                            domain_id: proposal.domain_id.0.clone(),
                            decided_at: now,
                        })
                        .await;
                }

                icn_obs::metrics::governance::proposals_vetoed_inc();
                info!("✓ Proposal vetoed: {}", proposal_id.0);
            }

            GovernanceCommand::ForceCloseProposal {
                proposal_id,
                forced_outcome,
                reason,
            } => {
                use icn_governance::ForcedOutcome;

                info!(
                    "⚡ Force closing proposal: {} as {:?} (reason: {})",
                    proposal_id.0, forced_outcome, reason
                );

                // Notify scheduler to cancel auto-close (if scheduled)
                let _ = self.close_tx.send(proposal_id.clone());

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Map ForcedOutcome to ProposalOutcome for the state
                let proposal_outcome = match &forced_outcome {
                    ForcedOutcome::Accept => ProposalOutcome::Accepted,
                    ForcedOutcome::Reject => ProposalOutcome::Rejected,
                    ForcedOutcome::Cancel => ProposalOutcome::NoQuorum, // Treat Cancel as NoQuorum
                };

                // Force close the proposal
                proposal.force_close(proposal_outcome.clone(), reason.clone())?;

                // Persist updated state
                self.store
                    .put(&proposal_key(&proposal_id), &serde_json::to_vec(&proposal)?)?;

                // Emit appropriate event
                if let Some(ref event_bus) = self.event_bus {
                    let now = now_seconds();
                    let event = match forced_outcome {
                        ForcedOutcome::Accept => SystemEvent::ProposalAccepted {
                            proposal_id: proposal_id.clone(),
                            domain_id: proposal.domain_id.0.clone(),
                            payload: proposal.payload.clone(),
                            decided_at: now,
                        },
                        _ => SystemEvent::ProposalRejected {
                            proposal_id: proposal_id.clone(),
                            domain_id: proposal.domain_id.0.clone(),
                            decided_at: now,
                        },
                    };
                    event_bus.emit(event).await;
                }

                icn_obs::metrics::governance::proposals_force_closed_inc();
                info!(
                    "✓ Proposal force-closed: {} as {:?}",
                    proposal_id.0, forced_outcome
                );
            }

            GovernanceCommand::UpdateDomainConfig {
                domain_id,
                new_config,
            } => {
                info!("⚙️  Updating domain config: {}", domain_id.0);

                // Load existing domain
                let mut domain = self
                    .load_domain(&domain_id)?
                    .ok_or_else(|| anyhow::anyhow!("Domain not found: {}", domain_id.0))?;

                // Update configuration
                domain.update_config(new_config);

                // Persist locally
                self.store
                    .put(&domain_key(&domain_id), &serde_json::to_vec(&domain)?)?;

                // Broadcast to network
                self.publish(GovernanceMessage::domain_updated(domain))
                    .await?;

                icn_obs::metrics::governance::domain_config_updated_inc();
                info!("✓ Domain config updated: {}", domain_id.0);
            }

            GovernanceCommand::UpdateMembership {
                domain_id,
                action,
                member,
            } => {
                info!(
                    "👥 Updating membership for domain {}: {:?} {}",
                    domain_id.0, action, member
                );

                // Load existing domain
                let mut domain = self
                    .load_domain(&domain_id)?
                    .ok_or_else(|| anyhow::anyhow!("Domain not found: {}", domain_id.0))?;

                // Update membership based on source type
                match &mut domain.config.membership.source {
                    MembershipSource::StaticList(members) => match action {
                        MembershipAction::Add => {
                            if !members.contains(&member) {
                                members.push(member.clone());
                                info!("✓ Added {} to domain {} membership", member, domain_id.0);
                            } else {
                                info!("Member {} already in domain {}", member, domain_id.0);
                            }
                        }
                        MembershipAction::Remove => {
                            if let Some(pos) = members.iter().position(|m| m == &member) {
                                members.remove(pos);
                                info!(
                                    "✓ Removed {} from domain {} membership",
                                    member, domain_id.0
                                );
                            } else {
                                warn!("Member {} not found in domain {}", member, domain_id.0);
                            }
                        }
                    },
                    MembershipSource::TrustThreshold(threshold) => {
                        // For trust-based membership, we cannot add/remove individual members
                        // because membership is derived from the trust graph, not a static list.
                        //
                        // Proper resolution would require:
                        // 1. Query trust graph for all DIDs meeting threshold
                        // 2. Convert to static list with those members
                        // 3. Apply add/remove action
                        //
                        // However, this is a governance decision - converting from trust-based
                        // to static membership is a significant policy change that should be
                        // handled via a ConfigChange proposal, not implicitly.
                        warn!(
                            "Domain {} uses trust-based membership (threshold: {}); \
                             explicit add/remove not supported. Submit a ConfigChange proposal \
                             to convert to static membership first.",
                            domain_id.0, threshold
                        );
                        bail!(
                            "Cannot add/remove members from trust-based membership domain '{}'. \
                             Use a ConfigChange proposal to convert to static membership first.",
                            domain_id.0
                        );
                    }
                }

                // Update timestamp
                domain.updated_at = icn_time::current_timestamp_secs();

                // Persist locally
                self.store
                    .put(&domain_key(&domain_id), &serde_json::to_vec(&domain)?)?;

                // Broadcast to network
                self.publish(GovernanceMessage::domain_updated(domain))
                    .await?;

                icn_obs::metrics::governance::membership_updated_inc();
                info!("✓ Membership update complete for domain {}", domain_id.0);
            }
        }

        Ok(())
    }

    /// Publish a governance message to the network
    async fn publish(&self, msg: GovernanceMessage) -> Result<[u8; 32]> {
        let bytes = msg.to_bytes()?;
        let mut g = self.gossip.write().await;
        let hash = g.publish(GOVERNANCE_TOPIC, bytes)?;
        Ok(hash)
    }

    /// List all domains
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let prefix = domain_key_prefix();
        let rows = self.store.scan(prefix)?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<GovernanceDomain>(&v)?))
            .collect()
    }

    /// List all proposals
    fn list_proposals(&self) -> Result<Vec<Proposal>> {
        let prefix = proposal_key_prefix();
        let rows = self.store.scan(prefix)?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<Proposal>(&v)?))
            .collect()
    }

    /// Load a domain by ID
    fn load_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        load_json(self.store.as_ref(), &domain_key(id))
    }

    /// Load a proposal by ID
    fn load_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        load_json(self.store.as_ref(), &proposal_key(id))
    }

    /// Load all votes for a proposal
    fn load_votes(&self, id: &ProposalId) -> Result<Vec<Vote>> {
        let prefix = vote_key_prefix(id);
        let rows = self.store.scan(&prefix)?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<Vote>(&v)?))
            .collect()
    }

    /// Create a new delegation
    fn create_delegation(&mut self, delegation: Delegation) -> Result<()> {
        // Validate no self-delegation
        if delegation.delegator == delegation.delegate {
            bail!("Cannot delegate to yourself");
        }

        // Store the delegation
        self.store.put(
            &delegation_key(&delegation.id),
            &serde_json::to_vec(&delegation)?,
        )?;

        info!(
            "✓ Delegation created: {} -> {} (scope: {:?})",
            delegation.delegator, delegation.delegate, delegation.scope
        );

        Ok(())
    }

    /// Load a delegation by ID
    fn load_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        load_json(self.store.as_ref(), &delegation_key(id))
    }

    /// List all delegations from a delegator
    fn list_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        let prefix = delegation_key_prefix();
        let rows = self.store.scan(prefix)?;
        rows.into_iter()
            .filter_map(|(k, v)| match serde_json::from_slice::<Delegation>(&v) {
                Ok(d) => {
                    if &d.delegator == delegator && d.revoked_at.is_none() {
                        Some(Ok(d))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        key = %String::from_utf8_lossy(&k),
                        error = %e,
                        "Failed to deserialize delegation record, skipping"
                    );
                    None
                }
            })
            .collect()
    }

    /// List all delegations to a delegate
    fn list_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        let prefix = delegation_key_prefix();
        let rows = self.store.scan(prefix)?;
        rows.into_iter()
            .filter_map(|(k, v)| match serde_json::from_slice::<Delegation>(&v) {
                Ok(d) => {
                    if &d.delegate == delegate && d.revoked_at.is_none() {
                        Some(Ok(d))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        key = %String::from_utf8_lossy(&k),
                        error = %e,
                        "Failed to deserialize delegation record, skipping"
                    );
                    None
                }
            })
            .collect()
    }

    /// Revoke a delegation
    fn revoke_delegation(&mut self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        let mut delegation = self
            .load_delegation(id)?
            .ok_or_else(|| anyhow::anyhow!("Delegation not found: {}", id.0))?;

        delegation.revoked_at = Some(revoked_at);

        self.store.put(
            &delegation_key(&delegation.id),
            &serde_json::to_vec(&delegation)?,
        )?;

        info!("✓ Delegation revoked: {}", id.0);

        Ok(())
    }
}

/// Handle incoming governance messages from gossip
fn handle_incoming(store: &dyn Store, msg: GovernanceMessage) -> Result<()> {
    match msg {
        GovernanceMessage::DomainCreated { domain } => {
            // Use domain.id as the key
            let id = GovernanceDomainId(domain.id.0.clone());
            store.put(&domain_key(&id), &serde_json::to_vec(&domain)?)?;
        }

        GovernanceMessage::DomainUpdated { domain } => {
            // Update existing domain with new config
            let id = GovernanceDomainId(domain.id.0.clone());
            store.put(&domain_key(&id), &serde_json::to_vec(&domain)?)?;
            info!("Domain config updated via gossip: {}", id.0);
        }

        GovernanceMessage::ProposalCreated { proposal } => {
            store.put(&proposal_key(&proposal.id), &serde_json::to_vec(&proposal)?)?;
        }

        GovernanceMessage::ProposalOpened {
            id,
            opened_at,
            closes_at,
        } => {
            // Load, update state, persist
            if let Some(proposal) = load_json::<Proposal>(store, &proposal_key(&id))? {
                // Force state to Open (idempotent for convergence)
                let updated = Proposal {
                    state: ProposalState::Open {
                        opened_at,
                        closes_at,
                    },
                    updated_at: now_seconds(),
                    ..proposal
                };
                store.put(&proposal_key(&id), &serde_json::to_vec(&updated)?)?;
            }
        }

        GovernanceMessage::VoteCast { vote, .. } => {
            store.put(
                &vote_key(&vote.proposal_id, &vote.voter),
                &serde_json::to_vec(&vote)?,
            )?;
        }

        GovernanceMessage::ProposalClosed {
            id,
            outcome,
            closed_at,
            ..
        } => {
            if let Some(mut proposal) = load_json::<Proposal>(store, &proposal_key(&id))? {
                let new_state = match outcome {
                    ProposalOutcome::Accepted => ProposalState::Accepted { closed_at },
                    ProposalOutcome::Rejected => ProposalState::Rejected { closed_at },
                    ProposalOutcome::NoQuorum => ProposalState::NoQuorum { closed_at },
                };
                proposal.close(new_state)?;
                store.put(&proposal_key(&id), &serde_json::to_vec(&proposal)?)?;
            }
        }

        _ => {
            // Ignore other message types for now (future: DomainUpdated, ProposalCancelled)
        }
    }

    Ok(())
}

// ---- Storage key helpers (aligned with icnctl) ----

fn domain_key(id: &GovernanceDomainId) -> Vec<u8> {
    format!("gov:domain:{}", id.0).into_bytes()
}

fn domain_key_prefix() -> &'static [u8] {
    b"gov:domain:"
}

fn proposal_key(id: &ProposalId) -> Vec<u8> {
    format!("gov:proposal:{}", id.0).into_bytes()
}

fn proposal_key_prefix() -> &'static [u8] {
    b"gov:proposal:"
}

fn vote_key(proposal_id: &ProposalId, voter: &Did) -> Vec<u8> {
    format!("gov:vote:{}:{}", proposal_id.0, voter).into_bytes()
}

fn vote_key_prefix(proposal_id: &ProposalId) -> Vec<u8> {
    format!("gov:vote:{}:", proposal_id.0).into_bytes()
}

fn delegation_key(id: &DelegationId) -> Vec<u8> {
    format!("gov:delegation:{}", id.0).into_bytes()
}

fn delegation_key_prefix() -> &'static [u8] {
    b"gov:delegation:"
}

// ---- Utility functions ----

fn load_json<T: for<'a> serde::Deserialize<'a>>(
    store: &dyn Store,
    key: &[u8],
) -> Result<Option<T>> {
    match store.get(key)? {
        Some(v) => Ok(Some(serde_json::from_slice::<T>(&v)?)),
        None => Ok(None),
    }
}

fn now_seconds() -> u64 {
    icn_time::current_timestamp_secs()
}
