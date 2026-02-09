# 03: Errors and Tracing — Distributed Visibility

> **Phase**: 1 | **Tier**: Reader  
> **Patterns introduced**: [Error Handling](#error-handling), [Receipt Pattern](#receipt-pattern), [Metrics Integration](#metrics-integration)  
> **Prerequisite**: 02-rust-through-icn.md

## Why This Matters

**"You can't debug what you can't see."** Distributed systems multiply this problem: failures happen across nodes, causality is non-obvious, and post-mortems rely on structured logs and metrics.

ICN makes visibility a first-class concern — errors carry structured context, tracing spans show request flows, and metrics expose system health. This layer teaches you to instrument code **before** building features, not as an afterthought.

→ See `manual.md` § "Observability First" for the design rationale.

## What You'll Read

### 1. Foundation: `icn-core/src/error.rs`

This is the simplest error pattern — `thiserror` for domain errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Actor not found: {0}")]
    ActorNotFound(String),
    
    #[error("Shutdown in progress")]
    ShuttingDown,
    
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

**Key observations**:
- `#[error("...")]` provides human-readable messages
- `#[from]` enables `?` operator for automatic error conversion
- `#[error(transparent)]` delegates Display to the inner error
- No `unwrap()` or `expect()` — always `Result<T, E>`

### 2. Structured Errors: `icn-ledger/src/error.rs`

Domain errors add **structured fields** for better diagnostics:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("Credit limit exceeded: account={account}, attempted={attempted}, limit={limit}")]
    CreditLimitExceeded {
        account: Did,
        attempted: i64,
        limit: i64,
    },
    
    #[error("Unbalanced entry: sum={sum}")]
    UnbalancedEntry { sum: i64 },
    
    #[error("Entry quarantined: {reason}")]
    Quarantined { reason: String },
}
```

**Why structured fields matter**:
- Errors become **queryable** (e.g., "Show all credit limit violations for account X")
- Metrics can extract values (e.g., histogram of attempted amounts)
- Logs preserve context without string parsing

### 3. Source Location Tracking: `icn-ccl/src/error.rs`

Contract execution errors include **source spans** (line/column) for debugging:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CclError {
    #[error("Type mismatch at {span:?}: expected {expected}, got {actual}")]
    TypeMismatch {
        span: Span,
        expected: String,
        actual: String,
    },
    
    #[error("Out of fuel at {span:?}")]
    OutOfFuel { span: Span },
}

pub struct Span {
    pub start: usize,
    pub end: usize,
}
```

**Why spans matter**: When a contract fails, developers need to know **where** in the source code the failure occurred. Spans enable IDE integration, syntax highlighting of errors, and precise debugging.

### 4. End-to-End Error Flow

**Trace this sequence**:

1. **Validation**: `Ledger::validate_entry()` in `icn-ledger/src/ledger.rs:487-520`
   ```rust
   if account_balance + amount < credit_limit {
       return Err(LedgerError::CreditLimitExceeded {
           account: posting.account.clone(),
           attempted: amount,
           limit: credit_limit,
       });
   }
   ```

2. **Quarantine**: Failed entry is moved to quarantine (not dropped)
   ```rust
   self.quarantine.insert(entry.id.clone(), entry);
   ```

3. **Metrics**: Failure increments counter
   ```rust
   metrics::counter!("ledger_validation_failures_total", 
       "reason" => "credit_limit_exceeded").increment(1);
   ```

4. **Gateway**: Error propagates to REST API in `icn-gateway/src/api/ledger.rs`
   ```rust
   Err(e) => {
       tracing::error!(error = %e, "Entry validation failed");
       ApiError::ValidationFailed(e.to_string())
   }
   ```

5. **Client**: Receives structured JSON error with code + details
   ```json
   {
       "error": {
           "code": "VALIDATION_FAILED",
           "message": "Credit limit exceeded: ...",
           "details": { "account": "...", "attempted": 1000, "limit": 500 }
       }
   }
   ```

### 5. The Lint Rule: Never Panic in Production

**File**: `icn/crates/icn-core/Cargo.toml` and all crate manifests

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
```

**Escape hatch for tests**:
```rust
#![deny(clippy::unwrap_used, clippy::expect_used)]

#[cfg(test)]
mod tests {
    // Tests can use unwrap/expect via this attribute
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    
    #[test]
    fn test_happy_path() {
        let result = some_operation().unwrap();  // OK in tests
        assert_eq!(result, expected);
    }
}
```

**Why**: Panics in protocol/network/actor paths crash the entire daemon. `Result<T, E>` forces explicit error handling.

### 6. Coding Convention from `AGENTS.md`

**Pattern**:
- **thiserror** for crate-local domain errors (e.g., `LedgerError`, `GossipError`)
- **anyhow** at service boundaries (e.g., Gateway API handlers, CLI commands)
- **Never panic** in protocol/network/actor/deserialization paths

**Example (anyhow at boundary)**:
```rust
use anyhow::{Context, Result};

pub async fn handle_request(req: Request) -> Result<Response> {
    let entry = parse_entry(&req.body)
        .context("Failed to parse ledger entry")?;
    
    ledger.submit_entry(entry).await
        .context("Failed to submit entry to ledger")?;
    
    Ok(Response::ok())
}
```

The `.context("...")` adds human-readable context to errors without changing the error type.

## Patterns Introduced

### Error Handling Pattern
**Used for**: Domain-specific errors with structured fields.

→ See `patterns.md` #2 for full template (thiserror + anyhow).

### Receipt Pattern
**Used for**: Explicit success/failure results (not "accepted" lies).

The quarantine system in `icn-ledger/src/ledger.rs` is a receipt pattern:
- Entries that fail validation aren't silently dropped
- They're moved to quarantine with reason
- Callers receive explicit rejection, not false success

→ See `patterns.md` #12 for full template.

### Metrics Integration Pattern
**Used for**: Prometheus metrics at key decision points.

**Example** from `icn-ledger/src/ledger.rs`:
```rust
metrics::counter!("ledger_entries_total", 
    "status" => "submitted").increment(1);

if validation_fails {
    metrics::counter!("ledger_validation_failures_total",
        "reason" => error_type).increment(1);
}
```

→ See `patterns.md` #7 for full template.

## What You'll Build

→ **Lab**: `labs/lab-02-error-receipt/`

Extend your workspace from Layers 01-02:
- Add structured errors with thiserror
- Add tracing spans showing request lifecycle
- Add receipt pattern: operations return typed receipts, failures include context
- Add metrics counters for success/failure paths

**Done when**: You can see a request flow through tracing output, and errors include structured context.

## Checkpoint

You've completed this layer when you can:

1. **Reproduce a bug**: Take a failing test, add tracing spans, run it, and explain causality from the output
2. **Instrument a function**: Add tracing span, structured error, and metrics to a new function
3. **Explain error patterns**: Describe when to use thiserror vs anyhow vs panic
4. **Trace an error end-to-end**: Follow an error from validation → quarantine → metrics → gateway → client

**Artifact**: Submit a tracing output snippet showing an end-to-end request flow with at least 3 nested spans.

## Deep Reference

→ `reference/module-12-observability.md` — Full metrics, tracing, logging guide  
→ `reference/module-13-security-privacy.md` — Security instrumentation patterns  
→ `docs/architecture/PRODUCTION_HARDENING.md` — Error handling best practices  
→ [tracing crate docs](https://docs.rs/tracing/) — Tracing span API reference
