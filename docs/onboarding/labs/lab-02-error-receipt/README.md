# Lab 02: Error Receipts and Tracing

## Goal
Add structured errors and tracing to lab-01, implementing the Receipt Pattern.

## Requirements

### 1. Structured Errors
In `core/src/error.rs`:
- Define `CoreError` enum with thiserror
- Include structured fields (e.g., `NotFound { id: String }`)
- Add `#[error("...")]` messages

### 2. Receipt Pattern
Create `Receipt<T>` type:
```rust
pub enum Receipt<T> {
    Accepted(T),
    Rejected { reason: String },
    Quarantined { id: String, reason: String },
}
```

Operations return `Receipt<T>`, not just `Result<T, E>`.

### 3. Tracing Spans
Add tracing to core operations:
```rust
#[tracing::instrument(skip(self))]
pub fn process_request(&self, req: Request) -> Receipt<Response> {
    // ...
}
```

Add nested spans showing lifecycle.

### 4. CLI Integration
- Initialize tracing subscriber with `EnvFilter`
- Run operations that succeed/fail/quarantine
- Show tracing output for all three paths

## Done When
- Errors include structured fields
- Operations return typed receipts
- Tracing shows nested spans for request lifecycle
- You can see different receipt types in tracing output

## Example Output
```
INFO core: processing request id="req-123"
WARN core: validation failed reason="invalid format" id="req-123"
INFO core: request quarantined id="req-123"
```

## Resources
- `icn/crates/icn-core/src/error.rs`
- `icn/crates/icn-ledger/src/ledger.rs` (quarantine pattern)
- [tracing crate docs](https://docs.rs/tracing/)
