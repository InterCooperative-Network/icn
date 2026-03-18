//! Abstract Syntax Tree for CCL contracts

use crate::types::Value;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Maximum contract name length
const MAX_NAME_LENGTH: usize = 128;

/// Maximum loop nesting depth
const MAX_LOOP_DEPTH: usize = 5;

/// Maximum expression depth
const MAX_EXPR_DEPTH: usize = 50;

/// Maximum number of participants in a contract
const MAX_PARTICIPANTS: usize = 100;

/// Maximum number of state variables
const MAX_STATE_VARS: usize = 100;

/// Maximum number of rules
const MAX_RULES: usize = 50;

/// Reserved keywords that cannot be used as variable names
const RESERVED_KEYWORDS: &[&str] = &[
    "if",
    "else",
    "for",
    "return",
    "true",
    "false",
    "none",
    "ledger_transfer",
    "set_credit_limit",
    "caller",
    "self",
];

/// A complete contract definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    /// Contract name
    pub name: String,

    /// Participant DIDs (set at instantiation)
    pub participants: Vec<Did>,

    /// Currency used in this contract (optional)
    pub currency: Option<String>,

    /// State variable declarations
    pub state_vars: Vec<StateVar>,

    /// Contract rules (functions)
    pub rules: Vec<Rule>,

    /// Scheduled triggers
    pub triggers: Vec<Trigger>,
}

/// State variable declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVar {
    /// Variable name
    pub name: String,

    /// Initial value
    pub initial_value: Value,
}

/// A rule (function) in a contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Rule name
    pub name: String,

    /// Parameters
    pub params: Vec<Param>,

    /// Preconditions (require statements)
    pub requires: Vec<Expr>,

    /// Rule body (statements to execute)
    pub body: Vec<Stmt>,
}

/// Function parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    /// Parameter name
    pub name: String,
}

/// Scheduled trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// Trigger name
    pub name: String,

    /// Cron schedule expression
    pub schedule: String,

    /// Action to perform
    pub action: Vec<Stmt>,
}

/// Statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    /// Variable assignment: x = expr (local variable, not persisted)
    Assign { name: String, value: Expr },

    /// Persistent state update: modifies contract state variable
    /// Unlike Assign, this persists the value to the contract's state storage.
    /// Requires WriteState capability for the specified key.
    SetState { key: String, value: Expr },

    /// Ledger transfer
    LedgerTransfer {
        from: Expr,
        to: Expr,
        amount: Expr,
        currency: Expr,
    },

    /// Set credit limit
    SetCreditLimit {
        account: Expr,
        currency: Expr,
        limit: Expr,
    },

    /// If statement
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_block: Option<Vec<Stmt>>,
    },

    /// For loop (limited iterations)
    For {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },

    /// Return statement
    Return { value: Expr },

    /// Expression statement
    Expr(Expr),
}

/// Expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// Literal value
    Literal(Value),

    /// Variable reference
    Var(String),

    /// Field access: expr.field
    FieldAccess { object: Box<Expr>, field: String },

    /// Binary operation
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Unary operation
    UnOp { op: UnOp, operand: Box<Expr> },

    /// Function call
    Call { name: String, args: Vec<Expr> },

    /// List literal
    List(Vec<Expr>),

    /// Set literal
    Set(Vec<Expr>),

    /// Map literal
    Map(Vec<(String, Expr)>),

    /// Member test: elem in collection
    In {
        elem: Box<Expr>,
        collection: Box<Expr>,
    },
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnOp {
    /// Logical not
    Not,

    /// Arithmetic negation
    Neg,
}

impl Contract {
    /// Create a new empty contract
    pub fn new(name: String) -> Self {
        Contract {
            name,
            participants: Vec::new(),
            currency: None,
            state_vars: Vec::new(),
            rules: Vec::new(),
            triggers: Vec::new(),
        }
    }

    /// Add a participant
    pub fn add_participant(mut self, did: Did) -> Self {
        self.participants.push(did);
        self
    }

    /// Set the currency
    pub fn with_currency(mut self, currency: String) -> Self {
        self.currency = Some(currency);
        self
    }

    /// Add a state variable
    pub fn add_state_var(mut self, name: String, initial_value: Value) -> Self {
        self.state_vars.push(StateVar {
            name,
            initial_value,
        });
        self
    }

    /// Add a rule
    pub fn add_rule(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Get a rule by name
    pub fn get_rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.name == name)
    }

    /// Validate contract structure and sanitize inputs
    ///
    /// Checks for:
    /// - Valid names (non-empty, length limits)
    /// - No reserved keywords used as identifiers
    /// - No name collisions (state vars, rules, params)
    /// - Bounded loop nesting depth
    /// - Bounded expression depth
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::{bail, Context};
        use std::collections::HashSet;

        // Validate contract name
        if self.name.is_empty() {
            bail!("Contract name cannot be empty");
        }
        if self.name.len() > MAX_NAME_LENGTH {
            bail!(
                "Contract name too long: {} chars (max {})",
                self.name.len(),
                MAX_NAME_LENGTH
            );
        }
        if RESERVED_KEYWORDS.contains(&self.name.as_str()) {
            bail!("Contract name '{}' is a reserved keyword", self.name);
        }

        // Validate currency
        if let Some(ref currency) = self.currency {
            if currency.is_empty() {
                bail!("Currency name cannot be empty");
            }
            if currency.len() > MAX_NAME_LENGTH {
                bail!("Currency name too long");
            }
        }

        // Validate participants (H1: Security fix)
        if self.participants.is_empty() {
            bail!("Contract must have at least one participant");
        }
        if self.participants.len() > MAX_PARTICIPANTS {
            bail!(
                "Too many participants: {} (max {})",
                self.participants.len(),
                MAX_PARTICIPANTS
            );
        }

        // Check for duplicate participants
        let mut participant_set = HashSet::new();
        for participant in &self.participants {
            if !participant_set.insert(participant) {
                bail!("Duplicate participant: {participant}");
            }
        }

        // Validate bounded counts (L1: Security fix)
        if self.state_vars.len() > MAX_STATE_VARS {
            bail!(
                "Too many state variables: {} (max {})",
                self.state_vars.len(),
                MAX_STATE_VARS
            );
        }
        if self.rules.len() > MAX_RULES {
            bail!("Too many rules: {} (max {})", self.rules.len(), MAX_RULES);
        }

        // Check for duplicate state variable names
        let mut state_names = HashSet::new();
        for var in &self.state_vars {
            if var.name.is_empty() {
                bail!("State variable name cannot be empty");
            }
            if var.name.len() > MAX_NAME_LENGTH {
                bail!("State variable name '{}' too long", var.name);
            }
            if RESERVED_KEYWORDS.contains(&var.name.as_str()) {
                bail!("State variable name '{}' is a reserved keyword", var.name);
            }
            if !state_names.insert(&var.name) {
                bail!("Duplicate state variable name: {}", var.name);
            }
        }

        // Validate rules
        let mut rule_names = HashSet::new();
        for rule in &self.rules {
            // Validate rule name
            if rule.name.is_empty() {
                bail!("Rule name cannot be empty");
            }
            if rule.name.len() > MAX_NAME_LENGTH {
                bail!("Rule name '{}' too long", rule.name);
            }
            if RESERVED_KEYWORDS.contains(&rule.name.as_str()) {
                bail!("Rule name '{}' is a reserved keyword", rule.name);
            }
            if !rule_names.insert(&rule.name) {
                bail!("Duplicate rule name: {}", rule.name);
            }

            // Validate parameters
            let mut param_names = HashSet::new();
            for param in &rule.params {
                if param.name.is_empty() {
                    bail!("Parameter name cannot be empty in rule '{}'", rule.name);
                }
                if param.name.len() > MAX_NAME_LENGTH {
                    bail!(
                        "Parameter name '{}' too long in rule '{}'",
                        param.name,
                        rule.name
                    );
                }
                if RESERVED_KEYWORDS.contains(&param.name.as_str()) {
                    bail!(
                        "Parameter name '{}' is a reserved keyword in rule '{}'",
                        param.name,
                        rule.name
                    );
                }
                if state_names.contains(&param.name) {
                    bail!(
                        "Parameter '{}' shadows state variable in rule '{}'",
                        param.name,
                        rule.name
                    );
                }
                if !param_names.insert(&param.name) {
                    bail!(
                        "Duplicate parameter name '{}' in rule '{}'",
                        param.name,
                        rule.name
                    );
                }
            }

            // Validate loop nesting depth in rule body
            Self::validate_stmt_depth(&rule.body, 0).context(format!("In rule '{}'", rule.name))?;

            // Validate expression depth in requires
            for (i, require) in rule.requires.iter().enumerate() {
                Self::validate_expr_depth(require, 0)
                    .context(format!("In rule '{}' require #{}", rule.name, i))?;
            }
        }

        Ok(())
    }

    /// Validate loop nesting depth in statements
    fn validate_stmt_depth(stmts: &[Stmt], current_depth: usize) -> anyhow::Result<()> {
        use anyhow::bail;

        for stmt in stmts {
            match stmt {
                Stmt::For { body, .. } => {
                    if current_depth >= MAX_LOOP_DEPTH {
                        bail!(
                            "Loop nesting too deep: {} (max {})",
                            current_depth + 1,
                            MAX_LOOP_DEPTH
                        );
                    }
                    Self::validate_stmt_depth(body, current_depth + 1)?;
                }
                Stmt::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    Self::validate_stmt_depth(then_block, current_depth)?;
                    if let Some(else_stmts) = else_block {
                        Self::validate_stmt_depth(else_stmts, current_depth)?;
                    }
                }
                Stmt::Assign { value, .. } => {
                    Self::validate_expr_depth(value, 0)?;
                }
                Stmt::SetState { value, .. } => {
                    Self::validate_expr_depth(value, 0)?;
                }
                Stmt::LedgerTransfer {
                    from,
                    to,
                    amount,
                    currency,
                } => {
                    Self::validate_expr_depth(from, 0)?;
                    Self::validate_expr_depth(to, 0)?;
                    Self::validate_expr_depth(amount, 0)?;
                    Self::validate_expr_depth(currency, 0)?;
                }
                Stmt::SetCreditLimit {
                    account,
                    currency,
                    limit,
                } => {
                    Self::validate_expr_depth(account, 0)?;
                    Self::validate_expr_depth(currency, 0)?;
                    Self::validate_expr_depth(limit, 0)?;
                }
                Stmt::Return { value } => {
                    Self::validate_expr_depth(value, 0)?;
                }
                Stmt::Expr(expr) => {
                    Self::validate_expr_depth(expr, 0)?;
                }
            }
        }
        Ok(())
    }

    /// Validate expression depth to prevent stack overflow
    fn validate_expr_depth(expr: &Expr, current_depth: usize) -> anyhow::Result<()> {
        use anyhow::bail;

        if current_depth >= MAX_EXPR_DEPTH {
            bail!("Expression nesting too deep: {current_depth} (max {MAX_EXPR_DEPTH})");
        }

        match expr {
            Expr::BinOp { left, right, .. } => {
                Self::validate_expr_depth(left, current_depth + 1)?;
                Self::validate_expr_depth(right, current_depth + 1)?;
            }
            Expr::UnOp { operand, .. } => {
                Self::validate_expr_depth(operand, current_depth + 1)?;
            }
            Expr::FieldAccess { object, .. } => {
                Self::validate_expr_depth(object, current_depth + 1)?;
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    Self::validate_expr_depth(arg, current_depth + 1)?;
                }
            }
            Expr::List(items) => {
                for item in items {
                    Self::validate_expr_depth(item, current_depth + 1)?;
                }
            }
            Expr::Set(items) => {
                for item in items {
                    Self::validate_expr_depth(item, current_depth + 1)?;
                }
            }
            Expr::Map(entries) => {
                for (_key, value) in entries {
                    Self::validate_expr_depth(value, current_depth + 1)?;
                }
            }
            Expr::In { elem, collection } => {
                Self::validate_expr_depth(elem, current_depth + 1)?;
                Self::validate_expr_depth(collection, current_depth + 1)?;
            }
            // Only Literal and Var don't contain nested expressions
            Expr::Literal(_) | Expr::Var(_) => {}
        }
        Ok(())
    }
}

impl Rule {
    /// Create a new rule
    pub fn new(name: String) -> Self {
        Rule {
            name,
            params: Vec::new(),
            requires: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Add a parameter
    pub fn add_param(mut self, name: String) -> Self {
        self.params.push(Param { name });
        self
    }

    /// Add a require condition
    pub fn add_require(mut self, condition: Expr) -> Self {
        self.requires.push(condition);
        self
    }

    /// Add a statement to the body
    pub fn add_stmt(mut self, stmt: Stmt) -> Self {
        self.body.push(stmt);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_empty_contract_name() {
        let contract = Contract::new("".to_string());
        let result = contract.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_reserved_keyword_as_name() {
        let contract = Contract::new("if".to_string());
        let result = contract.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("reserved keyword"));
    }

    #[test]
    fn test_duplicate_state_var() {
        let kp = KeyPair::generate().unwrap();
        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_state_var("count".to_string(), Value::Int(0))
            .add_state_var("count".to_string(), Value::Int(1));
        let result = contract.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate"));
    }

    #[test]
    fn test_valid_contract() {
        let kp = KeyPair::generate().unwrap();
        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_state_var("count".to_string(), Value::Int(0))
            .with_currency("hours".to_string());
        assert!(contract.validate().is_ok());
    }

    #[test]
    fn test_too_long_name() {
        let long_name = "a".repeat(MAX_NAME_LENGTH + 1);
        let contract = Contract::new(long_name);
        let result = contract.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_duplicate_rule_name() {
        let kp = KeyPair::generate().unwrap();
        let rule1 = Rule::new("transfer".to_string());
        let rule2 = Rule::new("transfer".to_string());

        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_rule(rule1)
            .add_rule(rule2);

        let result = contract.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate rule"));
    }

    #[test]
    fn test_deep_field_access_chain() {
        // Create deeply nested FieldAccess: obj.a.b.c.d...
        let kp = KeyPair::generate().unwrap();
        let mut expr = Expr::Var("obj".to_string());
        for _ in 0..MAX_EXPR_DEPTH + 5 {
            expr = Expr::FieldAccess {
                object: Box::new(expr),
                field: "field".to_string(),
            };
        }

        let rule = Rule::new("test".to_string()).add_stmt(Stmt::Return { value: expr });

        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_rule(rule);

        let result = contract.validate();
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("too deep"));
    }

    #[test]
    fn test_deep_set_nesting() {
        // Create deeply nested Set expressions
        let kp = KeyPair::generate().unwrap();
        let mut expr = Expr::Literal(Value::Int(1));
        for _ in 0..MAX_EXPR_DEPTH + 5 {
            expr = Expr::Set(vec![expr]);
        }

        let rule = Rule::new("test".to_string()).add_stmt(Stmt::Return { value: expr });

        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_rule(rule);

        let result = contract.validate();
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("too deep"));
    }

    #[test]
    fn test_deep_map_nesting() {
        // Create deeply nested Map expressions
        let kp = KeyPair::generate().unwrap();
        let mut expr = Expr::Literal(Value::Int(1));
        for _ in 0..MAX_EXPR_DEPTH + 5 {
            expr = Expr::Map(vec![("key".to_string(), expr)]);
        }

        let rule = Rule::new("test".to_string()).add_stmt(Stmt::Return { value: expr });

        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_rule(rule);

        let result = contract.validate();
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("too deep"));
    }

    #[test]
    fn test_deep_in_expression() {
        // Create deeply nested In expressions
        let kp = KeyPair::generate().unwrap();
        let mut expr = Expr::Literal(Value::Int(1));
        for _ in 0..MAX_EXPR_DEPTH + 5 {
            expr = Expr::In {
                elem: Box::new(Expr::Literal(Value::Int(1))),
                collection: Box::new(expr),
            };
        }

        let rule = Rule::new("test".to_string()).add_stmt(Stmt::Return { value: expr });

        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_rule(rule);

        let result = contract.validate();
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("too deep"));
    }

    #[test]
    fn test_shallow_expressions_accepted() {
        // Normal shallow expressions should pass validation
        let kp = KeyPair::generate().unwrap();
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Var("obj".to_string())),
            field: "name".to_string(),
        };

        let rule = Rule::new("test".to_string()).add_stmt(Stmt::Return { value: expr });

        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_rule(rule);

        assert!(contract.validate().is_ok());
    }

    // Security tests for participant validation (H1)
    #[test]
    fn test_empty_participants() {
        let contract = Contract::new("test".to_string());
        let result = contract.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one participant"));
    }

    #[test]
    fn test_duplicate_participants() {
        let kp = KeyPair::generate().unwrap();
        let contract = Contract::new("test".to_string())
            .add_participant(kp.did().clone())
            .add_participant(kp.did().clone()); // Duplicate!

        let result = contract.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate participant"));
    }

    #[test]
    fn test_too_many_participants() {
        let mut contract = Contract::new("test".to_string());

        // Add MAX_PARTICIPANTS + 1 participants
        for _ in 0..=MAX_PARTICIPANTS {
            let kp = KeyPair::generate().unwrap();
            contract = contract.add_participant(kp.did().clone());
        }

        let result = contract.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Too many participants"));
    }

    #[test]
    fn test_too_many_state_vars() {
        let kp = KeyPair::generate().unwrap();
        let mut contract = Contract::new("test".to_string()).add_participant(kp.did().clone());

        // Add MAX_STATE_VARS + 1 state variables
        for i in 0..=MAX_STATE_VARS {
            contract = contract.add_state_var(format!("var{i}"), Value::Int(0));
        }

        let result = contract.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Too many state variables"));
    }

    #[test]
    fn test_too_many_rules() {
        let kp = KeyPair::generate().unwrap();
        let mut contract = Contract::new("test".to_string()).add_participant(kp.did().clone());

        // Add MAX_RULES + 1 rules
        for i in 0..=MAX_RULES {
            let rule = Rule::new(format!("rule{i}"));
            contract = contract.add_rule(rule);
        }

        let result = contract.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Too many rules"));
    }
}
