//! Result quorum verification for high-value compute tasks.
//!
//! This module implements multi-executor verification to prevent incorrect
//! or malicious results from single executors on valuable computations.
//!
//! # Task Value Classification
//!
//! Tasks are classified by estimated value:
//! - **Low**: Single executor sufficient
//! - **Medium**: 2-executor verification
//! - **High**: 3+ executor quorum
//!
//! # Verification Flow
//!
//! 1. Task submitted with `verification_config`
//! 2. Scheduler assigns multiple executors based on value
//! 3. Results collected from all executors
//! 4. Quorum determines canonical result
//! 5. Divergent results escalate to dispute
//!
//! # Configuration
//!
//! ```toml
//! [compute.verification]
//! low_value_threshold = 100      # Below: single executor
//! medium_value_threshold = 1000  # Below: 2 executors
//! high_value_quorum = 3          # Above medium: 3+ executors
//! consensus_threshold = 0.67     # 2/3 majority for consensus
//! ```

use crate::error::{ComputeError, Result};
use crate::types::{ComputeResult, ExecutionOutcome, TaskHash};
use icn_identity::Did;
use icn_kernel_api::canonical::{CanonicalHasher, DomainTag};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

/// Domain tag of the quorum's outcome comparison key
/// (CANONICAL_ENCODING.md §4.1.3).
const QUORUM_OUTCOME_TAG: DomainTag = DomainTag::new(b"icn:compute:quorum-outcome:v1");

/// The key results are grouped by: BLAKE3 over the tag, a one-byte variant
/// discriminant (the values `ComputeResult::signing_payload` uses), then the
/// variant's payload as a length-prefixed field. Two outcomes share a key only
/// if they are the same variant with the same payload.
fn outcome_hash(outcome: &ExecutionOutcome) -> [u8; 32] {
    let mut hasher = CanonicalHasher::new(QUORUM_OUTCOME_TAG);
    match outcome {
        ExecutionOutcome::Success(data) => hasher.fixed(&[0x00]).bytes(data),
        ExecutionOutcome::Failed(msg) => hasher.fixed(&[0x01]).str(msg),
        ExecutionOutcome::OutOfFuel => hasher.fixed(&[0x02]),
        ExecutionOutcome::Timeout => hasher.fixed(&[0x03]),
    };
    hasher.finish()
}

/// Task value classification for verification requirements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskValue {
    /// Low value - single executor sufficient
    #[default]
    Low,
    /// Medium value - 2-executor verification
    Medium,
    /// High value - 3+ executor quorum
    High,
    /// Critical value - maximum verification
    Critical,
}

impl TaskValue {
    /// Get the required executor count for this value level
    pub fn required_executors(&self) -> usize {
        match self {
            TaskValue::Low => 1,
            TaskValue::Medium => 2,
            TaskValue::High => 3,
            TaskValue::Critical => 5,
        }
    }

    /// Classify a task based on its estimated value in credits
    pub fn from_credits(credits: u64, config: &VerificationConfig) -> Self {
        if credits < config.low_value_threshold {
            TaskValue::Low
        } else if credits < config.medium_value_threshold {
            TaskValue::Medium
        } else if credits < config.high_value_threshold {
            TaskValue::High
        } else {
            TaskValue::Critical
        }
    }
}

/// Configuration for result verification thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Value threshold below which single executor is used (credits)
    pub low_value_threshold: u64,

    /// Value threshold below which 2 executors are used (credits)
    pub medium_value_threshold: u64,

    /// Value threshold above which max executors are used (credits)
    pub high_value_threshold: u64,

    /// Number of executors for high-value tasks
    pub high_value_quorum: usize,

    /// Minimum consensus percentage (0.0-1.0) for accepting results
    pub consensus_threshold: f64,

    /// Time window (ms) to collect results before evaluating quorum
    pub collection_window_ms: u64,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            low_value_threshold: 100,
            medium_value_threshold: 1000,
            high_value_threshold: 10000,
            high_value_quorum: 3,
            consensus_threshold: 0.67,    // 2/3 majority
            collection_window_ms: 30_000, // 30 seconds
        }
    }
}

impl VerificationConfig {
    /// Determine the required executor count for a given task value
    pub fn required_executors(&self, value: TaskValue) -> usize {
        match value {
            TaskValue::Low => 1,
            TaskValue::Medium => 2,
            TaskValue::High => self.high_value_quorum,
            TaskValue::Critical => self.high_value_quorum.max(5),
        }
    }
}

/// Per-task verification requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskVerification {
    /// Task value classification
    pub value: TaskValue,

    /// Required number of executors
    pub required_executors: usize,

    /// Consensus threshold for this task
    pub consensus_threshold: f64,

    /// Explicit executor DIDs (if pre-selected)
    pub assigned_executors: Vec<Did>,
}

impl TaskVerification {
    /// Create verification requirements from task value
    pub fn from_value(value: TaskValue, config: &VerificationConfig) -> Self {
        Self {
            value,
            required_executors: config.required_executors(value),
            consensus_threshold: config.consensus_threshold,
            assigned_executors: Vec::new(),
        }
    }

    /// Check if single executor is sufficient
    pub fn is_single_executor(&self) -> bool {
        self.required_executors == 1
    }
}

impl Default for TaskVerification {
    fn default() -> Self {
        Self {
            value: TaskValue::Low,
            required_executors: 1,
            consensus_threshold: 0.67,
            assigned_executors: Vec::new(),
        }
    }
}

/// Status of result collection for a multi-executor task
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumStatus {
    /// Still collecting results
    Collecting { received: usize, required: usize },
    /// Consensus reached
    Consensus {
        result_hash: [u8; 32],
        agreeing: usize,
        total: usize,
    },
    /// No consensus - results diverge
    Divergent { result_groups: usize, total: usize },
    /// Collection timed out
    Timeout { received: usize, required: usize },
}

/// What the majority rule decided over the collected results
enum Majority<'a> {
    /// One strictly largest group meets the threshold
    Consensus {
        result_hash: [u8; 32],
        agreeing: usize,
        /// The group's member with the smallest executor DID
        canonical: &'a ComputeResult,
    },
    /// No group meets the threshold, or the largest groups tie
    Divergent { result_groups: usize },
}

/// Collected result from an executor
#[derive(Debug, Clone)]
pub struct CollectedResult {
    /// The compute result
    pub result: ComputeResult,
    /// Comparison key of the outcome, variant included (see `outcome_hash`)
    pub output_hash: [u8; 32],
    /// When the result was received
    pub received_at: u64,
}

impl CollectedResult {
    /// Create from a ComputeResult
    pub fn from_result(result: ComputeResult, received_at: u64) -> Self {
        let output_hash = outcome_hash(&result.outcome);

        Self {
            result,
            output_hash,
            received_at,
        }
    }
}

/// Aggregator for collecting and evaluating multi-executor results
#[derive(Debug)]
pub struct ResultAggregator {
    /// Task hash
    task_hash: TaskHash,
    /// Verification requirements
    verification: TaskVerification,
    /// Collected results by executor DID, iterated in DID order
    results: BTreeMap<String, CollectedResult>,
    /// When collection started (for debugging and timeout calculations)
    #[allow(dead_code)]
    started_at: u64,
    /// Collection deadline
    deadline: u64,
}

/// Default collection window in milliseconds (30 seconds)
pub const DEFAULT_COLLECTION_WINDOW_MS: u64 = 30_000;

/// Clock skew tolerance in milliseconds (1 second)
/// Results received within this tolerance after the deadline are still accepted
/// to account for NTP drift between nodes (typically 100-500ms)
pub const CLOCK_SKEW_TOLERANCE_MS: u64 = 1_000;

/// Maximum concurrent verifications to prevent memory exhaustion
/// If more tasks need verification, older ones should be cleaned up first
pub const MAX_CONCURRENT_VERIFICATIONS: usize = 1_000;

impl ResultAggregator {
    /// Create a new result aggregator for a task
    ///
    /// # Arguments
    /// * `task_hash` - Hash of the task being verified
    /// * `verification` - Verification requirements for this task
    /// * `started_at` - Timestamp when collection started (milliseconds)
    /// * `collection_window_ms` - Time window to collect results (milliseconds)
    pub fn new(
        task_hash: TaskHash,
        verification: TaskVerification,
        started_at: u64,
        collection_window_ms: u64,
    ) -> Self {
        let deadline = started_at + collection_window_ms;

        Self {
            task_hash,
            verification,
            results: BTreeMap::new(),
            started_at,
            deadline,
        }
    }

    /// Create a new result aggregator with default collection window (30s)
    pub fn with_default_window(
        task_hash: TaskHash,
        verification: TaskVerification,
        started_at: u64,
    ) -> Self {
        Self::new(
            task_hash,
            verification,
            started_at,
            DEFAULT_COLLECTION_WINDOW_MS,
        )
    }

    /// Set custom collection deadline
    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline = deadline;
        self
    }

    /// Add a result from an executor
    ///
    /// This method verifies:
    /// 1. Task hash matches the expected task
    /// 2. Ed25519 signature is valid for the claimed executor DID
    /// 3. No duplicate results from the same executor
    /// 4. Collection window has not expired
    pub fn add_result(&mut self, result: ComputeResult) -> Result<()> {
        self.add_result_at(result, icn_time::current_timestamp_millis())
    }

    /// [`Self::add_result`] at an explicit time, in milliseconds since the Unix epoch.
    pub fn add_result_at(&mut self, result: ComputeResult, now: u64) -> Result<()> {
        // Verify task hash matches
        if result.task_hash != self.task_hash {
            return Err(ComputeError::InvalidResult(format!(
                "Result task hash mismatch: expected {}, got {}",
                hex::encode(self.task_hash),
                hex::encode(result.task_hash)
            )));
        }

        let executor = result.executor.clone();

        // CRITICAL: Verify Ed25519 signature before accepting result
        // This prevents malicious nodes from forging results claiming to be from other executors
        let executor_did = Did::from_str(&executor).map_err(|e| {
            ComputeError::InvalidResult(format!("Invalid executor DID '{executor}': {e}"))
        })?;
        result.verify_signature(&executor_did)?;

        // Check if we already have a result from this executor
        if self.results.contains_key(&executor) {
            return Err(ComputeError::InvalidResult(format!(
                "Duplicate result from executor {executor}"
            )));
        }

        // Check if collection window has passed (with clock skew tolerance)
        // We allow results up to CLOCK_SKEW_TOLERANCE_MS after the deadline
        // to account for NTP drift between nodes
        if now > self.deadline + CLOCK_SKEW_TOLERANCE_MS {
            return Err(ComputeError::InvalidResult(
                "Result collection window has closed".into(),
            ));
        }

        let collected = CollectedResult::from_result(result, now);
        self.results.insert(executor, collected);

        Ok(())
    }

    /// Get current quorum status
    pub fn status(&self) -> QuorumStatus {
        self.status_at(icn_time::current_timestamp_millis())
    }

    /// [`Self::status`] at an explicit time, in milliseconds since the Unix epoch.
    pub fn status_at(&self, now: u64) -> QuorumStatus {
        let received = self.results.len();
        let required = self.verification.required_executors;

        // Still collecting
        if received < required {
            if now > self.deadline {
                return QuorumStatus::Timeout { received, required };
            }
            return QuorumStatus::Collecting { received, required };
        }

        // All results received - evaluate consensus
        self.evaluate_consensus()
    }

    /// Evaluate consensus among collected results
    fn evaluate_consensus(&self) -> QuorumStatus {
        let total = self.results.len();
        match self.majority() {
            Majority::Consensus {
                result_hash,
                agreeing,
                ..
            } => QuorumStatus::Consensus {
                result_hash,
                agreeing,
                total,
            },
            Majority::Divergent { result_groups } => QuorumStatus::Divergent {
                result_groups,
                total,
            },
        }
    }

    /// The one majority rule behind both `status()` and `canonical_result()`.
    ///
    /// Results are grouped by `output_hash`. A group qualifies when its size
    /// reaches `ceil(total * consensus_threshold)`. Consensus needs a single,
    /// strictly largest qualifying group; a tie for largest is divergence. The
    /// canonical member is the one with the smallest executor DID.
    fn majority(&self) -> Majority<'_> {
        // `results` iterates in DID order, so each group's first member has
        // the smallest DID.
        let mut groups: BTreeMap<[u8; 32], Vec<&CollectedResult>> = BTreeMap::new();
        for result in self.results.values() {
            groups.entry(result.output_hash).or_default().push(result);
        }

        let total = self.results.len();
        let threshold = (total as f64 * self.verification.consensus_threshold).ceil() as usize;

        let largest = groups.values().map(Vec::len).max().unwrap_or(0);
        let mut qualifying = groups
            .iter()
            .filter(|(_, group)| group.len() == largest && largest >= threshold);

        match (qualifying.next(), qualifying.next()) {
            (Some((hash, group)), None) => Majority::Consensus {
                result_hash: *hash,
                agreeing: group.len(),
                canonical: &group[0].result,
            },
            _ => Majority::Divergent {
                result_groups: groups.len(),
            },
        }
    }

    /// Get the canonical result (if consensus reached)
    ///
    /// Uses the same majority rule as `status()`, so this is `Some` exactly when
    /// `status()` reports `Consensus`, and it is that group's member with the
    /// smallest executor DID.
    pub fn canonical_result(&self) -> Option<&ComputeResult> {
        let received = self.results.len();
        let required = self.verification.required_executors;

        // Not enough results yet
        if received < required {
            return None;
        }

        match self.majority() {
            Majority::Consensus { canonical, .. } => Some(canonical),
            Majority::Divergent { .. } => None,
        }
    }

    /// Get all collected results, in executor DID order
    pub fn all_results(&self) -> impl Iterator<Item = &ComputeResult> {
        self.results.values().map(|r| &r.result)
    }

    /// Get executors grouped by their result, each group in DID order
    pub fn executor_groups(&self) -> HashMap<[u8; 32], Vec<String>> {
        let mut groups: HashMap<[u8; 32], Vec<String>> = HashMap::new();
        for (executor, result) in &self.results {
            groups
                .entry(result.output_hash)
                .or_default()
                .push(executor.clone());
        }
        groups
    }
}

/// The time source for collection windows: milliseconds since the Unix epoch.
///
/// Production uses `icn_time::current_timestamp_millis`. Tests inject a controlled clock so that
/// timeout and expiry are deterministic.
pub type QuorumClock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Why a registered task's quorum closed without accepting a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumRejection {
    /// The collected results did not agree.
    Divergent,
    /// A result arrived after the deadline, inside the skew tolerance, with too few results.
    TimedOut,
    /// The expiry sweep retired the collection after its deadline.
    Expired,
    /// The task was cancelled.
    Cancelled,
}

/// How a registered task's quorum closed.
///
/// It outlives the task's aggregator, so that a later result is refused instead of being taken
/// for one belonging to a task that never needed a quorum. It is deliberately small: the
/// canonical result is handed to the caller once, in [`QuorumDecision::Accepted`], and the task's
/// own record keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumClosure {
    /// Consensus accepted the result with this output hash, from this executor.
    Accepted {
        /// Output hash of the agreeing group
        result_hash: [u8; 32],
        /// Executor of the canonical result
        executor: String,
    },
    /// The quorum closed without accepting any result.
    Rejected(QuorumRejection),
}

/// The authoritative quorum state of a task, whether or not an aggregator is still active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumRecord {
    /// Never registered for quorum verification: the single-executor path applies.
    NotRegistered,
    /// Registered and still collecting results.
    Collecting { received: usize, required: usize },
    /// Registered, and its quorum has closed.
    Closed(QuorumClosure),
}

/// What one counted submission decided.
///
/// It is produced under the manager's lock, in the same step that closes the quorum, so the
/// canonical result cannot be lost to a concurrent expiry, cancellation or competing result
/// between "consensus was reached" and "here is the result that was accepted".
#[derive(Debug, Clone)]
pub enum QuorumDecision {
    /// The result was counted; more are needed.
    Collecting { received: usize, required: usize },
    /// Consensus: the quorum is closed and `canonical` is the accepted result.
    Accepted {
        canonical: ComputeResult,
        result_hash: [u8; 32],
        agreeing: usize,
        total: usize,
    },
    /// The results diverged: the quorum is closed without acceptance.
    Divergent {
        result_groups: usize,
        total: usize,
        /// Executors grouped by output hash, for dispute evidence
        executor_groups: HashMap<[u8; 32], Vec<String>>,
    },
    /// The deadline passed with too few results: the quorum is closed without acceptance.
    TimedOut { received: usize, required: usize },
}

impl QuorumDecision {
    /// The equivalent [`QuorumStatus`].
    pub fn status(&self) -> QuorumStatus {
        match self {
            Self::Collecting { received, required } => QuorumStatus::Collecting {
                received: *received,
                required: *required,
            },
            Self::Accepted {
                result_hash,
                agreeing,
                total,
                ..
            } => QuorumStatus::Consensus {
                result_hash: *result_hash,
                agreeing: *agreeing,
                total: *total,
            },
            Self::Divergent {
                result_groups,
                total,
                ..
            } => QuorumStatus::Divergent {
                result_groups: *result_groups,
                total: *total,
            },
            Self::TimedOut { received, required } => QuorumStatus::Timeout {
                received: *received,
                required: *required,
            },
        }
    }
}

/// Active collections and closed quorums, behind one lock so that closing is a single step.
#[derive(Default)]
struct QuorumBook {
    /// Collections in progress. Bounded by [`MAX_CONCURRENT_VERIFICATIONS`].
    active: HashMap<TaskHash, ResultAggregator>,
    /// Closed quorums, kept until the task itself is retired ([`ResultQuorumManager::retire_task`]).
    closed: HashMap<TaskHash, QuorumClosure>,
}

impl QuorumBook {
    /// Drop the task's aggregator, with every collected result, and keep only its closure.
    fn close(&mut self, task_hash: TaskHash, closure: QuorumClosure) {
        self.active.remove(&task_hash);
        self.closed.insert(task_hash, closure);
    }
}

/// Manager for coordinating result quorum across multiple tasks
///
/// # Acceptance
///
/// A registered task's quorum is either collecting or closed. [`Self::submit`] closes it, in the
/// same locked step that evaluates it, on consensus, divergence or timeout; [`Self::cancel_task`]
/// and [`Self::cleanup_expired`] close it too. Once closed, every further result is refused with
/// [`ComputeError::QuorumClosed`]. [`ComputeError::TaskNotFound`] from [`Self::submit`] means only
/// that the task was never registered, which is the one case where the single-executor path may
/// complete it.
///
/// # Retention
///
/// Active collections are bounded by [`MAX_CONCURRENT_VERIFICATIONS`]. When a quorum closes, its
/// aggregator is dropped and replaced by a small [`QuorumClosure`] that does not count against
/// that bound. A closure is kept until [`Self::retire_task`], which callers invoke only when the
/// task itself leaves the task manager. There is no time-based expiry: dropping a closure while
/// its task is still addressable would let a late result pass for one belonging to an
/// unregistered task. Closures therefore live exactly as long as the task records they describe.
/// (Today `TaskManager::cleanup` has no production caller, so both live for the process; a
/// closure is an output hash and an executor DID, against a task record that holds its code and
/// inputs.)
///
/// # Restart
///
/// All quorum state is in memory, as is the task manager. Nothing here survives a restart, and
/// no restart guarantee is made.
pub struct ResultQuorumManager {
    /// Active collections and closed quorums
    book: RwLock<QuorumBook>,
    /// Verification configuration
    config: VerificationConfig,
    /// Time source for collection windows
    clock: QuorumClock,
}

impl ResultQuorumManager {
    /// Create a new result quorum manager
    pub fn new(config: VerificationConfig) -> Self {
        Self::with_clock(config, Arc::new(icn_time::current_timestamp_millis))
    }

    /// Create a manager that reads time from `clock` instead of the system clock.
    pub fn with_clock(config: VerificationConfig, clock: QuorumClock) -> Self {
        Self {
            book: RwLock::new(QuorumBook::default()),
            config,
            clock,
        }
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn closed_error(task_hash: &TaskHash, closure: &QuorumClosure) -> ComputeError {
        ComputeError::QuorumClosed {
            task: hex::encode(task_hash),
            closure: closure.clone(),
        }
    }

    fn no_active_collection(task_hash: &TaskHash) -> ComputeError {
        ComputeError::TaskNotFound(format!(
            "No active quorum collection for task {}",
            hex::encode(task_hash)
        ))
    }

    /// Register a task for multi-executor verification
    ///
    /// # Errors
    /// - [`ComputeError::QuorumClosed`] if the task's quorum has already closed
    /// - Returns error if task is already registered
    /// - Returns error if MAX_CONCURRENT_VERIFICATIONS limit reached (DoS protection)
    pub fn register_task(&self, task_hash: TaskHash, verification: TaskVerification) -> Result<()> {
        let now = self.now();

        let aggregator = ResultAggregator::new(
            task_hash,
            verification,
            now,
            self.config.collection_window_ms,
        );

        let mut book = self.book.write().map_err(|_| ComputeError::LockError)?;

        if let Some(closure) = book.closed.get(&task_hash) {
            return Err(Self::closed_error(&task_hash, closure));
        }

        // DoS protection: limit concurrent verifications. Closed quorums do not count.
        if book.active.len() >= MAX_CONCURRENT_VERIFICATIONS {
            return Err(ComputeError::InvalidResult(format!(
                "Maximum concurrent verifications reached ({MAX_CONCURRENT_VERIFICATIONS}). Clean up expired tasks first."
            )));
        }

        if book.active.contains_key(&task_hash) {
            return Err(ComputeError::InvalidResult(format!(
                "Task {} already registered for quorum verification",
                hex::encode(task_hash)
            )));
        }

        book.active.insert(task_hash, aggregator);
        Ok(())
    }

    /// Submit a result for quorum evaluation, closing the quorum when the result decides it.
    ///
    /// # Errors
    /// - [`ComputeError::TaskNotFound`]: the task was never registered (or has been retired).
    /// - [`ComputeError::QuorumClosed`]: the task's quorum has closed; the result is refused.
    /// - [`ComputeError::InvalidResult`]: the aggregator refused the result (bad signature,
    ///   wrong task, duplicate executor, collection window closed). Nothing changes.
    pub fn submit(&self, result: ComputeResult) -> Result<QuorumDecision> {
        let task_hash = result.task_hash;
        let now = self.now();

        let mut book = self.book.write().map_err(|_| ComputeError::LockError)?;

        if let Some(closure) = book.closed.get(&task_hash) {
            return Err(Self::closed_error(&task_hash, closure));
        }

        let aggregator = book.active.get_mut(&task_hash).ok_or_else(|| {
            ComputeError::TaskNotFound(format!(
                "Task {} is not registered for quorum verification",
                hex::encode(task_hash)
            ))
        })?;

        aggregator.add_result_at(result, now)?;

        let (decision, closure) = match aggregator.status_at(now) {
            QuorumStatus::Collecting { received, required } => {
                return Ok(QuorumDecision::Collecting { received, required });
            }
            QuorumStatus::Consensus {
                result_hash,
                agreeing,
                total,
            } => {
                let canonical = aggregator.canonical_result().cloned().ok_or_else(|| {
                    ComputeError::Internal(
                        "quorum reported consensus without a canonical result".into(),
                    )
                })?;
                let closure = QuorumClosure::Accepted {
                    result_hash,
                    executor: canonical.executor.clone(),
                };
                (
                    QuorumDecision::Accepted {
                        canonical,
                        result_hash,
                        agreeing,
                        total,
                    },
                    closure,
                )
            }
            QuorumStatus::Divergent {
                result_groups,
                total,
            } => (
                QuorumDecision::Divergent {
                    result_groups,
                    total,
                    executor_groups: aggregator.executor_groups(),
                },
                QuorumClosure::Rejected(QuorumRejection::Divergent),
            ),
            QuorumStatus::Timeout { received, required } => (
                QuorumDecision::TimedOut { received, required },
                QuorumClosure::Rejected(QuorumRejection::TimedOut),
            ),
        };

        book.close(task_hash, closure);
        Ok(decision)
    }

    /// [`Self::submit`], reporting only the status.
    pub fn submit_result(&self, result: ComputeResult) -> Result<QuorumStatus> {
        self.submit(result).map(|decision| decision.status())
    }

    /// The authoritative quorum state of a task.
    pub fn record(&self, task_hash: &TaskHash) -> Result<QuorumRecord> {
        let book = self.book.read().map_err(|_| ComputeError::LockError)?;

        if let Some(closure) = book.closed.get(task_hash) {
            return Ok(QuorumRecord::Closed(closure.clone()));
        }
        Ok(match book.active.get(task_hash) {
            Some(aggregator) => QuorumRecord::Collecting {
                received: aggregator.results.len(),
                required: aggregator.verification.required_executors,
            },
            None => QuorumRecord::NotRegistered,
        })
    }

    /// Get the status of a task's *active* collection.
    ///
    /// [`ComputeError::TaskNotFound`] here means only that no collection is active: the task may
    /// never have been registered, or its quorum may have closed. Use [`Self::record`] to tell
    /// those apart.
    pub fn get_status(&self, task_hash: &TaskHash) -> Result<QuorumStatus> {
        let now = self.now();
        let book = self.book.read().map_err(|_| ComputeError::LockError)?;

        let aggregator = book
            .active
            .get(task_hash)
            .ok_or_else(|| Self::no_active_collection(task_hash))?;

        Ok(aggregator.status_at(now))
    }

    /// Get the canonical result of an *active* collection, if it has consensus.
    ///
    /// A closed quorum has no active collection. The accepted result is returned once, by
    /// [`Self::submit`], in [`QuorumDecision::Accepted`].
    pub fn get_canonical_result(&self, task_hash: &TaskHash) -> Result<Option<ComputeResult>> {
        let book = self.book.read().map_err(|_| ComputeError::LockError)?;

        let aggregator = book
            .active
            .get(task_hash)
            .ok_or_else(|| Self::no_active_collection(task_hash))?;

        Ok(aggregator.canonical_result().cloned())
    }

    /// Get executor groupings of an *active* collection (useful for dispute evidence)
    pub fn get_executor_groups(
        &self,
        task_hash: &TaskHash,
    ) -> Result<HashMap<[u8; 32], Vec<String>>> {
        let book = self.book.read().map_err(|_| ComputeError::LockError)?;

        let aggregator = book
            .active
            .get(task_hash)
            .ok_or_else(|| Self::no_active_collection(task_hash))?;

        Ok(aggregator.executor_groups())
    }

    /// Close a registered task's quorum because the task was cancelled.
    ///
    /// A quorum that has already closed keeps its closure, so an accepted result stays accepted.
    /// A task that was never registered is unaffected.
    pub fn cancel_task(&self, task_hash: &TaskHash) -> Result<()> {
        let mut book = self.book.write().map_err(|_| ComputeError::LockError)?;

        if book.active.contains_key(task_hash) {
            book.close(
                *task_hash,
                QuorumClosure::Rejected(QuorumRejection::Cancelled),
            );
        }
        Ok(())
    }

    /// Forget everything about a task's quorum.
    ///
    /// Call this only when the task itself is retired from the task manager. Afterwards a result
    /// for the task is indistinguishable from one for a task that was never registered.
    pub fn retire_task(&self, task_hash: &TaskHash) -> Result<()> {
        let mut book = self.book.write().map_err(|_| ComputeError::LockError)?;

        book.active.remove(task_hash);
        book.closed.remove(task_hash);
        Ok(())
    }

    /// Close every collection whose deadline has passed, as [`QuorumRejection::Expired`].
    ///
    /// Returns the tasks closed by this call.
    pub fn cleanup_expired(&self) -> Result<Vec<TaskHash>> {
        let now = self.now();

        let mut book = self.book.write().map_err(|_| ComputeError::LockError)?;

        let expired: Vec<TaskHash> = book
            .active
            .iter()
            .filter(|(_, agg)| now > agg.deadline)
            .map(|(hash, _)| *hash)
            .collect();

        for hash in &expired {
            book.close(*hash, QuorumClosure::Rejected(QuorumRejection::Expired));
        }

        Ok(expired)
    }

    /// Number of active collections (bounded by [`MAX_CONCURRENT_VERIFICATIONS`]).
    pub fn active_count(&self) -> Result<usize> {
        let book = self.book.read().map_err(|_| ComputeError::LockError)?;
        Ok(book.active.len())
    }

    /// Number of closed quorums still retained.
    pub fn closed_count(&self) -> Result<usize> {
        let book = self.book.read().map_err(|_| ComputeError::LockError)?;
        Ok(book.closed.len())
    }

    /// Get the verification config
    pub fn config(&self) -> &VerificationConfig {
        &self.config
    }

    /// Classify a task value based on estimated credits
    pub fn classify_value(&self, estimated_credits: u64) -> TaskValue {
        TaskValue::from_credits(estimated_credits, &self.config)
    }

    /// Create verification requirements for a task
    pub fn create_verification(&self, value: TaskValue) -> TaskVerification {
        TaskVerification::from_value(value, &self.config)
    }
}

impl Default for ResultQuorumManager {
    fn default() -> Self {
        Self::new(VerificationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExecutionOutcome;

    /// Test executor with keypair for signing results
    struct TestExecutor {
        keypair: icn_identity::KeyPair,
        did: String,
    }

    impl TestExecutor {
        fn new() -> Self {
            let keypair = icn_identity::KeyPair::generate().unwrap();
            let did = keypair.did().to_string();
            Self { keypair, did }
        }

        fn signed_result(&self, task_hash: TaskHash, output: &[u8]) -> ComputeResult {
            self.signed(task_hash, ExecutionOutcome::Success(output.to_vec()))
        }

        fn signed(&self, task_hash: TaskHash, outcome: ExecutionOutcome) -> ComputeResult {
            let mut result = ComputeResult {
                task_hash,
                task_id: "test-task".into(),
                executor: self.did.clone(),
                outcome,
                fuel_used: 1000,
                duration_ms: 100,
                completed_at: icn_time::current_timestamp_millis(),
                signature: vec![],
            };
            // Sign with the executor's keypair
            let payload = result.signing_payload();
            let signature = self.keypair.sign(&payload);
            result.signature = signature.to_bytes().to_vec();
            result
        }
    }

    /// `n` executors in ascending DID order, so a test can name the smallest.
    fn executors(n: usize) -> Vec<TestExecutor> {
        let mut executors: Vec<_> = (0..n).map(|_| TestExecutor::new()).collect();
        executors.sort_by(|a, b| a.did.cmp(&b.did));
        executors
    }

    fn aggregator(
        task_hash: TaskHash,
        required_executors: usize,
        consensus_threshold: f64,
    ) -> ResultAggregator {
        ResultAggregator::with_default_window(
            task_hash,
            TaskVerification {
                value: TaskValue::High,
                required_executors,
                consensus_threshold,
                assigned_executors: vec![],
            },
            icn_time::current_timestamp_millis(),
        )
    }

    /// The quorum comparison key of `outcome`. `from_result` reads only the
    /// outcome, so the other fields and the signature do not matter here.
    fn quorum_hash(outcome: ExecutionOutcome) -> [u8; 32] {
        let result = ComputeResult {
            task_hash: [0u8; 32],
            task_id: "t".into(),
            executor: "did:icn:x".into(),
            outcome,
            fuel_used: 0,
            duration_ms: 0,
            completed_at: 0,
            signature: vec![],
        };
        CollectedResult::from_result(result, 0).output_hash
    }

    /// `status()` and `canonical_result()` must describe the same decision.
    fn assert_status_agrees_with_canonical(aggregator: &ResultAggregator) {
        let status = aggregator.status();
        let canonical = aggregator.canonical_result();
        match status {
            QuorumStatus::Consensus { result_hash, .. } => {
                let canonical = canonical
                    .unwrap_or_else(|| panic!("{status:?} but canonical_result() is None"));
                assert_eq!(
                    quorum_hash(canonical.outcome.clone()),
                    result_hash,
                    "{status:?} but canonical_result() is {:?}",
                    canonical.outcome
                );
            }
            _ => assert!(
                canonical.is_none(),
                "{status:?} but canonical_result() is {canonical:?}"
            ),
        }
    }

    #[test]
    fn quorum_hash_binds_the_outcome_variant() {
        let outcomes = [
            ExecutionOutcome::Success(b"OUT_OF_FUEL".to_vec()),
            ExecutionOutcome::OutOfFuel,
            ExecutionOutcome::Success(b"TIMEOUT".to_vec()),
            ExecutionOutcome::Failed("TIMEOUT".into()),
            ExecutionOutcome::Timeout,
            ExecutionOutcome::Failed("x".into()),
            ExecutionOutcome::Success(b"x".to_vec()),
            ExecutionOutcome::Success(vec![]),
            ExecutionOutcome::Failed(String::new()),
        ];

        let mut aliases = Vec::new();
        for (i, a) in outcomes.iter().enumerate() {
            assert_eq!(quorum_hash(a.clone()), quorum_hash(a.clone()));
            for b in &outcomes[i + 1..] {
                if quorum_hash(a.clone()) == quorum_hash(b.clone()) {
                    aliases.push(format!("{a:?} = {b:?}"));
                }
            }
        }
        assert!(aliases.is_empty(), "aliased outcomes: {aliases:#?}");
    }

    #[test]
    fn quorum_outcome_hash_golden_vectors() {
        // The preimages are built by hand from the documented layout, not
        // through CanonicalHasher. The pinned hex was computed with a separate
        // BLAKE3 implementation.
        let tag: &[u8] = b"icn:compute:quorum-outcome:v1";
        let cases = [
            (
                ExecutionOutcome::Success(b"output".to_vec()),
                [tag, &[0x00], &6u64.to_le_bytes(), b"output"].concat(),
                "9297c9194c895c7e4cbd50da05e1053ee19c15ef0c6f3d197fc88cd920018ff4",
            ),
            (
                ExecutionOutcome::Failed("boom".into()),
                [tag, &[0x01], &4u64.to_le_bytes(), b"boom"].concat(),
                "eff743d7c7c56fe505f9a7dc07536a7623bc452cb0cb5222b87ce74086254d7c",
            ),
            (
                ExecutionOutcome::OutOfFuel,
                [tag, &[0x02]].concat(),
                "36acc8ca7bc6d1f74a874d11fb1d156ea922a7452d9a60f84ade59ebc88a9101",
            ),
            (
                ExecutionOutcome::Timeout,
                [tag, &[0x03]].concat(),
                "150ec4752d9e61742f52d9b4c86ecbffbb92b090ffc2c073712fa1cef9f9ba36",
            ),
        ];

        for (outcome, preimage, pinned) in cases {
            let hash = quorum_hash(outcome.clone());
            assert_eq!(
                hash,
                *blake3::hash(&preimage).as_bytes(),
                "{outcome:?} does not hash its documented preimage"
            );
            assert_eq!(hex::encode(hash), pinned, "{outcome:?} moved");
        }
    }

    #[test]
    fn aliased_outcomes_do_not_form_a_consensus_group() {
        let task_hash = [11u8; 32];
        let execs = executors(3);
        // (what two honest executors report, a different outcome whose payload
        // bytes used to hash the same)
        let pairs = [
            (
                ExecutionOutcome::OutOfFuel,
                ExecutionOutcome::Success(b"OUT_OF_FUEL".to_vec()),
            ),
            (
                ExecutionOutcome::Timeout,
                ExecutionOutcome::Failed("TIMEOUT".into()),
            ),
            (
                ExecutionOutcome::Failed("x".into()),
                ExecutionOutcome::Success(b"x".to_vec()),
            ),
        ];

        for (honest, impostor) in pairs {
            let results = [
                execs[0].signed(task_hash, honest.clone()),
                execs[1].signed(task_hash, honest.clone()),
                execs[2].signed(task_hash, impostor.clone()),
            ];

            let mut simple = aggregator(task_hash, 3, 0.50);
            for result in results.clone() {
                simple.add_result(result).unwrap();
            }
            let status = simple.status();
            assert!(
                matches!(
                    status,
                    QuorumStatus::Consensus {
                        agreeing: 2,
                        total: 3,
                        ..
                    }
                ),
                "{honest:?} vs {impostor:?} at 0.50: {status:?}"
            );
            assert_eq!(
                simple.canonical_result().map(|r| &r.outcome),
                Some(&honest),
                "{honest:?} vs {impostor:?} at 0.50"
            );
            assert_status_agrees_with_canonical(&simple);

            let mut strict = aggregator(task_hash, 3, 0.67);
            for result in results {
                strict.add_result(result).unwrap();
            }
            assert_eq!(
                strict.status(),
                QuorumStatus::Divergent {
                    result_groups: 2,
                    total: 3
                },
                "{honest:?} vs {impostor:?} at 0.67"
            );
            assert_status_agrees_with_canonical(&strict);
        }
    }

    #[test]
    fn each_variant_reaches_consensus_with_itself() {
        let task_hash = [12u8; 32];
        let execs = executors(3);
        let outcomes = [
            ExecutionOutcome::Success(b"o".to_vec()),
            ExecutionOutcome::Failed("e".into()),
            ExecutionOutcome::OutOfFuel,
            ExecutionOutcome::Timeout,
        ];

        for outcome in outcomes {
            // The default threshold. `add_result` verifies each signature, so
            // this also shows every variant signs and verifies.
            let mut aggregator = aggregator(task_hash, 3, 0.67);
            for exec in &execs {
                aggregator
                    .add_result(exec.signed(task_hash, outcome.clone()))
                    .unwrap();
            }
            assert_eq!(
                aggregator.status(),
                QuorumStatus::Consensus {
                    result_hash: quorum_hash(outcome.clone()),
                    agreeing: 3,
                    total: 3
                },
                "{outcome:?}"
            );
            assert_eq!(
                aggregator.canonical_result().map(|r| &r.outcome),
                Some(&outcome)
            );
            assert_status_agrees_with_canonical(&aggregator);
        }
    }

    #[test]
    fn a_tie_between_qualifying_groups_is_divergent() {
        let task_hash = [13u8; 32];
        let execs = executors(4);
        let mut aggregator = aggregator(task_hash, 4, 0.50);
        for (exec, output) in execs.iter().zip([b"a", b"a", b"b", b"b"]) {
            aggregator
                .add_result(exec.signed_result(task_hash, output))
                .unwrap();
        }

        assert_eq!(
            aggregator.status(),
            QuorumStatus::Divergent {
                result_groups: 2,
                total: 4
            }
        );
        assert!(aggregator.canonical_result().is_none());
        assert_status_agrees_with_canonical(&aggregator);
    }

    #[test]
    fn canonical_selection_is_deterministic() {
        let task_hash = [14u8; 32];
        let execs = executors(3);
        let results: Vec<_> = execs
            .iter()
            .map(|exec| exec.signed_result(task_hash, b"same"))
            .collect();

        for round in 0..20 {
            let mut aggregator = aggregator(task_hash, 3, 0.67);
            for i in 0..results.len() {
                aggregator
                    .add_result(results[(i + round) % results.len()].clone())
                    .unwrap();
            }

            assert_eq!(
                aggregator.canonical_result(),
                Some(&results[0]),
                "round {round}: the canonical member is the smallest executor DID"
            );
            let groups = aggregator.executor_groups();
            let members = groups.values().next().unwrap();
            let sorted: Vec<_> = execs.iter().map(|e| e.did.clone()).collect();
            assert_eq!(members, &sorted, "round {round}: groups are sorted");
        }
    }

    #[test]
    fn signing_payload_is_unchanged() {
        // Pinned before the quorum hash changed: the quorum hash must not move
        // signed bytes. The layout itself is ambiguous (no length prefixes);
        // fixing that is a separate protocol change.
        let cases = [
            (
                ExecutionOutcome::Success(b"output".to_vec()),
                "11111111111111111111111111111111111111111111111111111111111111117461736b2d316469643a69636e3a7a65786563006f7574707574e80300000000000064000000000000000068e5cf8b010000",
            ),
            (
                ExecutionOutcome::Failed("boom".into()),
                "11111111111111111111111111111111111111111111111111111111111111117461736b2d316469643a69636e3a7a6578656301626f6f6de80300000000000064000000000000000068e5cf8b010000",
            ),
            (
                ExecutionOutcome::OutOfFuel,
                "11111111111111111111111111111111111111111111111111111111111111117461736b2d316469643a69636e3a7a6578656302e80300000000000064000000000000000068e5cf8b010000",
            ),
            (
                ExecutionOutcome::Timeout,
                "11111111111111111111111111111111111111111111111111111111111111117461736b2d316469643a69636e3a7a6578656303e80300000000000064000000000000000068e5cf8b010000",
            ),
        ];

        for (outcome, pinned) in cases {
            let result = ComputeResult {
                task_hash: [0x11; 32],
                task_id: "task-1".into(),
                executor: "did:icn:zexec".into(),
                outcome: outcome.clone(),
                fuel_used: 1000,
                duration_ms: 100,
                completed_at: 1_700_000_000_000,
                signature: vec![],
            };
            assert_eq!(hex::encode(result.signing_payload()), pinned, "{outcome:?}");
        }
    }

    #[test]
    fn test_task_value_classification() {
        let config = VerificationConfig::default();

        assert_eq!(TaskValue::from_credits(50, &config), TaskValue::Low);
        assert_eq!(TaskValue::from_credits(500, &config), TaskValue::Medium);
        assert_eq!(TaskValue::from_credits(5000, &config), TaskValue::High);
        assert_eq!(TaskValue::from_credits(50000, &config), TaskValue::Critical);
    }

    #[test]
    fn test_required_executors() {
        assert_eq!(TaskValue::Low.required_executors(), 1);
        assert_eq!(TaskValue::Medium.required_executors(), 2);
        assert_eq!(TaskValue::High.required_executors(), 3);
        assert_eq!(TaskValue::Critical.required_executors(), 5);
    }

    #[test]
    fn test_single_executor_consensus() {
        let task_hash = [1u8; 32];
        let verification = TaskVerification {
            value: TaskValue::Low,
            required_executors: 1,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        aggregator
            .add_result(exec1.signed_result(task_hash, b"output"))
            .unwrap();

        assert!(matches!(
            aggregator.status(),
            QuorumStatus::Consensus { agreeing: 1, .. }
        ));
    }

    #[test]
    fn test_multi_executor_consensus() {
        let task_hash = [2u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 3,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();

        // Add 3 agreeing results
        aggregator
            .add_result(exec1.signed_result(task_hash, b"same"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"same"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"same"))
            .unwrap();

        let status = aggregator.status();
        assert!(
            matches!(
                status,
                QuorumStatus::Consensus {
                    agreeing: 3,
                    total: 3,
                    ..
                }
            ),
            "Expected consensus with 3 agreeing, got: {status:?}"
        );
    }

    #[test]
    fn test_divergent_results() {
        let task_hash = [3u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 3,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();

        // Add 3 different results - no consensus possible
        aggregator
            .add_result(exec1.signed_result(task_hash, b"output1"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"output2"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"output3"))
            .unwrap();

        let status = aggregator.status();
        assert!(
            matches!(
                status,
                QuorumStatus::Divergent {
                    result_groups: 3,
                    total: 3
                }
            ),
            "Expected divergent with 3 groups, got: {status:?}"
        );
    }

    #[test]
    fn test_majority_consensus() {
        let task_hash = [4u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 3,
            consensus_threshold: 0.50, // Simple majority: 2/3 > 50%
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();

        // 2 agree, 1 differs - 2/3 = 66.7% meets 50% threshold
        aggregator
            .add_result(exec1.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"wrong"))
            .unwrap();

        let status = aggregator.status();
        assert!(
            matches!(
                status,
                QuorumStatus::Consensus {
                    agreeing: 2,
                    total: 3,
                    ..
                }
            ),
            "Expected consensus with 2/3, got: {status:?}"
        );
    }

    #[test]
    fn test_duplicate_executor_rejected() {
        let task_hash = [5u8; 32];
        let verification = TaskVerification {
            value: TaskValue::Medium,
            required_executors: 2,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();

        // First submission succeeds
        aggregator
            .add_result(exec1.signed_result(task_hash, b"output"))
            .unwrap();

        // Second submission from same executor is rejected
        let result2 = exec1.signed_result(task_hash, b"different");
        assert!(aggregator.add_result(result2).is_err());
    }

    #[test]
    fn test_wrong_task_hash_rejected() {
        let task_hash = [6u8; 32];
        let wrong_hash = [7u8; 32];
        let verification = TaskVerification::default();

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        // Result signed for wrong task hash
        let result = exec1.signed_result(wrong_hash, b"output");
        assert!(aggregator.add_result(result).is_err());
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let task_hash = [10u8; 32];
        let verification = TaskVerification::default();

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();

        // Create result claiming to be from exec1 but signed by exec2
        let mut forged_result = ComputeResult {
            task_hash,
            task_id: "test-task".into(),
            executor: exec1.did.clone(), // Claims to be exec1
            outcome: ExecutionOutcome::Success(b"output".to_vec()),
            fuel_used: 1000,
            duration_ms: 100,
            completed_at: icn_time::current_timestamp_millis(),
            signature: vec![],
        };
        // Sign with exec2's key (wrong key)
        let payload = forged_result.signing_payload();
        let signature = exec2.keypair.sign(&payload);
        forged_result.signature = signature.to_bytes().to_vec();

        // Should be rejected due to signature verification failure
        let err = aggregator.add_result(forged_result).unwrap_err();
        assert!(
            err.to_string().contains("Signature verification failed"),
            "Expected signature verification error, got: {err}"
        );
    }

    #[test]
    fn test_result_quorum_manager() {
        let manager = ResultQuorumManager::default();
        let task_hash = [8u8; 32];

        // Classify and create verification
        let value = manager.classify_value(500); // Medium
        assert_eq!(value, TaskValue::Medium);

        let verification = manager.create_verification(value);
        assert_eq!(verification.required_executors, 2);

        // Register task
        manager.register_task(task_hash, verification).unwrap();

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();

        // Submit first result
        let result1 = exec1.signed_result(task_hash, b"output");
        let status = manager.submit_result(result1).unwrap();
        assert!(matches!(
            status,
            QuorumStatus::Collecting {
                received: 1,
                required: 2
            }
        ));

        // Submit second result (same output): consensus closes the quorum and hands over the
        // canonical result in the same step.
        let result2 = exec2.signed_result(task_hash, b"output");
        let decision = manager.submit(result2).unwrap();
        assert!(matches!(
            decision.status(),
            QuorumStatus::Consensus {
                agreeing: 2,
                total: 2,
                ..
            }
        ));
        let QuorumDecision::Accepted { canonical, .. } = decision else {
            panic!("consensus must carry the canonical result, got {decision:?}");
        };
        assert_eq!(canonical.task_hash, task_hash);
        assert!(matches!(
            manager.record(&task_hash).unwrap(),
            QuorumRecord::Closed(QuorumClosure::Accepted { .. })
        ));
    }

    #[test]
    fn test_executor_groups() {
        let task_hash = [9u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 4,
            consensus_threshold: 0.5,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();
        let exec4 = TestExecutor::new();

        // 2 groups: 3 correct, 1 wrong
        aggregator
            .add_result(exec1.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec4.signed_result(task_hash, b"wrong"))
            .unwrap();

        let groups = aggregator.executor_groups();
        assert_eq!(groups.len(), 2);

        // Find the majority group (should have 3 executors)
        let majority_count = groups.values().map(|v| v.len()).max().unwrap();
        assert_eq!(majority_count, 3);
    }

    // ── C0: closed quorums are authoritative (#2907 F-1) ───────────────────────────────────────

    use std::sync::atomic::{AtomicU64, Ordering};

    const T0: u64 = 1_800_000_000_000;
    const WINDOW: u64 = 30_000;

    fn clocked_manager() -> (ResultQuorumManager, Arc<AtomicU64>) {
        let now = Arc::new(AtomicU64::new(T0));
        let source = Arc::clone(&now);
        let manager = ResultQuorumManager::with_clock(
            VerificationConfig {
                collection_window_ms: WINDOW,
                ..VerificationConfig::default()
            },
            Arc::new(move || source.load(Ordering::SeqCst)),
        );
        (manager, now)
    }

    fn two_of_two() -> TaskVerification {
        TaskVerification {
            value: TaskValue::Medium,
            required_executors: 2,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        }
    }

    fn closure_of(manager: &ResultQuorumManager, hash: &TaskHash) -> QuorumClosure {
        match manager.record(hash).unwrap() {
            QuorumRecord::Closed(closure) => closure,
            other => panic!("expected a closed quorum, got {other:?}"),
        }
    }

    /// Every way a quorum closes, and that a closed quorum refuses every later result with the
    /// typed `QuorumClosed` error, never `TaskNotFound`.
    #[test]
    fn closed_quorums_refuse_later_results_with_a_typed_error() {
        let (manager, now) = clocked_manager();
        let (a, b, c) = (
            TestExecutor::new(),
            TestExecutor::new(),
            TestExecutor::new(),
        );

        let accepted = [1u8; 32];
        let divergent = [2u8; 32];
        let timed_out = [3u8; 32];
        let expired = [4u8; 32];
        let cancelled = [5u8; 32];
        for hash in [accepted, divergent, timed_out, expired, cancelled] {
            manager.register_task(hash, two_of_two()).unwrap();
        }
        assert_eq!(
            manager.record(&[9u8; 32]).unwrap(),
            QuorumRecord::NotRegistered
        );
        assert_eq!(
            manager.record(&accepted).unwrap(),
            QuorumRecord::Collecting {
                received: 0,
                required: 2
            }
        );

        manager.submit(a.signed_result(accepted, b"x")).unwrap();
        let decision = manager.submit(b.signed_result(accepted, b"x")).unwrap();
        assert!(matches!(decision, QuorumDecision::Accepted { .. }));

        manager.submit(a.signed_result(divergent, b"x")).unwrap();
        let decision = manager.submit(b.signed_result(divergent, b"y")).unwrap();
        assert!(matches!(decision, QuorumDecision::Divergent { .. }));

        manager.cancel_task(&cancelled).unwrap();

        // Past the deadline, inside the skew tolerance: one result times the collection out.
        now.store(T0 + WINDOW + CLOCK_SKEW_TOLERANCE_MS / 2, Ordering::SeqCst);
        let decision = manager.submit(a.signed_result(timed_out, b"x")).unwrap();
        assert!(matches!(decision, QuorumDecision::TimedOut { .. }));
        assert_eq!(manager.cleanup_expired().unwrap(), vec![expired]);

        assert!(matches!(
            closure_of(&manager, &accepted),
            QuorumClosure::Accepted { .. }
        ));
        for (hash, rejection) in [
            (divergent, QuorumRejection::Divergent),
            (timed_out, QuorumRejection::TimedOut),
            (expired, QuorumRejection::Expired),
            (cancelled, QuorumRejection::Cancelled),
        ] {
            assert_eq!(
                closure_of(&manager, &hash),
                QuorumClosure::Rejected(rejection)
            );
        }

        for hash in [accepted, divergent, timed_out, expired, cancelled] {
            let late = manager.submit(c.signed_result(hash, b"x"));
            assert!(
                matches!(late, Err(ComputeError::QuorumClosed { .. })),
                "a result after the quorum closed must be refused as QuorumClosed, got {late:?}"
            );
            assert!(matches!(
                manager.register_task(hash, two_of_two()),
                Err(ComputeError::QuorumClosed { .. })
            ));
        }
        assert_eq!(manager.active_count().unwrap(), 0);
        assert_eq!(manager.closed_count().unwrap(), 5);
    }

    /// A result the aggregator refuses changes nothing: the quorum keeps collecting.
    #[test]
    fn a_refused_result_leaves_the_quorum_collecting() {
        let (manager, _now) = clocked_manager();
        let a = TestExecutor::new();
        let hash = [6u8; 32];
        manager.register_task(hash, two_of_two()).unwrap();
        let first = a.signed_result(hash, b"x");
        manager.submit(first.clone()).unwrap();

        assert!(matches!(
            manager.submit(first),
            Err(ComputeError::InvalidResult(_))
        ));
        assert_eq!(
            manager.record(&hash).unwrap(),
            QuorumRecord::Collecting {
                received: 1,
                required: 2
            }
        );
    }

    /// Cancellation and expiry cannot override an acceptance: a closed quorum keeps its closure.
    #[test]
    fn an_accepted_quorum_stays_accepted() {
        let (manager, now) = clocked_manager();
        let (a, b) = (TestExecutor::new(), TestExecutor::new());
        let hash = [7u8; 32];
        manager.register_task(hash, two_of_two()).unwrap();

        // The second result lands after the deadline, inside the tolerance, and completes the
        // quorum: consensus, even though the sweep would now consider the collection expired.
        manager.submit(a.signed_result(hash, b"x")).unwrap();
        now.store(T0 + WINDOW + 1, Ordering::SeqCst);
        let decision = manager.submit(b.signed_result(hash, b"x")).unwrap();
        assert!(matches!(decision, QuorumDecision::Accepted { .. }));

        assert!(manager.cleanup_expired().unwrap().is_empty());
        manager.cancel_task(&hash).unwrap();
        assert!(matches!(
            closure_of(&manager, &hash),
            QuorumClosure::Accepted { .. }
        ));
    }

    /// Closed quorums do not hold active verification capacity, and are released only by
    /// retiring the task.
    #[test]
    fn closed_quorums_do_not_consume_active_capacity() {
        let (manager, _now) = clocked_manager();
        let hash = |i: usize| {
            let mut h = [0u8; 32];
            h[..8].copy_from_slice(&(i as u64).to_be_bytes());
            h
        };
        for i in 0..MAX_CONCURRENT_VERIFICATIONS {
            manager.register_task(hash(i), two_of_two()).unwrap();
        }
        assert!(manager
            .register_task(hash(MAX_CONCURRENT_VERIFICATIONS), two_of_two())
            .is_err());

        for i in 0..MAX_CONCURRENT_VERIFICATIONS {
            manager.cancel_task(&hash(i)).unwrap();
        }
        assert_eq!(manager.active_count().unwrap(), 0);
        assert_eq!(
            manager.closed_count().unwrap(),
            MAX_CONCURRENT_VERIFICATIONS
        );
        manager
            .register_task(hash(MAX_CONCURRENT_VERIFICATIONS), two_of_two())
            .unwrap();

        // Retiring a task is the only thing that forgets its closure.
        manager.retire_task(&hash(0)).unwrap();
        assert_eq!(
            manager.record(&hash(0)).unwrap(),
            QuorumRecord::NotRegistered
        );
        assert_eq!(
            manager.closed_count().unwrap(),
            MAX_CONCURRENT_VERIFICATIONS - 1
        );
    }
}
