# ICN Federation Launch Guide

This guide provides step-by-step instructions for launching a new ICN federation, from identity creation through proposal execution and DAG synchronization.

## Prerequisites

Before starting, ensure you have:

- ICN wallet installed and configured
- ICN runtime node installed
- AgoraNet server installed
- PostgreSQL database for AgoraNet
- Network access for federation participants

## 1. Federation Genesis

### 1.1 Create Federation Identity

```bash
# Use the genesis generation script
./scripts/generate_genesis_snapshot.sh "My Federation" "A federation for collaborative governance"

# This creates several files in the ./genesis_data directory:
# - federation_keypair.json (KEEP SECURE!)
# - genesis_bundle.json (Federation configuration)
# - trust_bundle.json (Signed genesis bundle)
# - anchor_cid.txt (DAG CID reference)
```

> **CRITICAL**: The federation keypair represents the root identity of your federation. Keep it secure and consider hardware security modules for production deployments.

### 1.2 Verify Genesis Snapshot

```bash
# Verify the genesis snapshot
./scripts/replay_genesis.sh --bundle ./genesis_data/trust_bundle.json

# Sample output:
# Federation Information:
# Name: My Federation
# ID: did:icn:fed:a1b2c3...
# Created: 2024-07-15T10:30:00Z
#
# Governance Policies:
# Voting Period: 48 hours
# Threshold: 0.5
# Min Voters: 1
#
# Initial Members (1):
#   - DID: did:icn:fed:a1b2c3...
#     Role: admin
#     Joined: 2024-07-15T10:30:00Z
```

### 1.3 Distribute Trust Bundle

Before participants can join, they need the trust bundle:

```bash
# Create a secure download link or distribution method
# IMPORTANT: Verify the integrity after distribution!

# For each participant, they should verify:
./scripts/replay_genesis.sh --bundle path/to/received_trust_bundle.json

# The output should match the expected federation information
```

## 2. Federation Node Deployment

### 2.1 Initialize Runtime Node

```bash
# Initialize a federation node with the genesis trust bundle
icn-federation-node init --genesis ./genesis_data/trust_bundle.json --data-dir ./federation_data

# Start the node
icn-federation-node start --data-dir ./federation_data

# Verify node is running
curl http://localhost:8000/api/v1/status
```

Example output:
```json
{
  "status": "online",
  "federation_id": "did:icn:fed:a1b2c3...",
  "node_id": "did:icn:node:d4e5f6...",
  "dag_height": 1,
  "connected_peers": 0,
  "version": "0.9.0"
}
```

### 2.2 Configure AgoraNet

```bash
# Create initial database
./scripts/setup_agoranet_db.sh

# Configure AgoraNet to connect to the federation
cat > ./agoranet/.env << EOF
# Federation configuration
FEDERATION_ID=did:icn:fed:a1b2c3...
NODE_ID=did:icn:node:d4e5f6...
FEDERATION_ENDPOINT=http://localhost:8000

# Database configuration 
DATABASE_URL=postgres://postgres:postgres@localhost:5432/agoranet
# ... other settings
EOF

# Start AgoraNet
cd agoranet && cargo run --release
```

## 3. Participant Onboarding

### 3.1 Wallet Identity Creation

Each participant needs to create their own wallet identity:

```bash
# Generate an individual identity
icn-wallet identity create --name "Alice" --scope individual

# Expected output:
# Created identity:
# DID: did:icn:user:g7h8i9...
# Name: Alice
# Scope: individual
```

### 3.2 Import Federation Trust Bundle

```bash
# Import the federation trust bundle into the wallet
icn-wallet federation import --genesis ./path/to/trust_bundle.json

# Expected output:
# Imported federation:
# ID: did:icn:fed:a1b2c3...
# Name: My Federation
# Verified: true
```

### 3.3 Request Federation Membership

For a new member to join, they need to:

1. Create a membership request:

```bash
# Generate a membership request
icn-wallet federation request-membership \
  --federation did:icn:fed:a1b2c3... \
  --identity did:icn:user:g7h8i9...

# This generates a credential request that must be submitted to AgoraNet
```

2. Submit the request through AgoraNet:

```bash
# Through the AgoraNet web interface or API
POST /api/v1/threads
{
  "title": "Membership Request - Alice",
  "proposal_ref": "credential_request_cid",
  "federation_id": "did:icn:fed:a1b2c3...",
  "topic_type": "membership_request"
}
```

## 4. Governance Workflow

### 4.1 Create CCL Policy

```bash
# Example CCL for member addition policy
cat > member_addition_policy.ccl << EOF
schema MembershipRequest {
  requestor_did: String,
  requestor_name: String,
  justification: String
}

rule ApproveMembershipRequest {
  description: "Approve a new member request"
  when:
    request oftype MembershipRequest
  then:
    authorize(federation, "federation:add_member")
}
EOF

# Compile CCL to WASM
icn-ccl-compiler compile \
  --input member_addition_policy.ccl \
  --output member_addition_policy.wasm
```

### 4.2 Submit Policy to Federation

```bash
# Upload the WASM module to the DAG
WASM_CID=$(icn-dag-tool store \
  --content ./member_addition_policy.wasm \
  --type "CCLPolicy")

# Create a proposal to adopt the policy
icn-wallet proposal create \
  --federation did:icn:fed:a1b2c3... \
  --title "Adopt Member Addition Policy" \
  --content '{
    "description": "CCL policy for member addition",
    "policy_type": "membership",
    "wasm_module_cid": "'"$WASM_CID"'"
  }'

# This returns a proposal ID (e.g., prop_j8k9l0...)
```

### 4.3 Vote on Proposal

```bash
# As a federation admin, vote on the proposal
icn-wallet proposal vote \
  --federation did:icn:fed:a1b2c3... \
  --proposal prop_j8k9l0... \
  --choice approve \
  --reason "The policy implements our agreed membership requirements"
```

### 4.4 Proposal Execution Flow

When a proposal is approved:

1. The governance kernel retrieves the proposal data
2. The WASM module is loaded and executed with the proposal input
3. Authorizations are derived from the execution result
4. An execution receipt is created and signed
5. The receipt is anchored in the DAG as a verifiable credential

```bash
# Check execution status
icn-wallet proposal status --proposal prop_j8k9l0...

# Expected output:
# Proposal: prop_j8k9l0...
# Status: Executed
# Execution Receipt: exec_m0n1o2...
# Execution Date: 2024-07-15T15:45:00Z
# Derived Authorizations:
#   - federation:add_member (did:icn:fed:a1b2c3...)
```

## 5. DAG Synchronization and Receipt Sharing

### 5.1 DAG Synchronization

Federation nodes automatically synchronize DAG states:

```bash
# Check DAG sync status
icn-federation-node dag status --data-dir ./federation_data

# Output:
# DAG Height: 42
# Nodes Count: 128
# Last Sync: 2024-07-15T16:00:00Z
# Connected Peers: 3
# Genesis CID: cid:base58:Qm...
```

### 5.2 Receipt Validation

Any federation member can validate execution receipts:

```bash
# Validate a receipt
icn-wallet receipt verify \
  --receipt exec_m0n1o2... \
  --federation did:icn:fed:a1b2c3...

# Output:
# Receipt: exec_m0n1o2...
# Subject: Proposal (prop_j8k9l0...)
# Issuer: did:icn:fed:a1b2c3...
# Verification: Valid
# Execution Match: True
# Authorizations Valid: True
```

### 5.3 Receipt Sharing

Receipts can be shared between participants:

```bash
# Share a receipt
icn-wallet receipt share \
  --receipt exec_m0n1o2... \
  --recipient did:icn:user:p4q5r6...
```

Recipients can import and verify shared receipts.

## 6. Federation Monitoring and Maintenance

### 6.1 Health Monitoring

```bash
# Check node health
curl http://localhost:8000/api/v1/health

# Expected output:
# {
#   "status": "healthy",
#   "uptime": "3d 2h 15m",
#   "dag_sync": true,
#   "peers_connected": 5,
#   "proposals_pending": 2
# }
```

### 6.2 Governance Metrics

```bash
# View governance statistics
icn-federation-node stats --data-dir ./federation_data

# Output:
# Total Proposals: 15
# Approved: 12
# Rejected: 2
# Pending: 1
# Average Voting Time: 22.5 hours
# Active Members: 7
```

## 7. Troubleshooting

### 7.1 DAG Synchronization Issues

If nodes fail to synchronize:

```bash
# Force DAG resync from peers
icn-federation-node dag resync --data-dir ./federation_data --force

# Analyze DAG integrity
icn-federation-node dag verify --data-dir ./federation_data
```

### 7.2 Execution Receipt Verification Failures

If receipt verification fails:

```bash
# Detailed verification with debug information
icn-wallet receipt verify \
  --receipt exec_m0n1o2... \
  --federation did:icn:fed:a1b2c3... \
  --verbose

# This will show detailed information about why verification failed
```

## 8. Federation Backup and Recovery

### 8.1 Create Federation Backup

```bash
# Create a full backup of federation state
icn-federation-node backup create \
  --data-dir ./federation_data \
  --output ./backups/federation_backup_$(date +%Y%m%d).tar.gz

# Include encryption for sensitive data
icn-federation-node backup create \
  --data-dir ./federation_data \
  --encrypt \
  --output ./backups/federation_backup_$(date +%Y%m%d).enc
```

### 8.2 Recovery from Backup

```bash
# Restore from backup
icn-federation-node backup restore \
  --backup ./backups/federation_backup_20240715.tar.gz \
  --data-dir ./federation_data_restored
```

## 9. Federation Evolution

As your federation grows, you may want to:

1. Update governance policies through new CCL modules
2. Adjust voting thresholds or parameters
3. Add specialized roles and permissions
4. Integrate with external systems via verifiable credentials

These changes can be proposed and approved through the same governance process described above.

## Conclusion

You've now successfully launched an ICN federation with:

- A secure genesis and trust anchor
- Runtime node deployment
- Governance workflows using CCL
- DAG synchronization and receipt verification
- Member onboarding and management

Your federation is now ready for collaborative decision making with verifiable execution! 