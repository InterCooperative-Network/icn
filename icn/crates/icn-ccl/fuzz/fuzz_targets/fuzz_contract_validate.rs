//! Fuzz target for Contract validation
//!
//! This target tests the validate() method's ability to handle arbitrary
//! contract structures without panicking. It exercises:
//! - Name validation (length limits, reserved keywords)
//! - Participant validation (duplicates, counts)
//! - State variable validation
//! - Rule validation (duplicates, parameters)
//! - Expression depth validation
//! - Loop nesting depth validation
//!
//! The goal is to find inputs that cause panics or excessive resource
//! consumption during validation.

#![no_main]

use libfuzzer_sys::fuzz_target;
use icn_ccl::Contract;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 string first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to deserialize as a Contract
        if let Ok(contract) = serde_json::from_str::<Contract>(s) {
            // If deserialization succeeds, run validation
            // We don't care about validation errors, just that it doesn't panic
            let _ = contract.validate();
        }
    }
});
