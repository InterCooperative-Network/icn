//! CCL interpreter with capability checking and fuel metering

use crate::ast::{BinOp, Contract, Expr, Stmt, UnOp};
use crate::types::{
    Capability, ContractState, ExecutionContext, ExecutionResult, LedgerOperation, Value,
};
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use tracing::{debug, trace};

/// Fuel cost constants
const FUEL_STMT: u64 = 1;
const FUEL_EXPR: u64 = 1;
const FUEL_CALL: u64 = 10;
const FUEL_LOOP_ITERATION: u64 = 5;
const MAX_LOOP_ITERATIONS: usize = 1000;

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
}

impl Interpreter {
    /// Create a new interpreter for a contract
    pub fn new(contract: Contract, state: ContractState, context: ExecutionContext) -> Self {
        Interpreter {
            contract,
            state,
            context,
            locals: HashMap::new(),
            ledger_ops: Vec::new(),
        }
    }

    /// Execute a rule with given arguments
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
            .context("Rule not found")?
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
                bail!("Precondition failed: require #{i}");
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
        self.context.consume_fuel(FUEL_STMT)?;

        match stmt {
            Stmt::Assign { name, value } => {
                let val = self.eval_expr(value)?;
                self.locals.insert(name.clone(), val);
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

                let from_did = self
                    .eval_expr(from)?
                    .as_did()
                    .context("Transfer 'from' must be a DID")?
                    .clone();
                let to_did = self
                    .eval_expr(to)?
                    .as_did()
                    .context("Transfer 'to' must be a DID")?
                    .clone();
                let amount_val = self
                    .eval_expr(amount)?
                    .as_int()
                    .context("Transfer amount must be an integer")?;
                let currency_str = self
                    .eval_expr(currency)?
                    .as_string()
                    .context("Currency must be a string")?
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

                let account_did = self
                    .eval_expr(account)?
                    .as_did()
                    .context("Account must be a DID")?
                    .clone();
                let currency_str = self
                    .eval_expr(currency)?
                    .as_string()
                    .context("Currency must be a string")?
                    .to_string();
                let limit_val = self
                    .eval_expr(limit)?
                    .as_int()
                    .context("Limit must be an integer")?;

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
                let items = match collection {
                    Value::List(ref list) => list.clone(),
                    Value::Set(ref set) => set.iter().cloned().collect(),
                    _ => bail!("For loop requires list or set"),
                };

                // Limit iterations
                if items.len() > MAX_LOOP_ITERATIONS {
                    bail!(
                        "For loop exceeds max iterations: {} > {}",
                        items.len(),
                        MAX_LOOP_ITERATIONS
                    );
                }

                for item in items {
                    self.context.consume_fuel(FUEL_LOOP_ITERATION)?;
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
        self.context.consume_fuel(FUEL_EXPR)?;

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
                    bail!("Undefined variable: {name}")
                }
            }

            Expr::FieldAccess { object, field } => {
                let obj = self.eval_expr(object)?;
                match obj {
                    Value::Map(ref map) => map
                        .get(field)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Field not found: {field}")),
                    _ => bail!("Cannot access field on non-map value"),
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
                self.context.consume_fuel(FUEL_CALL)?;
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

                let result = match coll_val {
                    Value::List(ref list) => list.contains(&elem_val),
                    Value::Set(ref set) => set.contains(&elem_val),
                    _ => bail!("'in' requires list or set"),
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
                    bail!("Division by zero")
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

            _ => bail!("Invalid binary operation: {left:?} {op:?} {right:?}"),
        }
    }

    /// Evaluate unary operation
    fn eval_unop(&self, op: UnOp, val: Value) -> Result<Value> {
        match (op, val) {
            (UnOp::Not, v) => Ok(Value::Bool(!v.is_truthy())),
            (UnOp::Neg, Value::Int(i)) => Ok(Value::Int(-i)),
            _ => bail!("Invalid unary operation"),
        }
    }

    /// Evaluate built-in function call
    fn eval_call(&mut self, name: &str, args: Vec<Value>) -> Result<Value> {
        match name {
            "len" => {
                if args.len() != 1 {
                    bail!("len() takes exactly 1 argument");
                }
                match &args[0] {
                    Value::List(list) => Ok(Value::Int(list.len() as i64)),
                    Value::Set(set) => Ok(Value::Int(set.len() as i64)),
                    Value::String(s) => Ok(Value::Int(s.len() as i64)),
                    _ => bail!("len() requires list, set, or string"),
                }
            }

            "abs" => {
                if args.len() != 1 {
                    bail!("abs() takes exactly 1 argument");
                }
                match args[0] {
                    Value::Int(i) => Ok(Value::Int(i.abs())),
                    _ => bail!("abs() requires integer"),
                }
            }

            _ => bail!("Unknown function: {name}"),
        }
    }

    /// Check WriteLedger capability
    fn check_write_ledger_capability(&self) -> Result<()> {
        for cap in &self.context.capabilities {
            if matches!(cap, Capability::WriteLedger { .. }) {
                return Ok(());
            }
        }
        bail!("WriteLedger capability required")
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
        assert!(result.unwrap_err().to_string().contains("fuel"));
    }
}
