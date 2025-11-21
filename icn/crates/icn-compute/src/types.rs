//! Core types for the compute layer.

use serde::{Deserialize, Serialize};

/// Unique task identifier (blake3 hash of task content)
pub type TaskHash = [u8; 32];

/// Human-readable task ID
pub type TaskId = String;

/// Fuel limit for execution (prevents infinite loops)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuelLimit(pub u64);

impl Default for FuelLimit {
    fn default() -> Self {
        Self(10_000) // 10k operations default
    }
}

/// Capabilities an executor advertises
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutorCapability {
    /// Can execute CCL contracts
    Ccl,
    /// Can execute WASM modules (future)
    Wasm,
    /// Custom capability
    Custom(String),
}

/// A compute task to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeTask {
    /// Unique task ID (set by submitter)
    pub id: TaskId,
    /// DID of task submitter
    pub submitter: String,
    /// Task code (CCL source or WASM bytes reference)
    pub code: TaskCode,
    /// Input data (serialized)
    pub inputs: Vec<u8>,
    /// Maximum fuel for execution
    pub fuel_limit: FuelLimit,
    /// Required capabilities
    pub required_capabilities: Vec<ExecutorCapability>,
    /// Timestamp (Unix millis)
    pub created_at: u64,
    /// Optional deadline (Unix millis)
    pub deadline: Option<u64>,
}

impl ComputeTask {
    /// Compute the task hash
    pub fn hash(&self) -> TaskHash {
        let bytes = bincode::serialize(self).unwrap_or_default();
        *blake3::hash(&bytes).as_bytes()
    }
}

/// Task code format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskCode {
    /// CCL source code
    Ccl(String),
    /// Reference to WASM module by hash
    WasmRef(TaskHash),
    /// Inline WASM bytes (small modules only)
    WasmInline(Vec<u8>),
}

/// Result of task execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeResult {
    /// Hash of the executed task
    pub task_hash: TaskHash,
    /// Task ID for correlation
    pub task_id: TaskId,
    /// DID of executor
    pub executor: String,
    /// Execution outcome
    pub outcome: ExecutionOutcome,
    /// Fuel consumed
    pub fuel_used: u64,
    /// Execution duration (millis)
    pub duration_ms: u64,
    /// Timestamp
    pub completed_at: u64,
    /// Ed25519 signature over result
    pub signature: Vec<u8>,
}

/// Outcome of task execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    /// Successful execution with output
    Success(Vec<u8>),
    /// Execution failed with error
    Failed(String),
    /// Ran out of fuel
    OutOfFuel,
    /// Deadline exceeded
    Timeout,
}

/// Messages sent over gossip for compute coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeMessage {
    /// New task submitted
    TaskSubmitted(ComputeTask),
    /// Executor claims a task
    TaskClaimed {
        task_hash: TaskHash,
        executor: String,
    },
    /// Task result published
    TaskResult(ComputeResult),
    /// Executor announces capabilities
    ExecutorAnnounce {
        executor: String,
        capabilities: Vec<ExecutorCapability>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_hash_deterministic() {
        let task = ComputeTask {
            id: "test-1".into(),
            submitter: "did:icn:alice".into(),
            code: TaskCode::Ccl("return 42".into()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            created_at: 1000,
            deadline: None,
        };

        let hash1 = task.hash();
        let hash2 = task.hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_fuel_limit_default() {
        assert_eq!(FuelLimit::default().0, 10_000);
    }

    #[test]
    fn test_message_serialization() {
        let msg = ComputeMessage::TaskClaimed {
            task_hash: [0u8; 32],
            executor: "did:icn:bob".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ComputeMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ComputeMessage::TaskClaimed { executor, .. } => {
                assert_eq!(executor, "did:icn:bob");
            }
            _ => panic!("wrong variant"),
        }
    }
}
