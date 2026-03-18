//! Commands sent to the ComputeActor.

use crate::error::ComputeError;
use crate::policy::{CoopSchedulingPolicy, UsageRecord};
use crate::task::TaskStatus;
use crate::types::{ComputeMessage, ComputeTask, TaskHash};

/// Commands sent to the ComputeActor
pub(crate) enum ComputeCommand {
    Submit {
        task: Box<ComputeTask>,
        resp: tokio::sync::oneshot::Sender<Result<TaskHash, ComputeError>>,
    },
    Status {
        hash: TaskHash,
        resp: tokio::sync::oneshot::Sender<Option<TaskStatus>>,
    },
    Cancel {
        hash: TaskHash,
        requester: String,
        reason: String,
        resp: tokio::sync::oneshot::Sender<Result<(), ComputeError>>,
    },
    GossipMessage(Box<ComputeMessage>),
    // Policy management commands (Phase 16E)
    SetPolicy {
        policy: CoopSchedulingPolicy,
        resp: tokio::sync::oneshot::Sender<Result<(), ComputeError>>,
    },
    GetPolicy {
        coop_id: String,
        resp: tokio::sync::oneshot::Sender<Option<CoopSchedulingPolicy>>,
    },
    ListPolicies {
        resp: tokio::sync::oneshot::Sender<Vec<CoopSchedulingPolicy>>,
    },
    RemovePolicy {
        coop_id: String,
        resp: tokio::sync::oneshot::Sender<Option<CoopSchedulingPolicy>>,
    },
    GetUsage {
        coop_id: String,
        member_did: String,
        resp: tokio::sync::oneshot::Sender<Result<UsageRecord, ComputeError>>,
    },
    ListCoopUsage {
        coop_id: String,
        resp: tokio::sync::oneshot::Sender<Result<Vec<UsageRecord>, ComputeError>>,
    },
    // Dispute resolution commands (Phase 18 Week 4)
    FileDispute {
        task_hash: TaskHash,
        executor: String,
        challenger: String,
        expected_result: icn_ccl::Value,
        actual_result: icn_ccl::Value,
        resp: tokio::sync::oneshot::Sender<Result<icn_ccl::DisputeId, ComputeError>>,
    },
    GetDisputeStatus {
        dispute_id: icn_ccl::DisputeId,
        resp: tokio::sync::oneshot::Sender<Option<icn_ccl::DisputeStatus>>,
    },
}
