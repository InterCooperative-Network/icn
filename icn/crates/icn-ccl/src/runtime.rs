//! Contract runtime that integrates interpreter with ledger

use crate::ast::Contract;
use crate::interpreter::Interpreter;
use crate::types::{ContractState, ExecutionContext, ExecutionResult, LedgerOperation};
use anyhow::{Context as _, Result};
use icn_ledger::{entry::JournalEntryBuilder, AccountDelta, ContentHash, Ledger};
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Contract runtime manages contract execution and ledger integration
pub struct ContractRuntime {
    /// Ledger instance
    ledger: Arc<RwLock<Ledger>>,

    /// Installed contracts (code_hash -> Contract)
    contracts: HashMap<ContentHash, Contract>,

    /// Contract states (code_hash -> State)
    states: HashMap<ContentHash, ContractState>,
}

impl ContractRuntime {
    /// Create a new contract runtime
    pub fn new(ledger: Arc<RwLock<Ledger>>) -> Self {
        ContractRuntime {
            ledger,
            contracts: HashMap::new(),
            states: HashMap::new(),
        }
    }

    /// Install a contract
    pub fn install_contract(
        &mut self,
        code_hash: ContentHash,
        contract: Contract,
    ) -> Result<()> {
        info!("Installing contract: {} ({})", contract.name, code_hash);

        // Validate contract structure before installation
        contract.validate().context("Contract validation failed")?;

        // Initialize state with state variables
        let mut state = ContractState::new();
        for state_var in &contract.state_vars {
            state.set(state_var.name.clone(), state_var.initial_value.clone());
        }

        self.contracts.insert(code_hash.clone(), contract);
        self.states.insert(code_hash, state);

        Ok(())
    }

    /// Execute a contract rule
    pub async fn execute_rule(
        &mut self,
        code_hash: &ContentHash,
        rule_name: &str,
        context: ExecutionContext,
        args: HashMap<String, crate::types::Value>,
    ) -> Result<ExecutionResult> {
        let contract = self
            .contracts
            .get(code_hash)
            .context("Contract not found")?
            .clone();

        let state = self
            .states
            .get(code_hash)
            .context("Contract state not found")?
            .clone();

        debug!(
            "Executing contract {} rule {}",
            contract.name, rule_name
        );

        // Run interpreter
        let interpreter = Interpreter::new(contract, state, context);
        let result = interpreter.execute_rule(rule_name, args)?;

        // Apply ledger operations
        if !result.ledger_ops.is_empty() {
            self.apply_ledger_operations(&result.ledger_ops).await?;
        }

        // Update contract state
        let new_state = ContractState {
            data: result.state_changes.clone(),
        };
        self.states.insert(code_hash.clone(), new_state);

        Ok(result)
    }

    /// Apply ledger operations from contract execution
    async fn apply_ledger_operations(&self, ops: &[LedgerOperation]) -> Result<()> {
        let mut ledger = self.ledger.write().await;

        for op in ops {
            match op {
                LedgerOperation::Transfer {
                    from,
                    to,
                    amount,
                    currency,
                } => {
                    debug!(
                        "Ledger transfer: {} -> {} ({} {})",
                        from, to, amount, currency
                    );

                    // Create journal entry
                    let entry = JournalEntryBuilder::new(from.clone())
                        .debit(from.clone(), currency.clone(), *amount)
                        .credit(to.clone(), currency.clone(), *amount)
                        .build()?;

                    ledger.append_entry(entry)?;
                }

                LedgerOperation::SetCreditLimit {
                    account,
                    currency,
                    limit,
                } => {
                    debug!(
                        "Set credit limit: {} = {} for {}",
                        account, limit, currency
                    );
                    // Note: Credit limits would be stored separately in a real implementation
                    // For now, we just log it
                }
            }
        }

        Ok(())
    }

    /// Get contract state
    pub fn get_state(&self, code_hash: &ContentHash) -> Option<&ContractState> {
        self.states.get(code_hash)
    }

    /// Get installed contract
    pub fn get_contract(&self, code_hash: &ContentHash) -> Option<&Contract> {
        self.contracts.get(code_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Rule, Stmt};
    use crate::types::{Capability, Value};
    use icn_identity::KeyPair;
    use icn_ledger::ContentHash as LedgerHash;
    use icn_store::SledStore;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_contract_execution() {
        // Setup
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(store).unwrap()));
        let mut runtime = ContractRuntime::new(ledger.clone());

        // Create test identities
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create a simple contract
        let contract = Contract::new("test".to_string())
            .add_participant(alice.clone())
            .add_participant(bob.clone())
            .with_currency("hours".to_string())
            .add_rule(
                Rule::new("transfer".to_string())
                    .add_param("recipient".to_string())
                    .add_param("amount".to_string())
                    .add_stmt(Stmt::LedgerTransfer {
                        from: Expr::Var("sender".to_string()),
                        to: Expr::Var("recipient".to_string()),
                        amount: Expr::Var("amount".to_string()),
                        currency: Expr::Literal(Value::String("hours".to_string())),
                    }),
            );

        // Install contract
        let code_hash = LedgerHash::from_bytes([1u8; 32]);
        runtime.install_contract(code_hash.clone(), contract).unwrap();

        // Execute rule
        let context = ExecutionContext::new(
            alice.clone(),
            1234567890,
            10000,
            vec![Capability::WriteLedger {
                accounts: vec![alice.clone(), bob.clone()],
            }],
            vec![alice.clone(), bob.clone()],
        );

        let mut args = HashMap::new();
        args.insert("recipient".to_string(), Value::Did(bob.clone()));
        args.insert("amount".to_string(), Value::Int(10));

        let result = runtime
            .execute_rule(&code_hash, "transfer", context, args)
            .await
            .unwrap();

        // Verify ledger operation was created
        assert_eq!(result.ledger_ops.len(), 1);
        match &result.ledger_ops[0] {
            LedgerOperation::Transfer {
                from,
                to,
                amount,
                currency,
            } => {
                assert_eq!(from, &alice);
                assert_eq!(to, &bob);
                assert_eq!(*amount, 10);
                assert_eq!(currency, "hours");
            }
            _ => panic!("Expected Transfer operation"),
        }

        // Verify ledger entry was created
        let ledger_read = ledger.read().await;
        assert_eq!(ledger_read.get_balance(&bob, "hours"), 10);
        assert_eq!(ledger_read.get_balance(&alice, "hours"), -10);
    }
}
