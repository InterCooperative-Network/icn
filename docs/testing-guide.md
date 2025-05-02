# ICN Wallet Testing Guide

This guide outlines procedures for comprehensive testing of the ICN Wallet, including end-to-end workflows, robustness testing, and security validation.

## Setup Testing Environment

Before running the tests, set up a complete testing environment:

```bash
# Create test directories
mkdir -p test-wallet-data/identities test-wallet-data/queue test-wallet-data/bundles

# Start mock AgoraNet server
node tests/mock_agoranet.js &
AGORANET_PID=$!

# Start the wallet API server
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data serve --agoranet-url http://localhost:8080/api &
WALLET_PID=$!

# Wait for services to start
sleep 5
```

## 1. End-to-End Workflow Validation

### 1.1 Identity Creation and Management

```bash
# Create a test identity
IDENTITY_1=$(cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data create --scope personal | grep "ID:" | cut -d' ' -f2)
echo "Created identity: $IDENTITY_1"

# Create a second identity
IDENTITY_2=$(cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data create --scope organization --metadata '{"name":"Test Org"}' | grep "ID:" | cut -d' ' -f2)
echo "Created identity: $IDENTITY_2"

# Activate first identity via API
curl -X POST "http://localhost:3000/api/did/activate/$IDENTITY_1"

# Verify active identity
curl "http://localhost:3000/api/did/list" | jq
```

### 1.2 TrustBundle Synchronization

```bash
# Sync TrustBundles
curl -X POST "http://localhost:3000/api/sync/trust-bundles" | jq

# List TrustBundles
curl "http://localhost:3000/api/trust-bundles" | jq
```

### 1.3 Credential Management

```bash
# Issue a credential
curl -X POST "http://localhost:3000/api/vc/issue" \
  -H "Content-Type: application/json" \
  -d '{
    "subject_data": {
      "id": "did:icn:subject123",
      "name": "Test Subject",
      "role": "Member"
    },
    "credential_types": ["MembershipCredential"]
  }' | jq
  
# Get credential ID from response
CREDENTIAL_ID=$(curl -X POST "http://localhost:3000/api/vc/list" | jq -r '.[0].id')

# Verify the credential
curl -X POST "http://localhost:3000/api/vc/verify" \
  -H "Content-Type: application/json" \
  -d "{\"credential\": $(curl -X POST \"http://localhost:3000/api/vc/list\" | jq '.[0]')}" | jq
```

### 1.4 Proposal and Governance Workflow

```bash
# List governance threads
curl "http://localhost:3000/api/agoranet/threads?topic=governance" | jq

# Get first thread ID
THREAD_ID=$(curl "http://localhost:3000/api/agoranet/threads?topic=governance" | jq -r '.[0].id')

# Link credential to thread
curl -X POST "http://localhost:3000/api/agoranet/credential-link" \
  -H "Content-Type: application/json" \
  -d "{\"thread_id\": \"$THREAD_ID\", \"credential_id\": \"$CREDENTIAL_ID\"}" | jq

# Create a new proposal
curl -X POST "http://localhost:3000/api/proposal/sign" \
  -H "Content-Type: application/json" \
  -d '{
    "proposal_type": "ConfigChange",
    "content": {
      "title": "Increase Voting Period",
      "description": "Increase the voting period to 14 days",
      "parameter": "voting_period",
      "value": "14d"
    }
  }' | jq

# Get proposal ID from the thread
PROPOSAL_ID=$(curl "http://localhost:3000/api/agoranet/threads?topic=governance" | jq -r '.[0].proposal_id')

# Vote on the proposal
curl -X POST "http://localhost:3000/api/proposals/$PROPOSAL_ID/vote" \
  -H "Content-Type: application/json" \
  -d '{
    "decision": "Approve",
    "reason": "This is a good proposal"
  }' | jq

# Create execution receipt
curl -X POST "http://localhost:3000/api/proposals/$PROPOSAL_ID/receipt" \
  -H "Content-Type: application/json" \
  -d '{
    "success": true,
    "timestamp": "2023-05-01T12:00:00Z",
    "votes": {
      "approve": 3,
      "reject": 1,
      "abstain": 0
    }
  }' | jq

# Notify about completion
curl -X POST "http://localhost:3000/api/agoranet/proposals/$PROPOSAL_ID/notify" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "executed",
    "timestamp": "2023-05-01T12:00:00Z"
  }' | jq
```

## 2. Robustness Testing

### 2.1 Error Handling and Recovery

#### AgoraNet Unavailability

```bash
# Stop AgoraNet mock server
kill $AGORANET_PID

# Try to access AgoraNet resources (should fail gracefully)
curl "http://localhost:3000/api/agoranet/threads" | jq

# Restart AgoraNet
node tests/mock_agoranet.js &
AGORANET_PID=$!
sleep 2

# Verify recovery
curl "http://localhost:3000/api/agoranet/threads" | jq
```

#### Corrupted Data Recovery

```bash
# Stop wallet
kill $WALLET_PID

# Backup current data
cp -r test-wallet-data test-wallet-data-backup

# Corrupt a file
echo "corrupted data" > test-wallet-data/identities/some-file.json

# Restart wallet
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data serve --agoranet-url http://localhost:8080/api &
WALLET_PID=$!
sleep 5

# Verify graceful handling
curl "http://localhost:3000/api/did/list" | jq

# Restore from backup
kill $WALLET_PID
rm -rf test-wallet-data
cp -r test-wallet-data-backup test-wallet-data
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data serve --agoranet-url http://localhost:8080/api &
WALLET_PID=$!
sleep 5
```

### 2.2 Multiple Identity Management

```bash
# Switch between identities
curl -X POST "http://localhost:3000/api/did/activate/$IDENTITY_1"
curl "http://localhost:3000/api/did/list" | jq  # Verify active identity

curl -X POST "http://localhost:3000/api/did/activate/$IDENTITY_2"
curl "http://localhost:3000/api/did/list" | jq  # Verify active identity
```

### 2.3 State Consistency After Offline Period

```bash
# Stop wallet
kill $WALLET_PID

# Simulate offline period, make changes to AgoraNet
curl -X POST "http://localhost:8080/api/threads" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "New Thread During Offline",
    "topic": "governance"
  }' | jq

# Restart wallet
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data serve --agoranet-url http://localhost:8080/api &
WALLET_PID=$!
sleep 5

# Sync state
curl -X POST "http://localhost:3000/api/sync/trust-bundles" | jq

# Verify new data appears
curl "http://localhost:3000/api/agoranet/threads" | jq
```

### 2.4 Performance with Many Credentials

```bash
# Create multiple credentials
for i in {1..10}; do
  curl -X POST "http://localhost:3000/api/vc/issue" \
    -H "Content-Type: application/json" \
    -d "{
      \"subject_data\": {
        \"id\": \"did:icn:subject$i\",
        \"name\": \"Test Subject $i\",
        \"role\": \"Member\"
      },
      \"credential_types\": [\"MembershipCredential\"]
    }" > /dev/null
done

# Verify performance listing credentials
time curl "http://localhost:3000/api/vc/list" > /dev/null
```

## 3. Security Testing

### 3.1 Private Key Protection

```bash
# Verify private keys aren't exposed in API responses
curl "http://localhost:3000/api/did/list" | jq
curl "http://localhost:3000/api/did/$IDENTITY_1" | jq

# Check filesystem permissions on key files
ls -la test-wallet-data/identities/
```

### 3.2 Authentication and Authorization

```bash
# Try to use inactive identity (should fail)
curl -X POST "http://localhost:3000/api/did/activate/invalid-id" | jq
```

### 3.3 Input Validation

```bash
# Test with invalid inputs
curl -X POST "http://localhost:3000/api/proposal/sign" \
  -H "Content-Type: application/json" \
  -d '{
    "proposal_type": "",
    "content": null
  }' | jq

curl -X POST "http://localhost:3000/api/vc/verify" \
  -H "Content-Type: application/json" \
  -d '{
    "credential": "invalid-json"
  }' | jq
```

## 4. WebSocket Notification Testing

```bash
# Using wscat to listen for notifications
npm install -g wscat
wscat -c ws://localhost:3000/ws &
WS_PID=$!

# Perform actions that should trigger notifications
curl -X POST "http://localhost:3000/api/did/activate/$IDENTITY_1"
curl -X POST "http://localhost:3000/api/sync/trust-bundles"

# View notifications in wscat terminal
```

## 5. Edge Cases

### 5.1 Large Data Handling

```bash
# Create proposal with large content
curl -X POST "http://localhost:3000/api/proposal/sign" \
  -H "Content-Type: application/json" \
  -d "{
    \"proposal_type\": \"ContentChange\",
    \"content\": {
      \"title\": \"Large Content Proposal\",
      \"description\": \"$(head -c 100000 < /dev/urandom | base64)\"
    }
  }" | jq
```

### 5.2 Concurrency Testing

```bash
# Multiple concurrent requests
for i in {1..10}; do
  curl "http://localhost:3000/api/did/list" &
  curl "http://localhost:3000/api/trust-bundles" &
  curl "http://localhost:3000/api/agoranet/threads" &
done
```

## 6. CLI Testing

```bash
# Test CLI help
cargo run --bin icn-wallet-cli -- --help

# Test identity creation
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data create --scope personal

# Test bundle commands
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data -i /path/to/identity.json Bundle List

# Test AgoraNet commands
cargo run --bin icn-wallet-cli -- --data-dir ./test-wallet-data -i /path/to/identity.json AgoraNet --url http://localhost:8080/api ListThreads
```

## Clean Up

```bash
# Stop all processes
kill $WALLET_PID
kill $AGORANET_PID
kill $WS_PID

# Clean up test data
rm -rf test-wallet-data test-wallet-data-backup
```

## Automated Testing

Run the automated test suite:

```bash
# Run all tests
cargo test

# Run specific end-to-end test
cargo test -- --nocapture tests::e2e_workflow_test

# Run AgoraNet integration test
cargo test -- --nocapture tests::agoranet_integration_test
```

### Test Coverage

To generate test coverage:

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

## Test Reporting

Create a test report for each task:

1. Document test results
2. Note any failures or unexpected behavior
3. Identify potential improvements
4. Prioritize critical issues

For detailed CI integration, see the project's CI configuration. 