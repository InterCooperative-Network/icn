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
        /// Action items to create on acceptance (decision-to-action bridge)
        action_items_on_accept: Vec<icn_governance::ActionItemSpec>,
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
        /// Optional set of member DIDs to exclude from close-time delegation expansion.
        ///
        /// When `Some`, members in this set will not have their vote weight flow via
        /// any existing delegation at close time. Specifically: if a suspended member
        /// did not vote directly, their absent weight is NOT applied via delegation,
        /// preventing indirect governance influence after a FreezeMember proposal.
        /// When `None`, all non-voter members may have delegation applied (default).
        excluded_delegators: Option<std::collections::HashSet<Did>>,
        /// Capability scope the caller actually presented at close time, when
        /// the close was driven by an authenticated request (#1868). `Some`
        /// only for the HTTP close path, which presents `governance:write`;
        /// `None` for scheduler/timer auto-close (no capability presented).
        ///
        /// This is **evidence**: it records what was actually presented, never
        /// a canonical constant. The actor emits a process-authorized
        /// `GovernanceDecisionReceiptV3` only when this is `Some`; the timer
        /// path emits no v3 because no scope was presented.
        capability_scope: Option<String>,
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

    /// Set the action item store for the decision-to-action bridge.
    ///
    /// When configured, proposals with `action_items_on_accept` specs will
    /// auto-create linked action items on acceptance. Call during initialization
    /// before cloning the handle.
    pub fn with_action_item_store(
        self,
        store: Arc<dyn icn_governance::ActionItemStoreBackend>,
    ) -> Self {
        // Set on inner actor for use during proposal close
        if let Ok(mut actor) = self.inner.try_write() {
            actor.action_item_store = Some(store);
        }
        self
    }

    /// Attach the receipt backend used to persist `InstitutionalEffectRecord`
    /// on proposal acceptance.
    ///
    /// With this wired, the actor's `CloseProposal` and `ForceCloseProposal`
    /// accept branches call the canonical emission path. Without it, those
    /// branches still fire their SystemEvent and materialize action items
    /// but emit no institutional record — leaving acceptance artifacts only
    /// to the HTTP close handler's post-actor emission (which does not run
    /// for actor-only force-close). Wire this for any deployment where
    /// force-accept must produce the same audit trail as normal accept.
    pub async fn with_receipt_store(
        self,
        store: Arc<dyn crate::receipt_backend::GovernanceReceiptBackend>,
    ) -> Self {
        self.install_receipt_store(store).await;
        self
    }

    /// Shared-ref variant of `with_receipt_store`: install the receipt backend
    /// on an already-cloned handle.
    ///
    /// The builder-style `with_receipt_store` consumes `self`, which is
    /// incompatible with production wiring where the concrete
    /// `GovernanceHandle` has already been cloned into `Arc<dyn GovernanceOps>`
    /// before the receipt_store is created (receipt_store's backing DB is only
    /// opened when the gateway server starts, after the actor is spawned).
    ///
    /// This setter mutates the shared actor state via the same
    /// `Arc<RwLock<GovernanceActor>>`, so every clone of the handle sees the
    /// newly-installed store. Closes the actor-path parity gap: without this,
    /// force-close accept and deadline auto-close emit no
    /// `InstitutionalEffectRecord` in daemon deployments (the HTTP-close path
    /// remained the sole writer).
    ///
    /// Awaits the inner write lock rather than using `try_write`: a transient
    /// lock contention window (e.g. an in-flight `submit`) during gateway
    /// startup must NOT silently drop the install and re-open the parity gap
    /// the setter exists to close. `submit` only holds the lock for the
    /// duration of a single command, so this await completes quickly.
    ///
    /// Idempotent: repeat calls replace the previously-installed store.
    pub async fn install_receipt_store(
        &self,
        store: Arc<dyn crate::receipt_backend::GovernanceReceiptBackend>,
    ) {
        let mut actor = self.inner.write().await;
        actor.receipt_store = Some(store);
    }

    /// Replay any incomplete write-ahead close-journal entries to completion.
    ///
    /// Deployment wiring MUST call this once at startup, AFTER every durable
    /// downstream sink a recovered close can feed is installed — in particular
    /// the receipt store ([`Self::install_receipt_store`]) AND the deferred
    /// dispatch-evidence sink. Recovery re-emits recovered `ProposalAccepted`
    /// events, which can drive executable effects whose `EffectDispatchEvidence`
    /// would be permanently dropped if its sink were not yet installed (the
    /// deferred sink drops batches until its backend is wired). This is
    /// deliberately DECOUPLED from `install_receipt_store` so the dispatch sink
    /// can be installed in between — see the gateway `server.rs` wiring.
    ///
    /// A no-op when there is nothing to recover.
    pub async fn recover_incomplete_closes(&self) {
        self.inner.read().await.recover_incomplete_closes().await;
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
        self.create_proposal_with_actions(domain_id, title, description, payload, scope, Vec::new())
            .await
    }

    /// Create a proposal with action item specs that will materialize on acceptance.
    async fn create_proposal_with_actions(
        &self,
        domain_id: GovernanceDomainId,
        title: String,
        description: String,
        payload: icn_governance::ProposalPayload,
        scope: ProposalScope,
        action_items_on_accept: Vec<icn_governance::ActionItemSpec>,
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
            action_items_on_accept,
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
            excluded_delegators: None,
            // GovernanceOps trait close: no authenticated request scope flows
            // through this entry point — emit no v3 (see CloseProposal docs).
            capability_scope: None,
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
            excluded_delegators: None,
            // GovernanceOps trait close: no authenticated request scope flows
            // through this entry point — emit no v3 (see CloseProposal docs).
            capability_scope: None,
        })
        .await
    }

    /// Override the trait default so a scoped HTTP close can carry the
    /// presented capability scope into the actor, enabling a process-authorized
    /// `GovernanceDecisionReceiptV3` (#1868).
    ///
    /// Behavior note: like the trait default, this does **not** wire
    /// `excluded_delegators` into the command (the actor's delegation-exclusion
    /// path is unchanged by this slice — it is submitted as `None`, exactly as
    /// the default's delegation to `close_proposal_filtered`/`close_proposal`
    /// produced). The only added behavior is threading `capability_scope`.
    async fn close_proposal_with_suspension(
        &self,
        proposal_id: ProposalId,
        eligible_voters: Option<std::collections::HashSet<Did>>,
        excluded_delegators: Option<std::collections::HashSet<Did>>,
        capability_scope: Option<String>,
    ) -> Result<()> {
        // Unchanged from the trait default: excluded_delegators is not wired
        // into the actor command in this slice. Carrying the presented scope is
        // the only added behavior.
        let _ = excluded_delegators;
        self.submit(GovernanceCommand::CloseProposal {
            proposal_id,
            eligible_voters,
            excluded_delegators: None,
            capability_scope,
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
    /// Optional action item store for materializing action items from accepted proposals.
    /// When present, proposals with `action_items_on_accept` specs will auto-create
    /// linked action items on acceptance (decision-to-action bridge).
    action_item_store: Option<Arc<dyn icn_governance::ActionItemStoreBackend>>,
    /// Optional receipt backend for emitting `InstitutionalEffectRecord` on
    /// proposal acceptance. When wired, the actor calls the canonical
    /// emission path (same as the HTTP handler) from both `CloseProposal`
    /// and `ForceCloseProposal` accept branches. Idempotence on
    /// (proposal_id, effect_kind) keeps this safe when the HTTP handler
    /// also invokes emission after the actor-backed close returns.
    receipt_store: Option<Arc<dyn crate::receipt_backend::GovernanceReceiptBackend>>,
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

        // Set up notification callback for incoming messages.
        //
        // F-P0-2 containment: this closure deliberately captures **no**
        // `GovernanceStateStore` handle. A replicated entry therefore cannot reach
        // governance state by construction, not merely by passing a check that a
        // later edit could bypass. See `refuse_replicated_governance_message`.
        let did_notify = did.clone();

        {
            let mut g = gossip.write().await;
            g.add_notification_callback(Arc::new(move |topic, entry, subscriber_did| {
                if !should_handle_governance_notification(&topic, &subscriber_did, &did_notify) {
                    return;
                }

                match GovernanceMessage::from_bytes(&entry.data) {
                    Ok(msg) => {
                        refuse_replicated_governance_message(&msg, &entry.author, &did_notify);
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
            action_item_store: None,
            receipt_store: None,
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
                                        excluded_delegators: None,
                                        // Scheduler/timer auto-close: no capability scope
                                        // was presented, so emit no process-authorized v3.
                                        capability_scope: None,
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

    /// Replay any lingering write-ahead close-journal entries to completion.
    ///
    /// Called once a receipt backend is installed — the point at which both the
    /// proposal state store (Db-B) and the receipt store (Db-A) are available.
    /// For each durable [`CloseJournalEntry`](crate::close_journal::CloseJournalEntry)
    /// left behind by a close whose terminal commit did not finish (the process
    /// crashed, or the terminal `save_proposal` failed after a receipt was
    /// already durable), re-run `commit` — every artifact write is idempotent —
    /// and clear the entry. This heals a phantom-accepted half-close: receipts
    /// already durable in the receipt store are matched by the now-committed
    /// terminal proposal state. Recovery never deletes an append-only receipt;
    /// it only completes the close that was already decided.
    async fn recover_incomplete_closes(&self) {
        let intents = match self.store.list_close_intents() {
            Ok(intents) => intents,
            Err(e) => {
                warn!("Could not scan governance close-journal for recovery: {e}");
                return;
            }
        };
        if intents.is_empty() {
            return;
        }
        info!(
            count = intents.len(),
            "Replaying incomplete governance close(s) from the write-ahead journal"
        );
        for entry in intents {
            // Best-effort on startup: failures are logged inside and the entry is
            // left for the next startup's replay, so the result is intentionally
            // ignored here (unlike the close handler's retry path, which surfaces it).
            let _ = self.complete_journaled_close(entry).await;
        }
    }

    /// Drive a single write-ahead close-journal entry to completion: re-commit
    /// its durable state (idempotent), replay the durable post-commit side
    /// effects, fsync every artifact store, and only then clear the entry. On any
    /// failure the entry is LEFT for an idempotent replay.
    ///
    /// Shared by startup recovery AND the close handler: if a close is retried
    /// before restart, the handler completes the EXISTING entry through here
    /// rather than overwriting it with a fresh (possibly different tally /
    /// decision-hash) one — so the first attempt's already-durable, append-only
    /// receipt is never orphaned.
    ///
    /// Returns `Ok(())` once the close is fully durable (proposal committed,
    /// side effects persisted, every artifact fsynced) — even if clearing the
    /// now-redundant journal marker afterward fails, since a recovery sweep
    /// clears it idempotently. Returns `Err` if completion did NOT reach
    /// durability, so the close handler's retry path can report the close as
    /// unfinished rather than falsely successful. Startup recovery ignores the
    /// result (best-effort; failures are logged and replayed next startup).
    async fn complete_journaled_close(
        &self,
        entry: crate::close_journal::CloseJournalEntry,
    ) -> std::result::Result<(), String> {
        let proposal_id = entry.proposal.id.clone();
        // 1. Re-commit the durable state (receipts + proof + proposal),
        //    idempotently. On failure, leave the entry for replay.
        if let Err(e) = entry.commit(self.receipt_store.as_deref(), self.store.as_ref()) {
            warn!(
                proposal_id = %proposal_id.0,
                error = %e,
                "Could not complete a journaled governance close; leaving the journal entry for replay"
            );
            return Err(format!("commit failed: {e}"));
        }
        // 2. Replay the durable post-commit side effects (downstream event,
        //    action items, non-execution-required institutional-effect +
        //    mandate). All idempotent.
        let outcome = match &entry.proposal.state {
            ProposalState::Accepted { .. } => DecisionOutcome::Accepted,
            ProposalState::NoQuorum { .. } => DecisionOutcome::NoQuorum,
            _ => DecisionOutcome::Rejected,
        };
        if let Err(e) = self
            .emit_close_downstream_effects(
                &entry.proposal,
                outcome,
                entry.decided_at,
                entry.governance_decision_hash.clone(),
            )
            .await
        {
            warn!(
                proposal_id = %proposal_id.0,
                error = %e,
                "Journaled close committed durable state but a durable side effect did not \
                 persist; leaving the journal entry for replay"
            );
            return Err(format!("durable side effect failed: {e}"));
        }
        // 3. Force every artifact store durable before clearing the entry.
        if let Err(e) = self.flush_close_durability() {
            warn!(
                proposal_id = %proposal_id.0,
                error = %e,
                "Journaled close completed but a durable artifact store did not fsync; \
                 leaving the journal entry for replay"
            );
            return Err(format!("durability flush failed: {e}"));
        }
        // 4. The close is now fully durable. Clearing the journal marker is
        //    best-effort: a failure here leaves the now-redundant entry for an
        //    idempotent recovery sweep, but the close itself has succeeded.
        match self.store.delete_close_intent(&proposal_id) {
            Ok(()) => info!(
                proposal_id = %proposal_id.0,
                "Completed journaled governance close from the write-ahead journal"
            ),
            Err(e) => warn!(
                proposal_id = %proposal_id.0,
                error = %e,
                "Journaled close completed but clearing its journal entry failed; \
                 it will be replayed idempotently"
            ),
        }
        Ok(())
    }

    /// Force every durable store a completed close wrote to fsync, before its
    /// write-ahead journal entry is cleared.
    ///
    /// A close persists artifacts across up to three independent sled DBs — the
    /// receipt store (v1/v3/allocation receipts), the action-item store
    /// (materialized obligations), and the proposal state store (proof bytes +
    /// terminal proposal) — each with independent flush timing. Without this
    /// barrier the state DB could flush the proposal + journal-clear while the
    /// receipt DB has not flushed, leaving a terminal proposal with a missing
    /// receipt and a broken audit chain. Returns `Err` (naming the failing store)
    /// so the caller retains the journal entry for an idempotent replay rather
    /// than clearing it over an un-fsynced artifact.
    fn flush_close_durability(&self) -> std::result::Result<(), String> {
        if let Some(ref backend) = self.receipt_store {
            backend.flush().map_err(|e| format!("receipt store: {e}"))?;
        }
        if let Some(ref store) = self.action_item_store {
            store
                .flush()
                .map_err(|e| format!("action item store: {e}"))?;
        }
        self.store
            .flush()
            .map_err(|e| format!("state store: {e}"))?;
        Ok(())
    }

    /// Run the durable, idempotent post-commit side effects of a close —
    /// downstream `SystemEvent` emission (event-driven execution), action-item
    /// materialization, and, for non-execution-required accepted proposals,
    /// institutional-effect + ADR-0014 mandate emission.
    ///
    /// Shared by the live close path and write-ahead recovery so a recovered
    /// close reproduces the same downstream obligations rather than losing them.
    /// Every effect is idempotent: action items dedup by linked proposal, and
    /// the institutional-effect / mandate seams are AlreadyEmitted/AlreadyMinted
    /// no-ops, so replaying on recovery (at-least-once) converges. The gossip
    /// broadcast is deliberately NOT run here — it is fired only on the live
    /// close path and is eventually-consistent via anti-entropy, so a recovered
    /// close does not need to re-broadcast.
    async fn emit_close_downstream_effects(
        &self,
        proposal: &Proposal,
        outcome: DecisionOutcome,
        decided_at: u64,
        governance_decision_hash: Option<String>,
    ) -> std::result::Result<(), String> {
        // Accumulates failures of the DURABLE obligations (action items,
        // institutional effect, mandate). If non-empty, the caller keeps the
        // write-ahead journal entry so a later idempotent replay can complete
        // them rather than dropping the obligation. The downstream event is a
        // fire-and-forget trigger and does not gate (re-emit is at-least-once).
        let mut failures: Vec<String> = Vec::new();

        // Downstream event — drives ledger transactions / event-driven execution.
        if let Some(ref event_bus) = self.event_bus {
            let event = match outcome {
                DecisionOutcome::Accepted => match serde_json::to_value(&proposal.payload) {
                    Ok(payload) => {
                        let canonical_payload_hash = serde_json::to_string(&payload)
                            .ok()
                            .map(|s| blake3::hash(s.as_bytes()).to_hex().to_string());
                        SystemEvent::ProposalAccepted {
                            proposal_id: proposal.id.0.clone(),
                            domain_id: proposal.domain_id.0.clone(),
                            payload,
                            decided_at,
                            canonical_payload_hash,
                            governance_decision_hash: governance_decision_hash.clone(),
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to serialize payload for accepted proposal {}: {e}",
                            proposal.id.0
                        );
                        SystemEvent::ProposalExecutionFailed {
                            proposal_id: proposal.id.0.clone(),
                            proposal_type: proposal.payload.type_name().to_string(),
                            error: format!("payload serialization failed: {e}"),
                            failed_at: decided_at,
                        }
                    }
                },
                _ => SystemEvent::ProposalRejected {
                    proposal_id: proposal.id.0.clone(),
                    domain_id: proposal.domain_id.0.clone(),
                    decided_at,
                },
            };
            event_bus.emit(event).await;
        }

        // Decision-to-Action bridge — materialize durable action items
        // (idempotent: `materialize_action_items` dedups by linked proposal).
        if matches!(outcome, DecisionOutcome::Accepted)
            && !proposal.action_items_on_accept.is_empty()
        {
            if let Some(ref store) = self.action_item_store {
                if let Err(e) =
                    Self::materialize_action_items(store, proposal, &proposal.id, decided_at)
                {
                    failures.push(e);
                }
            }
        }

        // Non-execution-required accepted proposals emit their
        // InstitutionalEffectRecord + ADR-0014 mandate here; execution-required
        // ones already did so as a preflight before the journal. Idempotent on
        // (proposal_id, effect_kind) / mandate id.
        let requires_execution_closure = matches!(outcome, DecisionOutcome::Accepted)
            && proposal.payload.requires_execution_closure();
        if matches!(outcome, DecisionOutcome::Accepted) && !requires_execution_closure {
            if let Some(ref store) = self.receipt_store {
                let decision_hash: Option<icn_kernel_api::receipts::Hash> =
                    governance_decision_hash
                        .as_deref()
                        .and_then(|h| hex::decode(h).ok())
                        .and_then(|v| <[u8; 32]>::try_from(v.as_slice()).ok());
                if let Err(e) = crate::institutional_effect::emit_accepted_effect(
                    store.as_ref(),
                    &proposal.id.0,
                    &proposal.domain_id.0,
                    decision_hash,
                    &proposal.payload,
                    decided_at,
                ) {
                    error!(
                        proposal_id = %proposal.id.0,
                        error = %e,
                        "Actor failed to emit InstitutionalEffectRecord"
                    );
                    failures.push(format!("institutional effect: {e}"));
                }
                if let Some(decision_hash) = decision_hash {
                    if let Err(e) = crate::grant_minting::mint_and_persist_for_accepted(
                        store.as_ref(),
                        &proposal.id.0,
                        &proposal.domain_id,
                        decision_hash,
                        &proposal.payload,
                        decided_at,
                    ) {
                        error!(
                            proposal_id = %proposal.id.0,
                            error = %e,
                            "Actor failed to persist ADR-0014 mandate"
                        );
                        failures.push(format!("mandate: {e}"));
                    }
                }
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
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
                action_items_on_accept,
            } => {
                info!("Creating proposal: {} (scope: {:?})", title, scope);

                let mut proposal = Proposal::new(
                    domain_id,
                    self.did.clone(),
                    title.clone(),
                    description,
                    payload,
                )
                .with_scope(scope)
                .with_action_items(action_items_on_accept);

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
                excluded_delegators,
                capability_scope,
            } => {
                info!("Closing proposal: {}", proposal_id.0);

                // Notify scheduler to cancel any pending events (if scheduled)
                let _ = self.cancel_tx.send(proposal_id.clone());

                // If a prior close attempt for this proposal already wrote a
                // durable write-ahead journal entry (it committed a receipt but
                // did not finish), COMPLETE that entry instead of starting a fresh
                // close. Re-running would recompute the tally / decision hash from
                // whatever votes/eligibility now exist and `save_close_intent`
                // would overwrite the only replay record — orphaning the first
                // attempt's already-durable, append-only receipt so recovery could
                // never reconcile it. Completing the existing entry is idempotent.
                if let Some(existing) = self.store.get_close_intent(&proposal_id)? {
                    info!(
                        proposal_id = %proposal_id.0,
                        "In-flight close-journal entry found; completing it instead of re-running the close"
                    );
                    // Propagate whether completion actually reached durability:
                    // a transient store failure here must surface as an error, not
                    // a falsely-successful close (the proposal may still be Open
                    // with the journal left for replay).
                    return self.complete_journaled_close(existing).await.map_err(|e| {
                        anyhow::anyhow!(
                            "Close retry for proposal '{}' could not complete the in-flight \
                             journal entry: {e}",
                            proposal_id.0
                        )
                    });
                }

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
                    excluded_delegators.as_ref(),
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

                // Evaluate outcome with proposal-type-specific thresholds,
                // respecting the domain's decision mode (majority or consent).
                let decision_mode = domain.config.params.decision_mode;
                let outcome_result = self.profile.evaluate_with_mode(
                    &tally,
                    thresholds,
                    eligible_count,
                    decision_mode,
                )?;

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

                let requires_execution_closure =
                    matches!(outcome_result, DecisionOutcome::Accepted)
                        && proposal.payload.requires_execution_closure();
                if requires_execution_closure && self.receipt_store.is_none() {
                    anyhow::bail!(
                        "Proposal '{}' requires execution closure but no receipt_store is installed on actor.",
                        proposal_id.0
                    );
                }

                // Invariant 7 gate + closure preflight run BEFORE terminal proposal
                // persistence. Any failure here must leave the proposal in its
                // prior state — the only way to keep `save_proposal` and the
                // caller's result in agreement is to bail before `close()`.
                //
                // The gate receipt here is canonical: its decision_hash keys the
                // institutional-effect record, the ADR-0014 mandate, and the
                // ProposalAccepted event. Construct it once and reuse.
                //
                // Deferred v1 coordination receipt-chain writes. Captured during the
                // execution-closure preflight but PERFORMED only after the v3 receipt
                // and GovernanceProofV2 preflights succeed (mirrors manager.rs's
                // deliberate v3-before-v1 ordering). Writing the v1 receipts here —
                // before those preflights — would let `GET /v1/receipts/chain` observe
                // an accepted governance/allocation receipt for a proposal whose close
                // later fails and stays Open. The chain audit reads exactly these v1
                // artifacts, so they must be the last preflight writes before the
                // terminal save.
                let mut pending_chain_receipts: Option<(
                    GovernanceDecisionReceipt,
                    Option<icn_kernel_api::AllocationReceipt>,
                )> = None;
                let governance_decision_hash: Option<String> = if matches!(
                    outcome_result,
                    DecisionOutcome::Accepted
                ) {
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
                            "Invariant 7 gate rejected accepted proposal — blocking terminal persistence"
                        );
                        bail!(
                            "Invariant 7 gate failure for proposal {}: {gate_err}",
                            proposal_id.0
                        );
                    }
                    let decision_hash = gate_receipt.decision_hash;
                    info!(
                        proposal_id = %proposal_id.0,
                        decision_hash = %hex::encode(decision_hash),
                        "Invariant 7 gate passed"
                    );

                    // Execution-required preflight: prove closure artifacts are
                    // persistable before committing terminal proposal state.
                    if requires_execution_closure {
                        if let Some(ref store) = self.receipt_store {
                            crate::institutional_effect::emit_accepted_effect(
                                store.as_ref(),
                                &proposal_id.0,
                                &proposal.domain_id.0,
                                Some(decision_hash),
                                &proposal.payload,
                                now,
                            )
                            .map_err(|e| {
                                anyhow::anyhow!(
                                    "Proposal '{}' requires execution closure but actor effect emission preflight failed: {e}",
                                    proposal_id.0
                                )
                            })?;

                            match crate::grant_minting::mint_and_persist_for_accepted(
                                store.as_ref(),
                                &proposal_id.0,
                                &proposal.domain_id,
                                decision_hash,
                                &proposal.payload,
                                now,
                            ) {
                                Ok(crate::grant_minting::MandateMintOutcome::Minted { .. })
                                | Ok(crate::grant_minting::MandateMintOutcome::AlreadyMinted {
                                    ..
                                }) => {}
                                Ok(crate::grant_minting::MandateMintOutcome::HashFailed) => {
                                    anyhow::bail!(
                                        "Proposal '{}' requires execution closure but actor mandate preflight hash failed.",
                                        proposal_id.0
                                    );
                                }
                                Err(e) => {
                                    anyhow::bail!(
                                        "Proposal '{}' requires execution closure but actor mandate preflight persistence failed: {e}",
                                        proposal_id.0
                                    );
                                }
                            }

                            // Gap C parity: capture the v1 GovernanceDecisionReceipt
                            // + allocation/contribution receipt that the in-process
                            // GovernanceManager close path persists
                            // (`manager.rs::close_proposal_inner`). The actor already
                            // wrote the institutional-effect record and the mandate
                            // above; these v1 artifacts are what the audit read path
                            // indexes by `decision_hash`
                            // (`GET /v1/receipts/chain/{decision_hash}` / `icnctl
                            // audit verify`). They are DEFERRED to after the v3 +
                            // proof preflights (drained just before the terminal save)
                            // so a later preflight failure can never leave a
                            // chain-observable accepted receipt behind for a still-Open
                            // proposal. `gate_receipt` is the canonical v1 receipt
                            // built above for the Invariant-7 gate (same
                            // `decision_hash`) and is unused afterwards, so it moves
                            // into the deferred holder.
                            let allocation_receipt =
                                crate::manager::GovernanceManager::create_allocation_receipt(
                                    &proposal.payload,
                                    decision_hash,
                                    &proposal_id,
                                    &proposal.domain_id,
                                );
                            pending_chain_receipts = Some((gate_receipt, allocation_receipt));
                        }
                    }

                    Some(hex::encode(decision_hash))
                } else {
                    None
                };

                // #1868: persist a process-authorized GovernanceDecisionReceiptV3
                // as a PREFLIGHT — before the GovernanceProofV2 below and before
                // the terminal `proposal.close`/`save_proposal` — but ONLY when the
                // close was driven by an authenticated request that actually
                // presented a capability scope. A normal democratic close is
                // authorized by the governance process itself (eligible voters,
                // period, quorum/threshold, tally) — not membership standing, not
                // a personal grant — so the attestation is `ProcessAuthorized`.
                //
                // `capability_scope` is `None` for scheduler/timer auto-close
                // (nothing was presented), so no v3 is emitted there: the field is
                // evidence and must record what was actually presented, never a
                // constant. Forced-accept (Bootstrap) is a separate path and is
                // intentionally not emitted in this slice.
                //
                // Ordering matters: this runs BEFORE proof persistence (and the
                // terminal close) so a v3 persistence failure fails closed without
                // leaving a durable closed-outcome proof — or any other decision
                // artifact — behind for a still-Open proposal. Routes through the
                // fail-closed `put_governance_decision_v3` → `put_opaque` seam; the
                // gateway never imports the v3 type.
                // Build the v3 receipt here — its `::new` validation still fails
                // closed BEFORE the close-journal intent is written — but DEFER its
                // durable write into the write-ahead `CloseJournalEntry` commit
                // below. Bundling the v3, proof, v1, and allocation writes with the
                // terminal proposal save behind the journal is what makes the close
                // cross-store-atomic and recoverable (crate::close_journal).
                let pending_v3: Option<crate::close_journal::DecisionV3Entry> =
                    if let Some(scope) = capability_scope.as_deref() {
                        if self.receipt_store.is_some() {
                            use icn_governance::proof::ProofOutcome;
                            let v3_outcome = match outcome_result {
                                DecisionOutcome::Accepted => ProofOutcome::Accepted,
                                DecisionOutcome::Rejected => ProofOutcome::Rejected,
                                DecisionOutcome::NoQuorum => ProofOutcome::NoQuorum,
                            };
                            let receipt_v3 = icn_governance::GovernanceDecisionReceiptV3::new(
                                proposal_id.0.clone(),
                                proposal.domain_id.0.clone(),
                                v3_outcome,
                                tally.clone(),
                                &votes,
                                scope.to_string(),
                                icn_governance::ReceiptMandateAttestation::ProcessAuthorized,
                            )
                            .map_err(|e| {
                                anyhow::anyhow!(
                                    "Invalid v3 decision receipt for proposal {}: {e}",
                                    proposal_id.0
                                )
                            })?;
                            Some(crate::close_journal::DecisionV3Entry {
                                receipt: receipt_v3,
                                recorded_at: now,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                // Build + sign GovernanceProofV2 after the v3 build and BEFORE the
                // terminal proposal save. Its durable write is DEFERRED into the
                // close-journal commit below (same as v3), so a proof-write failure
                // can never leave a durable closed-outcome proof behind for a
                // still-Open proposal — the journal replays it on recovery instead.
                // Non-Accepted outcomes still emit a proof (useful for audit of
                // rejection / no-quorum signals) but they bypass the Invariant 7
                // gate above.
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

                // Flip the proposal to its terminal state in memory. A bail here
                // (not Open / non-terminal state) leaves nothing durable behind.
                proposal.close(new_state)?;

                // Cross-store-atomic close via the write-ahead close journal
                // (Codex P2 on PR #1985). The v3 receipt, GovernanceProofV2 bytes,
                // and the v1 governance/allocation receipts (`pending_chain_receipts`)
                // were all BUILT above but not yet written durably. We now:
                //
                //   1. assemble a self-contained `CloseJournalEntry`;
                //   2. persist it FIRST (`save_close_intent`) — Db-B;
                //   3. replay every artifact idempotently then the terminal
                //      proposal state (`commit`) — Db-A receipts (v3→v1→allocation)
                //      then Db-B proof then the terminal proposal save. Every write
                //      is idempotent, so the exact ordering is not load-bearing:
                //      recovery covers any partial-failure interleaving.
                //   4. delete the journal entry only once everything is durable.
                //
                // The receipt store (Db-A) and proposal store (Db-B) are separate
                // sled `Db`s, so a single transaction cannot span them. If the
                // terminal save (or any artifact write) fails after a receipt is
                // already durable, the journal entry survives and
                // `recover_incomplete_closes` replays it to completion on the next
                // startup — converting a permanent phantom-accepted half-close into
                // an eventually-consistent, self-healing one. No append-only
                // audit/provenance receipt is ever rolled back, and the two stores
                // are never merged. The v1 fatal / allocation best-effort split and
                // the manager-parity v3-before-v1 ordering are preserved inside
                // `CloseReceipts::apply` (crate::close_journal).
                let (governance_receipt, allocation_receipt) = match pending_chain_receipts {
                    Some((gov_receipt, allocation_receipt)) => {
                        (Some(gov_receipt), allocation_receipt)
                    }
                    None => (None, None),
                };
                let journal_entry = crate::close_journal::CloseJournalEntry {
                    proposal: proposal.clone(),
                    proof_bytes: proof_bytes.clone(),
                    receipts: crate::close_journal::CloseReceipts {
                        decision_v3: pending_v3,
                        governance_receipt,
                        allocation_receipt,
                    },
                    decided_at: now,
                    governance_decision_hash: governance_decision_hash.clone(),
                };
                self.store.save_close_intent(&journal_entry)?;
                journal_entry.commit(self.receipt_store.as_deref(), self.store.as_ref())?;
                // The journal entry is NOT cleared here. It is cleared at the very
                // end of this arm, after the durable post-commit side effects have
                // run, so a crash or error anywhere below leaves the entry for
                // `recover_incomplete_closes` to replay the WHOLE completion
                // (durable state + side effects) idempotently.

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

                // Replay the durable, idempotent post-commit side effects through
                // the shared helper so write-ahead recovery reproduces them too:
                // downstream event emission (event-driven execution), action-item
                // materialization, and — for non-execution-required accepted
                // proposals — institutional-effect + ADR-0014 mandate emission.
                // (The gossip broadcast above is live-path only; it is
                // eventually-consistent via anti-entropy, so a recovered close does
                // not need to re-broadcast.)
                let side_effects = self
                    .emit_close_downstream_effects(
                        &proposal,
                        outcome_result.clone(),
                        now,
                        governance_decision_hash.clone(),
                    )
                    .await;

                // Clear the write-ahead journal entry LAST — and ONLY once every
                // durable obligation has persisted. If a durable side effect
                // failed, keep the entry so `recover_incomplete_closes` replays
                // the whole completion idempotently on the next startup rather
                // than dropping the obligation. A crash before this point has the
                // same effect: the entry survives for replay.
                match side_effects {
                    Ok(()) => match self.flush_close_durability() {
                        // Every artifact store is now fsynced; safe to clear the
                        // journal entry.
                        Ok(()) => {
                            if let Err(e) = self.store.delete_close_intent(&proposal_id) {
                                warn!(
                                    proposal_id = %proposal_id.0,
                                    error = %e,
                                    "Close fully completed but clearing its write-ahead journal \
                                     entry failed; it will be replayed idempotently on the next startup"
                                );
                            }
                        }
                        Err(e) => {
                            warn!(
                                proposal_id = %proposal_id.0,
                                error = %e,
                                "Close committed but a durable artifact store did not fsync; \
                                 leaving the write-ahead journal entry so recovery can replay it \
                                 once durability succeeds"
                            );
                        }
                    },
                    Err(e) => {
                        warn!(
                            proposal_id = %proposal_id.0,
                            error = %e,
                            "Close committed durable state but a durable side effect did not \
                             persist; leaving the write-ahead journal entry for idempotent \
                             replay on the next startup"
                        );
                    }
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

                let requires_execution_closure = matches!(&forced_outcome, ForcedOutcome::Accept)
                    && proposal.payload.requires_execution_closure();
                if requires_execution_closure && self.receipt_store.is_none() {
                    anyhow::bail!(
                        "Proposal '{}' requires execution closure but no receipt_store is installed on actor.",
                        proposal_id.0
                    );
                }

                // Invariant 7 gate + closure preflight for forced accepts.
                //
                // Force-close is an administrative override of vote thresholds,
                // not of governance-receipt correctness. The gate still applies:
                // a forced Accept must produce an internally consistent
                // GovernanceDecisionReceipt before terminal proposal state is
                // persisted. Forced accepts use VoteTally::empty() and no votes
                // — the receipt still verifies because the stored hash is
                // derived from those exact inputs.
                let force_decision_hash: Option<icn_kernel_api::receipts::Hash> = if matches!(
                    &forced_outcome,
                    ForcedOutcome::Accept
                ) {
                    use icn_governance::proof::ProofOutcome;
                    let forced_receipt = GovernanceDecisionReceipt::new(
                        proposal_id.0.clone(),
                        proposal.domain_id.0.clone(),
                        ProofOutcome::Accepted,
                        VoteTally::empty(),
                        &[],
                    );
                    if let Err(gate_err) = check_execution_gate(&forced_receipt) {
                        error!(
                            proposal_id = %proposal_id.0,
                            error = %gate_err,
                            "Invariant 7 gate rejected forced-accept — blocking terminal persistence"
                        );
                        bail!(
                            "Invariant 7 gate failure for forced-accept of proposal {}: {gate_err}",
                            proposal_id.0
                        );
                    }
                    let decision_hash = forced_receipt.decision_hash;
                    info!(
                        proposal_id = %proposal_id.0,
                        decision_hash = %hex::encode(decision_hash),
                        "Invariant 7 gate passed (forced accept)"
                    );

                    // Execution-required preflight: prove closure artifacts
                    // are persistable before committing terminal state.
                    if requires_execution_closure {
                        if let Some(ref store) = self.receipt_store {
                            let now = now_seconds();
                            crate::institutional_effect::emit_accepted_effect(
                                    store.as_ref(),
                                    &proposal_id.0,
                                    &proposal.domain_id.0,
                                    Some(decision_hash),
                                    &proposal.payload,
                                    now,
                                )
                                .map_err(|e| {
                                    anyhow::anyhow!(
                                        "Proposal '{}' requires execution closure but force-accept effect preflight failed: {e}",
                                        proposal_id.0
                                    )
                                })?;

                            match crate::grant_minting::mint_and_persist_for_accepted(
                                store.as_ref(),
                                &proposal_id.0,
                                &proposal.domain_id,
                                decision_hash,
                                &proposal.payload,
                                now,
                            ) {
                                Ok(crate::grant_minting::MandateMintOutcome::Minted { .. })
                                | Ok(crate::grant_minting::MandateMintOutcome::AlreadyMinted {
                                    ..
                                }) => {}
                                Ok(crate::grant_minting::MandateMintOutcome::HashFailed) => {
                                    anyhow::bail!(
                                            "Proposal '{}' requires execution closure but force-accept mandate preflight hash failed.",
                                            proposal_id.0
                                        );
                                }
                                Err(e) => {
                                    anyhow::bail!(
                                            "Proposal '{}' requires execution closure but force-accept mandate preflight persistence failed: {e}",
                                            proposal_id.0
                                        );
                                }
                            }
                        }
                    }

                    Some(decision_hash)
                } else {
                    None
                };

                // Force close the proposal
                proposal.force_close(proposal_outcome.clone(), reason.clone())?;

                // Persist updated state
                self.store.save_proposal(&proposal)?;

                // Emit appropriate event
                if let Some(ref event_bus) = self.event_bus {
                    let now = now_seconds();
                    let event = match &forced_outcome {
                        ForcedOutcome::Accept => match serde_json::to_value(&proposal.payload) {
                            Ok(payload) => {
                                let canonical_payload_hash = serde_json::to_string(&payload)
                                    .ok()
                                    .map(|s| blake3::hash(s.as_bytes()).to_hex().to_string());
                                // Reuse the gate-verified decision hash computed in the
                                // preflight block above. VoteTally{0,0,0} accurately
                                // represents "forced by authority — no votes were cast."
                                let forced_decision_hash = force_decision_hash.map(hex::encode);
                                SystemEvent::ProposalAccepted {
                                    proposal_id: proposal_id.0.clone(),
                                    domain_id: proposal.domain_id.0.clone(),
                                    payload,
                                    decided_at: now,
                                    canonical_payload_hash,
                                    governance_decision_hash: forced_decision_hash,
                                }
                            }
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

                // Decision-to-Action bridge for force-accepted proposals
                if matches!(forced_outcome, ForcedOutcome::Accept)
                    && !proposal.action_items_on_accept.is_empty()
                {
                    if let Some(ref store) = self.action_item_store {
                        let now = now_seconds();
                        // Force-close is outside the write-ahead-journal close path,
                        // so this stays best-effort: log materialization failures
                        // rather than gating (there is no journal entry to retain).
                        if let Err(e) =
                            Self::materialize_action_items(store, &proposal, &proposal_id, now)
                        {
                            warn!(
                                proposal_id = %proposal_id.0,
                                error = %e,
                                "Force-close failed to materialize one or more action items"
                            );
                        }
                    }
                }

                // Canonical institutional-effect emission for force-accepted
                // non-execution-required proposals. Execution-required
                // payloads already emitted + minted in the preflight block
                // above (this path's sole writer for them is the preflight);
                // repeating the idempotent helpers here would be wasted work.
                // For declarative / non-execution-required force-accepts we
                // still need a writer, and this block is it — keeping audit
                // semantics path-agnostic.
                if matches!(&forced_outcome, ForcedOutcome::Accept) && !requires_execution_closure {
                    // Reuse the gate-verified decision hash computed in the
                    // preflight block above so we don't rebuild the receipt
                    // twice per command. `force_decision_hash` is `Some`
                    // whenever `forced_outcome == Accept`; if that invariant
                    // ever breaks the inner block degrades to a no-op rather
                    // than panicking.
                    if let (Some(ref store), Some(forced_decision_hash)) =
                        (self.receipt_store.as_ref(), force_decision_hash)
                    {
                        let now = now_seconds();
                        match crate::institutional_effect::emit_accepted_effect(
                            store.as_ref(),
                            &proposal_id.0,
                            &proposal.domain_id.0,
                            Some(forced_decision_hash),
                            &proposal.payload,
                            now,
                        ) {
                            Ok(
                                crate::institutional_effect::AcceptanceEmissionOutcome::Emitted {
                                    ..
                                },
                            ) => {
                                debug!(
                                    proposal_id = %proposal_id.0,
                                    "Force-accept emitted InstitutionalEffectRecord"
                                );
                            }
                            Ok(_) => {}
                            Err(e) => {
                                error!(
                                    proposal_id = %proposal_id.0,
                                    error = %e,
                                    "Force-accept failed to emit InstitutionalEffectRecord (non-fatal)"
                                );
                            }
                        }

                        match crate::grant_minting::mint_and_persist_for_accepted(
                            store.as_ref(),
                            &proposal_id.0,
                            &proposal.domain_id,
                            forced_decision_hash,
                            &proposal.payload,
                            now,
                        ) {
                            Ok(crate::grant_minting::MandateMintOutcome::Minted { .. })
                            | Ok(crate::grant_minting::MandateMintOutcome::AlreadyMinted {
                                ..
                            }) => {}
                            Ok(crate::grant_minting::MandateMintOutcome::HashFailed) => {
                                tracing::error!(
                                    proposal_id = %proposal_id.0,
                                    "Force-accept failed to hash payload for mandate — declining to mint"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    proposal_id = %proposal_id.0,
                                    error = %e,
                                    "Force-accept failed to persist ADR-0014 mandate (non-fatal)"
                                );
                            }
                        }
                    }
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

    /// Materialize action items from an accepted proposal's specs.
    ///
    /// Idempotent: checks for existing linked items before creating new ones.
    /// Deterministic, position-stable action-item id for the `index`-th
    /// `action_items_on_accept` spec of a proposal.
    ///
    /// Materialization otherwise assigns a random id, which makes replay
    /// non-idempotent: re-running would duplicate items, so the prior dedup
    /// skipped *all* creation as soon as *any* linked item existed — which
    /// silently dropped the rest of a partially-materialized set on replay.
    /// A deterministic id keyed by `(proposal_id, index)` lets recovery
    /// reconcile each spec individually: an already-present item is detected
    /// and left untouched, and a missing one is created, without duplicating.
    fn deterministic_action_item_id(
        proposal_id: &ProposalId,
        index: usize,
    ) -> icn_governance::ActionItemId {
        let seed = format!("gov:action-item:{}:{index}", proposal_id.0);
        let digest = blake3::hash(seed.as_bytes());
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        icn_governance::ActionItemId::from_uuid(uuid::Uuid::from_bytes(bytes))
    }

    fn materialize_action_items(
        store: &Arc<dyn icn_governance::ActionItemStoreBackend>,
        proposal: &Proposal,
        proposal_id: &ProposalId,
        now: u64,
    ) -> std::result::Result<(), String> {
        let specs = &proposal.action_items_on_accept;
        let mut created = 0usize;
        let mut failures: Vec<String> = Vec::new();
        // Reconcile per spec/item (NOT "skip all if any exist"): each spec has a
        // deterministic, position-stable id, so a close and any later recovery
        // replay create exactly the missing items and never duplicate or clobber
        // already-materialized ones. This completes a partially-materialized set
        // on replay instead of dropping the remaining obligations.
        for (index, spec) in specs.iter().enumerate() {
            let mut item = spec.materialize(
                proposal.domain_id.clone(),
                proposal_id.clone(),
                proposal.proposer.clone(),
                now,
            );
            item.id = Self::deterministic_action_item_id(proposal_id, index);
            match store.get(&proposal.domain_id, &item.id) {
                // Already materialized — preserve its current state (e.g. an
                // in-progress/completed status), do not re-create.
                Ok(Some(_)) => continue,
                Ok(None) => match store.save(&item) {
                    Ok(()) => created += 1,
                    Err(e) => {
                        warn!(
                            "Failed to create action item '{}' from proposal {}: {e}",
                            spec.title, proposal_id.0
                        );
                        failures.push(format!("{}: {e}", spec.title));
                    }
                },
                Err(e) => {
                    // Cannot confirm whether this obligation already exists, so do
                    // not risk duplicating or clobbering it. Treat as a failure so
                    // the write-ahead journal entry is retained for a later
                    // idempotent replay.
                    warn!(
                        "Failed to check existing action item '{}' for proposal {}: {e}",
                        spec.title, proposal_id.0
                    );
                    failures.push(format!("{} (existence check failed): {e}", spec.title));
                }
            }
        }
        if created > 0 {
            info!(
                "📋 Created {created}/{} action items from accepted proposal {}",
                specs.len(),
                proposal_id.0
            );
        }
        if failures.is_empty() {
            Ok(())
        } else {
            // A durable obligation did not persist — surface it so the caller can
            // keep the write-ahead journal entry for an idempotent replay rather
            // than dropping the obligation.
            Err(format!(
                "{} action item(s) failed to persist for proposal {}: {}",
                failures.len(),
                proposal_id.0,
                failures.join("; ")
            ))
        }
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
        excluded_delegators: Option<&std::collections::HashSet<Did>>,
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

            // Suspension exclusion: a suspended member's weight must not flow
            // via delegation. Their absence contributes to quorum pressure instead.
            if let Some(excluded) = excluded_delegators {
                if excluded.contains(member) {
                    continue;
                }
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

// ---- Governance replication ingress (F-P0-2 containment) ----

/// What a replicated [`GovernanceMessage`] would have written to durable
/// governance state before the F-P0-2 containment.
///
/// This distinction is **diagnostic only, never an authorization decision**. No
/// replicated message is applied, whatever its variant — see
/// [`refuse_replicated_governance_message`]. The classification decides only
/// whether a refusal is worth reporting (a would-be mutation was stopped) or is
/// indistinguishable from the previous behavior (the variant already wrote
/// nothing). Keeping the security property unconditional means a misclassification
/// here cannot create a bypass.
///
/// The match in [`replicated_state_effect`] is exhaustive on purpose: a new
/// `GovernanceMessage` variant must not silently become an unguarded mutation, so
/// adding one fails the build until it is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicatedStateEffect {
    /// Applying this variant wrote durable governance state.
    WouldMutateGovernanceState,
    /// Applying this variant wrote nothing.
    NoStateEffect,
}

/// Fixed diagnostic emitted when a replicated governance mutation is refused.
///
/// Deliberately a constant, so the message body carries no attacker-supplied
/// content. The author is attached as a separate structured field and is always
/// described as *claimed*, never authenticated.
pub const REPLICATION_QUARANTINE_REASON: &str =
    "refused: unauthenticated governance replication — the entry's claimed author is not \
     verified and carries no authority over the affected governance domain";

/// Whether this gossip notification is the one this node should handle.
///
/// **This decides dispatch, never authority.** Neither condition authenticates anything
/// or authorizes any state change: the refusal in
/// [`refuse_replicated_governance_message`] is unconditional for every message that gets
/// past this point. A subscriber DID is as attacker-influenced as any other gossip field
/// and must never be read as governance authority.
///
/// 1. **Deduplication.** `GossipActor::store_entry` invokes every notification callback
///    inside its per-subscriber loop, and a peer can add arbitrary DIDs to that list with
///    an unauthenticated `Subscribe` (up to `MAX_SUBSCRIBERS_PER_TOPIC`). Without this,
///    one received entry would be handled once per subscriber, letting a remote peer
///    inflate the quarantine counter and the warning volume by orders of magnitude.
///    Subscriber DIDs are deduplicated on insert, so matching the local DID yields
///    exactly one handling per entry.
/// 2. **Topic.** The entry must be on the governance topic, or on
///    `federation:governance` / `federation:governance:<federation-id>`.
///
/// Extracted as a pure function so both conditions are directly testable; inline in the
/// closure they could only be exercised indirectly.
fn should_handle_governance_notification(
    topic: &str,
    subscriber_did: &Did,
    local_did: &Did,
) -> bool {
    // Subscriber first. This is the cheapest check and it discards the amplified
    // per-subscriber invocations before any topic comparison or allocation.
    if subscriber_did != local_did {
        return false;
    }

    let federation_root = icn_federation::TOPIC_FEDERATION_GOVERNANCE;
    topic == GOVERNANCE_TOPIC
        || topic == federation_root
        // `federation:governance:<federation-id>` only. Matching on the bare prefix
        // would also admit look-alikes such as `federation:governanceX`.
        || topic
            .strip_prefix(federation_root)
            .is_some_and(|rest| rest.starts_with(':'))
}

/// Classify what a replicated governance message would have mutated.
///
/// See [`ReplicatedStateEffect`] — this is observability, not authorization.
pub fn replicated_state_effect(msg: &GovernanceMessage) -> ReplicatedStateEffect {
    use ReplicatedStateEffect::{NoStateEffect, WouldMutateGovernanceState};

    match msg {
        // These arms wrote durable governance state through the pre-containment
        // ingress: domains, proposals, votes, delegations and close outcomes.
        GovernanceMessage::DomainCreated { .. }
        | GovernanceMessage::DomainUpdated { .. }
        | GovernanceMessage::ProposalCreated { .. }
        | GovernanceMessage::ProposalOpened { .. }
        | GovernanceMessage::VoteCast { .. }
        | GovernanceMessage::ProposalClosed { .. }
        | GovernanceMessage::DelegationCreated { .. }
        | GovernanceMessage::DelegationRevoked { .. } => WouldMutateGovernanceState,

        // These arms reached the ingress but wrote nothing.
        GovernanceMessage::ProposalCancelled { .. }
        | GovernanceMessage::DeliberationStarted { .. }
        | GovernanceMessage::DeliberationEnded { .. }
        | GovernanceMessage::CommentCreated { .. }
        | GovernanceMessage::CommentEdited { .. }
        | GovernanceMessage::CommentDeleted { .. }
        | GovernanceMessage::ReactionAdded { .. }
        | GovernanceMessage::ReactionRemoved { .. } => NoStateEffect,
    }
}

/// Governance replication ingress — refuses to apply remote governance state.
///
/// ## Why transport acceptance and state-application authority are separate
///
/// Permission to transport or replicate bytes is not permission to apply those
/// bytes as governance state.
///
/// A [`icn_gossip::GossipEntry`] carries a claimed `author` DID and **no
/// signature binding that DID to the entry contents**. The receive path
/// (`GossipActor::store_entry`) neither recomputes the entry hash — it dedups on
/// the sender-supplied `entry.hash` — nor enforces the topic ACL, and the
/// transport-level policy gate above it evaluates a self-declared sender DID with
/// no threshold attached. Every value that reaches this function is therefore
/// attacker-chosen *input*, not authenticated authority. Comparing anything
/// against `entry.author` is not an authorization check: a peer that wants to be
/// treated as some DID simply writes that DID into the field.
///
/// So this ingress applies nothing. Until authenticated governance replication
/// exists (issue #2469), a message that arrives here is *observed*, not *obeyed*.
/// Entries are
/// still accepted, stored, clock-merged and gossiped by the generic gossip layer;
/// only the application of remote governance state is suspended.
///
/// ## Why this does not break local governance
///
/// `GossipActor::store_entry` fires notification callbacks identically for
/// locally published entries and for entries received from the network, and no
/// field distinguishes them — `publish()` sets `author` to the local DID, but a
/// remote peer may claim that same DID. There is no trustworthy local/remote
/// discriminator at this layer, so the containment does not try to invent one.
///
/// It does not need one: every local `GovernanceCommand` persists through
/// `GovernanceActor`'s command path *before* it publishes (the delegation arms say
/// so outright — "failure doesn't roll back the local write"). The loopback copy
/// this ingress also receives is a redundant re-application of state the node
/// already wrote, so refusing it costs local governance nothing.
///
/// Returns the classification so callers and tests can observe the disposition.
fn refuse_replicated_governance_message(
    msg: &GovernanceMessage,
    claimed_author: &Did,
    local_did: &Did,
) -> ReplicatedStateEffect {
    let effect = replicated_state_effect(msg);

    if effect == ReplicatedStateEffect::WouldMutateGovernanceState {
        // `message_type()` is a `&'static str`, so it is safe both as a log field
        // and as a metric label — an attacker cannot inflate label cardinality.
        icn_obs::metrics::governance::replication_quarantined_inc(msg.message_type());
        warn!(
            local_did = %local_did,
            claimed_author = %claimed_author,
            message_type = msg.message_type(),
            "{REPLICATION_QUARANTINE_REASON}"
        );
    } else {
        debug!(
            message_type = msg.message_type(),
            "Replicated governance message observed; this variant applies no state"
        );
    }

    effect
}

// ---- Utility functions ----

fn now_seconds() -> u64 {
    icn_time::current_timestamp_secs()
}

/// Validate a stored `GovernanceProofV2` before it is served to a reader.
///
/// This is a **read-path** check on locally stored proofs (see
/// `GovernanceHandle::get_proof`). It verifies the receipt and each attestation
/// signature, but it does not establish that any signer held authority over the
/// proposal, so it is not an authentication of governance authority and is
/// deliberately not used to admit replicated state.
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
    use icn_governance::{CommentId, Delegation, DelegationScope, GovernanceProfileId, Proposal};
    use icn_identity::KeyPair;

    fn did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn sample_domain() -> icn_governance::GovernanceDomain {
        icn_governance::GovernanceDomain::new(
            "sample".to_string(),
            icn_governance::GovernanceConfig::new(
                GovernanceProfileId::builtin("cooperative_default"),
                icn_governance::MembershipConfig::static_list(vec![did()]),
                GovernanceParams::new(50, 50, 3600),
            ),
        )
    }

    fn sample_proposal() -> Proposal {
        Proposal::new(
            GovernanceDomainId("d".to_string()),
            did(),
            "t".to_string(),
            "d".to_string(),
            ProposalPayload::Text {
                body: "b".to_string(),
            },
        )
    }

    /// Every variant that the pre-containment ingress persisted must still be
    /// reported as a would-be mutation, so the refusal is visible to operators.
    ///
    /// These eight arms replace the delegation-replication tests that previously
    /// asserted gossip *could* create and revoke delegations. That behavior was the
    /// vulnerability: those arms compared the delegation owner against
    /// `GossipEntry::author`, which is attacker-chosen, so "correct sender accepted"
    /// only ever meant "correct sender successfully impersonated".
    #[test]
    fn mutating_variants_are_reported_as_would_be_mutations() {
        let d = Delegation::new(did(), did(), DelegationScope::Blanket);
        let mutating = vec![
            GovernanceMessage::domain_created(sample_domain()),
            GovernanceMessage::domain_updated(sample_domain()),
            GovernanceMessage::proposal_created(sample_proposal()),
            GovernanceMessage::proposal_opened(ProposalId("p".to_string()), 1, 2),
            GovernanceMessage::vote_cast(
                Vote::new(ProposalId("p".to_string()), did(), VoteChoice::For),
                None,
            ),
            GovernanceMessage::proposal_closed(
                ProposalId("p".to_string()),
                ProposalOutcome::Accepted,
                3,
                TallySnapshot::new(1, 0, 0, 1),
                None,
            ),
            GovernanceMessage::delegation_created(d.clone()),
            GovernanceMessage::delegation_revoked(d.id.clone(), d.delegator.clone(), 4),
        ];

        for msg in mutating {
            assert_eq!(
                replicated_state_effect(&msg),
                ReplicatedStateEffect::WouldMutateGovernanceState,
                "{} must be reported as a would-be governance mutation",
                msg.message_type()
            );
        }
    }

    /// Variants the ingress never applied stay classified as inert, so the refusal
    /// diagnostic does not cry wolf on ordinary deliberation traffic.
    #[test]
    fn inert_variants_are_reported_as_no_state_effect() {
        let inert = vec![
            GovernanceMessage::deliberation_started(ProposalId("p".to_string()), 1, 2),
            GovernanceMessage::comment_deleted(
                ProposalId("p".to_string()),
                CommentId("c".to_string()),
                1,
            ),
            GovernanceMessage::reaction_removed(
                ProposalId("p".to_string()),
                CommentId("c".to_string()),
                did(),
                "+1".to_string(),
            ),
        ];

        for msg in inert {
            assert_eq!(
                replicated_state_effect(&msg),
                ReplicatedStateEffect::NoStateEffect,
                "{} must be reported as applying no state",
                msg.message_type()
            );
        }
    }

    /// The refusal itself must not depend on the classification: both dispositions
    /// return without touching governance state. `refuse_replicated_governance_message`
    /// has no store handle at all, which is what makes that unconditional.
    #[test]
    fn refusal_returns_the_classification_for_both_dispositions() {
        let local = did();
        let claimed = did();

        assert_eq!(
            refuse_replicated_governance_message(
                &GovernanceMessage::domain_created(sample_domain()),
                &claimed,
                &local,
            ),
            ReplicatedStateEffect::WouldMutateGovernanceState
        );
        assert_eq!(
            refuse_replicated_governance_message(
                &GovernanceMessage::deliberation_started(ProposalId("p".to_string()), 1, 2),
                &claimed,
                &local,
            ),
            ReplicatedStateEffect::NoStateEffect
        );
    }

    /// The ingress must act on governance topics only, and exactly once per entry
    /// rather than once per subscriber.
    #[test]
    fn delivery_predicate_is_topic_scoped_and_once_per_entry() {
        let local = did();
        let other = did();
        let fed = icn_federation::TOPIC_FEDERATION_GOVERNANCE;
        let fed_scoped = format!("{fed}:fed-1");

        // Governance topics, this node's own subscription -> act.
        for topic in [GOVERNANCE_TOPIC, fed, fed_scoped.as_str()] {
            assert!(
                should_handle_governance_notification(topic, &local, &local),
                "should act on {topic} for the local subscription"
            );
        }

        // Same entry, some other subscriber's notification -> do not act again.
        // This is the per-subscriber amplification guard: an unauthenticated `Subscribe`
        // can add arbitrary DIDs, and each one re-invokes this callback for one entry.
        for topic in [GOVERNANCE_TOPIC, fed, fed_scoped.as_str()] {
            assert!(
                !should_handle_governance_notification(topic, &other, &local),
                "must not act a second time for a non-local subscriber on {topic}"
            );
        }

        // Unrelated topics are not ours, even for the local subscription.
        for topic in [
            "ledger:entries",
            "trust:attestations",
            "governance",
            "federation",
        ] {
            assert!(
                !should_handle_governance_notification(topic, &local, &local),
                "must not act on unrelated topic {topic}"
            );
        }

        // Look-alikes must not slip through on a bare prefix match.
        let fed_lookalike = format!("{fed}X");
        let fed_suffix = format!("{fed}-shadow");
        for topic in [
            "governance:proposals",
            "governance:proposal:extra",
            "xgovernance:proposal",
            fed_lookalike.as_str(),
            fed_suffix.as_str(),
        ] {
            assert!(
                !should_handle_governance_notification(topic, &local, &local),
                "must not act on look-alike topic {topic}"
            );
        }
    }

    /// The operator-facing reason must describe the author as *claimed* and must
    /// never imply the sender was authenticated.
    #[test]
    fn quarantine_reason_calls_the_author_claimed() {
        let reason = REPLICATION_QUARANTINE_REASON.to_ascii_lowercase();
        assert!(
            reason.contains("claimed"),
            "{REPLICATION_QUARANTINE_REASON}"
        );
        assert!(!reason.contains("authenticated sender"));
        assert!(!reason.contains("verified author"));
        assert!(REPLICATION_QUARANTINE_REASON.len() < 256);
    }
}
