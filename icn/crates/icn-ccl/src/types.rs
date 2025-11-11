//! Core types for Cooperative Contract Language (CCL)

use icn_identity::Did;
use icn_ledger::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Capability granted to a contract
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Read ledger balances for specific accounts
    ReadLedger { accounts: Vec<Did> },

    /// Write ledger entries for specific accounts
    WriteLedger { accounts: Vec<Did> },

    /// Send messages to specific participants
    SendMessage { to: Vec<Did> },

    /// Read contract state keys
    ReadState { keys: Vec<String> },

    /// Write contract state keys
    WriteState { keys: Vec<String> },

    /// Create sub-contracts
    CreateSubContract,

    /// Invoke another contract
    InvokeContract { contract: ContentHash },
}

/// Contract installation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInstallation {
    /// Hash of the contract code
    pub code_hash: ContentHash,

    /// Who installed this contract
    pub installed_by: Did,

    /// Capabilities granted to this contract
    pub capabilities: Vec<Capability>,

    /// Contract participants (must all sign)
    pub participants: Vec<Did>,

    /// Signatures from participants
    pub signatures: Vec<(Did, Vec<u8>)>,

    /// When the contract was installed
    pub installed_at: u64,
}

/// Contract state (key-value store per contract)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractState {
    /// State storage
    pub data: HashMap<String, Value>,
}

impl ContractState {
    /// Create new empty state
    pub fn new() -> Self {
        ContractState {
            data: HashMap::new(),
        }
    }

    /// Get a value from state
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Set a value in state
    pub fn set(&mut self, key: String, value: Value) {
        self.data.insert(key, value);
    }

    /// Remove a value from state
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.data.remove(key)
    }
}

/// Value types in CCL
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    /// Integer value (i64 for fixed-point math)
    Int(i64),

    /// String value
    String(String),

    /// Boolean value
    Bool(bool),

    /// DID identifier
    Did(Did),

    /// List of values
    List(Vec<Value>),

    /// Set of values (for participants, etc.)
    Set(HashSet<Value>),

    /// Map of values
    Map(HashMap<String, Value>),

    /// None/null value
    None,
}

impl Value {
    /// Check if value is truthy
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Set(s) => !s.is_empty(),
            Value::Map(m) => !m.is_empty(),
            Value::None => false,
            Value::Did(_) => true,
        }
    }

    /// Convert to i64 if possible
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Convert to string if possible
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Convert to bool if possible
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Convert to DID if possible
    pub fn as_did(&self) -> Option<&Did> {
        match self {
            Value::Did(did) => Some(did),
            _ => None,
        }
    }
}

// Implement Hash for Value to use in Sets
impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Value::Int(i) => {
                0u8.hash(state);
                i.hash(state);
            }
            Value::String(s) => {
                1u8.hash(state);
                s.hash(state);
            }
            Value::Bool(b) => {
                2u8.hash(state);
                b.hash(state);
            }
            Value::Did(did) => {
                3u8.hash(state);
                // Hash the DID's string representation
                format!("{:?}", did).hash(state);
            }
            Value::List(list) => {
                4u8.hash(state);
                for item in list {
                    item.hash(state);
                }
            }
            Value::Set(set) => {
                5u8.hash(state);
                let mut items: Vec<_> = set.iter().collect();
                items.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
                for item in items {
                    item.hash(state);
                }
            }
            Value::Map(map) => {
                6u8.hash(state);
                let mut items: Vec<_> = map.iter().collect();
                items.sort_by_key(|(k, _)| *k);
                for (k, v) in items {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::None => {
                7u8.hash(state);
            }
        }
    }
}

/// Execution context for contract runtime
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Who invoked the contract
    pub caller: Did,

    /// Current timestamp (passed in for determinism)
    pub timestamp: u64,

    /// Available fuel for execution
    pub fuel: u64,

    /// Contract capabilities
    pub capabilities: Vec<Capability>,

    /// Contract participants
    pub participants: Vec<Did>,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(
        caller: Did,
        timestamp: u64,
        fuel: u64,
        capabilities: Vec<Capability>,
        participants: Vec<Did>,
    ) -> Self {
        ExecutionContext {
            caller,
            timestamp,
            fuel,
            capabilities,
            participants,
        }
    }

    /// Consume fuel, return error if depleted
    pub fn consume_fuel(&mut self, amount: u64) -> anyhow::Result<()> {
        if self.fuel < amount {
            anyhow::bail!("Out of fuel");
        }
        self.fuel -= amount;
        Ok(())
    }

    /// Check if caller is a participant
    pub fn is_participant(&self, did: &Did) -> bool {
        self.participants.contains(did)
    }

    /// Check if capability is granted
    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }
}

/// Result of contract execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Return value
    pub value: Value,

    /// Fuel consumed
    pub fuel_consumed: u64,

    /// State changes
    pub state_changes: HashMap<String, Value>,

    /// Ledger operations (if any)
    pub ledger_ops: Vec<LedgerOperation>,
}

/// Ledger operation requested by contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedgerOperation {
    /// Transfer between accounts
    Transfer {
        from: Did,
        to: Did,
        amount: i64,
        currency: String,
    },

    /// Set credit limit
    SetCreditLimit {
        account: Did,
        currency: String,
        limit: i64,
    },
}
