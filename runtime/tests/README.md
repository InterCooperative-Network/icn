# ICN Runtime Integration Testing

This directory contains tools and documentation for integration testing the ICN Runtime, focusing on automated testing capabilities.

## Overview

Integration tests verify that the ICN Runtime works correctly with other components like AgoraNet and the Wallet. The tools in this directory help automate these tests, making them reliable and repeatable.

## Docker Deployment

The ICN Runtime supports reliable Docker Compose-based deployment for automated testing with:

- Fixed port assignments (8080, 8090, 4001, 9090)
- Built-in health checks
- Structured logging output
- Container dependencies properly configured

See `docker-compose.integration.yml` in the root directory for details.

## Testing Tools

### State Management

The `reset_icn_state.sh` script resets the ICN Runtime state between test runs. It supports:

- Full or partial state resets
- Backup creation
- Configurable data and log directories

Usage:
```bash
./tests/reset_icn_state.sh --mode full
```

### WebSocket Event Monitoring

The `websocket_monitor.js` script connects to the ICN Runtime's WebSocket endpoint to monitor events. It can:

- Log events to console or file
- Filter events by type
- Wait for specific events
- Exit with appropriate status codes

Usage:
```bash
# Monitor all events
./tests/websocket_monitor.js

# Wait for a specific event
./tests/websocket_monitor.js --wait-for ProposalFinalized --timeout 30000
```

Dependencies:
```bash
npm install ws
```

## Debugging API

The ICN Runtime provides debug API endpoints under `/api/v1/debug` for integration testing. These endpoints are read-only and allow querying internal state:

### Endpoints

- `/api/v1/debug` - Lists available debug endpoints
- `/api/v1/debug/proposal/:cid` - Get status of a proposal by ID
- `/api/v1/debug/dag/:cid` - Get details of a DAG node by CID
- `/api/v1/debug/federation/status` - Get current federation status
- `/api/v1/debug/federation/peers` - List connected federation peers
- `/api/v1/debug/federation/trust-bundle` - Get current trust bundle

### Response Format

All endpoints return JSON responses. Example query:

```bash
curl -X GET http://localhost:8080/api/v1/debug/federation/status
```

Response:
```json
{
  "current_epoch": 42,
  "node_count": 5,
  "connected_peers": 3,
  "validator_count": 3,
  "guardian_count": 1,
  "observer_count": 1
}
```

## Integration Test Patterns

### 1. Container-Based Testing

1. Start the ICN Runtime and dependencies with Docker Compose
2. Wait for health checks to pass
3. Run your test script/application
4. Verify results through the debug API or event monitoring
5. Reset the state for the next test

Example:
```bash
# Start containers
docker-compose -f docker-compose.integration.yml up -d

# Wait for services to be ready
./tests/wait_for_services.sh

# Run test that interacts with the Runtime
./tests/submit_proposal_test.sh

# Verify the proposal was created using the debug API
PROPOSAL_CID="bafybeihfklm..."
curl http://localhost:8080/api/v1/debug/proposal/$PROPOSAL_CID

# Monitor for proposal finalization
./tests/websocket_monitor.js --wait-for ProposalFinalized --timeout 30000

# Clean up for next test
./tests/reset_icn_state.sh
```

### 2. Event-Driven Testing

1. Start the WebSocket monitor to listen for specific events
2. Perform an action that should trigger those events
3. The monitor will exit successfully if events are received or fail on timeout

### 3. State Verification

1. Perform actions against the ICN Runtime
2. Use the debug API to verify internal state changes
3. Check for expected changes in storage, DAG, or federation state

## Best Practices

1. **Isolation**: Always reset state between tests
2. **Verification**: Use multiple points of verification (API, events, logs)
3. **Determinism**: Set fixed timeouts and ensure tests are repeatable
4. **Logging**: Enable structured logging for easier parsing
5. **Error Handling**: Validate proper error responses and recovery

## Integration Tests

The runtime integration tests demonstrate how different components of the ICN Runtime work together to provide a complete solution. These tests are intended to be more comprehensive than unit tests and cover realistic usage scenarios.

### Available Tests

1. **Entity Creation Test (`entity_creation_test.rs`)** - Tests creation of basic entities through the API
2. **CCL Entity Creation Test (`ccl_entity_creation_test.rs`)** - Tests creation of entities using CCL templates
3. **Full Governance Cycle Test (`full_governance_cycle.rs`)** - Tests the complete governance workflow from proposal submission through execution and credential issuance

### Wallet Integration

The `full_governance_cycle.rs` test demonstrates integration between wallet and runtime components:

1. Creating an identity
2. Submitting a governance proposal
3. Voting on the proposal
4. Finalizing the proposal
5. Executing the proposal
6. Retrieving and verifying credentials

This test ensures that wallet components can interact properly with the runtime using the shared types defined in `wallet-types`. It validates the full lifecycle of a governance proposal through both runtime and wallet perspectives.

### Running Tests

To run all integration tests:

```bash
cd runtime
cargo test --test '*'
```

To run a specific test:

```bash
cd runtime
cargo test --test full_governance_cycle
```

## CLI Testing Tools

In addition to automated tests, the CLI provides tools for manually testing wallet-runtime integration:

```bash
# Test the full governance cycle
cargo run --bin covm wallet-test governance-cycle

# Customize test parameters
cargo run --bin covm wallet-test governance-cycle --user-did "did:icn:custom:user1" --voting-period 3600
``` 