# ICN Wallet Testing Suite

This directory contains tests to verify the ICN Wallet's ability to:
- Integrate with AgoraNet for discussion and credential linking
- Interact with the ICN Runtime for governance actions
- Synchronize TrustBundles for federation consensus
- Manage identities and credentials
- Process proposals, votes, and other governance actions

## Prerequisites

To run the tests, you need:

1. Rust 1.75+ and Cargo
2. Node.js (for mock AgoraNet server)
3. Required NPM packages:
   ```
   npm install express cors body-parser
   ```

## Test Types

### 1. AgoraNet Integration Tests

Tests the wallet's ability to interact with AgoraNet services:
- Fetching threads and discussions
- Linking credentials to threads
- Notifying about proposal events

```bash
cargo test --test agoranet_integration_test -- --nocapture
```

### 2. Runtime Integration Tests

Tests the wallet's ability to interact with the ICN Runtime:
- Creating and signing proposals
- Voting on proposals
- Executing proposals
- Managing TrustBundles

```bash
cargo test --test runtime_integration_test -- --nocapture
```

### 3. End-to-End Workflow Tests

Tests the complete user workflow:
- Creating and activating identities
- Fetching discussions from AgoraNet
- Linking credentials to discussions
- Creating and voting on proposals
- Executing proposals and creating receipts
- Synchronizing TrustBundles

```bash
cargo test --test e2e_workflow_test -- --nocapture
```

## Setting Up a Test Environment

For automated setup:

```bash
# Create test directories and data
./tests/setup_test_env.sh

# Run the mock AgoraNet server
node tests/mock_agoranet.js
```

## Troubleshooting

1. **API Server Not Starting**
   - Ensure port 3000 is available
   - Check for error messages in the terminal output

2. **Mock AgoraNet Server Issues**
   - Ensure port 8080 is available
   - Verify that required Node.js packages are installed

3. **Test Failures**
   - The tests are designed to be resilient to service availability
   - Some endpoints may be mocked if not fully implemented

## Manual API Testing

You can also test the API endpoints manually:

```bash
# Activate an identity
curl -X POST http://localhost:3000/api/did/activate/{identity_id}

# List threads from AgoraNet
curl http://localhost:3000/api/agoranet/threads

# Fetch a specific thread
curl http://localhost:3000/api/agoranet/threads/{thread_id}

# Create a proposal
curl -X POST -H "Content-Type: application/json" -d '{
  "proposal_type": "ConfigChange",
  "content": {
    "title": "Test Proposal",
    "description": "A test proposal",
    "parameter": "voting_period",
    "value": "7d"
  }
}' http://localhost:3000/api/proposal/sign

# Link a credential to a thread
curl -X POST -H "Content-Type: application/json" -d '{
  "thread_id": "thread1",
  "credential_id": "cred123"
}' http://localhost:3000/api/agoranet/credential-link

# Sync TrustBundles
curl -X POST http://localhost:3000/api/sync/trust-bundles
``` 