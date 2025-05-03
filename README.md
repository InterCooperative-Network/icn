# ICN Wallet

A mobile-first light client wallet for verifiable governance and economic participation in federated DAG systems.

## Architecture

The wallet is organized into several crates:

### wallet-core

Core cryptographic and data structures including:
- DID (Decentralized Identity) management
- Verifiable Credentials handling
- Merkle tree and CID utilities
- DAG data structures
- Storage abstractions

### wallet-agent

Local wallet agent that:
- Maintains a queue of pending actions (proposals, votes, anchors)
- Handles signing of payloads
- Maintains local DAG state
- Works offline-first with serialization to disk

### wallet-sync

Sync adapter for communicating with the federation:
- Fetches DAG receipts
- Retrieves trust bundles
- Handles anchoring data
- Provides REST and libp2p connectivity

### wallet-ui-api

API layer for interacting with the wallet from user interfaces:
- Exposes REST endpoints for wallet operations
- Manages wallet state
- Provides identity, credential, and DAG operations

### cli

Command line interface for wallet operations:
- Identity management
- Credential issuance and verification
- Action queue management
- DAG operations
- API server control

## Key Features

- **Offline-first**: All operations can be queued locally and synchronized later
- **Mobile-compatible**: Light client designed for resource-constrained environments
- **DAG verification**: Verify and participate in DAG-based governance
- **DID & VC support**: Full W3C standards compatibility
- **Pluggable storage**: Flexible backend storage options

## Getting Started

### Prerequisites

- Rust 1.74 or later
- Cargo

### Building

```bash
cargo build --release
```

### Running the CLI

Create a new identity:
```bash
cargo run --bin icn-wallet-cli identity create --scope personal
```

List identities:
```bash
cargo run --bin icn-wallet-cli identity list
```

Issue a credential:
```bash
cargo run --bin icn-wallet-cli credential issue --issuer did:icn:abc123 --subject '{"name":"Alice","role":"Admin"}' --types Membership,Admin
```

Queue an action:
```bash
cargo run --bin icn-wallet-cli action queue --creator did:icn:abc123 --action-type proposal --payload '{"title":"New proposal","content":"Proposal content"}'
```

Start the API server:
```bash
cargo run --bin icn-wallet-cli serve --addr 127.0.0.1:3000
```

## API Usage

Once the API server is running, you can interact with it using REST:

```bash
# List identities
curl http://localhost:3000/api/identities

# Create identity
curl -X POST http://localhost:3000/api/identities -H "Content-Type: application/json" -d '{"scope":"personal"}'

# Issue credential
curl -X POST http://localhost:3000/api/credentials -H "Content-Type: application/json" -d '{"issuer_did":"did:icn:abc123","subject_data":{"name":"Alice"},"credential_types":["Membership"]}'

# Queue action
curl -X POST http://localhost:3000/api/actions -H "Content-Type: application/json" -d '{"creator_did":"did:icn:abc123","action_type":"proposal","payload":{"title":"My Proposal"}}'
```

## Mobile Integration

The wallet is designed to be integrated with mobile applications through the wallet-ui-api. You can:

1. Build native libraries using the wallet components
2. Access the API server from a mobile app
3. Use the crates directly in a Rust mobile SDK

## Mobile Integration with FFI

The wallet functionality can now be accessed from mobile platforms (iOS and Android) through the `wallet-ffi` crate, which provides a Foreign Function Interface using the UniFFI framework.

### Features Implemented

1. **FFI Bridge (wallet-ffi crate)**:
   - Exposes core wallet functionality to mobile platforms via UniFFI
   - Provides a clean, idiomatic API for Kotlin and Swift
   - Handles async operations seamlessly via a dedicated Tokio runtime

2. **Enhanced Data Validation**:
   - Added comprehensive validation for TrustBundles and DagNodes
   - Ensures data integrity and security before storing in the wallet
   - Protects against malformed or malicious data from network sources

3. **Mobile Bindings Generation**:
   - Script to automatically generate Swift and Kotlin bindings
   - Creates idiomatic wrapper classes for mobile platforms
   - Simplifies mobile integration process

### Current Limitations

There is currently a circular dependency between `wallet-agent` and `wallet-sync` crates that prevents the build from succeeding. This should be resolved by refactoring shared types into a new `wallet-types` crate.

### Next Steps

1. Resolve the circular dependency issue by creating a `wallet-types` crate
2. Complete end-to-end integration tests for the FFI interface
3. Add platform-specific packaging for Android (AAR) and iOS (XCFramework)
4. Implement advanced validation with cryptographic signature verification
5. Add more comprehensive error handling and recovery mechanisms

### Building and Usage

See the [wallet-ffi README](crates/wallet-ffi/README.md) for detailed instructions on building and using the FFI bindings in mobile applications.

## License

[MIT License](LICENSE)
