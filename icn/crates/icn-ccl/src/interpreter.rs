//! CCL interpreter with capability checking and fuel metering

use crate::ast::{BinOp, Contract, Expr, Stmt, UnOp};
use crate::error::{CclError, Result, Span};
use crate::types::{
    Capability, ContractState, ExecutionContext, ExecutionResult, FuelConfig, LedgerOperation,
    Value,
};
use std::collections::{HashMap, HashSet};
use tracing::{debug, instrument, trace};

/// CCL Interpreter
pub struct Interpreter {
    /// Current contract
    contract: Contract,

    /// Contract state
    state: ContractState,

    /// Execution context
    context: ExecutionContext,

    /// Local variable bindings
    locals: HashMap<String, Value>,

    /// Ledger operations accumulated during execution
    ledger_ops: Vec<LedgerOperation>,

    /// Fuel configuration for cost metering
    fuel_config: FuelConfig,
}

impl Interpreter {
    /// Create a new interpreter for a contract with default fuel configuration
    pub fn new(contract: Contract, state: ContractState, context: ExecutionContext) -> Self {
        Self::with_fuel_config(contract, state, context, FuelConfig::default())
    }

    /// Create a new interpreter with custom fuel configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// use icn_ccl::{Interpreter, FuelConfig};
    ///
    /// // Use restrictive config for untrusted contracts
    /// let interpreter = Interpreter::with_fuel_config(
    ///     contract,
    ///     state,
    ///     context,
    ///     FuelConfig::restrictive(),
    /// );
    /// ```
    pub fn with_fuel_config(
        contract: Contract,
        state: ContractState,
        context: ExecutionContext,
        fuel_config: FuelConfig,
    ) -> Self {
        Interpreter {
            contract,
            state,
            context,
            locals: HashMap::new(),
            ledger_ops: Vec::new(),
            fuel_config,
        }
    }

    /// Execute a rule with given arguments
    #[instrument(skip(self, args), fields(rule = %rule_name, fuel_limit = self.context.fuel))]
    pub fn execute_rule(
        mut self,
        rule_name: &str,
        args: HashMap<String, Value>,
    ) -> Result<ExecutionResult> {
        // Capture initial fuel BEFORE execution for accurate tracking
        let initial_fuel = self.context.fuel;

        let rule = self
            .contract
            .get_rule(rule_name)
            .ok_or_else(|| CclError::RuleNotFound {
                name: rule_name.to_string(),
            })?
            .clone();

        debug!("Executing rule: {}", rule_name);

        // Bind parameters
        for (param, value) in args {
            self.locals.insert(param, value);
        }

        // Check preconditions (require statements)
        for (i, require) in rule.requires.iter().enumerate() {
            trace!("Checking require condition {}", i);
            let value = self.eval_expr(require)?;
            if !value.is_truthy() {
                return Err(CclError::PreconditionFailed {
                    span: Span::unknown(),
                    index: i,
                });
            }
        }

        // Execute body
        let mut return_value = Value::None;
        for stmt in &rule.body {
            if let Some(value) = self.execute_stmt(stmt)? {
                return_value = value;
                break; // Return statement encountered
            }
        }

        // Compute fuel consumed (initial_fuel captured at start of function)
        let fuel_consumed = initial_fuel.saturating_sub(self.context.fuel);

        // Collect state changes
        let state_changes = self.state.data.clone();

        Ok(ExecutionResult {
            value: return_value,
            fuel_consumed,
            state_changes,
            ledger_ops: self.ledger_ops,
        })
    }

    /// Execute a statement, returns Some(value) if return statement
    fn execute_stmt(&mut self, stmt: &Stmt) -> Result<Option<Value>> {
        self.context.consume_fuel(self.fuel_config.stmt_cost)?;

        match stmt {
            Stmt::Assign { name, value } => {
                let val = self.eval_expr(value)?;
                self.locals.insert(name.clone(), val);
                Ok(None)
            }

            Stmt::SetState { key, value } => {
                // Check WriteState capability for this key
                self.check_write_state_capability(key)?;

                // Evaluate and persist the value
                let val = self.eval_expr(value)?;

                // DUAL UPDATE SEMANTICS (Issue #475):
                //
                // We intentionally update BOTH persistent state and local context:
                //
                // 1. `self.state.data` - The persistent contract state that will be
                //    returned in ExecutionResult.state_changes if the rule succeeds.
                //    This is the authoritative state for persistence.
                //
                // 2. `self.locals` - The local execution context that allows subsequent
                //    statements in the same rule to see the new value immediately.
                //    This is necessary for read-after-write consistency within a rule.
                //
                // ATOMICITY GUARANTEE:
                // - If any subsequent statement fails, the interpreter returns Err and
                //   the state_changes are NOT returned to the caller.
                // - The interpreter consumes self (takes ownership), so partial state
                //   updates from a failed rule cannot leak to the caller.
                // - Only successful rule executions return state_changes.
                //
                // This approach ensures:
                // - Read-after-write consistency within a rule
                // - All-or-nothing semantics for state changes
                // - No observable side effects on failure

                self.state.data.insert(key.clone(), val.clone());
                self.locals.insert(key.clone(), val);

                debug!("SetState: {} updated", key);
                Ok(None)
            }

            Stmt::LedgerTransfer {
                from,
                to,
                amount,
                currency,
            } => {
                // Check WriteLedger capability
                self.check_write_ledger_capability()?;

                let from_val = self.eval_expr(from)?;
                let from_did = from_val
                    .as_did()
                    .ok_or_else(|| CclError::TypeMismatch {
                        span: Span::unknown(),
                        expected: "DID",
                        actual: from_val.type_name().to_string(),
                    })?
                    .clone();

                let to_val = self.eval_expr(to)?;
                let to_did = to_val
                    .as_did()
                    .ok_or_else(|| CclError::TypeMismatch {
                        span: Span::unknown(),
                        expected: "DID",
                        actual: to_val.type_name().to_string(),
                    })?
                    .clone();

                let amount_val_expr = self.eval_expr(amount)?;
                let amount_val =
                    amount_val_expr
                        .as_int()
                        .ok_or_else(|| CclError::TypeMismatch {
                            span: Span::unknown(),
                            expected: "integer",
                            actual: amount_val_expr.type_name().to_string(),
                        })?;

                let currency_val = self.eval_expr(currency)?;
                let currency_str = currency_val
                    .as_string()
                    .ok_or_else(|| CclError::TypeMismatch {
                        span: Span::unknown(),
                        expected: "string",
                        actual: currency_val.type_name().to_string(),
                    })?
                    .to_string();

                self.ledger_ops.push(LedgerOperation::Transfer {
                    from: from_did,
                    to: to_did,
                    amount: amount_val,
                    currency: currency_str,
                });

                Ok(None)
            }

            Stmt::SetCreditLimit {
                account,
                currency,
                limit,
            } => {
                // Check WriteLedger capability
                self.check_write_ledger_capability()?;

                let account_val = self.eval_expr(account)?;
                let account_did = account_val
                    .as_did()
                    .ok_or_else(|| CclError::TypeMismatch {
                        span: Span::unknown(),
                        expected: "DID",
                        actual: account_val.type_name().to_string(),
                    })?
                    .clone();

                let currency_val = self.eval_expr(currency)?;
                let currency_str = currency_val
                    .as_string()
                    .ok_or_else(|| CclError::TypeMismatch {
                        span: Span::unknown(),
                        expected: "string",
                        actual: currency_val.type_name().to_string(),
                    })?
                    .to_string();

                let limit_val_expr = self.eval_expr(limit)?;
                let limit_val = limit_val_expr
                    .as_int()
                    .ok_or_else(|| CclError::TypeMismatch {
                        span: Span::unknown(),
                        expected: "integer",
                        actual: limit_val_expr.type_name().to_string(),
                    })?;

                self.ledger_ops.push(LedgerOperation::SetCreditLimit {
                    account: account_did,
                    currency: currency_str,
                    limit: limit_val,
                });

                Ok(None)
            }

            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_value = self.eval_expr(condition)?;
                if cond_value.is_truthy() {
                    for stmt in then_block {
                        if let Some(val) = self.execute_stmt(stmt)? {
                            return Ok(Some(val));
                        }
                    }
                } else if let Some(else_stmts) = else_block {
                    for stmt in else_stmts {
                        if let Some(val) = self.execute_stmt(stmt)? {
                            return Ok(Some(val));
                        }
                    }
                }
                Ok(None)
            }

            Stmt::For {
                var,
                iterable,
                body,
            } => {
                let collection = self.eval_expr(iterable)?;
                let items = match &collection {
                    Value::List(list) => list.clone(),
                    Value::Set(set) => set.iter().cloned().collect(),
                    _ => {
                        return Err(CclError::NotIterable {
                            span: Span::unknown(),
                            actual: collection.type_name().to_string(),
                        });
                    }
                };

                // Limit iterations
                if items.len() > self.fuel_config.max_loop_iterations {
                    return Err(CclError::LoopLimitExceeded {
                        span: Span::unknown(),
                        count: items.len(),
                        max: self.fuel_config.max_loop_iterations,
                    });
                }

                for item in items {
                    self.context
                        .consume_fuel(self.fuel_config.loop_iteration_cost)?;
                    self.locals.insert(var.clone(), item);

                    for stmt in body {
                        if let Some(val) = self.execute_stmt(stmt)? {
                            return Ok(Some(val));
                        }
                    }
                }

                Ok(None)
            }

            Stmt::Return { value } => {
                let val = self.eval_expr(value)?;
                Ok(Some(val))
            }

            Stmt::Expr(expr) => {
                self.eval_expr(expr)?;
                Ok(None)
            }
        }
    }

    /// Evaluate an expression
    fn eval_expr(&mut self, expr: &Expr) -> Result<Value> {
        self.context.consume_fuel(self.fuel_config.expr_cost)?;

        match expr {
            Expr::Literal(val) => Ok(val.clone()),

            Expr::Var(name) => {
                // Check locals first, then state
                if let Some(val) = self.locals.get(name) {
                    Ok(val.clone())
                } else if let Some(val) = self.state.get(name) {
                    Ok(val.clone())
                } else if name == "participants" {
                    // Special variable: contract participants
                    let dids: HashSet<Value> = self
                        .contract
                        .participants
                        .iter()
                        .map(|did| Value::Did(did.clone()))
                        .collect();
                    Ok(Value::Set(dids))
                } else if name == "sender" {
                    // Special variable: caller
                    Ok(Value::Did(self.context.caller.clone()))
                } else {
                    Err(CclError::UndefinedVariable {
                        span: Span::unknown(),
                        name: name.clone(),
                    })
                }
            }

            Expr::FieldAccess { object, field } => {
                let obj = self.eval_expr(object)?;
                match &obj {
                    Value::Map(map) => {
                        map.get(field)
                            .cloned()
                            .ok_or_else(|| CclError::FieldNotFound {
                                span: Span::unknown(),
                                field: field.clone(),
                            })
                    }
                    _ => Err(CclError::NotAMap {
                        span: Span::unknown(),
                        field: field.clone(),
                        actual: obj.type_name().to_string(),
                    }),
                }
            }

            Expr::BinOp { op, left, right } => {
                let left_val = self.eval_expr(left)?;
                let right_val = self.eval_expr(right)?;
                self.eval_binop(*op, left_val, right_val)
            }

            Expr::UnOp { op, operand } => {
                let val = self.eval_expr(operand)?;
                self.eval_unop(*op, val)
            }

            Expr::Call { name, args } => {
                self.context.consume_fuel(self.fuel_config.call_cost)?;
                let arg_vals: Result<Vec<_>> = args.iter().map(|arg| self.eval_expr(arg)).collect();
                let arg_vals = arg_vals?;
                self.eval_call(name, arg_vals)
            }

            Expr::List(items) => {
                let vals: Result<Vec<_>> = items.iter().map(|item| self.eval_expr(item)).collect();
                Ok(Value::List(vals?))
            }

            Expr::Set(items) => {
                let vals: Result<HashSet<_>> =
                    items.iter().map(|item| self.eval_expr(item)).collect();
                Ok(Value::Set(vals?))
            }

            Expr::Map(pairs) => {
                let mut map = HashMap::new();
                for (key, val_expr) in pairs {
                    let val = self.eval_expr(val_expr)?;
                    map.insert(key.clone(), val);
                }
                Ok(Value::Map(map))
            }

            Expr::In { elem, collection } => {
                let elem_val = self.eval_expr(elem)?;
                let coll_val = self.eval_expr(collection)?;

                let result = match &coll_val {
                    Value::List(list) => list.contains(&elem_val),
                    Value::Set(set) => set.contains(&elem_val),
                    _ => {
                        return Err(CclError::InvalidInOperator {
                            span: Span::unknown(),
                            actual: coll_val.type_name().to_string(),
                        });
                    }
                };

                Ok(Value::Bool(result))
            }
        }
    }

    /// Evaluate binary operation
    fn eval_binop(&self, op: BinOp, left: Value, right: Value) -> Result<Value> {
        use BinOp::*;
        use Value::*;

        match (op, &left, &right) {
            // Integer arithmetic
            (Add, Int(a), Int(b)) => Ok(Int(a + b)),
            (Sub, Int(a), Int(b)) => Ok(Int(a - b)),
            (Mul, Int(a), Int(b)) => Ok(Int(a * b)),
            (Div, Int(a), Int(b)) => {
                if *b == 0 {
                    return Err(CclError::DivisionByZero {
                        span: Span::unknown(),
                    });
                }
                Ok(Int(a / b))
            }
            (Mod, Int(a), Int(b)) => Ok(Int(a % b)),

            // String concatenation
            (Add, String(a), String(b)) => Ok(String(format!("{a}{b}"))),

            // Comparisons
            (Eq, a, b) => Ok(Bool(a == b)),
            (Ne, a, b) => Ok(Bool(a != b)),
            (Lt, Int(a), Int(b)) => Ok(Bool(a < b)),
            (Le, Int(a), Int(b)) => Ok(Bool(a <= b)),
            (Gt, Int(a), Int(b)) => Ok(Bool(a > b)),
            (Ge, Int(a), Int(b)) => Ok(Bool(a >= b)),

            // Logical
            (And, a, b) => Ok(Bool(a.is_truthy() && b.is_truthy())),
            (Or, a, b) => Ok(Bool(a.is_truthy() || b.is_truthy())),

            _ => Err(CclError::InvalidBinaryOp {
                span: Span::unknown(),
                op: format!("{op:?}"),
                left_type: left.type_name().to_string(),
                right_type: right.type_name().to_string(),
            }),
        }
    }

    /// Evaluate unary operation
    fn eval_unop(&self, op: UnOp, val: Value) -> Result<Value> {
        match (op, &val) {
            (UnOp::Not, v) => Ok(Value::Bool(!v.is_truthy())),
            (UnOp::Neg, Value::Int(i)) => Ok(Value::Int(-i)),
            _ => Err(CclError::InvalidUnaryOp {
                span: Span::unknown(),
                op: format!("{op:?}"),
                operand_type: val.type_name().to_string(),
            }),
        }
    }

    /// Evaluate built-in function call
    fn eval_call(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        match name {
            "len" => {
                if args.len() != 1 {
                    return Err(CclError::ArgumentCount {
                        span: Span::unknown(),
                        name: "len".to_string(),
                        expected: 1,
                        actual: args.len(),
                    });
                }
                match &args[0] {
                    Value::List(list) => Ok(Value::Int(list.len() as i64)),
                    Value::Set(set) => Ok(Value::Int(set.len() as i64)),
                    Value::String(s) => Ok(Value::Int(s.len() as i64)),
                    _ => Err(CclError::ArgumentType {
                        span: Span::unknown(),
                        name: "len".to_string(),
                        expected: "list, set, or string",
                        actual: args[0].type_name().to_string(),
                    }),
                }
            }

            "abs" => {
                if args.len() != 1 {
                    return Err(CclError::ArgumentCount {
                        span: Span::unknown(),
                        name: "abs".to_string(),
                        expected: 1,
                        actual: args.len(),
                    });
                }
                match &args[0] {
                    Value::Int(i) => Ok(Value::Int(i.abs())),
                    _ => Err(CclError::ArgumentType {
                        span: Span::unknown(),
                        name: "abs".to_string(),
                        expected: "integer",
                        actual: args[0].type_name().to_string(),
                    }),
                }
            }

            _ => Err(CclError::UnknownFunction {
                span: Span::unknown(),
                name: name.to_string(),
            }),
        }
    }

    /// Check WriteLedger capability
    fn check_write_ledger_capability(&self) -> Result<()> {
        for cap in &self.context.capabilities {
            if matches!(cap, Capability::WriteLedger { .. }) {
                return Ok(());
            }
        }
        Err(CclError::MissingCapability {
            capability: "WriteLedger".to_string(),
        })
    }

    /// Check WriteState capability for a specific key
    fn check_write_state_capability(&self, key: &str) -> Result<()> {
        for cap in &self.context.capabilities {
            if let Capability::WriteState { keys } = cap {
                // Empty keys list means all keys are allowed
                if keys.is_empty() || keys.iter().any(|k| k == key || k == "*") {
                    return Ok(());
                }
            }
        }
        Err(CclError::MissingCapability {
            capability: format!("WriteState for key '{key}'"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Contract;
    use icn_identity::KeyPair;

    fn create_test_context(caller: icn_identity::Did) -> ExecutionContext {
        ExecutionContext::new(
            caller,
            1234567890,
            10000,
            vec![Capability::WriteLedger { accounts: vec![] }],
            vec![],
        )
    }

    #[test]
    fn test_simple_arithmetic() {
        let contract = Contract::new("test".to_string());
        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let context = create_test_context(keypair.did().clone());

        let mut interp = Interpreter::new(contract, state, context);

        // 5 + 3
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Literal(Value::Int(5))),
            right: Box::new(Expr::Literal(Value::Int(3))),
        };

        let result = interp.eval_expr(&expr).unwrap();
        assert_eq!(result, Value::Int(8));
    }

    #[test]
    fn test_comparison() {
        let contract = Contract::new("test".to_string());
        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let context = create_test_context(keypair.did().clone());

        let mut interp = Interpreter::new(contract, state, context);

        // 10 > 5
        let expr = Expr::BinOp {
            op: BinOp::Gt,
            left: Box::new(Expr::Literal(Value::Int(10))),
            right: Box::new(Expr::Literal(Value::Int(5))),
        };

        let result = interp.eval_expr(&expr).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_fuel_consumption() {
        let contract = Contract::new("test".to_string());
        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let mut context = create_test_context(keypair.did().clone());
        context.fuel = 5; // Very low fuel
        context.fuel_limit = 5;

        let mut interp = Interpreter::new(contract, state, context);

        // This should fail due to fuel exhaustion
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Literal(Value::Int(1))),
                right: Box::new(Expr::Literal(Value::Int(2))),
            }),
            right: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Literal(Value::Int(3))),
                right: Box::new(Expr::Literal(Value::Int(4))),
            }),
        };

        let result = interp.eval_expr(&expr);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, crate::error::CclError::FuelExhausted { .. }),
            "Expected FuelExhausted error, got: {err}"
        );
    }

    // Issue #475: SetState dual update semantics tests

    fn create_context_with_write_state(
        caller: icn_identity::Did,
        state_key: &str,
    ) -> ExecutionContext {
        ExecutionContext::new(
            caller,
            1234567890,
            10000,
            vec![
                Capability::WriteLedger { accounts: vec![] },
                Capability::WriteState {
                    keys: vec![state_key.to_string()],
                },
            ],
            vec![],
        )
    }

    #[test]
    fn test_setstate_updates_both_state_and_locals() {
        // Verify SetState updates both persistent state and local context
        let mut contract = Contract::new("test".to_string());

        // Add a rule that sets state and then reads it
        contract.rules.push(crate::ast::Rule {
            name: "test_rule".to_string(),
            params: vec![],
            requires: vec![],
            body: vec![
                // Set state to 42
                Stmt::SetState {
                    key: "counter".to_string(),
                    value: Expr::Literal(Value::Int(42)),
                },
                // Return the value from state (reads from locals)
                Stmt::Return {
                    value: Expr::Var("counter".to_string()),
                },
            ],
        });

        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let context = create_context_with_write_state(keypair.did().clone(), "counter");

        let interp = Interpreter::new(contract, state, context);
        let result = interp.execute_rule("test_rule", HashMap::new()).unwrap();

        // Return value should be 42 (read from locals after SetState)
        assert_eq!(result.value, Value::Int(42));

        // State changes should also contain 42
        assert_eq!(result.state_changes.get("counter"), Some(&Value::Int(42)));
    }

    #[test]
    fn test_setstate_read_after_write_consistency() {
        // Verify that a value set via SetState can be read immediately
        let mut contract = Contract::new("test".to_string());

        contract.rules.push(crate::ast::Rule {
            name: "increment_rule".to_string(),
            params: vec![],
            requires: vec![],
            body: vec![
                // Set initial value
                Stmt::SetState {
                    key: "value".to_string(),
                    value: Expr::Literal(Value::Int(10)),
                },
                // Read it back and add 5
                Stmt::SetState {
                    key: "value".to_string(),
                    value: Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Var("value".to_string())),
                        right: Box::new(Expr::Literal(Value::Int(5))),
                    },
                },
                // Return the final value
                Stmt::Return {
                    value: Expr::Var("value".to_string()),
                },
            ],
        });

        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let context = create_context_with_write_state(keypair.did().clone(), "value");

        let interp = Interpreter::new(contract, state, context);
        let result = interp
            .execute_rule("increment_rule", HashMap::new())
            .unwrap();

        // Final value should be 15 (10 + 5)
        assert_eq!(result.value, Value::Int(15));
        assert_eq!(result.state_changes.get("value"), Some(&Value::Int(15)));
    }

    #[test]
    fn test_setstate_failure_atomicity() {
        // Verify that if a rule fails after SetState, state changes are not returned
        let mut contract = Contract::new("test".to_string());

        contract.rules.push(crate::ast::Rule {
            name: "failing_rule".to_string(),
            params: vec![],
            requires: vec![],
            body: vec![
                // Set state successfully
                Stmt::SetState {
                    key: "counter".to_string(),
                    value: Expr::Literal(Value::Int(100)),
                },
                // This will fail - division by zero
                Stmt::Assign {
                    name: "x".to_string(),
                    value: Expr::BinOp {
                        op: BinOp::Div,
                        left: Box::new(Expr::Literal(Value::Int(1))),
                        right: Box::new(Expr::Literal(Value::Int(0))),
                    },
                },
                Stmt::Return {
                    value: Expr::Var("counter".to_string()),
                },
            ],
        });

        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let context = create_context_with_write_state(keypair.did().clone(), "counter");

        let interp = Interpreter::new(contract, state, context);
        let result = interp.execute_rule("failing_rule", HashMap::new());

        // Rule should fail due to division by zero
        assert!(
            result.is_err(),
            "Expected rule to fail due to division by zero"
        );

        // Importantly: no ExecutionResult is returned, so no state changes can leak
        // The caller cannot observe the partial state update
    }

    #[test]
    fn test_setstate_multiple_keys() {
        // Verify multiple SetState statements work correctly
        let mut contract = Contract::new("test".to_string());

        contract.rules.push(crate::ast::Rule {
            name: "multi_set_rule".to_string(),
            params: vec![],
            requires: vec![],
            body: vec![
                Stmt::SetState {
                    key: "a".to_string(),
                    value: Expr::Literal(Value::Int(1)),
                },
                Stmt::SetState {
                    key: "b".to_string(),
                    value: Expr::Literal(Value::Int(2)),
                },
                Stmt::SetState {
                    key: "c".to_string(),
                    value: Expr::Literal(Value::Int(3)),
                },
                // Return sum of all three (verifies all are readable)
                Stmt::Return {
                    value: Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(Expr::Var("a".to_string())),
                            right: Box::new(Expr::Var("b".to_string())),
                        }),
                        right: Box::new(Expr::Var("c".to_string())),
                    },
                },
            ],
        });

        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let mut context = ExecutionContext::new(
            keypair.did().clone(),
            1234567890,
            10000,
            vec![
                Capability::WriteLedger { accounts: vec![] },
                Capability::WriteState {
                    keys: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                },
            ],
            vec![],
        );
        context.fuel = 10000;

        let interp = Interpreter::new(contract, state, context);
        let result = interp
            .execute_rule("multi_set_rule", HashMap::new())
            .unwrap();

        // Return value should be 6 (1 + 2 + 3)
        assert_eq!(result.value, Value::Int(6));

        // All state changes should be present
        assert_eq!(result.state_changes.get("a"), Some(&Value::Int(1)));
        assert_eq!(result.state_changes.get("b"), Some(&Value::Int(2)));
        assert_eq!(result.state_changes.get("c"), Some(&Value::Int(3)));
    }

    #[test]
    fn test_setstate_in_loop() {
        // Verify SetState works correctly inside a loop
        let mut contract = Contract::new("test".to_string());

        contract.rules.push(crate::ast::Rule {
            name: "loop_set_rule".to_string(),
            params: vec![],
            requires: vec![],
            body: vec![
                // Initialize counter to 0
                Stmt::SetState {
                    key: "sum".to_string(),
                    value: Expr::Literal(Value::Int(0)),
                },
                // Loop 5 times, adding i each time (iterate over [1,2,3,4,5])
                Stmt::For {
                    var: "i".to_string(),
                    iterable: Expr::List(vec![
                        Expr::Literal(Value::Int(1)),
                        Expr::Literal(Value::Int(2)),
                        Expr::Literal(Value::Int(3)),
                        Expr::Literal(Value::Int(4)),
                        Expr::Literal(Value::Int(5)),
                    ]),
                    body: vec![Stmt::SetState {
                        key: "sum".to_string(),
                        value: Expr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(Expr::Var("sum".to_string())),
                            right: Box::new(Expr::Var("i".to_string())),
                        },
                    }],
                },
                Stmt::Return {
                    value: Expr::Var("sum".to_string()),
                },
            ],
        });

        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();
        let context = create_context_with_write_state(keypair.did().clone(), "sum");

        let interp = Interpreter::new(contract, state, context);
        let result = interp
            .execute_rule("loop_set_rule", HashMap::new())
            .unwrap();

        // Sum of 1+2+3+4+5 = 15
        assert_eq!(result.value, Value::Int(15));
        assert_eq!(result.state_changes.get("sum"), Some(&Value::Int(15)));
    }

    #[test]
    fn test_setstate_with_initial_state() {
        // Verify SetState works when there's existing state
        let mut contract = Contract::new("test".to_string());

        contract.rules.push(crate::ast::Rule {
            name: "update_existing".to_string(),
            params: vec![],
            requires: vec![],
            body: vec![
                // Double the existing value
                Stmt::SetState {
                    key: "counter".to_string(),
                    value: Expr::BinOp {
                        op: BinOp::Mul,
                        left: Box::new(Expr::Var("counter".to_string())),
                        right: Box::new(Expr::Literal(Value::Int(2))),
                    },
                },
                Stmt::Return {
                    value: Expr::Var("counter".to_string()),
                },
            ],
        });

        // Start with existing state
        let mut state = ContractState::new();
        state.data.insert("counter".to_string(), Value::Int(21));

        let keypair = KeyPair::generate().unwrap();
        let context = create_context_with_write_state(keypair.did().clone(), "counter");

        let interp = Interpreter::new(contract, state, context);
        let result = interp
            .execute_rule("update_existing", HashMap::new())
            .unwrap();

        // 21 * 2 = 42
        assert_eq!(result.value, Value::Int(42));
        assert_eq!(result.state_changes.get("counter"), Some(&Value::Int(42)));
    }

    // ========================================================================
    // FuelConfig Tests (Issue #162)
    // ========================================================================

    #[test]
    fn test_fuel_config_default_values() {
        let config = FuelConfig::default();
        assert_eq!(config.stmt_cost, 1);
        assert_eq!(config.expr_cost, 1);
        assert_eq!(config.call_cost, 10);
        assert_eq!(config.loop_iteration_cost, 5);
        assert_eq!(config.max_loop_iterations, 1000);
    }

    #[test]
    fn test_fuel_config_restrictive() {
        let config = FuelConfig::restrictive();
        // Restrictive should have higher costs
        assert!(config.stmt_cost > FuelConfig::default().stmt_cost);
        assert!(config.call_cost > FuelConfig::default().call_cost);
        assert!(config.max_loop_iterations < FuelConfig::default().max_loop_iterations);
    }

    #[test]
    fn test_fuel_config_permissive() {
        let config = FuelConfig::permissive();
        // Permissive should have lower costs (or same for stmt/expr)
        assert!(config.call_cost < FuelConfig::default().call_cost);
        assert!(config.loop_iteration_cost < FuelConfig::default().loop_iteration_cost);
        assert!(config.max_loop_iterations > FuelConfig::default().max_loop_iterations);
    }

    #[test]
    fn test_interpreter_with_custom_fuel_config() {
        // Test that custom fuel config is actually used
        let contract = Contract::new("test".to_string());
        let state = ContractState::new();
        let keypair = KeyPair::generate().unwrap();

        // Create context with 10 fuel units
        let mut context = create_test_context(keypair.did().clone());
        context.fuel = 10;
        context.fuel_limit = 10;

        // Use a fuel config with high expression cost (5 per expr)
        let high_cost_config = FuelConfig {
            expr_cost: 5,
            ..Default::default()
        };

        let mut interp = Interpreter::with_fuel_config(contract, state, context, high_cost_config);

        // Simple expression should consume 5 fuel (our custom cost)
        let expr = Expr::Literal(Value::Int(42));
        let result = interp.eval_expr(&expr);
        assert!(result.is_ok());

        // Should have consumed 5 fuel, leaving 5
        assert_eq!(interp.context.fuel, 5);

        // Another expression consumes another 5, leaving 0
        let result = interp.eval_expr(&expr);
        assert!(result.is_ok());
        assert_eq!(interp.context.fuel, 0);

        // Next expression should fail (no fuel left)
        let result = interp.eval_expr(&expr);
        assert!(matches!(
            result,
            Err(crate::error::CclError::FuelExhausted { .. })
        ));
    }

    #[test]
    fn test_fuel_config_serde() {
        // Test that FuelConfig can be serialized/deserialized
        let config = FuelConfig {
            stmt_cost: 2,
            expr_cost: 3,
            call_cost: 15,
            loop_iteration_cost: 8,
            max_loop_iterations: 500,
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: FuelConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(config, parsed);
    }
}
