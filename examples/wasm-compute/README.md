# ICN WASM Compute Example

This directory contains example WASM modules for ICN distributed compute.

## Prerequisites

```bash
# Install the WASM target
rustup target add wasm32-unknown-unknown
```

## Building

```bash
cd examples/wasm-compute

# Build the WASM module
cargo build --release --target wasm32-unknown-unknown

# The output will be at:
# target/wasm32-unknown-unknown/release/icn_wasm_example.wasm
```

## Submitting Tasks

### Via CLI

```bash
# Submit the WASM module
icnctl compute submit-wasm \
    --wasm target/wasm32-unknown-unknown/release/icn_wasm_example.wasm \
    --fuel 10000

# Check status
icnctl compute status <task_hash>
```

### Via TypeScript SDK

```typescript
import { ICNClient } from '@anthropic/icn-sdk';
import * as fs from 'fs';

const client = new ICNClient({ baseUrl: 'http://localhost:8080' });

// Authenticate
await client.authenticate('did:icn:...', signatureProvider);

// Read WASM file
const wasmBytes = fs.readFileSync(
  'examples/wasm-compute/target/wasm32-unknown-unknown/release/icn_wasm_example.wasm'
);

// Submit task
const { task_hash } = await client.submitWasmTask(wasmBytes, {
  fuel_limit: 10000,
  priority: 'normal',
});

// Wait for result
const result = await client.waitForTask(task_hash);
console.log('Result:', result);
```

### Via Gateway REST API

```bash
# Encode WASM as base64
WASM_B64=$(base64 -w0 target/wasm32-unknown-unknown/release/icn_wasm_example.wasm)

# Submit via API
curl -X POST http://localhost:8080/v1/compute/submit \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"code_type\": \"wasm\",
    \"wasm_bytes\": \"$WASM_B64\",
    \"fuel_limit\": 10000
  }"
```

## WASM Module Requirements

ICN WASM modules must export a `run` function with one of these signatures:

```rust
// Simple: no inputs, returns i32
#[no_mangle]
pub extern "C" fn run() -> i32 { ... }

// With inputs: reads from memory, returns i32
#[no_mangle]
pub extern "C" fn run(input_ptr: i32, input_len: i32) -> i32 { ... }
```

## ICN Host Functions

WASM modules can call these host functions:

```rust
#[link(wasm_import_module = "icn")]
extern "C" {
    /// Log a message (reads UTF-8 string from memory at ptr)
    fn log(ptr: i32, len: i32);

    /// Get current Unix timestamp in milliseconds
    fn timestamp() -> i64;
}
```

## Example Functions

This example module provides:

- `run()` - Returns 42 (simple test)
- `run_with_input(ptr, len)` - Sums input bytes
- `fibonacci(n)` - Computes n-th Fibonacci number

## Size Optimization

The release profile is configured for minimal WASM size:

```toml
[profile.release]
opt-level = "s"      # Optimize for size
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization
strip = true         # Strip symbols
```

For even smaller modules, consider:

```bash
# Install wasm-opt
cargo install wasm-opt

# Optimize further
wasm-opt -Oz -o optimized.wasm module.wasm
```
