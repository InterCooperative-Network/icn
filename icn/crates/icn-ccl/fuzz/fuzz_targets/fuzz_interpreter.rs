//! Fuzz target for Contract interpreter execution
//!
//! This target tests the interpreter's ability to execute arbitrary contracts
//! without panicking. It exercises:
//! - Expression evaluation
//! - Statement execution
//! - Fuel metering
//! - Precondition checking
//! - Loop iteration limits
//! - Capability checking
//!
//! The goal is to find inputs that cause panics, infinite loops, or excessive
//! resource consumption during interpretation.

#![no_main]

use libfuzzer_sys::fuzz_target;
use icn_ccl::{Contract, Interpreter, Value};
use std::collections::HashMap;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 string first
    let s = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Attempt to deserialize as a Contract
    let contract: Contract = match serde_json::from_str(s) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Validate the contract - skip invalid ones to focus on interpreter bugs
    if contract.validate().is_err() {
        return;
    }

    // Skip contracts with no rules
    if contract.rules.is_empty() {
        return;
    }

    // Generate a caller DID for execution context
    // Use the first participant if available
    let caller = match contract.participants.first() {
        Some(did) => did.clone(),
        None => return,
    };

    // Create execution context with limited fuel to prevent resource exhaustion
    let context = icn_ccl::ExecutionContext::new(
        caller.clone(),
        0,      // timestamp
        10000,  // fuel limit - reasonable for fuzzing
        vec![], // no special capabilities
        contract.participants.clone(),
    );

    // Initialize contract state from state_vars
    let mut state = icn_ccl::ContractState::new();
    for var in &contract.state_vars {
        state.set(var.name.clone(), var.initial_value.clone());
    }

    // Get the first rule name
    let rule_name = contract.rules[0].name.clone();

    // Build empty arguments for the rule's parameters
    // Use None for all parameters since we don't know what values are expected
    let args: HashMap<String, Value> = contract.rules[0]
        .params
        .iter()
        .map(|p| (p.name.clone(), Value::None))
        .collect();

    // Create interpreter and execute rule
    let interpreter = Interpreter::new(contract, state, context);

    // Execute and ignore all errors - we just want to catch panics
    let _ = interpreter.execute_rule(&rule_name, args);
});
