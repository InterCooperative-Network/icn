//! Declarative milestone completion gate backed by CCL expression evaluation.
//!
//! A [`Milestone`](crate::program::Milestone) may optionally carry a
//! `completion_gate: Option<icn_ccl::Expr>`.  When the gate is present,
//! [`evaluate_milestone_gate`] is called during the completion attempt.
//! If the expression evaluates to false the completion is rejected with a
//! clear error; if it evaluates to true (or there is no gate) the
//! completion proceeds as normal.
//!
//! ## Evaluation context
//!
//! The expression runs against a small flat environment derived from the
//! milestone record at the time of the completion attempt.  The following
//! named variables are available:
//!
//! | Variable          | Type  | Meaning                                     |
//! |-------------------|-------|---------------------------------------------|
//! | `criteria_count`  | `Int` | `completion_criteria.len()` at eval time    |
//! | `phase_index`     | `Int` | milestone `phase_index` (0-based ordinal)   |
//!
//! **What is NOT available (deferred):**
//! - Action-item completion counts (requires a separate store query)
//! - Trust scores or ledger balances (no CCL capabilities granted)
//! - Proposal or vote state
//! - Cross-milestone dependency checks
//!
//! Only pure CCL boolean predicates composed from the variables above and
//! literal constants are supported in this first slice.  Ledger operations,
//! state writes, and function calls are not permitted.
//!
//! ## Example gate expressions
//!
//! ```rust
//! use icn_ccl::{Expr, BinOp};
//! use icn_ccl::types::Value;
//!
//! // Require at least 2 completion criteria to have been declared.
//! let gate = Expr::BinOp {
//!     op: BinOp::Ge,
//!     left: Box::new(Expr::Var("criteria_count".into())),
//!     right: Box::new(Expr::Literal(Value::Int(2))),
//! };
//! ```
//!
//! ## Gate semantics
//!
//! - **No gate** → completion proceeds unchanged.
//! - **Gate → `true`** → completion proceeds.
//! - **Gate → `false`** → `Err(GateBlocked { message })` is returned; the
//!   milestone is NOT modified.
//! - **Gate evaluation error** (syntax, fuel, unknown variable) →
//!   `Err(GateError { message })` is returned; the milestone is NOT modified.

use icn_ccl::{
    types::{ContractState, ExecutionContext, Value},
    CclError, Contract, Expr, Interpreter, Rule,
};
use icn_identity::Did;
use std::collections::HashMap;
use thiserror::Error;

/// Maximum CCL fuel allowed for gate evaluation.
///
/// Gate expressions are simple boolean predicates.  1 000 fuel is very
/// conservative — a complex `a && b && c` tree uses fewer than 10 steps.
/// This prevents runaway evaluation while leaving plenty of headroom for
/// realistic expressions.
const GATE_FUEL: u64 = 1_000;

/// Variables available inside a milestone gate expression.
///
/// Built from the milestone record immediately before the completion
/// attempt.  All fields are passed as named CCL variables.
#[derive(Debug, Clone)]
pub struct MilestoneGateContext {
    /// Number of strings in `completion_criteria`.
    pub criteria_count: i64,
    /// Ordinal phase index of the milestone (0-based).
    pub phase_index: i64,
    /// DID of the actor requesting completion (used as CCL caller).
    pub actor: Did,
    /// Current Unix timestamp (seconds).
    pub now: u64,
}

impl MilestoneGateContext {
    fn to_args(&self) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        map.insert("criteria_count".into(), Value::Int(self.criteria_count));
        map.insert("phase_index".into(), Value::Int(self.phase_index));
        map
    }
}

/// Errors produced by milestone gate evaluation.
#[derive(Debug, Error)]
pub enum MilestoneGateError {
    /// The gate expression evaluated to `false`.
    #[error("milestone gate blocked completion: {message}")]
    Blocked { message: String },

    /// The gate expression could not be evaluated (syntax, fuel, etc.).
    #[error("milestone gate evaluation failed: {message}")]
    EvalError { message: String },
}

/// Parse a JSON-encoded CCL expression string into an `Expr`.
///
/// The `Milestone.completion_gate` field is stored as `Option<String>`
/// (JSON-encoded `icn_ccl::Expr`) so that the milestone record remains
/// compatible with postcard binary encoding used by `SledMilestoneStore`.
///
/// Returns `Err(MilestoneGateError::EvalError)` if the JSON cannot be
/// parsed as a valid `Expr`.
pub fn parse_gate(gate_json: &str) -> Result<Expr, MilestoneGateError> {
    serde_json::from_str(gate_json).map_err(|e| MilestoneGateError::EvalError {
        message: format!("failed to parse gate expression: {e}"),
    })
}

/// Evaluate an optional CCL gate expression against the supplied context.
///
/// Returns `Ok(())` when:
/// - `gate` is `None` (no gate configured), OR
/// - the gate expression evaluates to a truthy value.
///
/// Returns `Err(MilestoneGateError::Blocked)` when the gate evaluates to a
/// falsy value (`PreconditionFailed` from the CCL interpreter).
///
/// Returns `Err(MilestoneGateError::EvalError)` for any other CCL runtime
/// error (fuel exhausted, unknown variable, type mismatch, etc.).
pub fn evaluate_milestone_gate(
    gate: Option<&Expr>,
    ctx: &MilestoneGateContext,
) -> Result<(), MilestoneGateError> {
    let expr = match gate {
        None => return Ok(()),
        Some(e) => e,
    };

    // Build a synthetic single-rule contract whose sole `require` is the
    // gate expression.  A rule with no body and a failing `require` returns
    // `CclError::PreconditionFailed`; a passing `require` returns
    // `ExecutionResult { value: None, .. }`.
    let rule = Rule::new("gate".into()).add_require(expr.clone());
    let contract = Contract::new("milestone-gate".into()).add_rule(rule);

    let exec_ctx = ExecutionContext::new(
        ctx.actor.clone(),
        ctx.now,
        GATE_FUEL,
        vec![], // no capabilities — read-only predicate
        vec![],
    );

    let interp = Interpreter::new(contract, ContractState::new(), exec_ctx);
    let args = ctx.to_args();

    match interp.execute_rule("gate", args) {
        Ok(_) => Ok(()),
        Err(CclError::PreconditionFailed { index, .. }) => Err(MilestoneGateError::Blocked {
            message: format!("require condition {index} evaluated to false"),
        }),
        Err(e) => Err(MilestoneGateError::EvalError {
            message: e.to_string(),
        }),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use icn_ccl::types::Value;
    use icn_ccl::BinOp;
    use icn_identity::KeyPair;

    fn ctx(criteria_count: i64, phase_index: i64) -> MilestoneGateContext {
        let did = KeyPair::generate().unwrap().did().clone();
        MilestoneGateContext {
            criteria_count,
            phase_index,
            actor: did,
            now: 1_700_000_000,
        }
    }

    fn gte(var: &str, n: i64) -> Expr {
        Expr::BinOp {
            op: BinOp::Ge,
            left: Box::new(Expr::Var(var.into())),
            right: Box::new(Expr::Literal(Value::Int(n))),
        }
    }

    /// No gate → always Ok.
    #[test]
    fn no_gate_always_passes() {
        let c = ctx(0, 0);
        assert!(evaluate_milestone_gate(None, &c).is_ok());
    }

    /// Gate evaluates true → Ok.
    #[test]
    fn gate_true_passes() {
        // criteria_count >= 1 with criteria_count = 2 → true
        let gate = gte("criteria_count", 1);
        let c = ctx(2, 0);
        assert!(evaluate_milestone_gate(Some(&gate), &c).is_ok());
    }

    /// Gate evaluates false → Blocked.
    #[test]
    fn gate_false_blocks() {
        // criteria_count >= 3 with criteria_count = 1 → false
        let gate = gte("criteria_count", 3);
        let c = ctx(1, 0);
        let err = evaluate_milestone_gate(Some(&gate), &c).unwrap_err();
        assert!(
            matches!(err, MilestoneGateError::Blocked { .. }),
            "expected Blocked, got {err:?}"
        );
    }

    /// AND of two conditions: both must be true.
    #[test]
    fn gate_and_passes_when_both_true() {
        let gate = Expr::BinOp {
            op: BinOp::And,
            left: Box::new(gte("criteria_count", 1)),
            right: Box::new(gte("phase_index", 0)),
        };
        let c = ctx(2, 1);
        assert!(evaluate_milestone_gate(Some(&gate), &c).is_ok());
    }

    /// AND of two conditions: first false → blocked.
    #[test]
    fn gate_and_blocks_when_first_false() {
        let gate = Expr::BinOp {
            op: BinOp::And,
            left: Box::new(gte("criteria_count", 5)),
            right: Box::new(gte("phase_index", 0)),
        };
        let c = ctx(1, 1); // criteria_count < 5
        let err = evaluate_milestone_gate(Some(&gate), &c).unwrap_err();
        assert!(matches!(err, MilestoneGateError::Blocked { .. }));
    }

    /// Gate expression with literal `true` always passes.
    #[test]
    fn gate_literal_true_passes() {
        let gate = Expr::Literal(Value::Bool(true));
        let c = ctx(0, 0);
        assert!(evaluate_milestone_gate(Some(&gate), &c).is_ok());
    }

    /// Gate expression with literal `false` always blocks.
    #[test]
    fn gate_literal_false_blocks() {
        let gate = Expr::Literal(Value::Bool(false));
        let c = ctx(0, 0);
        assert!(matches!(
            evaluate_milestone_gate(Some(&gate), &c).unwrap_err(),
            MilestoneGateError::Blocked { .. }
        ));
    }

    /// Accessing an undefined variable is a runtime error (EvalError), not
    /// Blocked — it means the expression is malformed.
    #[test]
    fn gate_unknown_variable_is_eval_error() {
        let gate = gte("nonexistent_var", 1);
        let c = ctx(0, 0);
        assert!(matches!(
            evaluate_milestone_gate(Some(&gate), &c).unwrap_err(),
            MilestoneGateError::EvalError { .. }
        ));
    }
}
