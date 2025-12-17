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
pub mod charter_rules;
pub mod charter_validator;
pub mod disputes;
pub mod interpreter;
pub mod messages;
pub mod registry;
pub mod runtime;
pub mod types;

// Re-export commonly used types
pub use actor::{ContractActor, GossipCallback, CONTRACTS_DEPLOY_TOPIC};
pub use ast::{BinOp, Contract, Expr, Rule, Stmt, UnOp};
pub use charter_rules::{CharterRule, CharterRuleSet, ValidationResult};
pub use charter_validator::CharterValidator;
pub use disputes::{
    Dispute, DisputeActor, DisputeActorHandle, DisputeConfig, DisputeEvidence,
    DisputeGossipCallback, DisputeId, DisputeMessage, DisputeOutcome, DisputeReason,
    DisputeResolutionSystem, DisputeStats, DisputeStatus, MediatorInfo, MisbehaviorCallback,
    OffenderRecord, PenaltyConfig, TrustCallback, TrustPenaltyCallback, TOPIC_DISPUTES_FILE,
    TOPIC_DISPUTES_RESOLVED,
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

    #[test]
    fn test_infrastructure_credit_protocol_contract() {
        let json = include_str!("../../../../contracts/protocol/infrastructure-credit-v1.ccl.json");
        let contract: Contract = serde_json::from_str(json)
            .expect("Failed to deserialize infrastructure-credit-v1.ccl.json");
        contract
            .validate()
            .expect("InfrastructureCreditProtocol contract validation failed");

        assert_eq!(contract.name, "InfrastructureCreditProtocol");
        assert_eq!(contract.participants.len(), 1);
        assert_eq!(contract.currency, Some("hours".to_string()));
        assert_eq!(contract.state_vars.len(), 5); // compute_rate, storage_rate, bandwidth_rate, uptime_rate, network_treasury
        assert_eq!(contract.rules.len(), 7); // calculate_credits, issue_credits, 4x update_*_rate, get_rates

        // Verify specific rules exist
        assert!(contract.get_rule("calculate_credits").is_some());
        assert!(contract.get_rule("issue_credits").is_some());
        assert!(contract.get_rule("update_compute_rate").is_some());
        assert!(contract.get_rule("get_rates").is_some());
    }

    #[test]
    fn test_fuel_allocation_protocol_contract() {
        let json = include_str!("../../../../contracts/protocol/fuel-allocation-v1.ccl.json");
        let contract: Contract =
            serde_json::from_str(json).expect("Failed to deserialize fuel-allocation-v1.ccl.json");
        contract
            .validate()
            .expect("FuelAllocationProtocol contract validation failed");

        assert_eq!(contract.name, "FuelAllocationProtocol");
        assert_eq!(contract.participants.len(), 1);
        assert_eq!(contract.currency, Some("fuel".to_string()));
        assert_eq!(contract.state_vars.len(), 9); // base, min_civic, min_regen, trust_mult, contrib_mult, 4 costs
        assert_eq!(contract.rules.len(), 10); // calculate_allowance, get_operation_cost, 6 update_*, 2 get_*

        // Verify specific rules exist
        assert!(contract.get_rule("calculate_allowance").is_some());
        assert!(contract.get_rule("get_operation_cost").is_some());
        assert!(contract.get_rule("update_message_cost").is_some());
        assert!(contract.get_rule("get_all_costs").is_some());
        assert!(contract.get_rule("get_multipliers").is_some());
    }
}
