//! ICN WASM Compute Example
//!
//! This module demonstrates how to write WASM modules for ICN distributed compute.
//!
//! # Building
//!
//! ```bash
//! # Install the WASM target
//! rustup target add wasm32-unknown-unknown
//!
//! # Build the WASM module
//! cargo build --release --target wasm32-unknown-unknown
//!
//! # The output will be at:
//! # target/wasm32-unknown-unknown/release/icn_wasm_example.wasm
//! ```
//!
//! # Submitting to ICN
//!
//! ```bash
//! # Via CLI
//! icnctl compute submit-wasm \
//!     --wasm target/wasm32-unknown-unknown/release/icn_wasm_example.wasm \
//!     --fuel 10000
//!
//! # Via TypeScript SDK
//! const wasmBytes = fs.readFileSync('module.wasm');
//! const result = await client.submitWasmTask(wasmBytes, { fuel_limit: 10000 });
//! ```
//!
//! # ICN Host Functions
//!
//! The following host functions are available to WASM modules:
//!
//! - `icn::log(ptr: i32, len: i32)` - Log a message (reads UTF-8 string from memory)
//! - `icn::timestamp() -> i64` - Get current Unix timestamp in milliseconds

// Declare ICN host functions
#[link(wasm_import_module = "icn")]
extern "C" {
    /// Log a message to the ICN runtime
    fn log(ptr: i32, len: i32);
    /// Get current timestamp in milliseconds
    fn timestamp() -> i64;
}

/// Helper to log a string
fn icn_log(msg: &str) {
    unsafe {
        log(msg.as_ptr() as i32, msg.len() as i32);
    }
}

/// Helper to get timestamp
fn icn_timestamp() -> i64 {
    unsafe { timestamp() }
}

/// Simple no-argument run function
/// Returns a constant value (42)
#[no_mangle]
pub extern "C" fn run() -> i32 {
    icn_log("Hello from WASM!");
    42
}

/// Run function with inputs
/// Reads input data from memory and returns sum of bytes
#[no_mangle]
pub extern "C" fn run_with_input(input_ptr: i32, input_len: i32) -> i32 {
    let _ts = icn_timestamp(); // Available for timestamp-based logic
    icn_log("Processing input...");

    // Safety: We trust the host to provide valid pointers
    let input = unsafe {
        core::slice::from_raw_parts(input_ptr as *const u8, input_len as usize)
    };

    // Simple computation: sum all input bytes
    let sum: i32 = input.iter().map(|&b| b as i32).sum();

    icn_log("Done!");
    sum
}

/// Fibonacci calculation (demonstrates computation)
#[no_mangle]
pub extern "C" fn fibonacci(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    let mut a = 0i32;
    let mut b = 1i32;
    for _ in 2..=n {
        let temp = a + b;
        a = b;
        b = temp;
    }
    b
}

/// Memory export (required for input passing)
#[no_mangle]
pub static mut MEMORY: [u8; 65536] = [0; 65536];
