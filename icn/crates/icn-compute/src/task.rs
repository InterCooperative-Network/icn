//! Task management and tracking.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ComputeError;
use crate::types::{ComputeResult, ComputeTask, TaskHash};

/// Status of a task in the system
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task submitted, waiting for executor
    Pending,
    /// Task claimed by an executor
    Claimed { executor: String, claimed_at: u64 },
    /// Task completed
    Completed { result: Box<ComputeResult> },
    /// Task failed or timed out
    Failed { reason: String },
    /// Task cancelled by submitter
    Cancelled { cancelled_at: u64, reason: String },
}

/// The final states of a task. Once a task reaches one, its status never changes again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalState {
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TerminalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TerminalState::Completed => "completed",
            TerminalState::Failed => "failed",
            TerminalState::Cancelled => "cancelled",
        })
    }
}

impl TaskStatus {
    /// The terminal state this status is in, or `None` while the task is still open.
    pub fn terminal_state(&self) -> Option<TerminalState> {
        match self {
            TaskStatus::Pending | TaskStatus::Claimed { .. } => None,
            TaskStatus::Completed { .. } => Some(TerminalState::Completed),
            TaskStatus::Failed { .. } => Some(TerminalState::Failed),
            TaskStatus::Cancelled { .. } => Some(TerminalState::Cancelled),
        }
    }
}

/// Manages task lifecycle
pub struct TaskManager {
    /// Tasks by hash
    tasks: HashMap<TaskHash, ComputeTask>,
    /// Task status by hash
    status: HashMap<TaskHash, TaskStatus>,
    /// Maximum pending tasks
    max_pending: usize,
}

impl TaskManager {
    /// Create a new task manager
    pub fn new(max_pending: usize) -> Self {
        Self {
            tasks: HashMap::new(),
            status: HashMap::new(),
            max_pending,
        }
    }

    /// Submit a new task.
    ///
    /// A hash this manager already holds, in any state, is refused with
    /// [`ComputeError::DuplicateTask`] and nothing changes: a resubmission can neither reset a
    /// claimed task nor reopen a terminal one (EX-019).
    pub fn submit(&mut self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        let hash = task.hash();
        if self.tasks.contains_key(&hash) {
            return Err(ComputeError::DuplicateTask {
                task: hex::encode(hash),
            });
        }
        if self.pending_count() >= self.max_pending {
            return Err(ComputeError::Internal("too many pending tasks".into()));
        }

        self.tasks.insert(hash, task);
        self.status.insert(hash, TaskStatus::Pending);
        Ok(hash)
    }

    /// Get a task by hash
    pub fn get(&self, hash: &TaskHash) -> Option<&ComputeTask> {
        self.tasks.get(hash)
    }

    /// Get task status
    pub fn status(&self, hash: &TaskHash) -> Option<&TaskStatus> {
        self.status.get(hash)
    }

    /// Claim a task for execution.
    ///
    /// Only a `Pending` task can be claimed. The refusals are typed so a caller can tell
    /// them apart: `TaskNotFound` (unknown task), `TaskAlreadyClaimed` (duplicate claim),
    /// `TaskTerminal` (completed, failed or cancelled). A refusal changes nothing.
    pub fn claim(&mut self, hash: &TaskHash, executor: String) -> Result<(), ComputeError> {
        self.ensure_open(hash)?;
        let status = self
            .status
            .get(hash)
            .ok_or_else(|| ComputeError::TaskNotFound(hex::encode(hash)))?;

        match status {
            TaskStatus::Pending => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_millis() as u64;
                self.status.insert(
                    *hash,
                    TaskStatus::Claimed {
                        executor,
                        claimed_at: now,
                    },
                );
                Ok(())
            }
            TaskStatus::Claimed { executor: e, .. } => {
                Err(ComputeError::TaskAlreadyClaimed(e.clone()))
            }
            // Unreachable after `ensure_open`; kept fail-closed rather than panicking.
            _ => Err(ComputeError::Internal("task not in pending state".into())),
        }
    }

    /// Refuse a transition out of a terminal state with [`ComputeError::TaskTerminal`].
    fn ensure_open(&self, hash: &TaskHash) -> Result<(), ComputeError> {
        match self.status.get(hash).and_then(TaskStatus::terminal_state) {
            Some(state) => Err(ComputeError::TaskTerminal {
                task: hex::encode(hash),
                state,
            }),
            None => Ok(()),
        }
    }

    /// Record task completion.
    ///
    /// Only an open (pending or claimed) task completes. A completed, failed or cancelled task
    /// is final: the result is refused with [`ComputeError::TaskTerminal`] and the stored status,
    /// including an accepted result, is unchanged (EX-019).
    pub fn complete(&mut self, result: ComputeResult) -> Result<(), ComputeError> {
        let hash = result.task_hash;
        if !self.tasks.contains_key(&hash) {
            return Err(ComputeError::TaskNotFound(hex::encode(hash)));
        }
        self.ensure_open(&hash)?;
        self.status.insert(
            hash,
            TaskStatus::Completed {
                result: Box::new(result),
            },
        );
        Ok(())
    }

    /// Mark task as failed.
    ///
    /// Like [`TaskManager::complete`], refuses a terminal task with
    /// [`ComputeError::TaskTerminal`] and changes nothing (EX-019).
    pub fn fail(&mut self, hash: &TaskHash, reason: String) -> Result<(), ComputeError> {
        if !self.tasks.contains_key(hash) {
            return Err(ComputeError::TaskNotFound(hex::encode(hash)));
        }
        self.ensure_open(hash)?;
        self.status.insert(*hash, TaskStatus::Failed { reason });
        Ok(())
    }

    /// Cancel a task (only if pending or claimed)
    pub fn cancel(
        &mut self,
        hash: &TaskHash,
        requester: &str,
        reason: String,
    ) -> Result<(), ComputeError> {
        let task = self
            .tasks
            .get(hash)
            .ok_or_else(|| ComputeError::TaskNotFound(hex::encode(hash)))?;

        // Only the submitter can cancel their own tasks
        if task.submitter != requester {
            return Err(ComputeError::Internal(
                "Only task submitter can cancel the task".into(),
            ));
        }

        let status = self
            .status
            .get(hash)
            .ok_or_else(|| ComputeError::TaskNotFound(hex::encode(hash)))?;

        // Can only cancel pending or claimed tasks
        match status {
            TaskStatus::Pending | TaskStatus::Claimed { .. } => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_millis() as u64;
                self.status.insert(
                    *hash,
                    TaskStatus::Cancelled {
                        cancelled_at: now,
                        reason,
                    },
                );
                Ok(())
            }
            TaskStatus::Completed { .. } => Err(ComputeError::Internal(
                "Cannot cancel completed task".into(),
            )),
            TaskStatus::Failed { .. } => {
                Err(ComputeError::Internal("Cannot cancel failed task".into()))
            }
            TaskStatus::Cancelled { .. } => {
                Err(ComputeError::Internal("Task already cancelled".into()))
            }
        }
    }

    /// Get pending tasks
    pub fn pending(&self) -> impl Iterator<Item = (&TaskHash, &ComputeTask)> {
        self.tasks
            .iter()
            .filter(|(h, _)| matches!(self.status.get(*h), Some(TaskStatus::Pending)))
    }

    /// Get pending tasks sorted by priority (highest priority first)
    pub fn pending_by_priority(&self) -> Vec<(TaskHash, &ComputeTask)> {
        let mut pending: Vec<_> = self
            .tasks
            .iter()
            .filter(|(h, _)| matches!(self.status.get(*h), Some(TaskStatus::Pending)))
            .map(|(h, task)| (*h, task))
            .collect();

        // Sort by priority descending (Critical > High > Normal > Low)
        // Then by created_at ascending (older tasks first) as tiebreaker
        pending.sort_by(|(_, a), (_, b)| match b.priority.cmp(&a.priority) {
            std::cmp::Ordering::Equal => a.created_at.cmp(&b.created_at),
            other => other,
        });

        pending
    }

    /// Count of pending tasks
    pub fn pending_count(&self) -> usize {
        self.status
            .values()
            .filter(|s| matches!(s, TaskStatus::Pending))
            .count()
    }

    /// Find tasks that have exceeded their deadline
    pub fn find_timed_out(&self, now: u64) -> Vec<(TaskHash, Option<String>)> {
        self.tasks
            .iter()
            .filter_map(|(hash, task)| {
                // Check if task has a deadline and it has passed
                if let Some(deadline) = task.deadline {
                    if deadline <= now {
                        // Only return pending or claimed tasks
                        match self.status.get(hash) {
                            Some(TaskStatus::Pending) => Some((*hash, None)),
                            Some(TaskStatus::Claimed { executor, .. }) => {
                                Some((*hash, Some(executor.clone())))
                            }
                            _ => None, // Already completed/failed/cancelled
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove old completed/failed/cancelled tasks
    pub fn cleanup(&mut self, max_age_ms: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_millis() as u64;

        let to_remove: Vec<_> = self
            .tasks
            .iter()
            .filter(|(h, t)| {
                let age = now.saturating_sub(t.created_at);
                age > max_age_ms
                    && matches!(
                        self.status.get(*h),
                        Some(TaskStatus::Completed { .. })
                            | Some(TaskStatus::Failed { .. })
                            | Some(TaskStatus::Cancelled { .. })
                    )
            })
            .map(|(h, _)| *h)
            .collect();

        for hash in to_remove {
            self.tasks.remove(&hash);
            self.status.remove(&hash);
        }
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        DeterminismClass, ExecutionOutcome, ExecutorCapability, FuelLimit, PrivacyClass, TaskCode,
    };

    fn make_task(id: &str) -> ComputeTask {
        ComputeTask {
            id: id.into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::Ccl("return 1".into()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            // E1: Workload manifest fields
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
            scope: Default::default(),
        }
    }

    #[test]
    fn test_submit_and_get() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task.clone()).unwrap();

        assert!(mgr.get(&hash).is_some());
        assert!(matches!(mgr.status(&hash), Some(TaskStatus::Pending)));
    }

    #[test]
    fn test_claim_task() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        mgr.claim(&hash, "did:icn:bob".into()).unwrap();

        // Verify task is in claimed state
        let status = mgr.status(&hash).expect("Task should exist");
        assert!(
            matches!(&status, TaskStatus::Claimed { .. }),
            "Expected Claimed status, got: {status:?}"
        );

        // Verify correct executor
        if let TaskStatus::Claimed { executor, .. } = status {
            assert_eq!(executor, "did:icn:bob");
        }
    }

    #[test]
    fn test_double_claim_fails() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        mgr.claim(&hash, "did:icn:bob".into()).unwrap();
        let result = mgr.claim(&hash, "did:icn:charlie".into());

        assert!(matches!(result, Err(ComputeError::TaskAlreadyClaimed(_))));
    }

    #[test]
    fn test_complete_task() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        let result = ComputeResult {
            task_hash: hash,
            task_id: "task-1".into(),
            executor: "did:icn:bob".into(),
            outcome: ExecutionOutcome::Success(vec![42]),
            fuel_used: 100,
            duration_ms: 50,
            completed_at: 2000,
            signature: vec![],
        };

        mgr.complete(result).unwrap();
        assert!(matches!(
            mgr.status(&hash),
            Some(TaskStatus::Completed { .. })
        ));
    }

    #[test]
    fn test_pending_count() {
        let mut mgr = TaskManager::new(100);
        mgr.submit(make_task("task-1")).unwrap();
        mgr.submit(make_task("task-2")).unwrap();

        assert_eq!(mgr.pending_count(), 2);
    }

    #[test]
    fn test_cancel_pending_task() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        mgr.cancel(&hash, "did:icn:alice", "User requested cancellation".into())
            .unwrap();

        let status = mgr.status(&hash).expect("Task should exist");
        assert!(
            matches!(status, TaskStatus::Cancelled { .. }),
            "Expected Cancelled status, got: {status:?}"
        );
    }

    #[test]
    fn test_cancel_claimed_task() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        mgr.claim(&hash, "did:icn:bob".into()).unwrap();
        mgr.cancel(&hash, "did:icn:alice", "Changed my mind".into())
            .unwrap();

        assert!(matches!(
            mgr.status(&hash),
            Some(TaskStatus::Cancelled { .. })
        ));
    }

    #[test]
    fn test_cancel_wrong_submitter() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        let result = mgr.cancel(&hash, "did:icn:charlie", "Unauthorized".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("submitter"));
    }

    #[test]
    fn test_cannot_cancel_completed_task() {
        let mut mgr = TaskManager::new(100);
        let task = make_task("task-1");
        let hash = mgr.submit(task).unwrap();

        let result = ComputeResult {
            task_hash: hash,
            task_id: "task-1".into(),
            executor: "did:icn:bob".into(),
            outcome: ExecutionOutcome::Success(vec![42]),
            fuel_used: 100,
            duration_ms: 50,
            completed_at: 2000,
            signature: vec![],
        };
        mgr.complete(result).unwrap();

        let cancel_result = mgr.cancel(&hash, "did:icn:alice", "Too late".into());
        assert!(cancel_result.is_err());
        assert!(cancel_result
            .unwrap_err()
            .to_string()
            .contains("Cannot cancel completed"));
    }

    #[test]
    fn test_cancel_nonexistent_task() {
        let mut mgr = TaskManager::new(100);
        let fake_hash = [0u8; 32];

        let result = mgr.cancel(&fake_hash, "did:icn:alice", "No such task".into());
        assert!(matches!(result, Err(ComputeError::TaskNotFound(_))));
    }

    // ── EX-019: terminal states are final, a duplicate submission changes nothing ──────────

    fn result_by(
        hash: TaskHash,
        executor: &str,
        outcome: ExecutionOutcome,
        fuel: u64,
    ) -> ComputeResult {
        ComputeResult {
            task_hash: hash,
            task_id: "task-1".into(),
            executor: executor.into(),
            outcome,
            fuel_used: fuel,
            duration_ms: 50,
            completed_at: 2000,
            signature: vec![],
        }
    }

    /// One manager per state: Pending, Claimed, and each terminal state reached through the
    /// manager's own transitions.
    fn manager_in(state: &str) -> (TaskManager, TaskHash) {
        let mut mgr = TaskManager::new(100);
        let hash = mgr.submit(make_task("task-1")).unwrap();
        match state {
            "pending" => {}
            "claimed" => mgr.claim(&hash, "did:icn:bob".into()).unwrap(),
            "completed" => mgr
                .complete(result_by(
                    hash,
                    "did:icn:bob",
                    ExecutionOutcome::Success(vec![42]),
                    100,
                ))
                .unwrap(),
            "failed" => mgr.fail(&hash, "Deadline exceeded".into()).unwrap(),
            "cancelled" => mgr.cancel(&hash, "did:icn:alice", "stop".into()).unwrap(),
            other => panic!("unknown fixture state {other}"),
        }
        (mgr, hash)
    }

    const TERMINAL: [(&str, TerminalState); 3] = [
        ("completed", TerminalState::Completed),
        ("failed", TerminalState::Failed),
        ("cancelled", TerminalState::Cancelled),
    ];

    #[test]
    fn complete_refuses_every_terminal_state_and_changes_nothing() {
        for (name, terminal) in TERMINAL {
            // An identical replay, another executor, another outcome, another fuel figure.
            let late = [
                result_by(
                    [0; 32],
                    "did:icn:bob",
                    ExecutionOutcome::Success(vec![42]),
                    100,
                ),
                result_by(
                    [0; 32],
                    "did:icn:mallory",
                    ExecutionOutcome::Success(vec![42]),
                    100,
                ),
                result_by(
                    [0; 32],
                    "did:icn:bob",
                    ExecutionOutcome::Failed("late".into()),
                    100,
                ),
                result_by(
                    [0; 32],
                    "did:icn:bob",
                    ExecutionOutcome::Success(vec![42]),
                    90_000,
                ),
            ];
            for mut result in late {
                let (mut mgr, hash) = manager_in(name);
                result.task_hash = hash;
                let before = mgr.status(&hash).cloned();

                let refused = mgr.complete(result);

                assert!(
                    matches!(
                        &refused,
                        Err(ComputeError::TaskTerminal { state, .. }) if *state == terminal
                    ),
                    "{name}: expected TaskTerminal({terminal:?}), got {refused:?}"
                );
                assert_eq!(mgr.status(&hash).cloned(), before, "{name}: status changed");
            }
        }
    }

    #[test]
    fn fail_refuses_every_terminal_state_and_changes_nothing() {
        for (name, terminal) in TERMINAL {
            let (mut mgr, hash) = manager_in(name);
            let before = mgr.status(&hash).cloned();

            let refused = mgr.fail(&hash, "Deadline exceeded".into());

            assert!(
                matches!(
                    &refused,
                    Err(ComputeError::TaskTerminal { state, .. }) if *state == terminal
                ),
                "{name}: expected TaskTerminal({terminal:?}), got {refused:?}"
            );
            assert_eq!(mgr.status(&hash).cloned(), before, "{name}: status changed");
        }
    }

    /// #2913 BLOCKER 1: a claim refusal is typed by cause and changes nothing, so the actor
    /// can make claim accounting conditional on a real Pending -> Claimed transition.
    #[test]
    fn claim_refusals_are_typed_and_change_nothing() {
        for (name, terminal) in TERMINAL {
            let (mut mgr, hash) = manager_in(name);
            let before = mgr.status(&hash).cloned();
            let refused = mgr.claim(&hash, "did:icn:carol".into());
            assert!(
                matches!(
                    &refused,
                    Err(ComputeError::TaskTerminal { state, .. }) if *state == terminal
                ),
                "{name}: expected TaskTerminal({terminal:?}), got {refused:?}"
            );
            assert_eq!(mgr.status(&hash).cloned(), before, "{name}: status changed");
        }

        let (mut mgr, hash) = manager_in("claimed");
        let before = mgr.status(&hash).cloned();
        let refused = mgr.claim(&hash, "did:icn:carol".into());
        assert!(
            matches!(&refused, Err(ComputeError::TaskAlreadyClaimed(e)) if e == "did:icn:bob"),
            "duplicate claim: got {refused:?}"
        );
        assert_eq!(
            mgr.status(&hash).cloned(),
            before,
            "duplicate claim changed status"
        );

        let mut mgr = TaskManager::new(100);
        assert!(matches!(
            mgr.claim(&[7; 32], "did:icn:carol".into()),
            Err(ComputeError::TaskNotFound(_))
        ));
        assert!(mgr.status(&[7; 32]).is_none());

        let (mut mgr, hash) = manager_in("pending");
        mgr.claim(&hash, "did:icn:carol".into())
            .unwrap_or_else(|e| panic!("pending claim refused: {e}"));
        assert!(matches!(
            mgr.status(&hash),
            Some(TaskStatus::Claimed { executor, .. }) if executor == "did:icn:carol"
        ));
    }

    #[test]
    fn complete_and_fail_still_close_an_open_task() {
        for open in ["pending", "claimed"] {
            let (mut mgr, hash) = manager_in(open);
            mgr.complete(result_by(
                hash,
                "did:icn:bob",
                ExecutionOutcome::Success(vec![42]),
                100,
            ))
            .unwrap_or_else(|e| panic!("{open}: complete refused: {e}"));
            assert!(matches!(
                mgr.status(&hash),
                Some(TaskStatus::Completed { .. })
            ));

            let (mut mgr, hash) = manager_in(open);
            mgr.fail(&hash, "Deadline exceeded".into())
                .unwrap_or_else(|e| panic!("{open}: fail refused: {e}"));
            assert!(matches!(mgr.status(&hash), Some(TaskStatus::Failed { .. })));
        }
    }

    #[test]
    fn submit_refuses_a_held_hash_in_every_state_and_changes_nothing() {
        for state in ["pending", "claimed", "completed", "failed", "cancelled"] {
            let (mut mgr, hash) = manager_in(state);
            let before = mgr.status(&hash).cloned();
            let pending_before = mgr.pending_count();

            let refused = mgr.submit(make_task("task-1"));

            assert!(
                matches!(&refused, Err(ComputeError::DuplicateTask { .. })),
                "{state}: expected DuplicateTask, got {refused:?}"
            );
            assert_eq!(
                mgr.status(&hash).cloned(),
                before,
                "{state}: status changed"
            );
            assert_eq!(
                mgr.pending_count(),
                pending_before,
                "{state}: pending count changed"
            );
        }
    }

    #[test]
    fn duplicate_submission_is_refused_before_the_pending_limit() {
        // A full queue must not mask the duplicate refusal with "too many pending tasks".
        let mut mgr = TaskManager::new(1);
        let hash = mgr.submit(make_task("task-1")).unwrap();
        let refused = mgr.submit(make_task("task-1"));
        assert!(
            matches!(&refused, Err(ComputeError::DuplicateTask { task }) if *task == hex::encode(hash)),
            "got {refused:?}"
        );
    }

    #[test]
    fn test_cleanup_includes_cancelled() {
        let mut mgr = TaskManager::new(100);
        let mut task = make_task("task-1");
        task.created_at = 1000; // Old task
        let hash = mgr.submit(task).unwrap();

        mgr.cancel(&hash, "did:icn:alice", "Cleanup test".into())
            .unwrap();

        // Cleanup tasks older than 1000ms
        mgr.cleanup(1000);

        // Task should be removed
        assert!(mgr.get(&hash).is_none());
        assert!(mgr.status(&hash).is_none());
    }
}
