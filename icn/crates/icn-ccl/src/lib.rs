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

pub mod ast;
pub mod interpreter;
pub mod runtime;
pub mod types;

// Re-export commonly used types
pub use ast::{BinOp, Contract, Expr, Rule, Stmt, UnOp};
pub use interpreter::Interpreter;
pub use runtime::ContractRuntime;
pub use types::{
    Capability, ContractInstallation, ContractState, ExecutionContext, ExecutionResult,
    LedgerOperation, Value,
};
