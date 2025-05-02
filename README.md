# ICN Wallet

A mobile-first wallet for the Intercooperative Network (ICN) that enables secure identity management, credential handling, and governance participation.

## Features

- **Identity Management**: Create and manage scoped DIDs with Ed25519 keypairs
- **Credential Handling**: Issue, verify, and selectively disclose verifiable credentials
- **Governance Participation**: Sign proposals and participate in votes offline
- **DAG Synchronization**: Sync TrustBundles and Guardian mandates from federated peers
- **Mobile-First Design**: API-focused architecture suitable for mobile integration

## Architecture

The project is structured as a workspace with the following crates:

- **wallet-core**: Core identity and cryptography functionality
- **wallet-agent**: Proposal handling and credential management
- **wallet-sync**: DAG synchronization with federation
- **wallet-ui-api**: Frontend integration layer
- **cli**: Command-line interface for development and debugging

## Getting Started

### Prerequisites

- Rust 1.75+ (2021 edition)
- Cargo

### Building

```bash
cargo build --release
```

### Using the CLI

```bash
# Create a new identity
cargo run --bin icn-wallet-cli -- create --scope personal

# Sign a proposal
cargo run --bin icn-wallet-cli -- sign -i /path/to/identity.json -p governance -c /path/to/proposal.json

# Sync from federation
cargo run --bin icn-wallet-cli -- sync -i /path/to/identity.json -v

# Start the API server
cargo run --bin icn-wallet-cli -- serve
```

### API Endpoints

#### Identity Management
- `GET /api/did/list` - List all identities
- `GET /api/did/:id` - Get specific identity
- `POST /api/did/create` - Create new identity
- `POST /api/did/activate/:id` - Set active identity

#### Proposal Handling
- `POST /api/proposal/sign` - Sign a proposal
- `GET /api/actions/:action_type` - List actions by type

#### Credential Management
- `POST /api/vc/verify` - Verify a credential

#### Synchronization
- `POST /api/sync/dag` - Sync TrustBundles from federation

#### Governance
- `POST /api/governance/appeal/:mandate_id` - Appeal a mandate

## Development

### Running Tests

```bash
cargo test
```

### Project Structure

```
icn-wallet/
├── crates/
│   ├── wallet-core/      # Core identity and cryptography
│   ├── wallet-agent/     # Proposal handling and governance
│   ├── wallet-sync/      # DAG synchronization
│   └── wallet-ui-api/    # Frontend API
├── cli/                  # Command-line interface
└── examples/             # Usage examples
```

## License

This project is licensed under [LICENSE TBD]
