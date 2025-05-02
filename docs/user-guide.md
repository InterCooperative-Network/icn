# ICN Wallet User Guide

This guide explains how to use the ICN Wallet for identity management, credential handling, and governance participation in the Intercooperative Network.

## Installation and Setup

### Prerequisites

- Rust 1.75+ (2021 edition)
- Cargo
- An internet connection for AgoraNet and federation synchronization

### Building from Source

1. Clone the repository:
   ```bash
   git clone https://github.com/icn-network/icn-wallet.git
   cd icn-wallet
   ```

2. Build the wallet:
   ```bash
   cargo build --release
   ```

3. Run the CLI:
   ```bash
   ./target/release/icn-wallet-cli --help
   ```

### Configuration

The wallet stores data in `./wallet-data` by default. You can specify a different location with the `--data-dir` option:

```bash
icn-wallet-cli --data-dir /path/to/data create --scope personal
```

## Identity Management

### Creating an Identity

Create a new identity with a specific scope:

```bash
icn-wallet-cli create --scope personal
```

Available scopes:
- `personal`: For individual users
- `organization`: For organizational accounts
- `device`: For device-specific identities
- `service`: For service accounts

You can add optional metadata as JSON:

```bash
icn-wallet-cli create --scope personal --metadata '{"name": "Jane Doe", "email": "jane@example.com"}'
```

### Managing Multiple Identities

The wallet supports multiple identities. Each identity is assigned a unique ID when created. Use this ID to select the identity for operations:

```bash
# List all identities
icn-wallet-cli serve  # then access GET /api/did/list

# Set active identity via API
# POST /api/did/activate/{id}
```

## Credential Management

### Issuing Credentials

Issue a new credential to a subject:

```bash
# Via API
# POST /api/vc/issue
# {
#   "subject_data": {
#     "id": "did:icn:subject",
#     "name": "Subject Name",
#     "role": "Member"
#   },
#   "credential_types": ["MembershipCredential"]
# }
```

### Verifying Credentials

Verify a credential's authenticity:

```bash
# Via API
# POST /api/vc/verify
# {
#   "credential": {
#     // Credential JSON
#   }
# }
```

### Linking Credentials

Link a credential to an AgoraNet discussion thread:

```bash
# Via CLI
icn-wallet-cli -i /path/to/identity.json AgoraNet --url https://agoranet.example.com LinkCredential --thread-id thread123 --credential-id cred456

# Via API
# POST /api/agoranet/credential-link
# {
#   "thread_id": "thread123",
#   "credential_id": "cred456"
# }
```

## Governance Participation

### Viewing Proposals

Browse active governance proposals:

```bash
# Via CLI
icn-wallet-cli -i /path/to/identity.json AgoraNet --url https://agoranet.example.com ListThreads --topic governance

# Via API
# GET /api/agoranet/threads?topic=governance
```

### Creating a Proposal

Create and sign a new governance proposal:

```bash
# Via CLI
icn-wallet-cli Sign -i /path/to/identity.json -p ConfigChange -c /path/to/proposal.json

# Via API
# POST /api/proposal/sign
# {
#   "proposal_type": "ConfigChange",
#   "content": {
#     "title": "Increase Voting Period",
#     "description": "Increase the voting period to 14 days",
#     "parameter": "voting_period",
#     "value": "14d"
#   }
# }
```

### Voting on Proposals

Vote on an existing proposal:

```bash
# Via API
# POST /api/proposals/{id}/vote
# {
#   "decision": "Approve",
#   "reason": "This is a good proposal"
# }
```

### Creating Execution Receipts

For guardians, create execution receipts for completed proposals:

```bash
# Via CLI
icn-wallet-cli -i /path/to/identity.json Bundle CreateReceipt -p proposal123 -r /path/to/result.json

# Via API
# POST /api/proposals/{id}/receipt
# {
#   "success": true,
#   "timestamp": "2023-05-01T12:00:00Z",
#   "votes": {
#     "approve": 3,
#     "reject": 1,
#     "abstain": 0
#   }
# }
```

## Federation Synchronization

### Syncing TrustBundles

Sync the latest TrustBundles from the federation:

```bash
# Via CLI
icn-wallet-cli Sync -i /path/to/identity.json -v

# Via API
# POST /api/sync/trust-bundles
```

### Viewing TrustBundles

List the synchronized TrustBundles:

```bash
# Via CLI
icn-wallet-cli -i /path/to/identity.json Bundle List --format json

# Via API
# GET /api/trust-bundles
```

### Checking Guardian Status

Check if your identity is an active guardian:

```bash
# Via CLI
icn-wallet-cli -i /path/to/identity.json Bundle CheckStatus

# Via API
# GET /api/guardian/status
```

## API Server

### Running the API Server

Start the wallet API server for frontend integration:

```bash
icn-wallet-cli serve --host 127.0.0.1 --port 3000 --agoranet-url https://agoranet.example.com/api
```

### API Endpoints

The API server provides the following endpoints:

#### Identity Management
- `GET /api/did/list` - List all identities
- `GET /api/did/:id` - Get specific identity
- `POST /api/did/create` - Create new identity
- `POST /api/did/activate/:id` - Set active identity

#### Proposal Handling
- `POST /api/proposal/sign` - Sign a proposal
- `GET /api/actions/:action_type` - List actions by type
- `POST /api/proposals/:id/vote` - Vote on a proposal
- `POST /api/proposals/:id/receipt` - Create execution receipt

#### Credential Management
- `POST /api/vc/issue` - Issue a credential
- `POST /api/vc/verify` - Verify a credential
- `GET /api/vc/list` - List credentials

#### AgoraNet Integration
- `GET /api/agoranet/threads` - List threads
- `GET /api/agoranet/threads/:id` - Get thread details
- `POST /api/agoranet/credential-link` - Link credential to thread
- `POST /api/agoranet/proposals/:id/notify` - Notify about proposal events

#### Synchronization
- `POST /api/sync/trust-bundles` - Sync TrustBundles from federation
- `GET /api/trust-bundles` - List TrustBundles

## WebSocket Notifications

The wallet provides real-time updates via WebSocket:

```
ws://localhost:3000/ws
```

Events include:
- Identity changes
- Proposal updates
- Sync completion
- Guardian status changes

For more information, see [README-WEBSOCKET.md](../README-WEBSOCKET.md).

## Troubleshooting

### Common Issues

1. **Connection Failures**:
   - Ensure AgoraNet URL is correct
   - Check network connectivity
   - Verify the identity has appropriate permissions

2. **Sync Failures**:
   - Ensure federation peers are accessible
   - Check for data directory permissions
   - Retry with verbose mode for detailed logs

3. **Credential Verification Failures**:
   - Verify the credential format is valid
   - Ensure the issuer's DID is resolvable
   - Check for expired credentials

### Logs

For detailed logging, set the `RUST_LOG` environment variable:

```bash
RUST_LOG=info icn-wallet-cli serve
```

For debugging, use:

```bash
RUST_LOG=debug icn-wallet-cli serve
```

### Recovery

If wallet data becomes corrupted, you can recover from backup or reinitialize:

1. Stop any running wallet processes
2. Move the corrupted data directory: `mv wallet-data wallet-data.bak`
3. Create a new data directory: `mkdir wallet-data`
4. Restore from backup or recreate identities

### Getting Help

For additional help:
- Check the [GitHub repository](https://github.com/icn-network/icn-wallet) issues
- Join the ICN community forum
- Refer to the API documentation in the `docs` directory 