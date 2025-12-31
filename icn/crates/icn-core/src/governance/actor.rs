//! GovernanceActor implementation
//!
//! This actor manages governance state and coordinates distributed decision-making
//! across the ICN network.

use anyhow::{bail, Result};
use async_trait::async_trait;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_store::Store;

use icn_entity::EntityId;
use icn_governance::{
    DecisionOutcome, Delegation, DelegationId, GovernanceConfig, GovernanceDomain,
    GovernanceDomainId, GovernanceMessage, GovernanceParams, GovernanceProfile,
    GovernanceProfileId, GovernanceRule, MembershipAction, MembershipConfig, MembershipResolver,
    MembershipSource, ParameterChange, Proposal, ProposalId, ProposalOutcome, ProposalPayload,
    ProposalState, ProtocolParameter, ProtocolParameterStore, TallySnapshot, Timestamp, Vote,
    VoteChoice, VoteTally,
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
    /// Protocol parameter store for governable parameters (Phase 20)
    protocol_params: Option<Arc<dyn ProtocolParameterStore>>,
    /// Entity registry for validating scope entity existence
    entity_registry: Option<Arc<dyn icn_entity::EntityRegistry>>,
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

    /// Get vote tally for a proposal
    pub async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        self.inner.read().await.get_vote_tally(proposal_id)
    }

    /// Get list of voter DIDs for a proposal
    pub async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        self.inner.read().await.get_voter_dids(proposal_id)
    }

    /// Set the protocol parameter store
    ///
    /// This must be called after spawn() to enable protocol parameter operations.
    ///
    /// **Note**: This method consumes self and returns a new handle. Any clones made
    /// before calling this method will NOT have the protocol parameter store configured.
    /// Always call this before cloning the handle.
    pub fn with_protocol_params(mut self, store: Arc<dyn ProtocolParameterStore>) -> Self {
        self.protocol_params = Some(store);
        self
    }

    /// Set the entity registry
    ///
    /// This enables validation that entities referenced in parameter scopes actually exist.
    ///
    /// **Note**: This method consumes self and returns a new handle. Any clones made
    /// before calling this method will NOT have the entity registry configured.
    /// Always call this before cloning the handle.
    pub fn with_entity_registry(mut self, registry: Arc<dyn icn_entity::EntityRegistry>) -> Self {
        self.entity_registry = Some(registry);
        self
    }

    /// List all protocol parameters
    pub fn list_protocol_parameters(&self) -> Result<Vec<ProtocolParameter>> {
        match &self.protocol_params {
            Some(store) => store.list(),
            None => bail!("Protocol parameter store not configured"),
        }
    }

    /// Get a specific protocol parameter by ID
    pub fn get_protocol_parameter(&self, id: &str) -> Result<Option<ProtocolParameter>> {
        match &self.protocol_params {
            Some(store) => store.get(id),
            None => bail!("Protocol parameter store not configured"),
        }
    }

    /// Get the effective value of a protocol parameter with scope resolution
    pub fn get_effective_protocol_parameter(
        &self,
        id: &str,
        coop_id: Option<&EntityId>,
        fed_id: Option<&EntityId>,
    ) -> Result<Option<ProtocolParameter>> {
        match &self.protocol_params {
            Some(store) => store.get_effective(id, coop_id, fed_id),
            None => bail!("Protocol parameter store not configured"),
        }
    }

    /// Get the change history for a protocol parameter
    pub fn get_protocol_parameter_history(&self, id: &str) -> Result<Vec<ParameterChange>> {
        match &self.protocol_params {
            Some(store) => store.get_history(id),
            None => bail!("Protocol parameter store not configured"),
        }
    }

    /// Set a protocol parameter value
    ///
    /// Used to persist approved ProtocolChange proposals.
    pub fn set_protocol_parameter(
        &self,
        param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> Result<()> {
        match &self.protocol_params {
            Some(store) => store.set(param, proposal_id, changed_by),
            None => bail!("Protocol parameter store not configured"),
        }
    }

    /// Schedule a pending parameter change for delayed execution
    ///
    /// Used when a ProtocolChange proposal has `effective_at` set.
    /// The change will be applied by the background scheduler when the time comes.
    pub fn schedule_pending_change(
        &self,
        change: icn_governance::PendingParameterChange,
    ) -> Result<()> {
        match &self.protocol_params {
            Some(store) => {
                store.add_pending_change(change)?;
                icn_obs::metrics::protocol::pending_parameter_changes_scheduled_inc();
                Ok(())
            }
            None => bail!("Protocol parameter store not configured"),
        }
    }

    /// Get the current count of active pending parameter changes
    pub fn count_pending_changes(&self) -> Result<usize> {
        match &self.protocol_params {
            Some(store) => store.count_pending_changes(),
            None => Ok(0),
        }
    }

    /// Check if an entity exists (for scope validation at execution time)
    ///
    /// Returns true if the entity registry is not configured (allowing scoped params
    /// without entity validation) or if the entity exists in the registry.
    pub fn entity_exists(&self, entity_id: &str) -> Result<bool> {
        match &self.entity_registry {
            Some(registry) => {
                let parsed_id = icn_entity::EntityId::from_str(entity_id)
                    .map_err(|e| anyhow::anyhow!("Invalid entity ID '{entity_id}': {e}"))?;
                registry.exists(&parsed_id).map_err(|e| e.into())
            }
            None => {
                // No registry configured - assume entity exists (best effort)
                // Log warning to help detect configuration issues
                warn!(
                    entity_id = %entity_id,
                    "Entity registry not configured - skipping entity existence validation. \
                     Configure with_entity_registry() for proper scoped parameter validation."
                );
                Ok(true)
            }
        }
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
        // Pre-validate ProtocolChange proposals to catch invalid parameters early
        // This prevents wasted governance cycles on proposals that would fail at execution
        if let ProposalPayload::ProtocolChange { ref proposal } = payload {
            // Check if protocol parameter store is configured
            let Some(ref store) = self.protocol_params else {
                bail!(
                    "Cannot create ProtocolChange proposal: protocol parameter store not configured"
                );
            };

            // Validate effective_at if set (delayed execution)
            if let Some(effective_at) = proposal.effective_at {
                let now = icn_time::current_timestamp_secs();

                // effective_at must be in the future
                if effective_at <= now {
                    bail!(
                        "Cannot create ProtocolChange proposal: effective_at ({effective_at}) must be in the future (current time: {now})"
                    );
                }

                // Optional: max delay limit (1 year = 365 * 24 * 60 * 60 = 31536000 seconds)
                const MAX_DELAY_SECONDS: u64 = 31_536_000;
                // Use checked arithmetic to prevent overflow if now is close to u64::MAX.
                // If overflow occurs, reject the proposal rather than allowing arbitrary future dates.
                let max_allowed = now.checked_add(MAX_DELAY_SECONDS).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Cannot create ProtocolChange proposal: timestamp overflow when calculating max allowed effective_at"
                    )
                })?;
                if effective_at > max_allowed {
                    bail!(
                        "Cannot create ProtocolChange proposal: effective_at is too far in the future (max: 1 year)"
                    );
                }
            }

            // Check if the parameter exists
            let param = match store.get(&proposal.parameter_id)? {
                Some(p) => p,
                None => {
                    // Try to find similar parameter names to suggest
                    let all_params = store.list().unwrap_or_default();
                    let similar: Vec<_> = all_params
                        .iter()
                        .filter(|p| {
                            // Simple similarity: shares prefix or contains search term
                            let id = &p.id;
                            let search = &proposal.parameter_id;
                            id.starts_with(search.split('.').next().unwrap_or(""))
                                || id.contains(search.split('.').next_back().unwrap_or(""))
                        })
                        .map(|p| p.id.as_str())
                        .take(3)
                        .collect();

                    let suggestion = if similar.is_empty() {
                        "Use list_protocol_parameters() to see available parameters.".to_string()
                    } else {
                        format!("Did you mean: {}?", similar.join(", "))
                    };

                    bail!(
                        "Cannot create ProtocolChange proposal: parameter '{}' not found. {}",
                        proposal.parameter_id,
                        suggestion
                    );
                }
            };

            // Validate the new value against constraints
            param.validate(&proposal.new_value).map_err(|e| {
                anyhow::anyhow!(
                    "Cannot create ProtocolChange proposal: validation failed for parameter '{}': {}",
                    proposal.parameter_id,
                    e
                )
            })?;

            // Validate scope override is allowed if specified
            if proposal.scope.is_some() && !param.constraints.allow_override {
                bail!(
                    "Cannot create ProtocolChange proposal: parameter '{}' does not allow scope overrides",
                    proposal.parameter_id
                );
            }

            // Validate entity exists if scope references an entity
            if let Some(ref scope) = proposal.scope {
                if let Some(entity_id) = scope.entity_id() {
                    match &self.entity_registry {
                        Some(registry) => {
                            match registry.exists(entity_id) {
                                Ok(true) => {} // Entity exists, proceed
                                Ok(false) => {
                                    bail!(
                                        "Cannot create ProtocolChange proposal: scope references non-existent entity '{entity_id}'"
                                    );
                                }
                                Err(e) => {
                                    bail!(
                                        "Cannot create ProtocolChange proposal: failed to verify entity '{entity_id}': {e}"
                                    );
                                }
                            }
                        }
                        None => {
                            // Entity registry is required for scoped parameter changes
                            // Without it, we cannot validate that the target entity exists,
                            // which could allow proposals targeting non-existent entities
                            bail!(
                                "Cannot create ProtocolChange proposal with scoped parameter: \
                                 entity registry not configured. Configure entity registry with \
                                 with_entity_registry() to enable scoped parameter changes."
                            );
                        }
                    }
                }
            }
        }

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

    // Vote tracking operations

    async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        Self::get_vote_tally(self, proposal_id).await
    }

    async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        Self::get_voter_dids(self, proposal_id).await
    }

    // Protocol parameter operations (Phase 20)

    async fn list_protocol_parameters(&self) -> Result<Vec<ProtocolParameter>> {
        Self::list_protocol_parameters(self)
    }

    async fn get_protocol_parameter(&self, id: &str) -> Result<Option<ProtocolParameter>> {
        Self::get_protocol_parameter(self, id)
    }

    async fn get_effective_protocol_parameter(
        &self,
        id: &str,
        coop_id: Option<&EntityId>,
        fed_id: Option<&EntityId>,
    ) -> Result<Option<ProtocolParameter>> {
        Self::get_effective_protocol_parameter(self, id, coop_id, fed_id)
    }

    async fn get_protocol_parameter_history(&self, id: &str) -> Result<Vec<ParameterChange>> {
        Self::get_protocol_parameter_history(self, id)
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
            protocol_params: None,
            entity_registry: None,
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

    /// Get vote tally for a proposal
    fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        let votes = self.load_votes(proposal_id)?;
        Ok(VoteTally::from(votes))
    }

    /// Get list of voter DIDs for a proposal
    fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        let votes = self.load_votes(proposal_id)?;
        Ok(votes.into_iter().map(|v| v.voter).collect())
    }

    /// Maximum depth for transitive delegations (number of hops allowed)
    ///
    /// With MAX_DELEGATION_DEPTH=3, a delegation chain can have at most 3 hops:
    /// A -> B -> C -> D (3 hops from A to D)
    ///
    /// Semantics:
    /// - `create_delegation` checks `incoming_depth >= MAX_DELEGATION_DEPTH` (exclusive)
    ///   So incoming_depth 0, 1, 2 allows creating the delegation
    /// - `detect_cycle` uses `0..=MAX_DELEGATION_DEPTH` (4 iterations) to check
    ///   delegator + delegate + up to 3 more hops for cycle detection
    const MAX_DELEGATION_DEPTH: usize = 3;

    /// Create a new delegation
    fn create_delegation(&mut self, delegation: Delegation) -> Result<()> {
        // Validate no self-delegation
        if delegation.delegator == delegation.delegate {
            bail!("Cannot delegate to yourself");
        }

        // Validate no duplicate delegation for this scope
        if self.has_active_delegation_for_scope(&delegation.delegator, &delegation.scope)? {
            bail!(
                "Active delegation already exists for scope {:?}",
                delegation.scope
            );
        }

        // Validate no cycles: check if following the chain from delegate leads back to delegator
        if self.would_create_cycle(
            &delegation.delegator,
            &delegation.delegate,
            &delegation.scope,
        )? {
            bail!(
                "Delegation would create a cycle: {} -> {} (scope: {:?})",
                delegation.delegator,
                delegation.delegate,
                delegation.scope
            );
        }

        // Validate max depth: check how many hops lead TO the delegator
        let incoming_depth =
            self.compute_incoming_depth(&delegation.delegator, &delegation.scope)?;
        if incoming_depth >= Self::MAX_DELEGATION_DEPTH {
            bail!(
                "Maximum delegation depth ({}) exceeded; current incoming depth is {}",
                Self::MAX_DELEGATION_DEPTH,
                incoming_depth
            );
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

    /// Check if a delegator already has an active delegation for a given scope
    fn has_active_delegation_for_scope(
        &self,
        delegator: &Did,
        scope: &icn_governance::DelegationScope,
    ) -> Result<bool> {
        let now = icn_time::current_timestamp_secs();
        let delegations = self.list_delegations_from(delegator)?;

        Ok(delegations
            .iter()
            .any(|d| d.revoked_at.is_none() && d.is_active(now) && d.scope == *scope))
    }

    /// Check if adding a delegation would create a cycle
    fn would_create_cycle(
        &self,
        delegator: &Did,
        delegate: &Did,
        scope: &icn_governance::DelegationScope,
    ) -> Result<bool> {
        use std::collections::HashSet;

        let now = icn_time::current_timestamp_secs();
        let mut current = delegate.clone();
        let mut visited = HashSet::new();
        visited.insert(delegator.clone());

        // Use inclusive range to allow exactly MAX_DELEGATION_DEPTH hops
        // With MAX_DELEGATION_DEPTH=3: 0..=3 checks 4 positions (delegate + 3 hops)
        // This ensures we detect cycles at the depth boundary
        for _ in 0..=Self::MAX_DELEGATION_DEPTH {
            if visited.contains(&current) {
                return Ok(true);
            }
            visited.insert(current.clone());

            // Find any active delegation from current that matches scope
            let delegations = self.list_delegations_from(&current)?;
            let next = delegations
                .into_iter()
                .find(|d| {
                    d.revoked_at.is_none()
                        && d.is_active(now)
                        && self.scopes_overlap(&d.scope, scope)
                })
                .map(|d| d.delegate.clone());

            match next {
                Some(d) => current = d,
                None => return Ok(false),
            }
        }

        Ok(false)
    }

    /// Check if two delegation scopes overlap (for cycle detection)
    ///
    /// This method handles scope overlap checking for the storage-backed GovernanceActor.
    ///
    /// # Why Not Using Shared Helper
    ///
    /// Unlike `DelegationManager` and `GovernanceMgr` which use the shared
    /// [`icn_governance::scopes_overlap`] function, this implementation has special
    /// error handling requirements:
    /// - `Ok(None)` (not found) → `false` (permissive - allow delegation)
    /// - `Err(e)` (storage error) → `true` (conservative - block delegation)
    ///
    /// The shared function only has a single `default_on_unknown` parameter and
    /// cannot distinguish between "not found" and "storage error" cases.
    ///
    /// # Eventual Consistency
    ///
    /// In a distributed gossip-based system, proposal info may not have propagated
    /// to all nodes when a delegation is created. By assuming no overlap for unknown
    /// proposals, we allow delegations to proceed without blocking valid use cases.
    ///
    /// Cycles that form during this propagation window are detected when the proposal
    /// is registered via [`icn_governance::DelegationManager::register_proposal`],
    /// which triggers cycle reconciliation and emits metrics for operator alerting.
    fn scopes_overlap(
        &self,
        a: &icn_governance::DelegationScope,
        b: &icn_governance::DelegationScope,
    ) -> bool {
        use icn_governance::DelegationScope;

        match (a, b) {
            (DelegationScope::Blanket, _) | (_, DelegationScope::Blanket) => true,
            (DelegationScope::Domain(d1), DelegationScope::Domain(d2)) => d1 == d2,
            (DelegationScope::Domain(d), DelegationScope::Proposal(p))
            | (DelegationScope::Proposal(p), DelegationScope::Domain(d)) => {
                // Look up proposal to get its domain for precise overlap checking
                match self.load_proposal(p) {
                    Ok(Some(proposal)) => &proposal.domain_id == d,
                    // Proposal-specific delegations are narrower than domain delegations,
                    // so assume no overlap when proposal is not found
                    Ok(None) => false,
                    // Keep conservative for storage errors (potential data corruption)
                    Err(e) => {
                        tracing::warn!(
                            proposal_id = %p.0,
                            error = %e,
                            "Storage error during cycle detection - assuming overlap conservatively. \
                             This may block valid delegations; check storage health."
                        );
                        icn_obs::metrics::governance::scope_overlap_storage_errors_inc();
                        true
                    }
                }
            }
            (DelegationScope::Proposal(p1), DelegationScope::Proposal(p2)) => p1 == p2,
        }
    }

    /// Compute the incoming delegation chain depth to a person
    fn compute_incoming_depth(
        &self,
        delegate: &Did,
        scope: &icn_governance::DelegationScope,
    ) -> Result<usize> {
        use std::collections::HashSet;

        let mut visited = HashSet::new();
        self.compute_incoming_depth_recursive(delegate, scope, &mut visited)
    }

    fn compute_incoming_depth_recursive(
        &self,
        delegate: &Did,
        scope: &icn_governance::DelegationScope,
        visited: &mut std::collections::HashSet<Did>,
    ) -> Result<usize> {
        if visited.contains(delegate) {
            return Ok(0);
        }
        visited.insert(delegate.clone());

        // Safety limit
        if visited.len() > Self::MAX_DELEGATION_DEPTH + 10 {
            return Ok(0);
        }

        let now = icn_time::current_timestamp_secs();

        // Find all delegators who have delegated to this delegate
        let delegations = self.list_delegations_to(delegate)?;
        let delegators: Vec<Did> = delegations
            .into_iter()
            .filter(|d| {
                d.revoked_at.is_none() && d.is_active(now) && self.scopes_overlap(&d.scope, scope)
            })
            .map(|d| d.delegator.clone())
            .collect();

        if delegators.is_empty() {
            return Ok(0);
        }

        // Recursively find the maximum depth
        let mut max_depth = 0;
        for delegator in delegators {
            let depth = 1 + self.compute_incoming_depth_recursive(&delegator, scope, visited)?;
            if depth > max_depth {
                max_depth = depth;
            }
        }

        Ok(max_depth)
    }

    /// Load a delegation by ID
    fn load_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        load_json(self.store.as_ref(), &delegation_key(id))
    }

    /// List all delegations from a delegator
    ///
    /// Returns error on deserialization failures to surface data corruption issues.
    fn list_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        let prefix = delegation_key_prefix();
        let rows = self.store.scan(prefix)?;
        let mut delegations = Vec::new();

        for (k, v) in rows {
            match serde_json::from_slice::<Delegation>(&v) {
                Ok(d) => {
                    if &d.delegator == delegator && d.revoked_at.is_none() {
                        delegations.push(d);
                    }
                }
                Err(e) => {
                    let key_str = String::from_utf8_lossy(&k);
                    tracing::error!(
                        key = %key_str,
                        error = %e,
                        "Failed to deserialize delegation record - data may be corrupted"
                    );
                    icn_obs::metrics::governance::deserialization_failures_inc();
                    anyhow::bail!(
                        "Failed to deserialize delegation record for key '{key_str}': {e}"
                    );
                }
            }
        }

        Ok(delegations)
    }

    /// List all delegations to a delegate
    ///
    /// Returns error on deserialization failures to surface data corruption issues.
    fn list_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        let prefix = delegation_key_prefix();
        let rows = self.store.scan(prefix)?;
        let mut delegations = Vec::new();

        for (k, v) in rows {
            match serde_json::from_slice::<Delegation>(&v) {
                Ok(d) => {
                    if &d.delegate == delegate && d.revoked_at.is_none() {
                        delegations.push(d);
                    }
                }
                Err(e) => {
                    let key_str = String::from_utf8_lossy(&k);
                    tracing::error!(
                        key = %key_str,
                        error = %e,
                        "Failed to deserialize delegation record - data may be corrupted"
                    );
                    icn_obs::metrics::governance::deserialization_failures_inc();
                    anyhow::bail!(
                        "Failed to deserialize delegation record for key '{key_str}': {e}"
                    );
                }
            }
        }

        Ok(delegations)
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
