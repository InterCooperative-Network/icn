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

    /// Submit a new task
    pub fn submit(&mut self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        if self.pending_count() >= self.max_pending {
            return Err(ComputeError::Internal("too many pending tasks".into()));
        }

        let hash = task.hash();
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

    /// Claim a task for execution
    pub fn claim(&mut self, hash: &TaskHash, executor: String) -> Result<(), ComputeError> {
        let status = self
            .status
            .get(hash)
            .ok_or_else(|| ComputeError::TaskNotFound(hex::encode(hash)))?;

        match status {
            TaskStatus::Pending => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
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
            _ => Err(ComputeError::Internal("task not in pending state".into())),
        }
    }

    /// Record task completion
    pub fn complete(&mut self, result: ComputeResult) -> Result<(), ComputeError> {
        let hash = result.task_hash;
        if !self.tasks.contains_key(&hash) {
            return Err(ComputeError::TaskNotFound(hex::encode(hash)));
        }
        self.status
            .insert(hash, TaskStatus::Completed { result: Box::new(result) });
        Ok(())
    }

    /// Mark task as failed
    pub fn fail(&mut self, hash: &TaskHash, reason: String) -> Result<(), ComputeError> {
        if !self.tasks.contains_key(hash) {
            return Err(ComputeError::TaskNotFound(hex::encode(hash)));
        }
        self.status.insert(*hash, TaskStatus::Failed { reason });
        Ok(())
    }

    /// Cancel a task (only if pending or claimed)
    pub fn cancel(&mut self, hash: &TaskHash, requester: &str, reason: String) -> Result<(), ComputeError> {
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
                    .unwrap()
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
            TaskStatus::Completed { .. } => {
                Err(ComputeError::Internal("Cannot cancel completed task".into()))
            }
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
        self.tasks.iter().filter(|(h, _)| {
            matches!(self.status.get(*h), Some(TaskStatus::Pending))
        })
    }

    /// Count of pending tasks
    pub fn pending_count(&self) -> usize {
        self.status
            .values()
            .filter(|s| matches!(s, TaskStatus::Pending))
            .count()
    }

    /// Remove old completed/failed/cancelled tasks
    pub fn cleanup(&mut self, max_age_ms: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
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
    use crate::types::{ExecutionOutcome, ExecutorCapability, FuelLimit, TaskCode};

    fn make_task(id: &str) -> ComputeTask {
        ComputeTask {
            id: id.into(),
            submitter: "did:icn:alice".into(),
            code: TaskCode::Ccl("return 1".into()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
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
            "Expected Claimed status, got: {:?}", status
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
        assert!(matches!(mgr.status(&hash), Some(TaskStatus::Completed { .. })));
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
            "Expected Cancelled status, got: {:?}", status
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

        assert!(matches!(mgr.status(&hash), Some(TaskStatus::Cancelled { .. })));
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
        assert!(cancel_result.unwrap_err().to_string().contains("Cannot cancel completed"));
    }

    #[test]
    fn test_cancel_nonexistent_task() {
        let mut mgr = TaskManager::new(100);
        let fake_hash = [0u8; 32];

        let result = mgr.cancel(&fake_hash, "did:icn:alice", "No such task".into());
        assert!(matches!(result, Err(ComputeError::TaskNotFound(_))));
    }

    #[test]
    fn test_cleanup_includes_cancelled() {
        let mut mgr = TaskManager::new(100);
        let mut task = make_task("task-1");
        task.created_at = 1000; // Old task
        let hash = mgr.submit(task).unwrap();

        mgr.cancel(&hash, "did:icn:alice", "Cleanup test".into()).unwrap();

        // Cleanup tasks older than 1000ms
        mgr.cleanup(1000);

        // Task should be removed
        assert!(mgr.get(&hash).is_none());
        assert!(mgr.status(&hash).is_none());
    }
}
