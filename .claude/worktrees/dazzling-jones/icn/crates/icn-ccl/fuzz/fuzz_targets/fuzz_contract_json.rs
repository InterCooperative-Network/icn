//! Fuzz target for Contract JSON deserialization
//!
//! This target tests the JSON parser's ability to handle arbitrary byte sequences
//! without panicking. It exercises:
//! - serde_json parsing
//! - Contract struct deserialization
//! - Nested type parsing (Expr, Stmt, Value, etc.)
//!
//! The goal is to find inputs that cause panics, memory issues, or excessive
//! resource consumption during deserialization.

#![no_main]

use libfuzzer_sys::fuzz_target;
use icn_ccl::Contract;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 string first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to deserialize as a Contract
        // We don't care about the result, just that it doesn't panic
        let _ = serde_json::from_str::<Contract>(s);
    }
});
