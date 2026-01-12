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

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use icn_ccl::{Contract, Interpreter, Value};
use std::collections::HashMap;

/// Input structure for the interpreter fuzzer
///
/// Uses structured fuzzing to generate both the contract JSON and
/// parameter values, enabling deeper coverage of interpreter logic.
#[derive(Debug, Arbitrary)]
struct FuzzInput<'a> {
    /// Contract JSON (will be parsed)
    contract_json: &'a str,
    /// Seed for generating parameter values
    param_seed: u64,
    /// Rule index to execute (modulo number of rules)
    rule_index: u8,
    /// Values to use for integer parameters
    int_values: Vec<i64>,
    /// Values to use for string parameters
    string_values: Vec<&'a str>,
    /// Values to use for bool parameters
    bool_values: Vec<bool>,
}

/// Generate a Value based on parameter name heuristics and seed data
fn generate_param_value(
    param_name: &str,
    seed: u64,
    int_values: &[i64],
    string_values: &[&str],
    bool_values: &[bool],
    participants: &[icn_identity::Did],
) -> Value {
    let name_lower = param_name.to_lowercase();

    // Heuristics based on common parameter naming conventions
    if name_lower.contains("amount") || name_lower.contains("value")
        || name_lower.contains("hours") || name_lower.contains("count")
        || name_lower.contains("limit") || name_lower.contains("rate") {
        // Numeric parameter - use fuzzed int or seed-based value
        let idx = (seed as usize) % (int_values.len().max(1));
        Value::Int(int_values.get(idx).copied().unwrap_or(seed as i64))
    } else if name_lower.contains("recipient") || name_lower.contains("from")
        || name_lower.contains("to") || name_lower.contains("account")
        || name_lower.contains("member") || name_lower.contains("voter") {
        // DID parameter - use a participant if available
        if !participants.is_empty() {
            let idx = (seed as usize) % participants.len();
            Value::Did(participants[idx].clone())
        } else {
            Value::None
        }
    } else if name_lower.contains("name") || name_lower.contains("currency")
        || name_lower.contains("reason") || name_lower.contains("message") {
        // String parameter
        let idx = (seed as usize) % (string_values.len().max(1));
        Value::String(string_values.get(idx).map(|s| s.to_string())
            .unwrap_or_else(|| format!("fuzz_{seed}")))
    } else if name_lower.contains("enabled") || name_lower.contains("active")
        || name_lower.contains("approved") || name_lower.contains("is_") {
        // Boolean parameter
        let idx = (seed as usize) % (bool_values.len().max(1));
        Value::Bool(bool_values.get(idx).copied().unwrap_or(seed % 2 == 0))
    } else {
        // Default: use seed to pick a type
        match seed % 5 {
            0 => Value::Int(seed as i64),
            1 => Value::String(format!("param_{seed}")),
            2 => Value::Bool(seed % 2 == 0),
            3 => Value::None,
            _ => {
                // Use participant DID if available
                if !participants.is_empty() {
                    let idx = (seed as usize) % participants.len();
                    Value::Did(participants[idx].clone())
                } else {
                    Value::Int(seed as i64)
                }
            }
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    // Attempt to deserialize as a Contract
    let contract: Contract = match serde_json::from_str(input.contract_json) {
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
        input.param_seed,  // Use seed as timestamp for variety
        10000,  // fuel limit - reasonable for fuzzing
        vec![], // no special capabilities
        contract.participants.clone(),
    );

    // Initialize contract state from state_vars
    let mut state = icn_ccl::ContractState::new();
    for var in &contract.state_vars {
        state.set(var.name.clone(), var.initial_value.clone());
    }

    // Select rule based on fuzzed index
    let rule_idx = (input.rule_index as usize) % contract.rules.len();
    let rule_name = contract.rules[rule_idx].name.clone();

    // Generate arguments based on parameter names and fuzz data
    let args: HashMap<String, Value> = contract.rules[rule_idx]
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let seed = input.param_seed.wrapping_add(i as u64);
            let value = generate_param_value(
                &p.name,
                seed,
                &input.int_values,
                &input.string_values,
                &input.bool_values,
                &contract.participants,
            );
            (p.name.clone(), value)
        })
        .collect();

    // Create interpreter and execute rule
    let interpreter = Interpreter::new(contract, state, context);

    // Execute and ignore all errors - we just want to catch panics
    let _ = interpreter.execute_rule(&rule_name, args);
});
