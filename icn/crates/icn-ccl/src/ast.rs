//! Abstract Syntax Tree for CCL contracts

use crate::types::Value;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

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
    /// Variable assignment: x = expr
    Assign { name: String, value: Expr },

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
