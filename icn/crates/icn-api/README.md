# icn-api

Shared API service layer for the Intercooperative Network (ICN).

## Overview

The `icn-api` crate provides reusable service implementations that sit between transport-specific API adapters (RPC, Gateway) and the core daemon actors. This architecture enables:

- **Single source of truth** for business logic
- **Consistent behavior** across all API transports (JSON-RPC, REST, WebSocket, etc.)
- **Reduced maintenance** by avoiding duplicate implementations
- **Testable services** independent of transport concerns
- **Clear separation** between transport concerns and business logic

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                  Transport Adapters                     │
├────────────────────┬────────────────────────────────────┤
│  icn-rpc (JSON-RPC)│    icn-gateway (REST/WS)          │
│  - Protocol format │    - HTTP routing                  │
│  - Error mapping   │    - Middleware (auth, coop)       │
│  - Rate limiting   │    - OpenAPI docs                  │
└────────┬───────────┴────────────────┬───────────────────┘
         │                            │
         ▼                            ▼
┌─────────────────────────────────────────────────────────┐
│                Shared Service Layer                     │
│                   (icn-api crate)                       │
├─────────────────────────────────────────────────────────┤
│  ComputeService │ LedgerService │ GovernanceService     │
│  - submit_task  │ - get_balance │ - create_domain       │
│  - cancel_task  │ - transfer    │ - cast_vote           │
│  - get_status   │ - get_history │ - get_proposal        │
├─────────────────────────────────────────────────────────┤
│  Shared Infrastructure:                                 │
│  - Unified error types (ApiError)                       │
│  - Scope definitions (scopes.rs)                        │
│  - Input validation                                     │
│  - Business logic                                       │
└─────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────┐
│                   Daemon Actors                         │
│   Ledger │ GossipActor │ TrustGraph │ ComputeActor     │
└─────────────────────────────────────────────────────────┘
```

## Current Status

### Implemented
- ✅ **ComputeService**: Task submission, status queries, cancellation
- ✅ **ApiError**: Unified error type with RPC code mapping
- ✅ **ApiContext**: Caller authentication context
- ✅ **Scopes**: Permission scope definitions

### Planned
- 🔄 **LedgerService**: Balance queries, transfers, transaction history
- 🔄 **GovernanceService**: Domain, proposal, and voting operations
- 🔄 **TrustService**: Trust graph queries and edge management
- 🔄 **Additional services** as needed

## Usage

### Creating a Service

Services are initialized with handles to the relevant daemon actors:

```rust
use icn_api::ComputeService;
use icn_compute::ComputeHandle;

let compute_handle = ComputeHandle::new(/* ... */);
let compute_service = ComputeService::new(compute_handle);
```

### Calling a Service Method

All service methods require an `ApiContext` for authentication:

```rust
use icn_api::{ApiContext, SubmitTaskParams};
use icn_api::compute::{CodeTypeParam, TaskPriorityParam};

// Build API context from authenticated request
let ctx = ApiContext {
    caller_did: "did:icn:alice".to_string(),
    coop_id: Some("tech-coop".to_string()),
};

// Build request parameters
let params = SubmitTaskParams {
    task_id: "task-123".to_string(),
    code: Some("{ /* CCL code */ }".to_string()),
    wasm_bytes: None,
    code_type: CodeTypeParam::Ccl,
    inputs: serde_json::Value::Null,
    fuel_limit: 10_000,
    priority: TaskPriorityParam::Normal,
    deadline_ms: None,
    payment_rate: None,
    payment_currency: None,
    coop_id: None, // Uses ctx.coop_id
    resource_profile: None,
};

// Call service - returns task hash (32-byte identifier)
let task_hash = compute_service.submit_task(&ctx, params).await?;
```

### Error Handling

Services return `Result<T, ApiError>`. Transport adapters map `ApiError` to their native error representation:

```rust
use icn_api::ApiError;

match compute_service.submit_task(&ctx, params).await {
    Ok(task_hash) => { /* success - task_hash is [u8; 32] */ }
    Err(ApiError::ValidationError(msg)) => {
        // Handle validation error (HTTP 400, JSON-RPC -32602)
    }
    Err(ApiError::NotAuthenticated) => {
        // Handle auth error (HTTP 401, JSON-RPC -32001)
    }
    Err(e) => {
        // Handle other errors
    }
}
```

## Permission Scopes

The `scopes` module defines all permission scopes used across the API:

```rust
use icn_api::scopes;

// Check if a scope grants a required permission
if scopes::matches("compute:*", scopes::compute::SUBMIT) {
    // User has wildcard compute permissions
}

// Exact match
if scopes::matches(scopes::ledger::READ, scopes::ledger::READ) {
    // User can read ledger
}
```

## Integration with Transport Adapters

### JSON-RPC (icn-rpc)

```rust
// In RPC handler
pub async fn handle_compute_submit(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    let compute_service = state.compute_service()?;
    
    // Parse RPC params
    let request: SubmitTaskRequest = serde_json::from_value(params.clone())?;
    
    // Build API context
    let api_ctx = ApiContext {
        caller_did: ctx.map(|c| c.caller_did.clone()).unwrap_or_default(),
        coop_id: ctx.and_then(|c| c.coop_id.clone()),
    };
    
    // Convert RPC params to API params
    let params = convert_rpc_to_api_params(request);
    
    // Call service
    match compute_service.submit_task(&api_ctx, params).await {
        Ok(task_hash) => RpcResponse::success(id, task_hash),
        Err(e) => RpcResponse::error(id, e.to_rpc_code(), e.to_string()),
    }
}
```

### REST/WebSocket (icn-gateway)

```rust
// In Gateway manager
pub async fn submit_task(&self, /* ... */) -> Result<TaskHash> {
    let compute_service = self.compute_service.as_ref()?;
    
    // Build API context
    let api_ctx = ApiContext {
        caller_did: submitter,
        coop_id,
    };
    
    // Build API params
    let params = SubmitTaskParams { /* ... */ };
    
    // Call service (errors map to HTTP status codes)
    compute_service.submit_task(&api_ctx, params).await
        .map_err(|e| anyhow::anyhow!("Submit failed: {e}"))
}
```

## Testing

The crate includes comprehensive unit tests for validation logic:

```bash
cd icn
cargo test -p icn-api
```

## Migration Guide

When adding a new service to `icn-api`:

1. **Create service module** (e.g., `ledger.rs`)
2. **Define service struct** with daemon actor handles
3. **Define parameter types** for each operation
4. **Implement validation** with clear error messages
5. **Add service methods** that use `ApiContext`
6. **Return `Result<T, ApiError>`** for all operations
7. **Add unit tests** for validation logic
8. **Update both RPC and Gateway** to use the new service
9. **Add integration tests** validating end-to-end flow

## Related Issues

- Issue #767: Create `icn-api` shared service layer ✅
- Issue #768: Extract ComputeService to shared layer ✅
- Issue #769: Enforce coop isolation in RPC
- Issue #770: Add trust-gated rate limiting to Gateway

## License

MIT OR Apache-2.0
