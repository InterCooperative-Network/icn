//! TimeBank contract example
//!
//! Demonstrates a time banking system where members exchange services measured in hours.

use icn_ccl::{BinOp, Capability, Contract, ContractRuntime, ExecutionContext, Expr, Rule, Stmt, Value};
use icn_identity::KeyPair;
use icn_ledger::{ContentHash, Ledger};
use icn_store::SledStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize ledger
    let store = Arc::new(SledStore::temporary()?);
    let ledger = Arc::new(RwLock::new(Ledger::new(store)?));
    let mut runtime = ContractRuntime::new(ledger.clone());

    // Create participants
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();
    let charlie = KeyPair::generate()?.did().clone();

    println!("=== TimeBank Contract Example ===\n");
    println!("Participants:");
    println!("  Alice:   {}", alice);
    println!("  Bob:     {}", bob);
    println!("  Charlie: {}\n", charlie);

    // Build TimeBank contract
    let contract = Contract::new("TimeBank".to_string())
        .add_participant(alice.clone())
        .add_participant(bob.clone())
        .add_participant(charlie.clone())
        .with_currency("hours".to_string())
        // State variable: credit limit
        .add_state_var("credit_limit".to_string(), Value::Int(100))
        // Rule: record_service (transfer hours for work completed)
        .add_rule(
            Rule::new("record_service".to_string())
                .add_param("recipient".to_string())
                .add_param("hours".to_string())
                // Require: sender is a participant
                .add_require(Expr::In {
                    elem: Box::new(Expr::Var("sender".to_string())),
                    collection: Box::new(Expr::Var("participants".to_string())),
                })
                // Require: recipient is a participant
                .add_require(Expr::In {
                    elem: Box::new(Expr::Var("recipient".to_string())),
                    collection: Box::new(Expr::Var("participants".to_string())),
                })
                // Require: hours > 0
                .add_require(Expr::BinOp {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Var("hours".to_string())),
                    right: Box::new(Expr::Literal(Value::Int(0))),
                })
                // Transfer hours from sender to recipient
                .add_stmt(Stmt::LedgerTransfer {
                    from: Expr::Var("sender".to_string()),
                    to: Expr::Var("recipient".to_string()),
                    amount: Expr::Var("hours".to_string()),
                    currency: Expr::Literal(Value::String("hours".to_string())),
                }),
        )
        // Rule: check_balance (returns sender's balance)
        .add_rule(
            Rule::new("check_balance".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::String("Balance check not yet implemented".to_string())),
                }),
        );

    // Install contract
    let code_hash = ContentHash::from_bytes([42u8; 32]);
    runtime.install_contract(code_hash.clone(), contract)?;

    println!("Contract installed: {}\n", code_hash);

    // Scenario: Alice provides 3 hours of web design to Bob
    println!("Scenario 1: Alice provides 3 hours of web design to Bob");

    let context1 = ExecutionContext::new(
        alice.clone(),
        1700000000,
        10000,
        vec![Capability::WriteLedger {
            accounts: vec![alice.clone(), bob.clone(), charlie.clone()],
        }],
        vec![alice.clone(), bob.clone(), charlie.clone()],
    );

    let mut args1 = HashMap::new();
    args1.insert("recipient".to_string(), Value::Did(bob.clone()));
    args1.insert("hours".to_string(), Value::Int(3));

    let result1 = runtime
        .execute_rule(&code_hash, "record_service", context1, args1)
        .await?;

    println!("  Execution result:");
    println!("    Fuel consumed: {}", result1.fuel_consumed);
    println!("    Ledger ops: {}", result1.ledger_ops.len());

    // Check balances
    let ledger_read = ledger.read().await;
    println!("\n  Balances after transaction:");
    println!("    Alice: {} hours", ledger_read.get_balance(&alice, "hours"));
    println!("    Bob:   {} hours\n", ledger_read.get_balance(&bob, "hours"));
    drop(ledger_read);

    // Scenario: Bob provides 5 hours of carpentry to Charlie
    println!("Scenario 2: Bob provides 5 hours of carpentry to Charlie");

    let context2 = ExecutionContext::new(
        bob.clone(),
        1700000100,
        10000,
        vec![Capability::WriteLedger {
            accounts: vec![alice.clone(), bob.clone(), charlie.clone()],
        }],
        vec![alice.clone(), bob.clone(), charlie.clone()],
    );

    let mut args2 = HashMap::new();
    args2.insert("recipient".to_string(), Value::Did(charlie.clone()));
    args2.insert("hours".to_string(), Value::Int(5));

    let result2 = runtime
        .execute_rule(&code_hash, "record_service", context2, args2)
        .await?;

    println!("  Fuel consumed: {}", result2.fuel_consumed);

    // Check final balances
    let ledger_read = ledger.read().await;
    println!("\n  Final balances:");
    println!("    Alice:   {} hours (earned 3)", ledger_read.get_balance(&alice, "hours"));
    println!("    Bob:     {} hours (earned 5, spent 3)", ledger_read.get_balance(&bob, "hours"));
    println!("    Charlie: {} hours (spent 5)", ledger_read.get_balance(&charlie, "hours"));

    println!("\n=== TimeBank demonstration complete ===");

    Ok(())
}
