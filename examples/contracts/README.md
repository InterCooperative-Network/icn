# CCL Contract Examples

This directory contains example contracts written in CCL (Cooperative Contract Language) that can be deployed to ICN nodes.

## Available Contracts

### 1. Echo Contract (`echo.json`)

A simple contract for testing that demonstrates basic CCL features:

**Rules:**
- `echo(message)` - Returns the message passed to it
- `add(a, b)` - Adds two numbers together

**Example Usage:**
```bash
# Deploy the contract
icnctl contract deploy examples/contracts/echo.json

# Call the echo rule
icnctl contract call <code_hash> echo did:icn:alice --args '{"message":"hello"}'

# Call the add rule
icnctl contract call <code_hash> add did:icn:alice --args '{"a":5,"b":3}'
```

### 2. TimeBank Contract (`timebank.json`)

A mutual credit time banking system where members exchange hours of service:

**Features:**
- Tracks service exchanges in hours
- Maintains running total of all hours exchanged
- Enforces positive hour amounts via preconditions

**State Variables:**
- `total_hours_exchanged` - Cumulative count of all hours traded

**Rules:**
- `record_service(recipient, hours)` - Record a service exchange
  - Transfers hours from sender to recipient
  - Updates total_hours_exchanged counter
  - Requires hours > 0
- `get_stats()` - Returns total hours exchanged in the timebank

**Example Usage:**
```bash
# Deploy the timebank contract
icnctl contract deploy examples/contracts/timebank.json

# Record a service (Alice helped Bob for 5 hours)
icnctl contract call <code_hash> record_service did:icn:alice \
  --args '{"recipient":"did:icn:bob","hours":5}'

# Check timebank statistics
icnctl contract call <code_hash> get_stats did:icn:alice
```

## Contract Structure

CCL contracts are JSON-serialized AST (Abstract Syntax Tree) structures with:

- **name**: Human-readable contract identifier
- **participants**: List of DIDs authorized to interact (optional)
- **currency**: Default currency for ledger operations (optional)
- **state_vars**: Persistent contract state variables
- **rules**: Callable functions with parameters and preconditions
- **triggers**: Scheduled actions (not yet implemented)

## Validation

All contracts are validated on deployment:
- Syntax correctness
- Expression depth limits (max 50)
- Reserved keyword checks
- Double-entry ledger invariants

## Security Model

Contracts execute with:
- **Fuel metering**: Bounded execution prevents infinite loops (10,000 fuel default)
- **Capability-based security**: Explicit permissions for ledger/state access
- **Deterministic execution**: Same inputs always produce same outputs
- **Not Turing-complete**: Safe subset of operations for predictability

## Creating New Contracts

To create a new contract:

1. Write contract logic using the CCL AST structure
2. Serialize to JSON following the examples above
3. Test locally with `icnctl contract deploy`
4. Deploy to network nodes
5. Call rules via `icnctl contract call`

For more details on CCL semantics, see `icn/crates/icn-ccl/src/lib.rs`.
