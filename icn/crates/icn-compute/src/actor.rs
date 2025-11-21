//! Compute actor for distributed task execution.

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::error::ComputeError;
use crate::executor::{Executor, LocalExecutor};
use crate::task::{TaskManager, TaskStatus};
use crate::types::{ComputeMessage, ComputeResult, ComputeTask, TaskHash};
use crate::{MIN_TRUST_EXECUTE, MIN_TRUST_SUBMIT};

/// Callback for sending compute messages via gossip
pub type SendCallback = Arc<dyn Fn(ComputeMessage) + Send + Sync>;

/// Callback for looking up trust scores
pub type TrustCallback = Arc<dyn Fn(&str) -> f64 + Send + Sync>;

/// Handle for interacting with the ComputeActor
#[derive(Clone)]
pub struct ComputeHandle {
    tx: mpsc::Sender<ComputeCommand>,
}

impl ComputeHandle {
    /// Submit a task for distributed execution
    pub async fn submit(&self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Submit { task, resp: resp_tx })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Get task status
    pub async fn status(&self, hash: TaskHash) -> Result<Option<TaskStatus>, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Status { hash, resp: resp_tx })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))
    }

    /// Handle incoming gossip message
    pub async fn handle_gossip(&self, msg: ComputeMessage) -> Result<(), ComputeError> {
        self.tx
            .send(ComputeCommand::GossipMessage(msg))
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))
    }
}

/// Commands sent to the ComputeActor
enum ComputeCommand {
    Submit {
        task: ComputeTask,
        resp: tokio::sync::oneshot::Sender<Result<TaskHash, ComputeError>>,
    },
    Status {
        hash: TaskHash,
        resp: tokio::sync::oneshot::Sender<Option<TaskStatus>>,
    },
    GossipMessage(ComputeMessage),
}

/// Actor managing distributed compute tasks
pub struct ComputeActor {
    /// Our DID
    own_did: String,
    /// Task manager
    task_manager: Arc<Mutex<TaskManager>>,
    /// Local executor
    executor: Arc<LocalExecutor>,
    /// Callback to send gossip messages
    send_callback: Option<SendCallback>,
    /// Callback to lookup trust scores
    trust_callback: TrustCallback,
    /// Signing key for results (placeholder)
    #[allow(dead_code)]
    signing_key: Vec<u8>,
}

impl ComputeActor {
    /// Create a new compute actor
    pub fn new(own_did: String, trust_callback: TrustCallback) -> Self {
        Self {
            own_did,
            task_manager: Arc::new(Mutex::new(TaskManager::default())),
            executor: Arc::new(LocalExecutor::new()),
            send_callback: None,
            trust_callback,
            signing_key: vec![],
        }
    }

    /// Set the callback for sending gossip messages
    pub fn set_send_callback(&mut self, cb: SendCallback) {
        self.send_callback = Some(cb);
    }

    /// Spawn the actor and return a handle
    pub fn spawn(self) -> ComputeHandle {
        let (tx, mut rx) = mpsc::channel::<ComputeCommand>(256);

        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    ComputeCommand::Submit { task, resp } => {
                        let result = self.handle_submit(task).await;
                        let _ = resp.send(result);
                    }
                    ComputeCommand::Status { hash, resp } => {
                        let status = self.task_manager.lock().await.status(&hash).cloned();
                        let _ = resp.send(status);
                    }
                    ComputeCommand::GossipMessage(msg) => {
                        if let Err(e) = self.handle_message(msg).await {
                            tracing::warn!("compute message error: {}", e);
                        }
                    }
                }
            }
        });

        ComputeHandle { tx }
    }

    /// Handle task submission
    async fn handle_submit(&self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        // Check submitter trust
        let trust = (self.trust_callback)(&task.submitter);
        if trust < MIN_TRUST_SUBMIT {
            return Err(ComputeError::InsufficientTrust {
                required: MIN_TRUST_SUBMIT,
                actual: trust,
            });
        }

        // Add to local task manager
        let hash = self.task_manager.lock().await.submit(task.clone())?;

        // Broadcast to network
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskSubmitted(task));
        }

        Ok(hash)
    }

    /// Handle incoming compute message
    async fn handle_message(&self, msg: ComputeMessage) -> Result<(), ComputeError> {
        match msg {
            ComputeMessage::TaskSubmitted(task) => {
                self.on_task_submitted(task).await
            }
            ComputeMessage::TaskClaimed { task_hash, executor } => {
                self.on_task_claimed(task_hash, executor).await
            }
            ComputeMessage::TaskResult(result) => {
                self.on_task_result(result).await
            }
            ComputeMessage::ExecutorAnnounce { .. } => {
                // TODO: Track available executors
                Ok(())
            }
        }
    }

    /// Handle received task submission
    async fn on_task_submitted(&self, task: ComputeTask) -> Result<(), ComputeError> {
        // Check if we can execute
        let our_trust = (self.trust_callback)(&self.own_did);
        if our_trust < MIN_TRUST_EXECUTE {
            return Ok(()); // We're not trusted enough to execute
        }

        // Check if we have required capabilities
        if !self.executor.can_execute(&task) {
            return Ok(()); // Can't execute this task
        }

        // Store task
        let hash = self.task_manager.lock().await.submit(task.clone())?;

        // Claim it
        self.task_manager
            .lock()
            .await
            .claim(&hash, self.own_did.clone())?;

        // Broadcast claim
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskClaimed {
                task_hash: hash,
                executor: self.own_did.clone(),
            });
        }

        // Execute
        let result = self
            .executor
            .execute_task(&task, &self.own_did, &self.signing_key)?;

        // Record completion
        self.task_manager.lock().await.complete(result.clone())?;

        // Broadcast result
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskResult(result));
        }

        Ok(())
    }

    /// Handle task claimed by another executor
    async fn on_task_claimed(
        &self,
        task_hash: TaskHash,
        executor: String,
    ) -> Result<(), ComputeError> {
        // Just record the claim if we know about the task
        let mut mgr = self.task_manager.lock().await;
        if mgr.get(&task_hash).is_some() {
            let _ = mgr.claim(&task_hash, executor);
        }
        Ok(())
    }

    /// Handle task result
    async fn on_task_result(&self, result: ComputeResult) -> Result<(), ComputeError> {
        // TODO: Verify signature
        // TODO: Compare with other results for consensus

        let mut mgr = self.task_manager.lock().await;
        if mgr.get(&result.task_hash).is_some() {
            mgr.complete(result)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExecutorCapability, FuelLimit, TaskCode};

    fn simple_ccl() -> String {
        r#"{
            "name": "SimpleReturn",
            "participants": ["did:icn:alice"],
            "currency": null,
            "state_vars": [],
            "rules": [{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{ "Return": { "value": { "Literal": { "Int": 42 } } } }]
            }],
            "triggers": []
        }"#.to_string()
    }

    fn make_task(id: &str, submitter: &str) -> ComputeTask {
        ComputeTask {
            id: id.into(),
            submitter: submitter.into(),
            code: TaskCode::Ccl(simple_ccl()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            created_at: 1000,
            deadline: None,
        }
    }

    #[tokio::test]
    async fn test_submit_task() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.5);
        let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        let handle = actor.spawn();

        let task = make_task("task-1", "did:icn:alice");
        let hash = handle.submit(task).await.unwrap();

        let status = handle.status(hash).await.unwrap();
        assert!(status.is_some());
    }

    #[tokio::test]
    async fn test_submit_low_trust_rejected() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.05); // Below MIN_TRUST_SUBMIT
        let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        let handle = actor.spawn();

        let task = make_task("task-1", "did:icn:untrusted");
        let result = handle.submit(task).await;

        assert!(matches!(result, Err(ComputeError::InsufficientTrust { .. })));
    }

    #[tokio::test]
    async fn test_gossip_message_handling() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.5);
        let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        let handle = actor.spawn();

        let task = make_task("task-1", "did:icn:alice");
        let msg = ComputeMessage::TaskSubmitted(task.clone());

        handle.handle_gossip(msg).await.unwrap();

        // Task should be stored and executed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let hash = task.hash();
        let status = handle.status(hash).await.unwrap();
        assert!(matches!(status, Some(TaskStatus::Completed { .. })));
    }
}
