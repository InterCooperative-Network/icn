//! RPC types and message definitions

use serde::{Deserialize, Serialize};

/// JSON-RPC request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
}

/// JSON-RPC response to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: u64,
}

/// RPC error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Network peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub did: String,
    pub addr: String,
    pub version: String,
}

/// Network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub peers_discovered: usize,
    pub connections_active: usize,
    pub connections_total: u64,
}

/// Network status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub running: bool,
    pub listen_addr: String,
}

/// Ledger balance for a specific account and currency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerBalance {
    pub account_id: String,
    pub currency: String,
    pub amount: i64,
}

/// Ledger entry (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub hash: String,
    pub timestamp: u64,
    pub author: String,
    pub accounts: Vec<LedgerAccountDelta>,
}

/// Account delta in a ledger entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerAccountDelta {
    pub account_id: String,
    pub currency: String,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}

/// Contract information (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInfo {
    pub code_hash: String,
    pub name: String,
    pub participants: Vec<String>,
    pub currency: Option<String>,
    pub rules: Vec<String>,
}

/// Contract execution response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractExecutionResponse {
    pub success: bool,
    pub fuel_consumed: u64,
    pub return_value: serde_json::Value,
}

/// Governance domain (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDomainInfo {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub profile: String,
    pub membership_type: String,
    pub params: GovernanceParamsInfo,
}

/// Governance parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceParamsInfo {
    pub quorum_percentage: u8,
    pub approval_threshold_percentage: u8,
    pub voting_period_seconds: u64,
}

/// Proposal information (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalInfo {
    pub id: String,
    pub domain_id: String,
    pub proposer: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opened_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<u64>,
}

/// Vote information (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteInfo {
    pub proposal_id: String,
    pub voter: String,
    pub choice: String,
    pub cast_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

// Governance write operation requests

/// Request to create a new governance domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDomainRequest {
    pub domain_id: String,
    pub name: String,
    pub profile: String,
    pub params: GovernanceParamsInfo,
    pub membership: MembershipConfigInfo,
}

/// Membership configuration for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MembershipConfigInfo {
    #[serde(rename = "static_list")]
    StaticList { members: Vec<String> },
    #[serde(rename = "trust_threshold")]
    TrustThreshold { threshold: f64 },
}

/// Request to create a new proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub payload: ProposalPayloadInfo,
}

/// Proposal payload for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProposalPayloadInfo {
    #[serde(rename = "text")]
    Text { body: String },
    #[serde(rename = "budget")]
    Budget {
        amount: i64,
        currency: String,
        recipient: String,
        purpose: String,
    },
    #[serde(rename = "config_change")]
    ConfigChange { new_config: String },
    #[serde(rename = "membership")]
    Membership { action: String, member: String },
}

/// Response from creating a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProposalResponse {
    pub proposal_id: String,
}

/// Request to open a proposal for voting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenProposalRequest {
    pub proposal_id: String,
    pub voting_period_seconds: u64,
}

/// Request to cast a vote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastVoteRequest {
    pub proposal_id: String,
    pub choice: String, // "for", "against", or "abstain"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Request to close a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseProposalRequest {
    pub proposal_id: String,
}

// Compute task types

/// Request to submit a compute task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTaskRequest {
    pub task_id: String,
    pub code: String, // CCL JSON
    #[serde(default)]
    pub inputs: serde_json::Value,
    #[serde(default = "default_fuel_limit")]
    pub fuel_limit: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_rate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_currency: Option<String>,
}

fn default_fuel_limit() -> u64 {
    10_000
}

/// Response from submitting a compute task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTaskResponse {
    pub task_hash: String,
}

/// Compute task status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusInfo {
    pub task_hash: String,
    pub status: String, // "pending", "claimed", "completed", "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResultInfo>,
}

/// Compute task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultInfo {
    pub outcome: String, // "success", "failed", "out_of_fuel"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub fuel_used: u64,
    pub duration_ms: u64,
}

impl RpcResponse {
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: u64, code: i32, message: String) -> Self {
        RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RpcError { code, message }),
            id,
        }
    }
}
