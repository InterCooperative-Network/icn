# ICN Wallet Architecture

## Overview

The ICN Wallet is a modular system designed to enable secure identity management, credential handling, and governance participation in the Intercooperative Network (ICN). It connects to key ecosystem components:

- **AgoraNet**: The discussion and deliberation platform where proposals are discussed and credentials can be linked
- **ICN Runtime**: The execution environment that processes proposals, manages guardian roles, and maintains the TrustBundle DAG

## Component Architecture

The wallet is structured as a Rust workspace with several focused crates:

### 1. `wallet-core`

Core identity and cryptographic functionality:
- Identity creation and management (DIDs)
- Cryptographic operations (signing, verification)
- Credential schema definitions and validation

Key components:
- `IdentityWallet`: Manages keypairs and DIDs
- `CredentialSigner`: Issues and verifies credentials

### 2. `wallet-agent`

Business logic for proposal handling and governance:
- Proposal creation, signing, and queuing
- Guardian role management
- TrustBundle handling
- Communication with AgoraNet

Key components:
- `ProposalQueue`: Manages pending proposals and actions
- `Guardian`: Handles guardian-specific operations
- `AgoraNetClient`: Interface to AgoraNet API

### 3. `wallet-sync`

Synchronization with the federation:
- TrustBundle DAG synchronization
- Peer discovery and communication
- Data integrity verification

Key components:
- `SyncClient`: Manages synchronization with federated peers

### 4. `wallet-ui-api`

Frontend integration layer:
- RESTful API for UI interaction
- WebSocket notifications for state changes
- Session management

Key components:
- API handlers in `handlers.rs`
- Shared state management in `state.rs`

### 5. CLI Tool

Command-line interface for development and debugging:
- Identity management commands
- Proposal signing and handling
- AgoraNet integration commands
- TrustBundle management

## Data Flow

1. **Identity Creation**:
   - User creates identity through CLI or UI API
   - `IdentityWallet` generates keypair and DID
   - Identity is stored locally

2. **Credential Operations**:
   - Credentials can be issued by the wallet or received from external sources
   - `CredentialSigner` validates and stores credentials
   - Credentials can be linked to AgoraNet threads for governance participation

3. **Proposal Workflow**:
   - User creates proposal via UI API
   - `Guardian` signs the proposal
   - `ProposalQueue` manages proposal state
   - Proposal is submitted to AgoraNet for discussion
   - When approved, can be executed by Runtime

4. **TrustBundle Synchronization**:
   - `SyncClient` fetches latest TrustBundles from federation
   - Bundles are validated and stored locally
   - Guardian status is determined based on TrustBundle membership

## Integration Points

### AgoraNet Integration

- Discussion thread management
- Credential linking
- Proposal notifications
- Event publishing

Interface: `AgoraNetClient` in `wallet-agent/agoranet.rs`

### ICN Runtime Integration

- Proposal execution
- Guardian actions
- TrustBundle validation
- Credential verification

## Security Model

- Private keys are managed exclusively by `wallet-core`
- All external communications are authenticated with DIDs
- Proposals require explicit signing
- TrustBundles require threshold signatures for validity
- Credentials follow W3C Verifiable Credential standards

## State Management

- Identity information stored in `./wallet-data/identities/`
- Proposals queue in `./wallet-data/queue/`
- TrustBundles in `./wallet-data/bundles/`
- In-memory state during API operation managed by `SharedState`

## Error Handling

- Errors propagated through `anyhow` for internal operations
- API errors standardized through `ApiError` types
- User feedback provided through appropriate HTTP status codes and error messages

## Extension Points

- Additional credential types can be added to `wallet-core`
- New proposal handlers can be implemented in `wallet-agent`
- UI API can be extended with additional endpoints
- WebSocket notifications can be added for real-time updates 