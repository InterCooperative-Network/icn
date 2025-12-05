//! ICN CCL - Cooperative Contract Language runtime
//!
//! This crate implements a domain-specific language (DSL) for expressing cooperative
//! agreements with built-in safety features:
//!
//! - **Deterministic execution**: Same inputs always produce same outputs
//! - **Capability-based security**: Explicit permissions for ledger access
//! - **Fuel metering**: Bounded execution prevents infinite loops
//! - **Not Turing-complete**: Safe subset of operations
//!
//! ## Example: TimeBank Contract
//!
//! ```rust
//! use icn_ccl::{Contract, Rule, Stmt, Expr, Value, BinOp};
//! use icn_identity::KeyPair;
//!
//! // Create a time bank contract
//! let alice = KeyPair::generate().unwrap().did().clone();
//! let bob = KeyPair::generate().unwrap().did().clone();
//!
//! let contract = Contract::new("TimeBank".to_string())
//!     .add_participant(alice.clone())
//!     .add_participant(bob.clone())
//!     .with_currency("hours".to_string())
//!     .add_rule(
//!         Rule::new("record_service".to_string())
//!             .add_param("recipient".to_string())
//!             .add_param("hours".to_string())
//!             .add_require(Expr::BinOp {
//!                 op: BinOp::Gt,
//!                 left: Box::new(Expr::Var("hours".to_string())),
//!                 right: Box::new(Expr::Literal(Value::Int(0))),
//!             })
//!             .add_stmt(Stmt::LedgerTransfer {
//!                 from: Expr::Var("sender".to_string()),
//!                 to: Expr::Var("recipient".to_string()),
//!                 amount: Expr::Var("hours".to_string()),
//!                 currency: Expr::Literal(Value::String("hours".to_string())),
//!             })
//!     );
//! ```

pub mod actor;
pub mod ast;
pub mod disputes;
pub mod interpreter;
pub mod messages;
pub mod registry;
pub mod runtime;
pub mod types;

// Re-export commonly used types
pub use actor::{ContractActor, GossipCallback, CONTRACTS_DEPLOY_TOPIC};
pub use ast::{BinOp, Contract, Expr, Rule, Stmt, UnOp};
pub use disputes::{
    Dispute, DisputeActor, DisputeActorHandle, DisputeConfig, DisputeEvidence, DisputeId,
    DisputeMessage, DisputeOutcome, DisputeReason, DisputeResolutionSystem, DisputeStats,
    DisputeStatus, MisbehaviorCallback, TOPIC_DISPUTES_FILE,
};
pub use interpreter::Interpreter;
pub use messages::{
    ContractDeploymentMessage, ContractExecutionRequest, ContractExecutionResponse,
};
pub use registry::{
    compute_hash, ContentHash, ContractMetadata, ContractRegistry, RegistryError, RegistryStats,
};
pub use runtime::{ContractInfo, ContractRuntime};
pub use types::{
    Capability, ContractInstallation, ContractState, ExecutionContext, ExecutionResult,
    LedgerOperation, Value,
};

#[cfg(test)]
mod example_contract_tests {
    use super::*;

    #[test]
    fn test_timebank_contract_deserializes() {
        let json = include_str!("../../../../examples/contracts/timebank.ccl.json");
        let contract: Contract =
            serde_json::from_str(json).expect("Failed to deserialize timebank.ccl.json");
        contract
            .validate()
            .expect("TimeBank contract validation failed");
        assert_eq!(contract.name, "TimeBank");
        assert_eq!(contract.participants.len(), 2);
        assert_eq!(contract.rules.len(), 1);
        assert_eq!(contract.currency, Some("hours".to_string()));
    }

    #[test]
    fn test_simple_agreement_contract_deserializes() {
        let json = include_str!("../../../../examples/contracts/simple-agreement.ccl.json");
        let contract: Contract =
            serde_json::from_str(json).expect("Failed to deserialize simple-agreement.ccl.json");
        contract
            .validate()
            .expect("SimpleAgreement contract validation failed");
        assert_eq!(contract.name, "SimpleAgreement");
        assert_eq!(contract.participants.len(), 1);
        assert_eq!(contract.rules.len(), 1);
    }

    #[test]
    fn test_calculator_contract_deserializes() {
        let json = include_str!("../../../../examples/contracts/calculator.ccl.json");
        let contract: Contract =
            serde_json::from_str(json).expect("Failed to deserialize calculator.ccl.json");
        contract
            .validate()
            .expect("Calculator contract validation failed");
        assert_eq!(contract.name, "Calculator");
        assert_eq!(contract.participants.len(), 1);
        assert_eq!(contract.rules.len(), 1);
        assert_eq!(contract.state_vars.len(), 0);
    }
}
