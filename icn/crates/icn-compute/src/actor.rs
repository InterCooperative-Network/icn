//! Compute actor for distributed task execution.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::error::ComputeError;
use crate::executor::{Executor, LocalExecutor};
use crate::task::{TaskManager, TaskStatus};
use crate::types::{ComputeMessage, ComputeResult, ComputeTask, ExecutorCapability, TaskHash};
use crate::{MIN_TRUST_EXECUTE, MIN_TRUST_SUBMIT};

/// Information about an available executor
#[derive(Debug, Clone)]
struct ExecutorInfo {
    /// Executor's DID
    did: String,
    /// Capabilities this executor offers
    capabilities: Vec<ExecutorCapability>,
    /// Current trust score (used for future executor selection)
    #[allow(dead_code)]
    trust_score: f64,
    /// Last announcement timestamp (milliseconds since epoch, used for staleness detection)
    #[allow(dead_code)]
    last_seen: u64,
}

/// Consensus tracking for task results
#[derive(Debug, Clone)]
struct ResultConsensus {
    /// Task hash (used for tracking in HashMap)
    #[allow(dead_code)]
    task_hash: TaskHash,
    /// Results received from different executors
    results: Vec<ComputeResult>,
    /// Number of required confirmations (default: 1 for now)
    required: usize,
}

/// Callback for sending compute messages via gossip
pub type SendCallback = Arc<dyn Fn(ComputeMessage) + Send + Sync>;

/// Callback for looking up trust scores
pub type TrustCallback = Arc<dyn Fn(&str) -> f64 + Send + Sync>;

/// Payment settlement request
#[derive(Debug, Clone)]
pub struct PaymentRequest {
    /// Payer DID (task submitter)
    pub from: String,
    /// Payee DID (executor)
    pub to: String,
    /// Amount to pay
    pub amount: u64,
    /// Currency
    pub currency: String,
    /// Task ID for memo
    pub task_id: String,
}

/// Callback for settling payments via ledger
pub type PaymentCallback = Arc<dyn Fn(PaymentRequest) + Send + Sync>;

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
    /// Callback to settle payments
    payment_callback: Option<PaymentCallback>,
    /// Signing key for results (placeholder)
    #[allow(dead_code)]
    signing_key: Vec<u8>,
    /// Registry of available executors
    executor_registry: Arc<Mutex<HashMap<String, ExecutorInfo>>>,
    /// Pending consensus tracking for task results
    pending_consensus: Arc<Mutex<HashMap<TaskHash, ResultConsensus>>>,
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
            payment_callback: None,
            signing_key: vec![],
            executor_registry: Arc::new(Mutex::new(HashMap::new())),
            pending_consensus: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Set the callback for sending gossip messages
    pub fn set_send_callback(&mut self, cb: SendCallback) {
        self.send_callback = Some(cb);
    }

    /// Set the callback for settling payments
    pub fn set_payment_callback(&mut self, cb: PaymentCallback) {
        self.payment_callback = Some(cb);
    }

    /// Set the signing key for result signatures
    pub fn set_signing_key(&mut self, key: Vec<u8>) {
        self.signing_key = key;
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
            icn_obs::metrics::compute::tasks_rejected_trust_inc(&task.submitter, trust);
            return Err(ComputeError::InsufficientTrust {
                required: MIN_TRUST_SUBMIT,
                actual: trust,
            });
        }

        // Add to local task manager
        let hash = self.task_manager.lock().await.submit(task.clone())?;
        icn_obs::metrics::compute::tasks_submitted_inc();

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
            ComputeMessage::ExecutorAnnounce { executor, capabilities } => {
                self.on_executor_announce(executor, capabilities).await
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
        icn_obs::metrics::compute::tasks_claimed_inc();

        // Broadcast claim
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskClaimed {
                task_hash: hash,
                executor: self.own_did.clone(),
            });
        }

        // Execute
        let start = std::time::Instant::now();
        let result = self
            .executor
            .execute_task(&task, &self.own_did, &self.signing_key)?;
        let duration = start.elapsed().as_secs_f64();

        // Record metrics
        icn_obs::metrics::compute::task_duration_record(duration);
        icn_obs::metrics::compute::fuel_used_record(result.fuel_used);
        icn_obs::metrics::compute::fuel_total_add(result.fuel_used);

        match &result.outcome {
            crate::types::ExecutionOutcome::Success(_) => {
                icn_obs::metrics::compute::tasks_completed_inc("success");
            }
            crate::types::ExecutionOutcome::Failed(reason) => {
                icn_obs::metrics::compute::tasks_failed_inc(reason);
            }
            crate::types::ExecutionOutcome::OutOfFuel => {
                icn_obs::metrics::compute::tasks_out_of_fuel_inc();
                icn_obs::metrics::compute::tasks_completed_inc("out_of_fuel");
            }
            crate::types::ExecutionOutcome::Timeout => {
                icn_obs::metrics::compute::tasks_timeout_inc();
                icn_obs::metrics::compute::tasks_completed_inc("timeout");
            }
        }

        // Record completion
        self.task_manager.lock().await.complete(result.clone())?;

        // Settle payment if configured and execution succeeded
        if let crate::types::ExecutionOutcome::Success(_) = &result.outcome {
            if let (Some(rate), Some(ref payment_cb)) = (task.payment_rate, &self.payment_callback) {
                let amount = (result.fuel_used * rate) / 1000; // rate is per 1000 fuel
                if amount > 0 {
                    let currency = task.payment_currency.clone().unwrap_or_else(|| "credits".to_string());
                    payment_cb(PaymentRequest {
                        from: task.submitter.clone(),
                        to: self.own_did.clone(),
                        amount,
                        currency,
                        task_id: task.id.clone(),
                    });
                    icn_obs::metrics::compute::payments_settled_inc();
                    icn_obs::metrics::compute::payment_amount_add(amount);
                }
            }
        }

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

    /// Handle executor announcement
    async fn on_executor_announce(
        &self,
        did: String,
        capabilities: Vec<ExecutorCapability>,
    ) -> Result<(), ComputeError> {
        let trust_score = (self.trust_callback)(&did);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let info = ExecutorInfo {
            did: did.clone(),
            capabilities,
            trust_score,
            last_seen: now,
        };

        let mut registry = self.executor_registry.lock().await;
        registry.insert(did, info);
        icn_obs::metrics::compute::executors_available_set(registry.len() as f64);

        Ok(())
    }

    /// Get list of available executors with required capabilities
    #[allow(dead_code)]
    pub async fn find_executors(&self, required_caps: &[ExecutorCapability]) -> Vec<String> {
        let registry = self.executor_registry.lock().await;
        registry
            .values()
            .filter(|info| {
                // Check if executor has all required capabilities
                required_caps.iter().all(|cap| info.capabilities.contains(cap))
            })
            .map(|info| info.did.clone())
            .collect()
    }

    /// Handle task result with consensus checking
    async fn on_task_result(&self, result: ComputeResult) -> Result<(), ComputeError> {
        // Verify signature
        let executor_did: icn_identity::Did = result.executor.parse()
            .map_err(|e| {
                icn_obs::metrics::compute::signatures_invalid_inc("invalid_did");
                ComputeError::InvalidSignature(format!("Invalid executor DID: {}", e))
            })?;

        if let Err(e) = result.verify_signature(&executor_did) {
            icn_obs::metrics::compute::signatures_invalid_inc("verification_failed");
            return Err(e);
        }
        icn_obs::metrics::compute::signatures_verified_inc();

        // Consensus checking: For now, we require just 1 result (single-executor mode)
        // In the future, this can be extended to require multiple matching results
        let required_confirmations = 1;

        let mut consensus_map = self.pending_consensus.lock().await;
        let consensus = consensus_map
            .entry(result.task_hash)
            .or_insert_with(|| ResultConsensus {
                task_hash: result.task_hash,
                results: vec![],
                required: required_confirmations,
            });

        // Add this result if not already present (by executor)
        if !consensus.results.iter().any(|r| r.executor == result.executor) {
            consensus.results.push(result.clone());
        }

        // Check if we have enough confirmations
        if consensus.results.len() >= consensus.required {
            // For now, just accept the first result
            // Future: compare results and handle disagreements
            let accepted_result = consensus.results[0].clone();

            // Clean up consensus tracking
            consensus_map.remove(&result.task_hash);
            drop(consensus_map);

            // Complete the task
            let mut mgr = self.task_manager.lock().await;
            if mgr.get(&accepted_result.task_hash).is_some() {
                mgr.complete(accepted_result)?;
            }
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
            payment_rate: None,
            payment_currency: None,
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
        let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        // Set a valid signing key for result signatures
        actor.set_signing_key(vec![1u8; 32]);
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

    #[tokio::test]
    async fn test_executor_tracking() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);
        actor.set_signing_key(vec![1u8; 32]);
        let handle = actor.spawn();

        // Announce two executors with different capabilities
        let msg1 = ComputeMessage::ExecutorAnnounce {
            executor: "did:icn:executor1".into(),
            capabilities: vec![ExecutorCapability::Ccl],
        };
        let msg2 = ComputeMessage::ExecutorAnnounce {
            executor: "did:icn:executor2".into(),
            capabilities: vec![ExecutorCapability::Ccl, ExecutorCapability::Wasm],
        };

        handle.handle_gossip(msg1).await.unwrap();
        handle.handle_gossip(msg2).await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Both executors should be tracked in the registry
        // (We can't directly access the registry from here, but the announcements succeeded)
        // This test validates the message handling works without errors
    }

    #[tokio::test]
    async fn test_consensus_single_result() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);
        actor.set_signing_key(vec![1u8; 32]);
        let handle = actor.spawn();

        // Submit a task
        let task = make_task("consensus-task", "did:icn:alice");
        let task_hash = handle.submit(task.clone()).await.unwrap();

        // Generate a valid keypair and result
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().as_str();
        let signing_key = keypair.to_signing_key_bytes();

        let mut result = crate::types::ComputeResult {
            task_hash,
            task_id: task.id.clone(),
            executor: executor_did.to_string(),
            outcome: crate::types::ExecutionOutcome::Success(b"42".to_vec()),
            fuel_used: 100,
            duration_ms: 10,
            completed_at: 1000,
            signature: vec![],
        };

        // Sign the result
        let ed25519_key = ed25519_dalek::SigningKey::from_bytes(&signing_key);
        result.sign(&ed25519_key);

        // Submit result via gossip
        let msg = ComputeMessage::TaskResult(result);
        handle.handle_gossip(msg).await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Task should be completed after consensus (1 result required)
        let status = handle.status(task_hash).await.unwrap();
        assert!(matches!(status, Some(TaskStatus::Completed { .. })));
    }
}
