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
use tracing::{debug, error, info, warn};

use crate::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_gossip::GossipActor;
use icn_identity::Did;

use icn_governance::{
    check_execution_gate, DecisionOutcome, Delegation, DelegationId, GovernanceConfig,
    GovernanceDecisionReceipt, GovernanceDomain, GovernanceDomainId, GovernanceMessage,
    GovernanceParams, GovernanceProfile, GovernanceProfileId, MembershipAction, MembershipConfig,
    MembershipResolver, MembershipSource, PaginatedResult, ParameterChange, Proposal, ProposalId,
    ProposalOutcome, ProposalPayload, ProposalScope, ProposalState, ProtocolParameter,
    ProtocolParameterStore, TallySnapshot, Timestamp, Vote, VoteChoice, VoteTally,
};

use icn_kernel_api::events::{EventEmitter, SystemEvent};

/// Gossip topic for governance messages
const GOVERNANCE_TOPIC: &str = "governance:proposal";

/// Interval for checking scheduled governance events (proposal close, deliberation end).
/// 10 seconds provides reasonable responsiveness without excessive polling.
const SCHEDULER_INTERVAL: Duration = Duration::from_secs(10);

/// Default voting period when domain config is unavailable.
/// 7 days is a reasonable default for cooperative decision-making, allowing
/// time for member participation while not delaying governance indefinitely.
const DEFAULT_VOTING_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Scheduled governance event types
#[derive(Clone, Debug)]
enum ScheduledEvent {
    /// Close voting for a proposal (Open → Closed)
    CloseVoting { proposal_id: ProposalId },
    /// End deliberation and open voting (Deliberation → Open)
    EndDeliberation {
        proposal_id: ProposalId,
        /// Voting period to use when transitioning to Open
        voting_period_seconds: u64,
    },
}

/// Scheduled governance event with timestamp
#[derive(Clone, Debug)]
struct ScheduledGovernanceEvent {
    /// When the event should fire
    at: Instant,
    /// The event to execute
    event: ScheduledEvent,
}

impl Eq for ScheduledGovernanceEvent {}

impl PartialEq for ScheduledGovernanceEvent {
    fn eq(&self, other: &Self) -> bool {
        // Compare both timestamp and proposal_id for correct equality semantics.
        // Two events at the same time for different proposals are not equal.
        self.at == other.at && self.proposal_id() == other.proposal_id()
    }
}

impl Ord for ScheduledGovernanceEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Earlier times have higher priority
        self.at.cmp(&other.at)
    }
}

impl PartialOrd for ScheduledGovernanceEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ScheduledGovernanceEvent {
    fn proposal_id(&self) -> &ProposalId {
        match &self.event {
            ScheduledEvent::CloseVoting { proposal_id } => proposal_id,
            ScheduledEvent::EndDeliberation { proposal_id, .. } => proposal_id,
        }
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
        /// Scope — local or federation-wide
        scope: ProposalScope,
    },
    /// Start deliberation period for a proposal
    ///
    /// Transitions the proposal from Draft to Deliberation state.
    /// During deliberation, members can discuss and refine the proposal
    /// before voting begins.
    StartDeliberation {
        /// Proposal to start deliberation on
        proposal_id: ProposalId,
        /// Duration of deliberation period in seconds
        deliberation_period_seconds: u64,
    },
    /// End deliberation and open for voting
    ///
    /// Transitions the proposal from Deliberation to Open state.
    /// Can be called manually or automatically when deliberation period ends.
    EndDeliberationAndOpen {
        /// Proposal to open for voting
        proposal_id: ProposalId,
        /// Duration of voting period in seconds
        voting_period_seconds: u64,
    },
    /// Open a proposal for voting (skip deliberation)
    ///
    /// Transitions directly from Draft to Open state.
    /// Use StartDeliberation for proposals that need discussion first.
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
        /// DID of the voter (authenticated caller)
        voter: Did,
        /// Vote choice
        choice: VoteChoice,
        /// Optional comment explaining the vote
        comment: Option<String>,
    },
    /// Close voting on a proposal
    CloseProposal {
        /// Proposal to close
        proposal_id: ProposalId,
        /// Optional set of eligible voter DIDs for close-time standing revalidation.
        ///
        /// When `Some`, only votes from DIDs in this set are counted in the tally.
        /// Votes from members who lost commons standing after casting are excluded.
        /// When `None`, all cast votes are counted (no eligibility filter applied).
        eligible_voters: Option<std::collections::HashSet<Did>>,
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
    /// Create a new vote delegation (persists + publishes gossip)
    CreateDelegation {
        /// The delegation to create
        delegation: Delegation,
    },
    /// Revoke an existing vote delegation (persists + publishes gossip)
    RevokeDelegation {
        /// Delegation ID to revoke
        id: DelegationId,
        /// When the revocation takes effect
        revoked_at: Timestamp,
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
    /// Optional kernel governance executor for delegated proposal execution
    executor: Option<Arc<dyn icn_kernel_api::governance::GovernanceExecutor>>,
    /// Shutdown signal for the background scheduler task.
    /// All clones share the same Arc so `shutdown()` works from any clone.
    /// Wrapped in Option so the signal is sent exactly once.
    scheduler_shutdown: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// JoinHandle for the background scheduler task.
    /// Stored so `shutdown()` can await task completion deterministically instead
    /// of relying on a fixed sleep. Taken (set to None) on first shutdown call.
    scheduler_task: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl GovernanceHandle {
    /// Signal the background scheduler task to exit and wait for it to complete.
    ///
    /// Sends the shutdown signal and awaits the task's `JoinHandle`, giving a
    /// deterministic guarantee that the task has fully exited and dropped all its
    /// Arc references (including to the sled store) by the time this returns.
    ///
    /// This allows the sled database to be cleanly closed and reopened within the
    /// same process — enabling true same-runtime restart proofs in tests.
    ///
    /// Idempotent: subsequent calls are no-ops (JoinHandle is taken on first call).
    pub async fn shutdown(&self) {
        // 1. Send the shutdown signal.
        if let Ok(mut guard) = self.scheduler_shutdown.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
        // 2. Take the JoinHandle outside the lock (never hold a sync mutex across .await).
        let task = self.scheduler_task.lock().ok().and_then(|mut g| g.take());
        // 3. Await task completion. When this returns, handle_clone inside the task
        //    has been dropped, releasing all captured Arc references.
        if let Some(t) = task {
            let _ = t.await;
        }
    }

    /// Submit a command to the governance actor
    pub async fn submit(&self, cmd: GovernanceCommand) -> Result<()> {
        self.inner.write().await.handle(cmd).await
    }

    /// List all governance domains
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        self.inner.read().await.list_domains()
    }

    /// List governance domains with pagination
    ///
    /// Returns a page of domains starting from the cursor position.
    pub async fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        self.inner
            .read()
            .await
            .list_domains_paginated(cursor, limit)
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

    /// Create a new delegation (persists to store and publishes gossip)
    pub async fn create_delegation(&self, delegation: Delegation) -> Result<()> {
        self.submit(GovernanceCommand::CreateDelegation { delegation })
            .await
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

    /// Revoke a delegation (persists to store and publishes gossip)
    pub async fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        self.submit(GovernanceCommand::RevokeDelegation {
            id: id.clone(),
            revoked_at,
        })
        .await
    }

    /// Get vote tally for a proposal
    pub async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        self.inner.read().await.get_vote_tally(proposal_id)
    }

    /// Get list of voter DIDs for a proposal
    pub async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        self.inner.read().await.get_voter_dids(proposal_id)
    }

    /// Get the GovernanceProofV2 for a closed proposal (if one was generated)
    pub async fn get_proof(
        &self,
        proposal_id: &ProposalId,
    ) -> Result<Option<icn_governance::GovernanceProofV2>> {
        let actor = self.inner.read().await;
        match actor.store.get_proof_bytes(proposal_id)? {
            Some(bytes) => {
                // Only return securely verifiable V2 proofs. Legacy proofs are not
                // returned because they cannot carry canonical V2 attestations.
                if let Ok(proof_v2) =
                    serde_json::from_slice::<icn_governance::GovernanceProofV2>(&bytes)
                {
                    match validate_secure_v2_proof_for_proposal(&proof_v2, proposal_id) {
                        Ok(()) => return Ok(Some(proof_v2)),
                        Err(reason) => {
                            warn!(
                                "Ignoring invalid stored governance proof for {}: {}",
                                proposal_id.0, reason
                            );
                            return Ok(None);
                        }
                    }
                }

                if serde_json::from_slice::<icn_governance::GovernanceProof>(&bytes).is_ok() {
                    warn!(
                        "Ignoring legacy governance proof for {}: missing canonical attestations",
                        proposal_id.0
                    );
                    return Ok(None);
                }

                // Treat invalid/malformed stored proofs as missing to avoid poisoning
                // the gateway call path with persistent 500 errors.
                warn!(
                    "Failed to deserialize governance proof for {}",
                    proposal_id.0
                );
                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// Start deliberation period for a proposal
    ///
    /// Transitions the proposal from Draft to Deliberation state.
    /// Members can discuss the proposal during this period.
    pub async fn start_deliberation(
        &self,
        proposal_id: ProposalId,
        deliberation_period_seconds: u64,
    ) -> Result<()> {
        self.submit(GovernanceCommand::StartDeliberation {
            proposal_id,
            deliberation_period_seconds,
        })
        .await
    }

    /// End deliberation and open for voting
    ///
    /// Transitions the proposal from Deliberation to Open state.
    /// Can only be called after the deliberation period has ended.
    pub async fn end_deliberation_and_open(
        &self,
        proposal_id: ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        self.submit(GovernanceCommand::EndDeliberationAndOpen {
            proposal_id,
            voting_period_seconds,
        })
        .await
    }

    /// Set the protocol parameter store
    ///
    /// This must be called after spawn() to enable protocol parameter operations.
    ///
    /// **Note**: This method consumes self and returns a new handle. Any clones made
    /// before calling this method will NOT have the protocol parameter store configured.
    /// Always call this before cloning the handle.
    pub fn with_protocol_params(mut self, store: Arc<dyn ProtocolParameterStore>) -> Self {
        self.protocol_params = Some(store.clone());
        // Also set on inner actor for use in handle() method
        // SAFETY: Called during initialization before handle is shared
        if let Ok(mut actor) = self.inner.try_write() {
            actor.protocol_params = Some(store);
        }
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

    /// Set the kernel governance executor for delegated proposal execution.
    ///
    /// When an executor is configured, the governance actor can delegate treasury
    /// and protocol operations to the kernel-provided executors. This enables clean
    /// separation between governance domain logic and kernel execution services.
    ///
    /// **Note**: This method consumes self and returns a new handle. Any clones made
    /// before calling this method will NOT have the executor configured.
    /// Always call this before cloning the handle.
    pub fn with_executor(
        mut self,
        executor: Arc<dyn icn_kernel_api::governance::GovernanceExecutor>,
    ) -> Self {
        self.executor = Some(executor.clone());
        // Also set on inner actor for use in handle() method
        // SAFETY: Called during initialization before handle is shared
        if let Ok(mut actor) = self.inner.try_write() {
            actor.executor = Some(executor);
        }
        self
    }

    /// Get the configured executor (if any).
    ///
    /// Returns the kernel governance executor, which provides access to
    /// treasury and protocol executors for delegated proposal execution.
    pub fn executor(&self) -> Option<&Arc<dyn icn_kernel_api::governance::GovernanceExecutor>> {
        self.executor.as_ref()
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
        coop_id: Option<&str>,
        fed_id: Option<&str>,
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

    /// Get thresholds for a proposal from protocol parameters
    ///
    /// Looks up quorum and approval thresholds from protocol parameters based on
    /// the proposal type. Falls back to None if parameters not found or store
    /// not configured.
    ///
    /// Parameter IDs follow the pattern:
    /// - `governance.quorum.<type>` for quorum percentage
    /// - `governance.approval.<type>` for approval percentage
    ///
    /// Where `<type>` is: freeze, veto, force_close, rollback, treasury_budget,
    /// treasury_withdrawal, treasury_rule
    pub fn get_thresholds_from_params(
        &self,
        payload: &ProposalPayload,
        coop_id: Option<&str>,
    ) -> Option<icn_governance::ProposalThresholds> {
        use icn_kernel_api::protocol_params::ParameterValue;

        let store = self.protocol_params.as_ref()?;

        // Determine parameter suffix based on proposal type
        let param_suffix = match payload {
            ProposalPayload::FreezeMember { .. } | ProposalPayload::UnfreezeMember { .. } => {
                Some("freeze")
            }
            ProposalPayload::VetoProposal { .. } => Some("veto"),
            ProposalPayload::ForceCloseProposal { .. } => Some("force_close"),
            ProposalPayload::RollbackLedger { .. } => Some("rollback"),
            ProposalPayload::Treasury { operation } => {
                use icn_governance::TreasuryProposalOperation;
                match operation {
                    TreasuryProposalOperation::CreateBudget { .. }
                    | TreasuryProposalOperation::CancelBudget { .. }
                    | TreasuryProposalOperation::ReclaimBudget { .. } => Some("treasury_budget"),
                    TreasuryProposalOperation::Withdraw { .. }
                    | TreasuryProposalOperation::TransferBetweenBudgets { .. }
                    | TreasuryProposalOperation::Spend { .. } => Some("treasury_withdrawal"),
                    TreasuryProposalOperation::ModifySpendingRule { .. } => Some("treasury_rule"),
                }
            }
            // Normal proposals use default min_quorum and min_approval
            _ => None,
        };

        // For normal proposals, try to get default thresholds
        if param_suffix.is_none() {
            let quorum = store
                .get_effective("governance.min_quorum", coop_id, None)
                .ok()
                .flatten()
                .and_then(|p| {
                    if let ParameterValue::Percentage(v) = p.value {
                        Some(v as u8)
                    } else {
                        None
                    }
                })?;

            let approval = store
                .get_effective("governance.min_approval", coop_id, None)
                .ok()
                .flatten()
                .and_then(|p| {
                    if let ParameterValue::Percentage(v) = p.value {
                        Some(v as u8)
                    } else {
                        None
                    }
                })?;

            return Some(icn_governance::ProposalThresholds::new(quorum, approval));
        }

        let suffix = param_suffix?;
        let quorum_id = format!("governance.quorum.{suffix}");
        let approval_id = format!("governance.approval.{suffix}");

        let quorum = store
            .get_effective(&quorum_id, coop_id, None)
            .ok()
            .flatten()
            .and_then(|p| {
                if let ParameterValue::Percentage(v) = p.value {
                    Some(v as u8)
                } else {
                    None
                }
            })?;

        let approval = store
            .get_effective(&approval_id, coop_id, None)
            .ok()
            .flatten()
            .and_then(|p| {
                if let ParameterValue::Percentage(v) = p.value {
                    Some(v as u8)
                } else {
                    None
                }
            })?;

        Some(icn_governance::ProposalThresholds::new(quorum, approval))
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

    async fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        Self::list_domains_paginated(self, cursor, limit).await
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
        scope: ProposalScope,
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

                // Get max delay from domain params (configurable per cooperative)
                // Falls back to default (1 year) if domain not found
                let max_delay_seconds = self
                    .get_domain(&domain_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|d| d.config.params.max_execution_delay_seconds)
                    .unwrap_or_else(icn_governance::default_max_execution_delay);

                // Use checked arithmetic to prevent overflow if now is close to u64::MAX.
                // If overflow occurs, reject the proposal rather than allowing arbitrary future dates.
                let max_allowed = now.checked_add(max_delay_seconds).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Cannot create ProtocolChange proposal: timestamp overflow when calculating max allowed effective_at"
                    )
                })?;
                if effective_at > max_allowed {
                    // Format the max delay in human-readable units
                    let max_display = if max_delay_seconds >= 24 * 60 * 60 {
                        let days = max_delay_seconds / (24 * 60 * 60);
                        format!("{days} days")
                    } else if max_delay_seconds >= 60 * 60 {
                        let hours = max_delay_seconds / (60 * 60);
                        format!("{hours} hours")
                    } else {
                        format!("{max_delay_seconds} seconds")
                    };
                    bail!(
                        "Cannot create ProtocolChange proposal: effective_at is too far in the future (max: {max_display})"
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
                if let Some(entity_id_str) = scope.entity_id_str() {
                    let entity_id: icn_entity::EntityId = entity_id_str.parse().map_err(|e| {
                        anyhow::anyhow!(
                            "Cannot create ProtocolChange proposal: invalid entity ID '{entity_id_str}': {e}"
                        )
                    })?;
                    match &self.entity_registry {
                        Some(registry) => {
                            match registry.exists(&entity_id) {
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
            scope,
        })
        .await?;

        Ok(proposal_id)
    }

    async fn start_deliberation(
        &self,
        proposal_id: ProposalId,
        deliberation_period_seconds: u64,
    ) -> Result<()> {
        Self::start_deliberation(self, proposal_id, deliberation_period_seconds).await
    }

    async fn end_deliberation_and_open(
        &self,
        proposal_id: ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        Self::end_deliberation_and_open(self, proposal_id, voting_period_seconds).await
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
        voter: Did,
        choice: icn_governance::VoteChoice,
        comment: Option<String>,
    ) -> Result<()> {
        self.submit(GovernanceCommand::CastVote {
            proposal_id,
            voter,
            choice,
            comment,
        })
        .await
    }

    async fn close_proposal(&self, proposal_id: ProposalId) -> Result<()> {
        self.submit(GovernanceCommand::CloseProposal {
            proposal_id,
            eligible_voters: None,
        })
        .await
    }

    async fn close_proposal_filtered(
        &self,
        proposal_id: ProposalId,
        eligible_voters: &std::collections::HashSet<Did>,
    ) -> Result<()> {
        self.submit(GovernanceCommand::CloseProposal {
            proposal_id,
            eligible_voters: Some(eligible_voters.clone()),
        })
        .await
    }

    async fn update_domain_membership(
        &self,
        domain_id: GovernanceDomainId,
        member: Did,
        action: MembershipAction,
    ) -> Result<()> {
        self.submit(GovernanceCommand::UpdateMembership {
            domain_id,
            action,
            member,
        })
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

    async fn get_proof(
        &self,
        proposal_id: &ProposalId,
    ) -> Result<Option<icn_governance::GovernanceProofV2>> {
        Self::get_proof(self, proposal_id).await
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
        coop_id: Option<&str>,
        fed_id: Option<&str>,
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
    store: Arc<dyn GovernanceStateStore>,
    gossip: Arc<RwLock<GossipActor>>,
    resolver: Arc<dyn MembershipResolver + Send + Sync>,
    profile: GovernanceProfile,
    event_scheduler: Arc<RwLock<BinaryHeap<Reverse<ScheduledGovernanceEvent>>>>,
    cancel_tx: mpsc::UnboundedSender<ProposalId>,
    event_bus: Option<Arc<dyn EventEmitter>>,
    /// Protocol parameter store for governable parameters (Phase 20)
    protocol_params: Option<Arc<dyn ProtocolParameterStore>>,
    /// Ed25519 signing key for generating GovernanceProofs
    signing_key: Option<Arc<ed25519_dalek::SigningKey>>,
    /// Optional kernel governance executor for delegated proposal execution
    executor: Option<Arc<dyn icn_kernel_api::governance::GovernanceExecutor>>,
}

impl GovernanceActor {
    /// Spawn a new governance actor
    pub async fn spawn(
        did: Did,
        store: Arc<dyn icn_store::Store>,
        gossip: Arc<RwLock<GossipActor>>,
        resolver: Arc<dyn MembershipResolver + Send + Sync>,
        event_bus: Option<Arc<dyn EventEmitter>>,
        signing_key: Option<Arc<ed25519_dalek::SigningKey>>,
    ) -> Result<GovernanceHandle> {
        info!("Spawning GovernanceActor for DID: {}", did);
        // Wrap raw store in typed state store abstraction
        let store: Arc<dyn GovernanceStateStore> = Arc::new(SledGovernanceStateStore::new(store));

        // Subscribe to governance topic
        {
            let mut g = gossip.write().await;
            g.subscribe(GOVERNANCE_TOPIC, did.clone()).await?;
        }

        // Set up notification callback for incoming messages
        let store_notify = store.clone();
        let did_notify = did.clone();

        {
            let mut g = gossip.write().await;
            g.add_notification_callback(Arc::new(move |topic, entry, _subscriber_did| {
                // Accept local governance topic and any federation governance topic
                // (federation topics have format "federation:governance:<fed_id>")
                let is_federation_gov = topic == icn_federation::TOPIC_FEDERATION_GOVERNANCE
                    || topic
                        .starts_with(&format!("{}:", icn_federation::TOPIC_FEDERATION_GOVERNANCE));
                if topic != GOVERNANCE_TOPIC && !is_federation_gov {
                    return;
                }

                match GovernanceMessage::from_bytes(&entry.data) {
                    Ok(msg) => {
                        info!(
                            "[{}] Received governance message: {}",
                            did_notify,
                            msg.message_type()
                        );
                        if let Err(e) =
                            handle_incoming(store_notify.as_ref(), msg, Some(&entry.author))
                        {
                            warn!("Failed to handle incoming governance message: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to deserialize governance message: {}", e);
                    }
                }
            }));
        }

        // Create unified event scheduler and cancellation channel
        let event_scheduler = Arc::new(RwLock::new(BinaryHeap::new()));
        let (cancel_tx, mut cancel_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let actor = GovernanceActor {
            did: did.clone(),
            store: store.clone(),
            gossip: gossip.clone(),
            resolver: resolver.clone(),
            profile: GovernanceProfile::cooperative_default(),
            event_scheduler: event_scheduler.clone(),
            cancel_tx,
            event_bus,
            protocol_params: None,
            signing_key,
            executor: None,
        };

        // Slot for the scheduler task JoinHandle; filled in after spawn.
        // The Arc is shared with the handle so shutdown() can await it.
        let scheduler_task_slot: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>> =
            Arc::new(std::sync::Mutex::new(None));

        let handle = GovernanceHandle {
            inner: Arc::new(RwLock::new(actor)),
            protocol_params: None,
            entity_registry: None,
            executor: None,
            scheduler_shutdown: Arc::new(std::sync::Mutex::new(Some(shutdown_tx))),
            scheduler_task: scheduler_task_slot.clone(),
        };

        // Spawn background timer task for scheduled governance events
        let handle_clone = handle.clone();
        let scheduler_clone = event_scheduler.clone();
        let scheduler_join = tokio::spawn(async move {
            let mut interval = tokio::time::interval(SCHEDULER_INTERVAL);
            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown_rx => {
                        info!("Governance scheduler shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        // Check for expired events
                        let now = Instant::now();
                        let mut expired_events = Vec::new();

                        {
                            let mut scheduler = scheduler_clone.write().await;
                            while let Some(Reverse(scheduled)) = scheduler.peek() {
                                if scheduled.at <= now {
                                    // SAFETY: We just peeked and confirmed an element exists
                                    #[allow(clippy::unwrap_used)]
                                    expired_events.push(scheduler.pop().unwrap().0.event.clone());
                                } else {
                                    break;
                                }
                            }
                        }

                        // Process expired events.
                        // Note: Command handlers validate proposal state before executing.
                        // If a manual transition races with the scheduler, the command will
                        // fail gracefully with a warning. This is expected behavior.
                        for event in expired_events {
                            match event {
                                ScheduledEvent::CloseVoting { proposal_id } => {
                                    info!("Auto-closing expired proposal: {}", proposal_id.0);
                                    if let Err(e) = handle_clone.submit(GovernanceCommand::CloseProposal {
                                        proposal_id: proposal_id.clone(),
                                        eligible_voters: None,
                                    }).await {
                                        // May fail if proposal was manually closed (race condition - expected)
                                        warn!("Scheduled close for proposal {} skipped: {}", proposal_id.0, e);
                                    }
                                }
                                ScheduledEvent::EndDeliberation { proposal_id, voting_period_seconds } => {
                                    info!("Auto-transitioning proposal {} from deliberation to voting", proposal_id.0);
                                    if let Err(e) = handle_clone.submit(GovernanceCommand::EndDeliberationAndOpen {
                                        proposal_id: proposal_id.clone(),
                                        voting_period_seconds,
                                    }).await {
                                        // May fail if deliberation was manually ended (race condition - expected)
                                        warn!("Scheduled deliberation end for proposal {} skipped: {}", proposal_id.0, e);
                                    }
                                }
                            }
                        }
                    }

                    Some(proposal_id) = cancel_rx.recv() => {
                        // Manual action - remove any pending events for this proposal
                        let mut scheduler = scheduler_clone.write().await;
                        scheduler.retain(|Reverse(sc)| sc.proposal_id() != &proposal_id);
                    }
                }
            }
        });

        // Store the JoinHandle so shutdown() can await task completion.
        if let Ok(mut slot) = scheduler_task_slot.lock() {
            *slot = Some(scheduler_join);
        }

        info!(
            "✓ Governance scheduler started (checking every {}s)",
            SCHEDULER_INTERVAL.as_secs()
        );

        Ok(handle)
    }

    /// Get proposal-type-specific thresholds from protocol parameters
    ///
    /// Returns `Some(thresholds)` if protocol parameters are configured and contain
    /// the relevant threshold parameters, `None` otherwise (fallback to domain config).
    fn get_thresholds_from_params(
        &self,
        payload: &ProposalPayload,
        coop_id: Option<&str>,
    ) -> Option<icn_governance::ProposalThresholds> {
        use icn_kernel_api::protocol_params::ParameterValue;

        let store = self.protocol_params.as_ref()?;

        // Determine parameter suffix based on proposal type
        let param_suffix = match payload {
            ProposalPayload::FreezeMember { .. } | ProposalPayload::UnfreezeMember { .. } => {
                Some("freeze")
            }
            ProposalPayload::VetoProposal { .. } => Some("veto"),
            ProposalPayload::ForceCloseProposal { .. } => Some("force_close"),
            ProposalPayload::RollbackLedger { .. } => Some("rollback"),
            ProposalPayload::Treasury { operation } => {
                use icn_governance::TreasuryProposalOperation;
                match operation {
                    TreasuryProposalOperation::CreateBudget { .. }
                    | TreasuryProposalOperation::CancelBudget { .. }
                    | TreasuryProposalOperation::ReclaimBudget { .. } => Some("treasury_budget"),
                    TreasuryProposalOperation::Withdraw { .. }
                    | TreasuryProposalOperation::TransferBetweenBudgets { .. }
                    | TreasuryProposalOperation::Spend { .. } => Some("treasury_withdrawal"),
                    TreasuryProposalOperation::ModifySpendingRule { .. } => Some("treasury_rule"),
                }
            }
            // Normal proposals use default min_quorum and min_approval
            _ => None,
        };

        // For normal proposals, try to get default thresholds
        if param_suffix.is_none() {
            let quorum = store
                .get_effective("governance.min_quorum", coop_id, None)
                .ok()
                .flatten()
                .and_then(|p| {
                    if let ParameterValue::Percentage(v) = p.value {
                        Some(v as u8)
                    } else {
                        None
                    }
                })?;

            let approval = store
                .get_effective("governance.min_approval", coop_id, None)
                .ok()
                .flatten()
                .and_then(|p| {
                    if let ParameterValue::Percentage(v) = p.value {
                        Some(v as u8)
                    } else {
                        None
                    }
                })?;

            return Some(icn_governance::ProposalThresholds::new(quorum, approval));
        }

        // For special proposal types, look up specific thresholds
        let suffix = param_suffix?;

        let quorum_key = format!("governance.quorum.{suffix}");
        let approval_key = format!("governance.approval.{suffix}");

        let quorum = store
            .get_effective(&quorum_key, coop_id, None)
            .ok()
            .flatten()
            .and_then(|p| {
                if let ParameterValue::Percentage(v) = p.value {
                    Some(v as u8)
                } else {
                    None
                }
            })?;

        let approval = store
            .get_effective(&approval_key, coop_id, None)
            .ok()
            .flatten()
            .and_then(|p| {
                if let ParameterValue::Percentage(v) = p.value {
                    Some(v as u8)
                } else {
                    None
                }
            })?;

        Some(icn_governance::ProposalThresholds::new(quorum, approval))
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
                let domain = GovernanceDomain::with_id(
                    domain_id.clone(),
                    name,
                    GovernanceConfig::new(profile_id, config.membership, config.params),
                );

                // Persist locally
                self.store.save_domain(&domain)?;

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
                scope,
            } => {
                info!("Creating proposal: {} (scope: {:?})", title, scope);

                let mut proposal = Proposal::new(
                    domain_id,
                    self.did.clone(),
                    title.clone(),
                    description,
                    payload,
                )
                .with_scope(scope);

                // Use the provided proposal ID instead of the generated one
                proposal.id = proposal_id.clone();

                // Persist locally
                self.store.save_proposal(&proposal)?;

                // Broadcast to network
                self.publish(GovernanceMessage::proposal_created(proposal.clone()))
                    .await?;

                // Also broadcast on federation topic if federation-scoped
                self.publish_federation_if_scoped(
                    &proposal,
                    GovernanceMessage::proposal_created(proposal.clone()),
                )
                .await;

                info!("✓ Proposal created: {} (ID: {})", title, proposal_id.0);
            }

            GovernanceCommand::StartDeliberation {
                proposal_id,
                deliberation_period_seconds,
            } => {
                info!("Starting deliberation for proposal: {}", proposal_id.0);

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Start deliberation
                proposal.start_deliberation(deliberation_period_seconds)?;

                // Extract timestamps from state
                let (started_at, ends_at) = match &proposal.state {
                    ProposalState::Deliberation {
                        started_at,
                        ends_at,
                    } => (*started_at, *ends_at),
                    _ => bail!("Proposal failed to transition to Deliberation state"),
                };

                // Persist updated state
                self.store.save_proposal(&proposal)?;

                // Load domain to get default voting period for auto-transition
                // Use graceful fallback if domain load fails to avoid blocking deliberation
                let voting_period_seconds = match self.load_domain(&proposal.domain_id) {
                    Ok(Some(domain)) => domain.config.params.voting_period_seconds,
                    Ok(None) => {
                        warn!(
                            "Domain {} not found for proposal {}, using default voting period",
                            proposal.domain_id.0, proposal_id.0
                        );
                        DEFAULT_VOTING_PERIOD_SECONDS
                    }
                    Err(e) => {
                        warn!(
                            "Failed to load domain {} for proposal {}: {}, using default voting period",
                            proposal.domain_id.0, proposal_id.0, e
                        );
                        DEFAULT_VOTING_PERIOD_SECONDS
                    }
                };

                // Schedule auto-transition from Deliberation to Open when deliberation ends
                let ends_at_instant =
                    Instant::now() + Duration::from_secs(deliberation_period_seconds);
                let scheduled = ScheduledGovernanceEvent {
                    at: ends_at_instant,
                    event: ScheduledEvent::EndDeliberation {
                        proposal_id: proposal_id.clone(),
                        voting_period_seconds,
                    },
                };
                self.event_scheduler.write().await.push(Reverse(scheduled));

                // Broadcast to network
                let delib_msg = GovernanceMessage::deliberation_started(
                    proposal_id.clone(),
                    started_at,
                    ends_at,
                );
                self.publish(delib_msg.clone()).await?;

                // Also broadcast on federation topic if federation-scoped
                self.publish_federation_if_scoped(&proposal, delib_msg)
                    .await;

                info!(
                    "✓ Deliberation started for proposal: {} (ends in {}s)",
                    proposal_id.0, deliberation_period_seconds
                );
            }

            GovernanceCommand::EndDeliberationAndOpen {
                proposal_id,
                voting_period_seconds,
            } => {
                info!(
                    "Ending deliberation and opening voting for proposal: {}",
                    proposal_id.0
                );

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // End deliberation and open for voting
                proposal.end_deliberation_and_open(voting_period_seconds)?;

                // Extract timestamps from state
                let (opened_at, closes_at) = match &proposal.state {
                    ProposalState::Open {
                        opened_at,
                        closes_at,
                    } => (*opened_at, *closes_at),
                    _ => bail!("Proposal failed to transition to Open state"),
                };

                // Persist updated state
                self.store.save_proposal(&proposal)?;

                // Schedule auto-close for voting
                let closes_at_instant = Instant::now() + Duration::from_secs(voting_period_seconds);
                let scheduled = ScheduledGovernanceEvent {
                    at: closes_at_instant,
                    event: ScheduledEvent::CloseVoting {
                        proposal_id: proposal_id.clone(),
                    },
                };
                self.event_scheduler.write().await.push(Reverse(scheduled));

                // Broadcast deliberation ended to network
                // Note: comment_count and participant_count are 0 since
                // comment tracking is not yet implemented in the actor
                let ended_msg = GovernanceMessage::deliberation_ended(
                    proposal_id.clone(),
                    opened_at, // ended_at == opened_at (same moment)
                    0,         // comment_count - not yet tracked
                    0,         // participant_count - not yet tracked
                );
                self.publish(ended_msg.clone()).await?;

                // Also broadcast on federation topic if federation-scoped
                self.publish_federation_if_scoped(&proposal, ended_msg)
                    .await;

                // Broadcast proposal opened to network
                let opened_msg =
                    GovernanceMessage::proposal_opened(proposal_id.clone(), opened_at, closes_at);
                self.publish(opened_msg.clone()).await?;

                // Also broadcast on federation topic if federation-scoped
                self.publish_federation_if_scoped(&proposal, opened_msg)
                    .await;

                info!(
                    "✓ Deliberation ended, voting opened for proposal: {} (closes in {}s)",
                    proposal_id.0, voting_period_seconds
                );
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
                self.store.save_proposal(&proposal)?;

                // Schedule auto-close
                let closes_at_instant = Instant::now() + Duration::from_secs(voting_period_seconds);
                let scheduled = ScheduledGovernanceEvent {
                    at: closes_at_instant,
                    event: ScheduledEvent::CloseVoting {
                        proposal_id: proposal_id.clone(),
                    },
                };
                self.event_scheduler.write().await.push(Reverse(scheduled));

                // Broadcast to network
                let opened_msg =
                    GovernanceMessage::proposal_opened(proposal_id.clone(), opened_at, closes_at);
                self.publish(opened_msg.clone()).await?;

                // Also broadcast on federation topic if federation-scoped
                self.publish_federation_if_scoped(&proposal, opened_msg)
                    .await;

                info!(
                    "✓ Proposal opened: {} (auto-close scheduled for {}s)",
                    proposal_id.0, voting_period_seconds
                );
            }

            GovernanceCommand::CastVote {
                proposal_id,
                voter,
                choice,
                comment,
            } => {
                info!("Casting vote on proposal: {} by {}", proposal_id.0, voter);

                let mut vote = Vote::new(proposal_id.clone(), voter, choice);
                if let Some(c) = comment {
                    vote = vote.with_comment(c);
                }

                // Persist locally
                self.store.save_vote(&proposal_id, &vote)?;

                // Broadcast to network
                let vote_msg = GovernanceMessage::vote_cast(vote, None);
                self.publish(vote_msg.clone()).await?;

                // Forward vote to federation topic if proposal is federation-scoped
                if let Some(proposal) = self.load_proposal(&proposal_id)? {
                    self.publish_federation_if_scoped(&proposal, vote_msg).await;
                }

                info!("✓ Vote cast: {:?}", choice);
            }

            GovernanceCommand::CloseProposal {
                proposal_id,
                eligible_voters,
            } => {
                info!("Closing proposal: {}", proposal_id.0);

                // Notify scheduler to cancel any pending events (if scheduled)
                let _ = self.cancel_tx.send(proposal_id.clone());

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Load domain
                let domain = self
                    .load_domain(&proposal.domain_id)?
                    .ok_or_else(|| anyhow::anyhow!("Domain not found: {}", proposal.domain_id.0))?;

                // Load all cast votes, then apply close-time eligibility filter if provided.
                // When `eligible_voters` is Some (from handler-level standing revalidation),
                // votes from members who lost commons standing after casting are excluded.
                // The proof records only the votes that counted in the final decision.
                let all_votes = self.load_votes(&proposal_id)?;
                let votes: Vec<Vote> = match &eligible_voters {
                    Some(filter) => all_votes
                        .into_iter()
                        .filter(|v| filter.contains(&v.voter))
                        .collect(),
                    None => all_votes,
                };
                let mut tally = VoteTally::empty();
                for v in &votes {
                    tally.add_vote(v);
                }

                // Resolve eligible membership (list needed for delegation resolution).
                // eligible_count uses the FULL member list as the quorum denominator —
                // standing revalidation filters whose votes count but does not shrink
                // the community for quorum purposes.
                let eligible_members = self.resolver.resolve_members(&domain)?;
                let eligible_count = eligible_members.len();

                // Edge case: cannot evaluate proposal with zero eligible voters
                // This prevents division issues and ensures meaningful quorum calculation
                if eligible_count == 0 {
                    return Err(anyhow::anyhow!(
                        "Cannot close proposal: no eligible voters in domain {}",
                        proposal.domain_id.0
                    ));
                }

                // Close-time liquid democracy: expand delegated votes for non-voters.
                //
                // Delegation scope: when standing revalidation is active (`eligible_voters`
                // is Some), only members with CURRENT standing can have delegation applied.
                // A member who lost standing cannot have their absent weight flow through
                // their delegation record — their absence must contribute to quorum failure,
                // not be silently covered by a pre-existing delegation.
                //
                // Without a standing filter (eligible_voters=None), the full member list
                // is used and any active delegation is honoured.
                let delegation_scope: Vec<Did> = match &eligible_voters {
                    Some(filter) => eligible_members
                        .iter()
                        .filter(|m| filter.contains(m))
                        .cloned()
                        .collect(),
                    None => eligible_members.clone(),
                };
                self.apply_delegation_to_tally(
                    &votes,
                    &delegation_scope,
                    &proposal.domain_id,
                    &proposal_id,
                    &mut tally,
                );

                // Get proposal-type-specific thresholds (Issue #477)
                // Emergency proposals (freeze, veto, rollback) require higher quorum/approval
                // to prevent low-turnout manipulation attacks.
                // First try protocol parameters (runtime programmable via ProtocolChange proposals),
                // fall back to domain config. Currently uses global scope; cooperative-specific
                // overrides come from domain config until domain<->entity mapping is established.
                let thresholds = self
                    .get_thresholds_from_params(&proposal.payload, None)
                    .unwrap_or_else(|| domain.config.thresholds_for_proposal(&proposal.payload));

                // Evaluate outcome with proposal-type-specific thresholds
                let outcome_result =
                    self.profile
                        .evaluate_with_thresholds(&tally, thresholds, eligible_count)?;

                // Record participation metrics (Issue #477)
                let proposal_type = proposal.payload.type_name();
                let total_votes = tally.total_votes();
                let participation_pct = if eligible_count > 0 {
                    (total_votes as f64 / eligible_count as f64) * 100.0
                } else {
                    0.0
                };
                let quorum_required_pct = thresholds.quorum_percentage as f64;
                let quorum_margin = participation_pct - quorum_required_pct;

                icn_obs::metrics::governance::participation_percentage_observe(
                    proposal_type,
                    participation_pct,
                );
                icn_obs::metrics::governance::quorum_margin_observe(proposal_type, quorum_margin);

                // Track quorum failures
                if matches!(outcome_result, DecisionOutcome::NoQuorum) {
                    icn_obs::metrics::governance::quorum_not_met_inc(proposal_type);

                    // Track emergency proposal quorum failures specifically
                    if let Some(emergency_type) = proposal.payload.emergency_type() {
                        icn_obs::metrics::governance::emergency_quorum_not_met_inc(emergency_type);
                    }
                }

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
                self.store.save_proposal(&proposal)?;

                // Invariant 7: gate all Accepted decisions before effect dispatch.
                //
                // Construct the receipt from current vote state and verify it passes
                // check_execution_gate. This is the production enforcement point — no
                // ProposalAccepted event fires without a receipt that is (a) Accepted and
                // (b) internally consistent (verify() passes). Non-Accepted outcomes skip
                // the gate entirely; they never produce resource-allocation effects.
                if matches!(outcome_result, DecisionOutcome::Accepted) {
                    use icn_governance::proof::ProofOutcome;
                    let gate_receipt = GovernanceDecisionReceipt::new(
                        proposal_id.0.clone(),
                        proposal.domain_id.0.clone(),
                        ProofOutcome::Accepted,
                        tally.clone(),
                        &votes,
                    );
                    if let Err(gate_err) = check_execution_gate(&gate_receipt) {
                        error!(
                            proposal_id = %proposal_id.0,
                            error = %gate_err,
                            "Invariant 7 gate rejected accepted proposal — blocking effect dispatch"
                        );
                        bail!(
                            "Invariant 7 gate failure for proposal {}: {gate_err}",
                            proposal_id.0
                        );
                    }
                    info!(
                        proposal_id = %proposal_id.0,
                        decision_hash = %hex::encode(gate_receipt.decision_hash),
                        "Invariant 7 gate passed"
                    );
                }

                // Generate GovernanceProofV2 if signing key is available
                let proof_bytes = if let Some(ref signing_key) = self.signing_key {
                    use icn_governance::ProofOutcome;

                    let proof_outcome = match outcome_result {
                        DecisionOutcome::Accepted => ProofOutcome::Accepted,
                        DecisionOutcome::Rejected => ProofOutcome::Rejected,
                        DecisionOutcome::NoQuorum => ProofOutcome::NoQuorum,
                    };

                    let receipt = icn_governance::GovernanceDecisionReceipt::new(
                        proposal_id.0.clone(),
                        proposal.domain_id.0.clone(),
                        proof_outcome,
                        tally.clone(),
                        &votes,
                    );

                    let attestation = icn_governance::GovernanceDecisionAttestation::sign(
                        receipt.decision_hash,
                        self.did.to_string(),
                        now,
                        signing_key,
                    );
                    let proof = icn_governance::GovernanceProofV2::new(receipt, vec![attestation]);

                    let serialized = serde_json::to_vec(&proof)?;

                    // Persist proof for later retrieval
                    self.store.save_proof_bytes(&proposal_id, &serialized)?;

                    info!(
                        "GovernanceProofV2 signed for proposal {} (decision_hash: {})",
                        proposal_id.0,
                        hex::encode(proof.receipt.decision_hash)
                    );

                    Some(serialized)
                } else {
                    tracing::debug!(
                        proposal_id = %proposal_id.0,
                        "GovernanceProof skipped — no signing key configured on this node"
                    );
                    None
                };

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
                    outcome_msg.clone(),
                    now,
                    tally_snapshot.clone(),
                    proof_bytes.clone(),
                ))
                .await?;

                // Also broadcast on federation topic if federation-scoped
                self.publish_federation_if_scoped(
                    &proposal,
                    GovernanceMessage::proposal_closed(
                        proposal_id.clone(),
                        outcome_msg,
                        now,
                        tally_snapshot,
                        proof_bytes,
                    ),
                )
                .await;

                // Emit event for downstream processing (e.g., ledger transactions)
                if let Some(ref event_bus) = self.event_bus {
                    let event = match outcome_result {
                        DecisionOutcome::Accepted => {
                            match serde_json::to_value(&proposal.payload) {
                                Ok(payload) => SystemEvent::ProposalAccepted {
                                    proposal_id: proposal_id.0.clone(),
                                    domain_id: proposal.domain_id.0.clone(),
                                    payload,
                                    decided_at: now,
                                },
                                Err(e) => {
                                    warn!(
                                        "Failed to serialize payload for accepted proposal {}: {e}",
                                        proposal_id.0
                                    );
                                    SystemEvent::ProposalExecutionFailed {
                                        proposal_id: proposal_id.0.clone(),
                                        proposal_type: proposal.payload.type_name().to_string(),
                                        error: format!("payload serialization failed: {e}"),
                                        failed_at: now,
                                    }
                                }
                            }
                        }
                        _ => SystemEvent::ProposalRejected {
                            proposal_id: proposal_id.0.clone(),
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

                // Notify scheduler to cancel any pending events (if scheduled)
                let _ = self.cancel_tx.send(proposal_id.clone());

                // Load proposal
                let mut proposal = self
                    .load_proposal(&proposal_id)?
                    .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

                // Veto the proposal
                proposal.veto(reason.clone())?;

                // Persist updated state
                self.store.save_proposal(&proposal)?;

                // Emit event for downstream processing
                if let Some(ref event_bus) = self.event_bus {
                    let now = now_seconds();
                    event_bus
                        .emit(SystemEvent::ProposalRejected {
                            proposal_id: proposal_id.0.clone(),
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

                // Notify scheduler to cancel any pending events (if scheduled)
                let _ = self.cancel_tx.send(proposal_id.clone());

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
                self.store.save_proposal(&proposal)?;

                // Emit appropriate event
                if let Some(ref event_bus) = self.event_bus {
                    let now = now_seconds();
                    let event = match forced_outcome {
                        ForcedOutcome::Accept => match serde_json::to_value(&proposal.payload) {
                            Ok(payload) => SystemEvent::ProposalAccepted {
                                proposal_id: proposal_id.0.clone(),
                                domain_id: proposal.domain_id.0.clone(),
                                payload,
                                decided_at: now,
                            },
                            Err(e) => {
                                warn!(
                                        "Failed to serialize payload for force-accepted proposal {}: {e}",
                                        proposal_id.0
                                    );
                                SystemEvent::ProposalExecutionFailed {
                                    proposal_id: proposal_id.0.clone(),
                                    proposal_type: proposal.payload.type_name().to_string(),
                                    error: format!("payload serialization failed: {e}"),
                                    failed_at: now,
                                }
                            }
                        },
                        _ => SystemEvent::ProposalRejected {
                            proposal_id: proposal_id.0.clone(),
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
                self.store.save_domain(&domain)?;

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
                self.store.save_domain(&domain)?;

                // Broadcast to network
                self.publish(GovernanceMessage::domain_updated(domain))
                    .await?;

                icn_obs::metrics::governance::membership_updated_inc();
                info!("✓ Membership update complete for domain {}", domain_id.0);
            }

            GovernanceCommand::CreateDelegation { delegation } => {
                let delegation_clone = delegation.clone();
                self.create_delegation(delegation)?;
                // Best-effort gossip publish — failure doesn't roll back the local write.
                if let Err(e) = self
                    .publish(GovernanceMessage::delegation_created(delegation_clone))
                    .await
                {
                    warn!("Failed to publish DelegationCreated gossip: {e}");
                }
            }

            GovernanceCommand::RevokeDelegation { id, revoked_at } => {
                // Load the delegator DID before revoking — needed for the gossip message.
                // If the delegation doesn't exist, revoke_delegation will also fail below.
                let revoked_by = self.load_delegation(&id)?.map(|d| d.delegator.clone());
                self.revoke_delegation(&id, revoked_at)?;
                if let Some(revoked_by) = revoked_by {
                    if let Err(e) = self
                        .publish(GovernanceMessage::delegation_revoked(
                            id, revoked_by, revoked_at,
                        ))
                        .await
                    {
                        warn!("Failed to publish DelegationRevoked gossip: {e}");
                    }
                }
            }
        }

        Ok(())
    }

    /// Publish a governance message to the network
    async fn publish(&self, msg: GovernanceMessage) -> Result<[u8; 32]> {
        let bytes = msg.to_bytes()?;
        let mut g = self.gossip.write().await;
        let hash = g.publish(GOVERNANCE_TOPIC, bytes).await?;
        Ok(hash)
    }

    /// Publish a governance message to a specific topic
    async fn publish_to_topic(&self, topic: &str, msg: GovernanceMessage) -> Result<[u8; 32]> {
        let bytes = msg.to_bytes()?;
        let mut g = self.gossip.write().await;
        let hash = g.publish(topic, bytes).await?;
        Ok(hash)
    }

    /// If the proposal is federation-scoped, also publish on the federation-specific topic.
    ///
    /// Topic format: `federation:governance:<fed_id>` — each federation gets its own topic
    /// so multi-federation nodes don't mix governance outcomes.
    async fn publish_federation_if_scoped(
        &self,
        proposal: &icn_governance::Proposal,
        msg: GovernanceMessage,
    ) {
        if let icn_governance::ProposalScope::Federation(ref fed_id) = proposal.scope {
            let topic = format!("{}:{}", icn_federation::TOPIC_FEDERATION_GOVERNANCE, fed_id);
            if let Err(e) = self.publish_to_topic(&topic, msg).await {
                warn!(
                    "Failed to publish federation governance message to {}: {}",
                    topic, e
                );
            }
        }
    }

    /// List all domains
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        self.store.list_domains()
    }

    /// List domains with pagination
    ///
    /// Returns a page of domains starting from the cursor position.
    /// Uses offset-based pagination with cursor format "offset:<number>".
    fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        // Parse offset from cursor
        let offset = match cursor {
            Some(c) => {
                if let Some(offset_str) = c.strip_prefix("offset:") {
                    offset_str.parse::<usize>().unwrap_or(0)
                } else {
                    0
                }
            }
            None => 0,
        };

        // Scan all domains (storage layer doesn't support native pagination)
        let all_domains = self.store.list_domains()?;
        let total = all_domains.len();

        // Skip to offset and take limit
        let page: Vec<GovernanceDomain> =
            all_domains.into_iter().skip(offset).take(limit).collect();

        // Calculate next cursor
        let next_offset = offset + page.len();
        let next_cursor = if next_offset < total {
            Some(format!("offset:{next_offset}"))
        } else {
            None
        };

        Ok(PaginatedResult {
            items: page,
            next_cursor,
            total: Some(total),
        })
    }

    /// List all proposals
    fn list_proposals(&self) -> Result<Vec<Proposal>> {
        self.store.list_proposals()
    }

    /// Load a domain by ID
    fn load_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        self.store.get_domain(id)
    }

    /// Load a proposal by ID
    fn load_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        self.store.get_proposal(id)
    }

    /// Load all votes for a proposal
    fn load_votes(&self, id: &ProposalId) -> Result<Vec<Vote>> {
        self.store.list_votes(id)
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
        self.store.save_delegation(&delegation)?;

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
    /// ## Example Error Handling Difference
    ///
    /// ```text
    /// Shared helper with default_on_unknown=false:
    ///   - Storage error → returns false (permissive, no metric)
    ///
    /// GovernanceActor (this implementation):
    ///   - Proposal not found → returns false (permissive)
    ///   - Storage error → returns true (conservative) + emits metric
    /// ```
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
        self.store.get_delegation(id)
    }

    /// List all delegations from a delegator
    ///
    /// Returns error on deserialization failures to surface data corruption issues.
    fn list_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        let rows = self.store.list_all_delegations()?;
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
        let rows = self.store.list_all_delegations()?;
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

    /// Resolve the delegation chain for a voter at a specific proposal, returning the
    /// final effective voter DID.
    ///
    /// Follows active (non-expired, non-revoked) delegations matching the proposal's
    /// domain or a blanket scope. Returns `voter` unchanged if no active delegation exists.
    /// Stops at `MAX_DELEGATION_DEPTH` to prevent runaway chains (cycles are guaranteed
    /// impossible by creation-time cycle detection, but the depth cap is a safety net).
    ///
    /// This mirrors `DelegationManager::resolve_delegate` but reads directly from the
    /// Sled store instead of an in-memory manager.
    fn resolve_delegate_from_store(
        &self,
        voter: &Did,
        domain_id: &icn_governance::GovernanceDomainId,
        proposal_id: &ProposalId,
    ) -> Did {
        let now = icn_time::current_timestamp_secs();
        let mut current = voter.clone();
        let mut visited = std::collections::HashSet::new();

        for _ in 0..Self::MAX_DELEGATION_DEPTH {
            visited.insert(current.clone());

            // Load active delegations from current node, pick the most specific scope match
            let delegations = match self.list_delegations_from(&current) {
                Ok(d) => d,
                Err(_) => break, // storage error → stop resolving, return best-so-far
            };

            let next = delegations
                .into_iter()
                .filter(|d| d.revoked_at.is_none() && d.is_active(now))
                .filter_map(|d| {
                    // Score by specificity: proposal-scope > domain-scope > blanket
                    match &d.scope {
                        icn_governance::DelegationScope::Proposal(pid) if pid == proposal_id => {
                            Some((3u8, d.delegate))
                        }
                        icn_governance::DelegationScope::Domain(did) if did == domain_id => {
                            Some((2, d.delegate))
                        }
                        icn_governance::DelegationScope::Blanket => Some((1, d.delegate)),
                        _ => None,
                    }
                })
                .max_by_key(|(score, _)| *score)
                .map(|(_, delegate)| delegate);

            match next {
                Some(d) if !visited.contains(&d) => current = d,
                _ => break,
            }
        }

        current
    }

    /// Apply delegation to a raw vote tally, expanding delegated votes for non-voters.
    ///
    /// For each eligible member who did NOT vote directly, resolves their delegation
    /// chain. If the final delegate DID voted, the non-voter's vote is attributed to
    /// the delegate's choice (liquid democracy: absent members' weight flows to
    /// whomever they trusted).
    ///
    /// Direct voters always override delegation — a member who voted directly is
    /// never replaced by a delegated vote, regardless of what delegation records exist.
    fn apply_delegation_to_tally(
        &self,
        votes: &[Vote],
        eligible_members: &[Did],
        domain_id: &icn_governance::GovernanceDomainId,
        proposal_id: &ProposalId,
        tally: &mut VoteTally,
    ) {
        // Build a fast lookup of who voted directly
        let direct_voters: std::collections::HashSet<&Did> =
            votes.iter().map(|v| &v.voter).collect();
        let vote_by_did: std::collections::HashMap<&Did, &Vote> =
            votes.iter().map(|v| (&v.voter, v)).collect();

        let mut delegated = 0usize;
        for member in eligible_members {
            if direct_voters.contains(member) {
                continue; // voted directly — no delegation needed
            }

            let delegate = self.resolve_delegate_from_store(member, domain_id, proposal_id);
            if &delegate == member {
                continue; // no active delegation
            }

            if let Some(delegate_vote) = vote_by_did.get(&delegate) {
                // Create a synthetic vote for the delegating member using the delegate's choice.
                let delegated_vote =
                    Vote::new(proposal_id.clone(), member.clone(), delegate_vote.choice);
                tally.add_vote(&delegated_vote);
                delegated += 1;
                tracing::debug!(
                    member = %member,
                    delegate = %delegate,
                    choice = ?delegate_vote.choice,
                    "Delegation applied: member's vote resolved through delegate"
                );
            }
        }

        if delegated > 0 {
            tracing::info!(
                proposal_id = %proposal_id.0,
                delegated_votes = delegated,
                "Close-time delegation resolution: {} votes applied via delegation",
                delegated
            );
        }
    }

    /// Revoke a delegation
    fn revoke_delegation(&mut self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        let mut delegation = self
            .load_delegation(id)?
            .ok_or_else(|| anyhow::anyhow!("Delegation not found: {}", id.0))?;

        delegation.revoked_at = Some(revoked_at);

        self.store
            .save_revoked_delegation(&delegation, revoked_at)?;

        info!("✓ Delegation revoked: {}", id.0);

        Ok(())
    }
}

/// Handle incoming governance messages from gossip.
///
/// `sender` is the `GossipEntry::author` DID of the peer that published the message.
/// For delegation messages, the sender must match the delegator (or `revoked_by`) to
/// prevent any peer from injecting or revoking delegations they do not own.
/// If `sender` is `None` (e.g. in tests that don't have a real gossip entry), delegation
/// sender validation is skipped.
fn handle_incoming(
    store: &dyn GovernanceStateStore,
    msg: GovernanceMessage,
    sender: Option<&icn_identity::Did>,
) -> Result<()> {
    match msg {
        GovernanceMessage::DomainCreated { domain } => {
            store.save_domain(&domain)?;
        }

        GovernanceMessage::DomainUpdated { domain } => {
            // Update existing domain with new config
            store.save_domain(&domain)?;
            info!("Domain config updated via gossip: {}", domain.id.0);
        }

        GovernanceMessage::ProposalCreated { proposal } => {
            store.save_proposal(&proposal)?;
        }

        GovernanceMessage::ProposalOpened {
            id,
            opened_at,
            closes_at,
        } => {
            // Load, update state, persist
            if let Some(proposal) = store.get_proposal(&id)? {
                // Force state to Open (idempotent for convergence)
                let updated = Proposal {
                    state: ProposalState::Open {
                        opened_at,
                        closes_at,
                    },
                    updated_at: now_seconds(),
                    ..proposal
                };
                store.save_proposal(&updated)?;
            }
        }

        GovernanceMessage::VoteCast { vote, .. } => {
            store.save_vote(&vote.proposal_id.clone(), &vote)?;
        }

        GovernanceMessage::ProposalClosed {
            id,
            outcome,
            closed_at,
            proof_bytes,
            ..
        } => {
            if let Some(mut proposal) = store.get_proposal(&id)? {
                let new_state = match outcome {
                    ProposalOutcome::Accepted => ProposalState::Accepted { closed_at },
                    ProposalOutcome::Rejected => ProposalState::Rejected { closed_at },
                    ProposalOutcome::NoQuorum => ProposalState::NoQuorum { closed_at },
                };
                proposal.close(new_state)?;
                store.save_proposal(&proposal)?;
            }
            // Persist proof from remote node (if included and valid)
            if let Some(bytes) = proof_bytes {
                // Validate remote proof before storing (untrusted gossip input)
                let incoming_v2 = match serde_json::from_slice::<icn_governance::GovernanceProofV2>(
                    &bytes,
                ) {
                    Ok(proof) => match validate_secure_v2_proof_for_proposal(&proof, &id) {
                        Ok(()) => Some(proof),
                        Err(reason) => {
                            warn!("Rejected remote proof for proposal {} — {}", id.0, reason);
                            None
                        }
                    },
                    Err(_) => {
                        match serde_json::from_slice::<icn_governance::GovernanceProof>(&bytes) {
                            Ok(legacy) => {
                                // Legacy proofs must still verify cryptographically. We reject them
                                // for storage because they cannot carry canonical V2 attestations.
                                if !legacy.verify_binding() {
                                    warn!(
                                    "Rejected legacy remote proof for proposal {} — invalid binding",
                                    id.0
                                );
                                } else {
                                    match Did::from_str(&legacy.signer_did)
                                    .and_then(|did| did.to_verifying_key())
                                {
                                    Ok(vk) if legacy.verify_signature(&vk) => warn!(
                                        "Rejected legacy remote proof for proposal {} — missing canonical attestations",
                                        id.0
                                    ),
                                    Ok(_) => warn!(
                                        "Rejected legacy remote proof for proposal {} — invalid signature",
                                        id.0
                                    ),
                                    Err(e) => warn!(
                                        "Rejected legacy remote proof for proposal {} — cannot resolve signer DID: {}",
                                        id.0, e
                                    ),
                                }
                                }
                                None
                            }
                            Err(e) => {
                                warn!(
                                    "Rejected remote proof for proposal {} — invalid JSON: {}",
                                    id.0, e
                                );
                                None
                            }
                        }
                    }
                };

                if let Some(proof) = incoming_v2 {
                    let should_store = match store.get_proof_bytes(&id)? {
                        Some(existing_bytes) => {
                            match serde_json::from_slice::<icn_governance::GovernanceProofV2>(
                                &existing_bytes,
                            ) {
                                Ok(existing)
                                    if validate_secure_v2_proof_for_proposal(&existing, &id)
                                        .is_ok() =>
                                {
                                    debug!(
                                        "Skipping remote proof for proposal {} — valid proof already exists",
                                        id.0
                                    );
                                    false
                                }
                                _ => true,
                            }
                        }
                        None => true,
                    };

                    if should_store {
                        let canonical_bytes = serde_json::to_vec(&proof)?;
                        store.save_proof_bytes(&id, &canonical_bytes)?;
                        info!(
                            "Stored validated governance proof from remote for proposal {}",
                            id.0
                        );
                    }
                }
            }
        }

        GovernanceMessage::DelegationCreated { delegation } => {
            // Security: verify the gossip sender is the delegator.
            // Any peer that can publish to the governance topic could otherwise inject
            // delegations on behalf of DIDs they don't control.
            if let Some(sender_did) = sender {
                if *sender_did != delegation.delegator {
                    warn!(
                        "DelegationCreated gossip rejected: sender {} does not match delegator {} \
                         (delegation {})",
                        sender_did, delegation.delegator, delegation.id.0
                    );
                    return Ok(());
                }
            }
            match store.get_delegation(&delegation.id)? {
                None => {
                    store.save_delegation(&delegation)?;
                    info!(
                        "Delegation synced via gossip: {} -> {} (scope: {:?})",
                        delegation.delegator, delegation.delegate, delegation.scope
                    );
                }
                Some(existing) => {
                    let existing_bytes = serde_json::to_vec(&existing)?;
                    let incoming_bytes = serde_json::to_vec(&delegation)?;
                    if existing_bytes == incoming_bytes {
                        debug!(
                            "Delegation gossip replay ignored (already stored): {} -> {} (scope: {:?})",
                            delegation.delegator, delegation.delegate, delegation.scope
                        );
                    } else {
                        warn!(
                            "Delegation gossip conflict ignored: incoming {} differs from stored — possible tampering",
                            delegation.id.0
                        );
                    }
                }
            }
        }

        GovernanceMessage::DelegationRevoked {
            id,
            revoked_at,
            revoked_by,
        } => {
            if let Some(delegation) = store.get_delegation(&id)? {
                // Security: verify the gossip sender is the original delegator.
                // The `revoked_by` field in the message must also match the stored delegator,
                // preventing a third party from revoking a delegation they don't own.
                if let Some(sender_did) = sender {
                    if *sender_did != delegation.delegator {
                        warn!(
                            "DelegationRevoked gossip rejected: sender {} does not match stored \
                             delegator {} (delegation {})",
                            sender_did, delegation.delegator, id.0
                        );
                        return Ok(());
                    }
                }
                if revoked_by != delegation.delegator {
                    warn!(
                        "DelegationRevoked gossip rejected: revoked_by {} does not match stored \
                         delegator {} (delegation {})",
                        revoked_by, delegation.delegator, id.0
                    );
                    return Ok(());
                }
                store.save_revoked_delegation(&delegation, revoked_at)?;
                info!("Delegation revocation synced via gossip: {}", id.0);
            } else {
                // May arrive before the DelegationCreated gossip — log and ignore.
                debug!(
                    "DelegationRevoked gossip for unknown delegation {} — ignoring",
                    id.0
                );
            }
        }

        _ => {
            // Ignore other message types for now (future: DomainUpdated, ProposalCancelled)
        }
    }

    Ok(())
}

// ---- Utility functions ----

fn now_seconds() -> u64 {
    icn_time::current_timestamp_secs()
}

fn validate_secure_v2_proof_for_proposal(
    proof: &icn_governance::GovernanceProofV2,
    proposal_id: &ProposalId,
) -> std::result::Result<(), String> {
    if !proof.verify_receipt() {
        return Err("invalid decision receipt".to_string());
    }
    if proof.receipt.proposal_id != proposal_id.0 {
        return Err(format!(
            "proposal_id mismatch: expected {}, got {}",
            proposal_id.0, proof.receipt.proposal_id
        ));
    }
    if proof.attestations.is_empty() {
        return Err("missing attestations".to_string());
    }

    for attestation in &proof.attestations {
        if attestation.decision_hash != proof.receipt.decision_hash {
            return Err("attestation decision_hash mismatch".to_string());
        }
        let vk = Did::from_str(&attestation.signer_did)
            .and_then(|did| did.to_verifying_key())
            .map_err(|e| format!("cannot resolve signer DID: {e}"))?;
        if !attestation.verify(&vk) {
            return Err("invalid attestation signature".to_string());
        }
    }

    Ok(())
}

// ---- Tests ----

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use icn_governance::{Delegation, DelegationScope};
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn make_store() -> SledGovernanceStateStore {
        SledGovernanceStateStore::new(Arc::new(SledStore::temporary().expect("temp store")))
    }

    fn did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// DelegationCreated gossip → delegation is persisted and readable.
    #[test]
    fn test_delegation_created_persisted() {
        let store = make_store();
        let d = Delegation::new(did(), did(), DelegationScope::Blanket);

        handle_incoming(
            &store,
            GovernanceMessage::delegation_created(d.clone()),
            None,
        )
        .unwrap();

        let loaded = store
            .get_delegation(&d.id)
            .unwrap()
            .expect("delegation not persisted");
        assert_eq!(loaded.id, d.id);
        assert_eq!(loaded.delegator, d.delegator);
        assert_eq!(loaded.delegate, d.delegate);
        assert!(loaded.revoked_at.is_none());
    }

    /// Repeated DelegationCreated with the same delegation is idempotent — no error, same result.
    #[test]
    fn test_delegation_created_idempotent() {
        let store = make_store();
        let d = Delegation::new(did(), did(), DelegationScope::Blanket);

        handle_incoming(
            &store,
            GovernanceMessage::delegation_created(d.clone()),
            None,
        )
        .unwrap();
        // Second application of the same message must not error.
        handle_incoming(
            &store,
            GovernanceMessage::delegation_created(d.clone()),
            None,
        )
        .unwrap();

        let loaded = store.get_delegation(&d.id).unwrap().unwrap();
        assert_eq!(loaded.id, d.id);
        assert!(loaded.revoked_at.is_none());
    }

    /// DelegationRevoked gossip after a DelegationCreated → revoked_at is set.
    #[test]
    fn test_delegation_revoked_sets_revoked_at() {
        let store = make_store();
        let delegator = did();
        let d = Delegation::new(delegator.clone(), did(), DelegationScope::Blanket);

        // Persist first so the revoke handler finds it.
        store.save_delegation(&d).unwrap();

        let revoke_ts: Timestamp = 999_999;
        handle_incoming(
            &store,
            GovernanceMessage::delegation_revoked(d.id.clone(), delegator, revoke_ts),
            None,
        )
        .unwrap();

        let loaded = store.get_delegation(&d.id).unwrap().unwrap();
        assert_eq!(loaded.revoked_at, Some(revoke_ts));
    }

    /// DelegationRevoked gossip for an unknown delegation is silently ignored (no error).
    ///
    /// This covers the out-of-order gossip case where the revoke arrives before the create.
    #[test]
    fn test_delegation_revoked_before_create_is_ignored() {
        let store = make_store();
        let delegator = did();
        let d = Delegation::new(delegator.clone(), did(), DelegationScope::Blanket);

        // Do NOT persist the delegation — simulate out-of-order gossip.
        let result = handle_incoming(
            &store,
            GovernanceMessage::delegation_revoked(d.id.clone(), delegator, 1234),
            None,
        );

        assert!(result.is_ok(), "out-of-order revoke must not error");
        assert!(
            store.get_delegation(&d.id).unwrap().is_none(),
            "unknown delegation must not be created by revoke"
        );
    }

    /// DelegationCreated gossip with wrong sender is rejected (security: #1340).
    #[test]
    fn test_delegation_created_wrong_sender_rejected() {
        let store = make_store();
        let delegator = did();
        let attacker = did();
        let d = Delegation::new(delegator.clone(), did(), DelegationScope::Blanket);

        // Attacker tries to inject a delegation owned by `delegator`.
        let result = handle_incoming(
            &store,
            GovernanceMessage::delegation_created(d.clone()),
            Some(&attacker),
        );

        assert!(result.is_ok(), "wrong-sender rejection must not error");
        assert!(
            store.get_delegation(&d.id).unwrap().is_none(),
            "delegation from wrong sender must not be persisted"
        );
    }

    /// DelegationCreated gossip with correct sender is accepted (security: #1340).
    #[test]
    fn test_delegation_created_correct_sender_accepted() {
        let store = make_store();
        let delegator = did();
        let d = Delegation::new(delegator.clone(), did(), DelegationScope::Blanket);

        handle_incoming(
            &store,
            GovernanceMessage::delegation_created(d.clone()),
            Some(&delegator),
        )
        .unwrap();

        assert!(
            store.get_delegation(&d.id).unwrap().is_some(),
            "delegation from correct sender must be persisted"
        );
    }

    /// DelegationRevoked gossip with wrong sender is rejected (security: #1340).
    #[test]
    fn test_delegation_revoked_wrong_sender_rejected() {
        let store = make_store();
        let delegator = did();
        let attacker = did();
        let d = Delegation::new(delegator.clone(), did(), DelegationScope::Blanket);

        store.save_delegation(&d).unwrap();

        let result = handle_incoming(
            &store,
            GovernanceMessage::delegation_revoked(d.id.clone(), delegator.clone(), 1234),
            Some(&attacker),
        );

        assert!(result.is_ok(), "wrong-sender rejection must not error");
        // revoked_at should still be None — revocation was dropped
        let loaded = store.get_delegation(&d.id).unwrap().unwrap();
        assert!(
            loaded.revoked_at.is_none(),
            "delegation revoked by wrong sender must not be revoked"
        );
    }

    /// DelegationRevoked with mismatched revoked_by field is rejected (security: #1340).
    #[test]
    fn test_delegation_revoked_mismatched_revoked_by_rejected() {
        let store = make_store();
        let delegator = did();
        let impersonator = did();
        let d = Delegation::new(delegator.clone(), did(), DelegationScope::Blanket);

        store.save_delegation(&d).unwrap();

        // Sender matches delegator but revoked_by is a different DID.
        let result = handle_incoming(
            &store,
            GovernanceMessage::delegation_revoked(d.id.clone(), impersonator.clone(), 1234),
            Some(&delegator),
        );

        assert!(result.is_ok(), "mismatched revoked_by must not error");
        let loaded = store.get_delegation(&d.id).unwrap().unwrap();
        assert!(
            loaded.revoked_at.is_none(),
            "delegation with mismatched revoked_by must not be revoked"
        );
    }
}
