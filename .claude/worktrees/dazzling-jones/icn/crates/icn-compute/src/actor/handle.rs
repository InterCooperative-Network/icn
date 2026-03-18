//! Handle for interacting with the ComputeActor.

use tokio::sync::mpsc;

use crate::error::ComputeError;
use crate::policy::{CoopSchedulingPolicy, UsageRecord};
use crate::task::TaskStatus;
use crate::types::{ComputeMessage, ComputeTask, TaskHash};

use super::command::ComputeCommand;

/// Handle for interacting with the ComputeActor
#[derive(Clone)]
pub struct ComputeHandle {
    pub(crate) tx: mpsc::Sender<ComputeCommand>,
}

impl ComputeHandle {
    /// Submit a task for distributed execution
    pub async fn submit(&self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Submit {
                task: Box::new(task),
                resp: resp_tx,
            })
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
            .send(ComputeCommand::Status {
                hash,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))
    }

    /// Cancel a task
    pub async fn cancel_task(
        &self,
        hash: &TaskHash,
        requester: &str,
        reason: String,
    ) -> Result<(), ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Cancel {
                hash: *hash,
                requester: requester.to_string(),
                reason,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Handle incoming gossip message
    pub async fn handle_gossip(&self, msg: ComputeMessage) -> Result<(), ComputeError> {
        self.tx
            .send(ComputeCommand::GossipMessage(Box::new(msg)))
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))
    }

    // Policy management methods (Phase 16E)

    /// Set or update a cooperative scheduling policy
    pub async fn set_policy(&self, policy: CoopSchedulingPolicy) -> Result<(), ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::SetPolicy {
                policy,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Get policy for a cooperative
    pub async fn get_policy(&self, coop_id: &str) -> Option<CoopSchedulingPolicy> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::GetPolicy {
                coop_id: coop_id.to_string(),
                resp: resp_tx,
            })
            .await
            .ok()?;
        resp_rx.await.ok()?
    }

    /// List all policies
    pub async fn list_policies(&self) -> Vec<CoopSchedulingPolicy> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::ListPolicies { resp: resp_tx })
            .await
            .ok();
        resp_rx.await.unwrap_or_default()
    }

    /// Remove a policy
    pub async fn remove_policy(&self, coop_id: &str) -> Option<CoopSchedulingPolicy> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::RemovePolicy {
                coop_id: coop_id.to_string(),
                resp: resp_tx,
            })
            .await
            .ok()?;
        resp_rx.await.ok()?
    }

    /// Get usage record for a member in a cooperative
    pub async fn get_usage(
        &self,
        coop_id: &str,
        member_did: &str,
    ) -> Result<UsageRecord, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::GetUsage {
                coop_id: coop_id.to_string(),
                member_did: member_did.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// List all usage records for a cooperative
    pub async fn list_coop_usage(&self, coop_id: &str) -> Result<Vec<UsageRecord>, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::ListCoopUsage {
                coop_id: coop_id.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    // Dispute resolution methods (Phase 18 Week 4)

    /// File a dispute for a compute task result
    pub async fn file_dispute(
        &self,
        task_hash: TaskHash,
        executor: String,
        challenger: String,
        expected_result: icn_ccl::Value,
        actual_result: icn_ccl::Value,
    ) -> Result<icn_ccl::DisputeId, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::FileDispute {
                task_hash,
                executor,
                challenger,
                expected_result,
                actual_result,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Get dispute status by ID
    pub async fn get_dispute_status(
        &self,
        dispute_id: icn_ccl::DisputeId,
    ) -> Option<icn_ccl::DisputeStatus> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::GetDisputeStatus {
                dispute_id,
                resp: resp_tx,
            })
            .await
            .ok()?;
        resp_rx.await.ok()?
    }
}
