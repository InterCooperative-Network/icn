//! CCL Contract Integration Tests
//!
//! These tests validate the full contract lifecycle:
//! 1. Contract creation and validation
//! 2. Contract deployment to registry
//! 3. Contract execution with rule invocation
//! 4. Ledger operation generation
//! 5. State management
//! 6. Capability checking
//! 7. Fuel metering
//! 8. Error scenarios

use icn_ccl::interpreter::Interpreter;
use icn_ccl::types::{Capability, ContractState, ExecutionContext};
use icn_ccl::{BinOp, Contract, ContractMetadata, ContractRegistry, Expr, Rule, Stmt, Value};
use icn_identity::KeyPair;
use std::collections::HashMap;

/// Helper to create a test identity
fn test_identity() -> icn_identity::Did {
    KeyPair::generate().unwrap().did().clone()
}

// === Contract Creation Tests ===

#[test]
fn test_create_simple_contract() {
    let alice = test_identity();

    let contract = Contract::new("SimpleContract".to_string()).add_participant(alice.clone());

    assert_eq!(contract.name, "SimpleContract");
    assert_eq!(contract.participants.len(), 1);
    assert!(contract.validate().is_ok());
}

#[test]
fn test_create_contract_with_rule() {
    let alice = test_identity();
    let bob = test_identity();

    let contract = Contract::new("PaymentContract".to_string())
        .add_participant(alice.clone())
        .add_participant(bob.clone())
        .with_currency("credits".to_string())
        .add_rule(
            Rule::new("pay".to_string())
                .add_param("amount".to_string())
                .add_stmt(Stmt::LedgerTransfer {
                    from: Expr::Var("sender".to_string()),
                    to: Expr::Literal(Value::Did(bob.clone())),
                    amount: Expr::Var("amount".to_string()),
                    currency: Expr::Literal(Value::String("credits".to_string())),
                }),
        );

    assert_eq!(contract.name, "PaymentContract");
    assert_eq!(contract.participants.len(), 2);
    assert_eq!(contract.rules.len(), 1);
    assert_eq!(contract.currency, Some("credits".to_string()));
    assert!(contract.validate().is_ok());
}

#[test]
fn test_create_contract_with_precondition() {
    let alice = test_identity();

    let contract = Contract::new("ValidatedContract".to_string())
        .add_participant(alice.clone())
        .add_rule(
            Rule::new("validated_action".to_string())
                .add_param("value".to_string())
                // Require value > 0
                .add_require(Expr::BinOp {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Var("value".to_string())),
                    right: Box::new(Expr::Literal(Value::Int(0))),
                })
                .add_stmt(Stmt::Return {
                    value: Expr::Var("value".to_string()),
                }),
        );

    assert!(contract.validate().is_ok());
    let rule = contract.get_rule("validated_action").unwrap();
    assert_eq!(rule.requires.len(), 1);
}

// === Contract Validation Tests ===

#[test]
fn test_contract_validation_empty_name_fails() {
    let alice = test_identity();

    let contract = Contract::new("".to_string()).add_participant(alice);

    assert!(contract.validate().is_err());
}

#[test]
fn test_contract_validation_no_participants_fails() {
    let contract = Contract::new("NoParticipants".to_string());

    assert!(contract.validate().is_err());
}

#[test]
fn test_contract_validation_duplicate_rule_names_fails() {
    let alice = test_identity();

    let contract = Contract::new("DuplicateRules".to_string())
        .add_participant(alice)
        .add_rule(Rule::new("action".to_string()))
        .add_rule(Rule::new("action".to_string())); // Duplicate

    assert!(contract.validate().is_err());
}

// === Registry Tests ===

#[tokio::test]
async fn test_deploy_contract_to_registry() {
    let alice = test_identity();

    let contract = Contract::new("RegistryTest".to_string())
        .add_participant(alice.clone())
        .add_rule(Rule::new("test_rule".to_string()));

    let registry = ContractRegistry::new();

    let hash = registry
        .deploy(contract.clone(), &alice.to_string(), None)
        .await
        .unwrap();

    // Verify contract is in registry
    let retrieved = registry.get(&hash).await.unwrap().unwrap();
    assert_eq!(retrieved.name, "RegistryTest");
}

#[tokio::test]
async fn test_deploy_and_get_metadata() {
    let alice = test_identity();

    let contract = Contract::new("MetadataTest".to_string())
        .add_participant(alice.clone())
        .with_currency("tokens".to_string())
        .add_rule(Rule::new("action1".to_string()))
        .add_rule(Rule::new("action2".to_string()));

    let registry = ContractRegistry::new();
    let hash = registry
        .deploy(contract.clone(), &alice.to_string(), None)
        .await
        .unwrap();

    let metadata = registry.get_metadata(&hash).await.unwrap().unwrap();

    assert_eq!(metadata.name, "MetadataTest");
    assert_eq!(metadata.owner, alice.to_string());
    assert_eq!(metadata.currency, Some("tokens".to_string()));
    assert_eq!(metadata.rules.len(), 2);
    assert!(metadata.rules.contains(&"action1".to_string()));
    assert!(metadata.rules.contains(&"action2".to_string()));
}

#[tokio::test]
async fn test_list_contracts_by_owner() {
    let alice = test_identity();
    let bob = test_identity();

    let contract1 = Contract::new("Alice1".to_string())
        .add_participant(alice.clone())
        .add_rule(Rule::new("rule1".to_string()));

    let contract2 = Contract::new("Alice2".to_string())
        .add_participant(alice.clone())
        .add_rule(Rule::new("rule2".to_string()));

    let contract3 = Contract::new("Bob1".to_string())
        .add_participant(bob.clone())
        .add_rule(Rule::new("rule3".to_string()));

    let registry = ContractRegistry::new();

    registry
        .deploy(contract1.clone(), &alice.to_string(), None)
        .await
        .unwrap();
    registry
        .deploy(contract2.clone(), &alice.to_string(), None)
        .await
        .unwrap();
    registry
        .deploy(contract3.clone(), &bob.to_string(), None)
        .await
        .unwrap();

    let alice_contracts = registry.list_by_owner(&alice.to_string()).await.unwrap();
    let bob_contracts = registry.list_by_owner(&bob.to_string()).await.unwrap();

    assert_eq!(alice_contracts.len(), 2);
    assert_eq!(bob_contracts.len(), 1);
}

#[tokio::test]
async fn test_deploy_duplicate_contract() {
    let alice = test_identity();

    let contract = Contract::new("Duplicate".to_string())
        .add_participant(alice.clone())
        .add_rule(Rule::new("rule".to_string()));

    let registry = ContractRegistry::new();

    let _hash1 = registry
        .deploy(contract.clone(), &alice.to_string(), None)
        .await
        .unwrap();
    let hash2_result = registry
        .deploy(contract.clone(), &alice.to_string(), None)
        .await;

    // Second deploy should error (AlreadyExists)
    assert!(hash2_result.is_err());
}

// === Interpreter Execution Tests ===

#[test]
fn test_execute_arithmetic_rule() {
    let alice = test_identity();

    let contract = Contract::new("Calculator".to_string())
        .add_participant(alice.clone())
        .add_rule(
            Rule::new("add".to_string())
                .add_param("a".to_string())
                .add_param("b".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Var("a".to_string())),
                        right: Box::new(Expr::Var("b".to_string())),
                    },
                }),
        );

    let state = ContractState::new();
    let context = ExecutionContext::new(
        alice.clone(),
        0,                   // timestamp
        1000,                // fuel
        vec![],              // capabilities
        vec![alice.clone()], // participants
    );

    let mut args = HashMap::new();
    args.insert("a".to_string(), Value::Int(5));
    args.insert("b".to_string(), Value::Int(3));

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("add", args).unwrap();

    assert_eq!(result.value, Value::Int(8));
    assert!(result.fuel_consumed > 0, "Expected fuel to be consumed");
}

#[test]
fn test_execute_rule_with_precondition_pass() {
    let alice = test_identity();

    let contract = Contract::new("Validated".to_string())
        .add_participant(alice.clone())
        .add_rule(
            Rule::new("positive_only".to_string())
                .add_param("value".to_string())
                .add_require(Expr::BinOp {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Var("value".to_string())),
                    right: Box::new(Expr::Literal(Value::Int(0))),
                })
                .add_stmt(Stmt::Return {
                    value: Expr::Var("value".to_string()),
                }),
        );

    let state = ContractState::new();
    let context = ExecutionContext::new(alice.clone(), 0, 1000, vec![], vec![alice.clone()]);

    let mut args = HashMap::new();
    args.insert("value".to_string(), Value::Int(10));

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("positive_only", args).unwrap();

    assert_eq!(result.value, Value::Int(10));
}

#[test]
fn test_execute_rule_with_precondition_fail() {
    let alice = test_identity();

    let contract = Contract::new("Validated".to_string())
        .add_participant(alice.clone())
        .add_rule(
            Rule::new("positive_only".to_string())
                .add_param("value".to_string())
                .add_require(Expr::BinOp {
                    op: BinOp::Gt,
                    left: Box::new(Expr::Var("value".to_string())),
                    right: Box::new(Expr::Literal(Value::Int(0))),
                })
                .add_stmt(Stmt::Return {
                    value: Expr::Var("value".to_string()),
                }),
        );

    let state = ContractState::new();
    let context = ExecutionContext::new(alice.clone(), 0, 1000, vec![], vec![alice.clone()]);

    let mut args = HashMap::new();
    args.insert("value".to_string(), Value::Int(-5)); // Negative value fails precondition

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("positive_only", args);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Precondition"));
}

#[test]
fn test_execute_rule_not_found() {
    let alice = test_identity();

    let contract = Contract::new("Test".to_string())
        .add_participant(alice.clone())
        .add_rule(Rule::new("existing_rule".to_string()));

    let state = ContractState::new();
    let context = ExecutionContext::new(alice.clone(), 0, 1000, vec![], vec![alice.clone()]);

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("nonexistent_rule", HashMap::new());

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Rule not found"));
}

// === Fuel Metering Tests ===

#[test]
fn test_fuel_consumed_during_execution() {
    let alice = test_identity();

    let contract = Contract::new("FuelTest".to_string())
        .add_participant(alice.clone())
        .add_rule(
            Rule::new("compute".to_string())
                .add_stmt(Stmt::Assign {
                    name: "x".to_string(),
                    value: Expr::Literal(Value::Int(1)),
                })
                .add_stmt(Stmt::Assign {
                    name: "y".to_string(),
                    value: Expr::Literal(Value::Int(2)),
                })
                .add_stmt(Stmt::Return {
                    value: Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Var("x".to_string())),
                        right: Box::new(Expr::Var("y".to_string())),
                    },
                }),
        );

    let state = ContractState::new();
    let context = ExecutionContext::new(alice.clone(), 0, 1000, vec![], vec![alice.clone()]);

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("compute", HashMap::new()).unwrap();

    assert_eq!(result.value, Value::Int(3));
    // Should have consumed fuel: at least 1 per statement (3 stmts) + expressions
    assert!(
        result.fuel_consumed >= 3,
        "Expected at least 3 fuel consumed, got {}",
        result.fuel_consumed
    );
}

#[test]
fn test_fuel_exhaustion() {
    let alice = test_identity();

    let contract = Contract::new("FuelExhaustion".to_string())
        .add_participant(alice.clone())
        .add_rule(
            Rule::new("expensive".to_string())
                .add_stmt(Stmt::Assign {
                    name: "a".to_string(),
                    value: Expr::Literal(Value::Int(1)),
                })
                .add_stmt(Stmt::Assign {
                    name: "b".to_string(),
                    value: Expr::Literal(Value::Int(2)),
                })
                .add_stmt(Stmt::Assign {
                    name: "c".to_string(),
                    value: Expr::Literal(Value::Int(3)),
                }),
        );

    let state = ContractState::new();
    // Only give 1 fuel - should run out
    let context = ExecutionContext::new(alice.clone(), 0, 1, vec![], vec![alice.clone()]);

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("expensive", HashMap::new());

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(err_msg.contains("fuel") || err_msg.contains("out of"));
}

// === Ledger Operation Tests ===

#[test]
fn test_ledger_transfer_operation() {
    let alice = test_identity();
    let bob = test_identity();

    let contract = Contract::new("Transfer".to_string())
        .add_participant(alice.clone())
        .add_participant(bob.clone())
        .with_currency("credits".to_string())
        .add_rule(
            Rule::new("transfer".to_string())
                .add_param("amount".to_string())
                .add_stmt(Stmt::LedgerTransfer {
                    from: Expr::Var("sender".to_string()),
                    to: Expr::Literal(Value::Did(bob.clone())),
                    amount: Expr::Var("amount".to_string()),
                    currency: Expr::Literal(Value::String("credits".to_string())),
                }),
        );

    let state = ContractState::new();
    let context = ExecutionContext::new(
        alice.clone(),
        0,
        1000,
        // Grant WriteLedger capability
        vec![Capability::WriteLedger {
            accounts: vec![alice.clone(), bob.clone()],
        }],
        vec![alice.clone(), bob.clone()],
    );

    let mut args = HashMap::new();
    args.insert("amount".to_string(), Value::Int(100));

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("transfer", args).unwrap();

    // Should have generated a transfer operation
    assert_eq!(result.ledger_ops.len(), 1);
    match &result.ledger_ops[0] {
        icn_ccl::types::LedgerOperation::Transfer {
            from,
            to,
            amount,
            currency,
        } => {
            assert_eq!(from, &alice);
            assert_eq!(to, &bob);
            assert_eq!(*amount, 100);
            assert_eq!(currency, "credits");
        }
        _ => panic!("Expected Transfer operation"),
    }
}

// === State Management Tests ===

#[test]
fn test_state_variable_initialization() {
    let alice = test_identity();

    let contract = Contract::new("StatefulContract".to_string())
        .add_participant(alice.clone())
        .add_state_var("counter".to_string(), Value::Int(0))
        .add_state_var("name".to_string(), Value::String("test".to_string()));

    assert_eq!(contract.state_vars.len(), 2);
}

// === JSON Serialization Tests ===

#[test]
fn test_contract_json_roundtrip() {
    let alice = test_identity();

    let contract = Contract::new("JsonTest".to_string())
        .add_participant(alice.clone())
        .with_currency("tokens".to_string())
        .add_rule(
            Rule::new("action".to_string())
                .add_param("value".to_string())
                .add_stmt(Stmt::Return {
                    value: Expr::Var("value".to_string()),
                }),
        );

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&contract).unwrap();

    // Deserialize back
    let deserialized: Contract = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.name, contract.name);
    assert_eq!(deserialized.participants.len(), contract.participants.len());
    assert_eq!(deserialized.rules.len(), contract.rules.len());
    assert_eq!(deserialized.currency, contract.currency);
}

// === Comparison Operations Tests ===

#[test]
fn test_comparison_operations() {
    let alice = test_identity();

    // Test all comparison operators
    // Using: Eq, Ne, Lt, Le, Gt, Ge
    let tests = vec![
        (BinOp::Eq, 5, 5, true),
        (BinOp::Eq, 5, 3, false),
        (BinOp::Ne, 5, 3, true),
        (BinOp::Ne, 5, 5, false),
        (BinOp::Lt, 3, 5, true),
        (BinOp::Lt, 5, 5, false),
        (BinOp::Le, 5, 5, true),
        (BinOp::Le, 6, 5, false),
        (BinOp::Gt, 5, 3, true),
        (BinOp::Gt, 3, 5, false),
        (BinOp::Ge, 5, 5, true),
        (BinOp::Ge, 4, 5, false),
    ];

    for (op, a, b, expected) in tests {
        let contract = Contract::new("Compare".to_string())
            .add_participant(alice.clone())
            .add_rule(Rule::new("compare".to_string()).add_stmt(Stmt::Return {
                value: Expr::BinOp {
                    op,
                    left: Box::new(Expr::Literal(Value::Int(a))),
                    right: Box::new(Expr::Literal(Value::Int(b))),
                },
            }));

        let state = ContractState::new();
        let context = ExecutionContext::new(alice.clone(), 0, 100, vec![], vec![alice.clone()]);

        let interpreter = Interpreter::new(contract, state, context);
        let result = interpreter.execute_rule("compare", HashMap::new()).unwrap();

        assert_eq!(
            result.value,
            Value::Bool(expected),
            "Failed for {:?}({}, {})",
            op,
            a,
            b
        );
    }
}

// === Metadata Creation Tests ===

#[test]
fn test_contract_metadata_creation() {
    let alice = test_identity();

    let contract = Contract::new("MetadataTest".to_string())
        .add_participant(alice.clone())
        .with_currency("credits".to_string())
        .add_rule(Rule::new("rule1".to_string()))
        .add_rule(Rule::new("rule2".to_string()));

    let metadata = ContractMetadata::from_contract(&contract, &alice.to_string(), 1).unwrap();

    assert_eq!(metadata.name, "MetadataTest");
    assert_eq!(metadata.owner, alice.to_string());
    assert_eq!(metadata.version, 1);
    assert_eq!(metadata.currency, Some("credits".to_string()));
    assert_eq!(metadata.rules.len(), 2);
    assert!(metadata.deployed_at > 0);
}

// === Additional Tests for Error Cases ===

#[test]
fn test_missing_capability_for_ledger_transfer() {
    let alice = test_identity();
    let bob = test_identity();

    let contract = Contract::new("Transfer".to_string())
        .add_participant(alice.clone())
        .add_participant(bob.clone())
        .add_rule(
            Rule::new("transfer".to_string())
                .add_param("amount".to_string())
                .add_stmt(Stmt::LedgerTransfer {
                    from: Expr::Var("sender".to_string()),
                    to: Expr::Literal(Value::Did(bob.clone())),
                    amount: Expr::Var("amount".to_string()),
                    currency: Expr::Literal(Value::String("credits".to_string())),
                }),
        );

    let state = ContractState::new();
    // No WriteLedger capability granted
    let context = ExecutionContext::new(
        alice.clone(),
        0,
        1000,
        vec![], // No capabilities
        vec![alice.clone(), bob.clone()],
    );

    let mut args = HashMap::new();
    args.insert("amount".to_string(), Value::Int(100));

    let interpreter = Interpreter::new(contract, state, context);
    let result = interpreter.execute_rule("transfer", args);

    // Should fail due to missing capability
    assert!(result.is_err());
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains("capability") || err.contains("permission") || err.contains("unauthorized")
    );
}

#[tokio::test]
async fn test_get_nonexistent_contract() {
    let registry = ContractRegistry::new();
    let fake_hash = [0u8; 32];

    let result = registry.get(&fake_hash).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_resolve_by_name() {
    let alice = test_identity();

    let contract = Contract::new("NamedContract".to_string())
        .add_participant(alice.clone())
        .add_rule(Rule::new("action".to_string()));

    let registry = ContractRegistry::new();
    let hash = registry
        .deploy(contract.clone(), &alice.to_string(), Some(1))
        .await
        .unwrap();

    // Resolve by name (latest)
    let resolved = registry
        .resolve("NamedContract", None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved, hash);

    // Resolve by name + version
    let resolved_v1 = registry
        .resolve("NamedContract", Some(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved_v1, hash);

    // Non-existent version
    let no_v2 = registry.resolve("NamedContract", Some(2)).await.unwrap();
    assert!(no_v2.is_none());
}
