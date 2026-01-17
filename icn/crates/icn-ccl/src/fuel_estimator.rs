//! Fuel estimation for CCL contracts
//!
//! Provides static analysis of contract rules to estimate fuel consumption
//! before execution. This helps clients set appropriate fuel limits.

use crate::ast::{Contract, Expr, Rule, Stmt};
use crate::types::FuelEstimate;

/// Fuel cost constants for estimation (matches interpreter defaults)
const FUEL_STMT: u64 = 1;
const FUEL_EXPR: u64 = 1;
const FUEL_CALL: u64 = 10;
const FUEL_LOOP_ITERATION: u64 = 5;
const MAX_LOOP_ITERATIONS: u64 = 1000;

/// Fuel estimator for CCL contracts
///
/// Performs static analysis of contract rules to estimate fuel consumption.
///
/// # Example
///
/// ```
/// use icn_ccl::{Contract, FuelEstimator};
///
/// let contract = Contract::new("Test".to_string());
/// let estimator = FuelEstimator::new(&contract);
///
/// if let Some(estimate) = estimator.estimate_rule("my_rule") {
///     println!("Recommended fuel: {}", estimate.recommended());
/// }
/// ```
pub struct FuelEstimator<'a> {
    contract: &'a Contract,
}

impl<'a> FuelEstimator<'a> {
    /// Create a new fuel estimator for a contract
    pub fn new(contract: &'a Contract) -> Self {
        Self { contract }
    }

    /// Estimate fuel for a specific rule
    ///
    /// Returns None if the rule doesn't exist.
    pub fn estimate_rule(&self, rule_name: &str) -> Option<FuelEstimate> {
        let rule = self.contract.get_rule(rule_name)?;
        Some(self.estimate_rule_internal(rule))
    }

    /// Estimate fuel for all rules in the contract
    pub fn estimate_all_rules(&self) -> Vec<(String, FuelEstimate)> {
        self.contract
            .rules
            .iter()
            .map(|rule| (rule.name.clone(), self.estimate_rule_internal(rule)))
            .collect()
    }

    fn estimate_rule_internal(&self, rule: &Rule) -> FuelEstimate {
        let mut estimate = FuelEstimate::zero();

        // Estimate require conditions
        for require in &rule.requires {
            estimate = estimate.then(self.estimate_expr(require));
        }

        // Estimate body statements
        for stmt in &rule.body {
            estimate = estimate.then(self.estimate_stmt(stmt));
        }

        estimate
    }

    fn estimate_stmt(&self, stmt: &Stmt) -> FuelEstimate {
        // Base cost for any statement
        let base = FuelEstimate::new(FUEL_STMT, FUEL_STMT, FUEL_STMT);

        match stmt {
            Stmt::Assign { value, .. } => base.then(self.estimate_expr(value)),

            Stmt::SetState { value, .. } => base.then(self.estimate_expr(value)),

            Stmt::LedgerTransfer {
                from,
                to,
                amount,
                currency,
            } => base
                .then(self.estimate_expr(from))
                .then(self.estimate_expr(to))
                .then(self.estimate_expr(amount))
                .then(self.estimate_expr(currency)),

            Stmt::SetCreditLimit {
                account,
                currency,
                limit,
            } => base
                .then(self.estimate_expr(account))
                .then(self.estimate_expr(currency))
                .then(self.estimate_expr(limit)),

            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_estimate = self.estimate_expr(condition);
                let then_estimate = self.estimate_block(then_block);
                let else_estimate = else_block
                    .as_ref()
                    .map(|b| self.estimate_block(b))
                    .unwrap_or_else(FuelEstimate::zero);

                base.then(cond_estimate)
                    .then(then_estimate.branch(else_estimate))
            }

            Stmt::For {
                iterable, body, ..
            } => {
                let iter_estimate = self.estimate_expr(iterable);
                let body_estimate = self.estimate_block(body);

                // Loop iteration cost per iteration
                let iteration_cost =
                    FuelEstimate::new(FUEL_LOOP_ITERATION, FUEL_LOOP_ITERATION, FUEL_LOOP_ITERATION);
                let per_iteration = iteration_cost.then(body_estimate);

                // Estimate: min 0 iterations, expected half of max, max is MAX_LOOP_ITERATIONS
                let loop_estimate =
                    per_iteration.loop_scale(0, MAX_LOOP_ITERATIONS / 2, MAX_LOOP_ITERATIONS);

                base.then(iter_estimate)
                    .then(loop_estimate)
                    .with_warning(format!(
                        "Loop estimation assumes max {} iterations",
                        MAX_LOOP_ITERATIONS
                    ))
            }

            Stmt::Return { value } => base.then(self.estimate_expr(value)),

            Stmt::Expr(expr) => base.then(self.estimate_expr(expr)),
        }
    }

    fn estimate_expr(&self, expr: &Expr) -> FuelEstimate {
        // Base cost for any expression
        let base = FuelEstimate::new(FUEL_EXPR, FUEL_EXPR, FUEL_EXPR);

        match expr {
            Expr::Literal(_) | Expr::Var(_) => base,

            Expr::FieldAccess { object, .. } => base.then(self.estimate_expr(object)),

            Expr::BinOp { left, right, .. } => {
                base.then(self.estimate_expr(left))
                    .then(self.estimate_expr(right))
            }

            Expr::UnOp { operand, .. } => base.then(self.estimate_expr(operand)),

            Expr::Call { args, .. } => {
                let call_cost = FuelEstimate::new(FUEL_CALL, FUEL_CALL, FUEL_CALL);
                let mut estimate = base.then(call_cost);
                for arg in args {
                    estimate = estimate.then(self.estimate_expr(arg));
                }
                estimate
            }

            Expr::List(items) | Expr::Set(items) => {
                let mut estimate = base;
                for item in items {
                    estimate = estimate.then(self.estimate_expr(item));
                }
                estimate
            }

            Expr::Map(entries) => {
                let mut estimate = base;
                for (_, value) in entries {
                    estimate = estimate.then(self.estimate_expr(value));
                }
                estimate
            }

            Expr::In { elem, collection } => base
                .then(self.estimate_expr(elem))
                .then(self.estimate_expr(collection)),
        }
    }

    fn estimate_block(&self, stmts: &[Stmt]) -> FuelEstimate {
        let mut estimate = FuelEstimate::zero();
        for stmt in stmts {
            estimate = estimate.then(self.estimate_stmt(stmt));
        }
        estimate
    }
}

/// Convenience function to estimate fuel for a rule using static analysis
pub fn estimate_fuel(contract: &Contract, rule_name: &str) -> Option<FuelEstimate> {
    FuelEstimator::new(contract).estimate_rule(rule_name)
}

/// Estimate fuel by executing a dry run
///
/// This provides more accurate estimation than static analysis by actually
/// executing the contract with the given arguments, but without side effects.
///
/// Returns the actual fuel consumed during execution.
///
/// # Arguments
///
/// * `contract` - The contract to estimate
/// * `rule_name` - The rule to execute
/// * `args` - Arguments to pass to the rule
/// * `caller` - The DID of the caller
/// * `timestamp` - Current timestamp for deterministic execution
/// * `fuel_limit` - Maximum fuel to allow (prevents infinite loops)
///
/// # Example
///
/// ```ignore
/// use icn_ccl::estimate_fuel_dry_run;
/// use std::collections::HashMap;
///
/// let fuel = estimate_fuel_dry_run(
///     &contract,
///     "my_rule",
///     HashMap::new(),
///     caller_did,
///     1234567890,
///     10000,
/// );
/// ```
pub fn estimate_fuel_dry_run(
    contract: &Contract,
    rule_name: &str,
    args: std::collections::HashMap<String, crate::Value>,
    caller: icn_identity::Did,
    timestamp: u64,
    fuel_limit: u64,
) -> Result<u64, crate::error::CclError> {
    use crate::types::{ContractState, ExecutionContext};
    use crate::Interpreter;

    // Create empty state and context for dry run
    let state = ContractState::new();
    let context = ExecutionContext::new(
        caller,
        timestamp,
        fuel_limit,
        vec![], // No capabilities needed for dry run fuel estimation
        vec![], // No participants needed
    );

    // Execute in dry run mode
    let interpreter = Interpreter::dry_run(contract.clone(), state, context);
    let result = interpreter.execute_rule(rule_name, args)?;

    Ok(result.fuel_consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Rule;
    use crate::Value;

    fn simple_contract() -> Contract {
        Contract::new("TestContract".to_string()).add_rule(
            Rule::new("simple_rule".to_string())
                .add_param("x".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Var("x".to_string()),
                }),
        )
    }

    #[test]
    fn test_estimate_simple_rule() {
        let contract = simple_contract();
        let estimate = estimate_fuel(&contract, "simple_rule").unwrap();

        // Should have: 1 stmt (return) + 1 expr (var reference)
        assert!(estimate.minimum > 0);
        assert!(estimate.expected > 0);
        assert!(estimate.maximum > 0);
        assert_eq!(estimate.minimum, estimate.expected);
        assert_eq!(estimate.expected, estimate.maximum);
    }

    #[test]
    fn test_estimate_nonexistent_rule() {
        let contract = simple_contract();
        let estimate = estimate_fuel(&contract, "nonexistent");
        assert!(estimate.is_none());
    }

    #[test]
    fn test_estimate_with_loop() {
        let contract = Contract::new("TestContract".to_string()).add_rule(
            Rule::new("loop_rule".to_string()).add_stmt(Stmt::For {
                var: "i".to_string(),
                iterable: Expr::List(vec![
                    Expr::Literal(Value::Int(1)),
                    Expr::Literal(Value::Int(2)),
                ]),
                body: vec![Stmt::Expr(Expr::Var("i".to_string()))],
            }),
        );

        let estimate = estimate_fuel(&contract, "loop_rule").unwrap();

        // Loop should have warning
        assert!(!estimate.warnings.is_empty());
        // Maximum should be much larger than minimum (loop iterations)
        assert!(estimate.maximum > estimate.minimum);
    }

    #[test]
    fn test_estimate_with_branch() {
        let contract = Contract::new("TestContract".to_string()).add_rule(
            Rule::new("branch_rule".to_string()).add_stmt(Stmt::If {
                condition: Expr::Literal(Value::Bool(true)),
                then_block: vec![
                    Stmt::Expr(Expr::Literal(Value::Int(1))),
                    Stmt::Expr(Expr::Literal(Value::Int(2))),
                ],
                else_block: Some(vec![Stmt::Expr(Expr::Literal(Value::Int(3)))]),
            }),
        );

        let estimate = estimate_fuel(&contract, "branch_rule").unwrap();

        // Should account for branching
        assert!(estimate.minimum > 0);
        assert!(estimate.maximum >= estimate.minimum);
    }

    #[test]
    fn test_estimate_with_call() {
        let contract = Contract::new("TestContract".to_string()).add_rule(
            Rule::new("call_rule".to_string()).add_stmt(Stmt::Expr(Expr::Call {
                name: "some_function".to_string(),
                args: vec![Expr::Literal(Value::Int(1)), Expr::Literal(Value::Int(2))],
            })),
        );

        let estimate = estimate_fuel(&contract, "call_rule").unwrap();

        // Should include call cost
        assert!(estimate.minimum >= FUEL_CALL);
    }

    #[test]
    fn test_estimate_all_rules() {
        let contract = Contract::new("TestContract".to_string())
            .add_rule(Rule::new("rule1".to_string()))
            .add_rule(Rule::new("rule2".to_string()));

        let estimator = FuelEstimator::new(&contract);
        let estimates = estimator.estimate_all_rules();

        assert_eq!(estimates.len(), 2);
    }

    #[test]
    fn test_recommended_adds_buffer() {
        let estimate = FuelEstimate::new(100, 150, 200);
        let recommended = estimate.recommended();

        // Should be 20% more than maximum
        assert_eq!(recommended, 240);
    }

    #[test]
    fn test_fuel_estimate_then() {
        let a = FuelEstimate::new(10, 20, 30);
        let b = FuelEstimate::new(5, 10, 15);
        let combined = a.then(b);

        assert_eq!(combined.minimum, 15);
        assert_eq!(combined.expected, 30);
        assert_eq!(combined.maximum, 45);
    }

    #[test]
    fn test_fuel_estimate_branch() {
        let a = FuelEstimate::new(10, 20, 30);
        let b = FuelEstimate::new(5, 10, 50);
        let combined = a.branch(b);

        assert_eq!(combined.minimum, 5); // min of both
        assert_eq!(combined.expected, 15); // average
        assert_eq!(combined.maximum, 50); // max of both
    }

    #[test]
    fn test_dry_run_estimation() {
        use icn_identity::KeyPair;
        use std::collections::HashMap;

        let contract = simple_contract();
        let caller = KeyPair::generate().unwrap().did().clone();

        // Dry run should return actual fuel consumed
        let fuel = estimate_fuel_dry_run(
            &contract,
            "simple_rule",
            HashMap::from([("x".to_string(), Value::Int(42))]),
            caller,
            1234567890,
            1000, // fuel limit
        )
        .unwrap();

        // Should consume some fuel
        assert!(fuel > 0);
        // Should be less than static analysis maximum (which assumes worst case)
        let static_estimate = estimate_fuel(&contract, "simple_rule").unwrap();
        assert!(fuel <= static_estimate.maximum);
    }

    #[test]
    fn test_dry_run_no_side_effects() {
        use crate::types::{Capability, ContractState, ExecutionContext};
        use crate::Interpreter;
        use icn_identity::KeyPair;
        use std::collections::HashMap;

        // Create a contract that modifies state
        let contract = Contract::new("TestContract".to_string()).add_rule(
            Rule::new("set_value".to_string())
                .add_param("val".to_string())
                .add_stmt(Stmt::SetState {
                    key: "counter".to_string(),
                    value: Expr::Var("val".to_string()),
                }),
        );

        let caller = KeyPair::generate().unwrap().did().clone();
        let state = ContractState::new();

        // Provide WriteState capability for the counter key
        let capabilities = vec![Capability::WriteState {
            keys: vec!["counter".to_string()],
        }];
        let context =
            ExecutionContext::new(caller.clone(), 1234567890, 1000, capabilities, vec![]);

        // Dry run execution
        let interpreter = Interpreter::dry_run(contract.clone(), state, context);
        let result = interpreter
            .execute_rule(
                "set_value",
                HashMap::from([("val".to_string(), Value::Int(100))]),
            )
            .unwrap();

        // State changes should be empty in dry run
        assert!(result.state_changes.is_empty());
        // But fuel should still be consumed
        assert!(result.fuel_consumed > 0);
    }
}
