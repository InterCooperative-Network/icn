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

/// Trust edge information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEdgeInfo {
    pub target_did: String,
    pub score: f64,
    pub labels: Vec<String>,
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

/// Code type for compute tasks
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodeType {
    /// CCL contract (default)
    #[default]
    Ccl,
    /// WebAssembly module
    Wasm,
}

/// Resource requirements for compute tasks (RPC representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProfileRequest {
    /// CPU cores required (e.g., 0.5 = half a core, 2.0 = two cores)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<f64>,
    /// Memory required in megabytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u64>,
    /// Temporary storage required in megabytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_mb: Option<u64>,
    /// Network bandwidth required in Mbps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mbps: Option<f64>,
}

/// Request to submit a compute task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTaskRequest {
    pub task_id: String,
    /// CCL contract JSON (for code_type: ccl)
    #[serde(default)]
    pub code: Option<String>,
    /// WASM bytecode as base64 (for code_type: wasm)
    #[serde(default)]
    pub wasm_bytes: Option<String>,
    /// Code type: "ccl" (default) or "wasm"
    #[serde(default)]
    pub code_type: CodeType,
    #[serde(default)]
    pub inputs: serde_json::Value,
    #[serde(default = "default_fuel_limit")]
    pub fuel_limit: u64,
    /// Task priority: "low", "normal", "high", or "critical" (default: "normal")
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_rate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_currency: Option<String>,
    /// Cooperative ID for task attribution (optional, can also be derived from JWT claims)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coop_id: Option<String>,
    /// Resource requirements for the task
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_profile: Option<ResourceProfileRequest>,
}

fn default_priority() -> String {
    "normal".to_string()
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

// ============================================================================
// Recovery types (for social recovery of lost identities)
// ============================================================================

/// Recovery event information (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEventInfo {
    pub id: String,
    pub old_did: String,
    pub new_did: String,
    pub initiated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finalized_at: Option<u64>,
    pub threshold: usize,
    pub delay_period: u64,
    pub status: String, // "pending", "delayed", "ready", "finalized", "cancelled"
    pub attestations_count: usize,
    pub progress_summary: String,
}

/// Recovery attestation information (simplified for RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttestationInfo {
    pub trustee: String,
    pub verification_method: String,
    pub timestamp: u64,
}

/// Request to initiate recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiateRecoveryRequest {
    pub old_did: String,
    pub threshold: usize,
    pub delay_period: u64,
}

/// Response from initiating recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiateRecoveryResponse {
    pub recovery_id: String,
    pub status: String,
}

/// Request to add recovery attestation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttestRequest {
    pub recovery_id: String,
    pub verification_method: String,
}

/// Request to cancel recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRecoveryRequest {
    pub recovery_id: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_response_success() {
        let result = serde_json::json!({"key": "value"});
        let response = RpcResponse::success(42, result.clone());

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, 42);
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap(), result);
    }

    #[test]
    fn test_rpc_response_error() {
        let response = RpcResponse::error(123, -32600, "Invalid Request".to_string());

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, 123);
        assert!(response.result.is_none());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid Request");
    }

    #[test]
    fn test_rpc_request_serialization() {
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test.method".to_string(),
            params: serde_json::json!({"param1": 42}),
            id: 1,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: RpcRequest = serde_json::from_str(&json).unwrap();

        // Verify all fields round-trip correctly
        assert_eq!(parsed.jsonrpc, "2.0");
        assert_eq!(parsed.method, "test.method");
        assert_eq!(parsed.params["param1"], 42);
        assert_eq!(parsed.id, 1);
    }

    #[test]
    fn test_peer_info_serialization() {
        let peer = PeerInfo {
            did: "did:icn:zPeerKeyABCDEFGHJ".to_string(), // Valid base58btc format
            addr: "127.0.0.1:9000".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&peer).unwrap();
        let parsed: PeerInfo = serde_json::from_str(&json).unwrap();

        // Verify round-trip
        assert_eq!(parsed.did, peer.did);
        assert_eq!(parsed.addr, peer.addr);
        assert_eq!(parsed.version, peer.version);
    }

    #[test]
    fn test_code_type_default() {
        let code_type: CodeType = Default::default();
        assert!(matches!(code_type, CodeType::Ccl));
    }

    #[test]
    fn test_code_type_serialization() {
        let ccl = CodeType::Ccl;
        let wasm = CodeType::Wasm;

        assert_eq!(serde_json::to_string(&ccl).unwrap(), "\"ccl\"");
        assert_eq!(serde_json::to_string(&wasm).unwrap(), "\"wasm\"");
    }

    #[test]
    fn test_submit_task_request_defaults() {
        let json = r#"{"task_id": "task-1"}"#;
        let request: SubmitTaskRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.task_id, "task-1");
        assert_eq!(request.fuel_limit, 10_000); // default
        assert_eq!(request.priority, "normal"); // default
        assert!(matches!(request.code_type, CodeType::Ccl)); // default
    }

    #[test]
    fn test_membership_config_serialization() {
        let static_list = MembershipConfigInfo::StaticList {
            members: vec!["did:icn:zAliceKeyABCDEF".to_string(), "did:icn:zBobKeyABCDEFG".to_string()],
        };
        let trust_threshold = MembershipConfigInfo::TrustThreshold { threshold: 0.5 };

        let static_json = serde_json::to_string(&static_list).unwrap();
        let trust_json = serde_json::to_string(&trust_threshold).unwrap();

        assert!(static_json.contains("\"type\":\"static_list\""));
        assert!(trust_json.contains("\"type\":\"trust_threshold\""));

        // Verify round-trip deserialization
        let parsed_static: MembershipConfigInfo = serde_json::from_str(&static_json).unwrap();
        let parsed_trust: MembershipConfigInfo = serde_json::from_str(&trust_json).unwrap();

        match parsed_static {
            MembershipConfigInfo::StaticList { members } => {
                assert_eq!(members.len(), 2);
                assert_eq!(members[0], "did:icn:zAliceKeyABCDEF");
            }
            _ => panic!("Expected StaticList variant"),
        }

        match parsed_trust {
            MembershipConfigInfo::TrustThreshold { threshold } => {
                assert!((threshold - 0.5).abs() < f64::EPSILON);
            }
            _ => panic!("Expected TrustThreshold variant"),
        }
    }

    #[test]
    fn test_proposal_payload_serialization() {
        let text = ProposalPayloadInfo::Text {
            body: "Test proposal".to_string(),
        };
        let budget = ProposalPayloadInfo::Budget {
            amount: 1000,
            currency: "credits".to_string(),
            recipient: "did:icn:zBobKeyABCDEFGH".to_string(),
            purpose: "Development".to_string(),
        };

        let text_json = serde_json::to_string(&text).unwrap();
        let budget_json = serde_json::to_string(&budget).unwrap();

        assert!(text_json.contains("\"type\":\"text\""));
        assert!(budget_json.contains("\"type\":\"budget\""));
        assert!(budget_json.contains("\"amount\":1000"));

        // Verify round-trip deserialization
        let parsed_text: ProposalPayloadInfo = serde_json::from_str(&text_json).unwrap();
        let parsed_budget: ProposalPayloadInfo = serde_json::from_str(&budget_json).unwrap();

        match parsed_text {
            ProposalPayloadInfo::Text { body } => {
                assert_eq!(body, "Test proposal");
            }
            _ => panic!("Expected Text variant"),
        }

        match parsed_budget {
            ProposalPayloadInfo::Budget { amount, currency, recipient, purpose } => {
                assert_eq!(amount, 1000);
                assert_eq!(currency, "credits");
                assert_eq!(recipient, "did:icn:zBobKeyABCDEFGH");
                assert_eq!(purpose, "Development");
            }
            _ => panic!("Expected Budget variant"),
        }
    }

    #[test]
    fn test_network_stats_serialization() {
        let stats = NetworkStats {
            peers_discovered: 10,
            connections_active: 5,
            connections_total: 100,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let parsed: NetworkStats = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.peers_discovered, 10);
        assert_eq!(parsed.connections_active, 5);
        assert_eq!(parsed.connections_total, 100);
    }

    #[test]
    fn test_task_result_info_optional_fields() {
        // With all fields
        let full_result = TaskResultInfo {
            outcome: "success".to_string(),
            output: Some(serde_json::json!({"result": 42})),
            error: None,
            fuel_used: 500,
            duration_ms: 100,
        };

        let json = serde_json::to_string(&full_result).unwrap();
        assert!(json.contains("\"outcome\":\"success\""));
        assert!(json.contains("\"output\"")); // Verify Some field is present
        assert!(!json.contains("\"error\"")); // skip_serializing_if = "Option::is_none"

        // Verify round-trip
        let parsed: TaskResultInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.outcome, "success");
        assert_eq!(parsed.output.as_ref().unwrap()["result"], 42);
        assert!(parsed.error.is_none());

        // With error
        let error_result = TaskResultInfo {
            outcome: "failed".to_string(),
            output: None,
            error: Some("Out of memory".to_string()),
            fuel_used: 100,
            duration_ms: 50,
        };

        let error_json = serde_json::to_string(&error_result).unwrap();
        assert!(error_json.contains("Out of memory"));
        assert!(!error_json.contains("\"output\"")); // None field should be skipped
    }

    #[test]
    fn test_recovery_event_info_serialization() {
        let event = RecoveryEventInfo {
            id: "recovery-1".to_string(),
            old_did: "did:icn:old".to_string(),
            new_did: "did:icn:new".to_string(),
            initiated_at: 1700000000,
            finalized_at: None,
            threshold: 3,
            delay_period: 86400,
            status: "pending".to_string(),
            attestations_count: 1,
            progress_summary: "1/3 attestations".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: RecoveryEventInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "recovery-1");
        assert_eq!(parsed.threshold, 3);
        assert!(parsed.finalized_at.is_none());
    }

    #[test]
    fn test_ledger_entry_serialization() {
        let entry = LedgerEntry {
            hash: "abc123".to_string(),
            timestamp: 1700000000,
            author: "did:icn:alice".to_string(),
            accounts: vec![
                LedgerAccountDelta {
                    account_id: "alice".to_string(),
                    currency: "credits".to_string(),
                    debit: Some(100),
                    credit: None,
                },
                LedgerAccountDelta {
                    account_id: "bob".to_string(),
                    currency: "credits".to_string(),
                    debit: None,
                    credit: Some(100),
                },
            ],
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: LedgerEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.accounts.len(), 2);
        assert_eq!(parsed.accounts[0].debit, Some(100));
        assert_eq!(parsed.accounts[1].credit, Some(100));
    }

    #[test]
    fn test_resource_profile_optional_fields() {
        let json = r#"{"cpu_cores": 2.0}"#;
        let profile: ResourceProfileRequest = serde_json::from_str(json).unwrap();

        assert_eq!(profile.cpu_cores, Some(2.0));
        assert!(profile.memory_mb.is_none());
        assert!(profile.storage_mb.is_none());
        assert!(profile.network_mbps.is_none());

        // Verify skip_serializing_if works
        let output = serde_json::to_string(&profile).unwrap();
        assert!(output.contains("cpu_cores"));
        assert!(!output.contains("memory_mb")); // None should be skipped
        assert!(!output.contains("storage_mb"));
        assert!(!output.contains("network_mbps"));
    }

    // Error case tests
    #[test]
    fn test_invalid_membership_config_type() {
        let json = r#"{"type": "invalid_type"}"#;
        let result: Result<MembershipConfigInfo, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_proposal_payload_type() {
        let json = r#"{"type": "unknown_payload"}"#;
        let result: Result<ProposalPayloadInfo, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_fields() {
        // RpcRequest missing required fields
        let json = r#"{"jsonrpc": "2.0"}"#;
        let result: Result<RpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());

        // SubmitTaskRequest missing task_id
        let json = r#"{"code": "some code"}"#;
        let result: Result<SubmitTaskRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_code_type_deserialization() {
        // Valid cases
        assert!(matches!(
            serde_json::from_str::<CodeType>(r#""ccl""#).unwrap(),
            CodeType::Ccl
        ));
        assert!(matches!(
            serde_json::from_str::<CodeType>(r#""wasm""#).unwrap(),
            CodeType::Wasm
        ));

        // Invalid case
        let result: Result<CodeType, _> = serde_json::from_str(r#""invalid""#);
        assert!(result.is_err());
    }
}
