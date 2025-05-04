# ICN - Internet Cooperation Network

This repository contains the core components of the Internet Cooperation Network (ICN).

## Repository Structure

- **wallet/** - The ICN Wallet implementation
  - Modern, modular architecture with clean separation of concerns
  - Support for DIDs, secure storage, and federation protocol
  
- **runtime/** - The ICN Runtime implementation
  - Execution environment for ICN applications
  - DAG-based state management
  
- **agoranet/** - AgoraNet implementation (federation node)
  - Networking and synchronization for the ICN network
  
- **docs/** - Documentation for the ICN system
  - Architecture guides
  - Protocol specifications
  
- **scripts/** - Utility scripts for development and deployment

## Getting Started

### Prerequisites

- Rust 1.70+ (`rustc` and `cargo`)
- Node.js 18+ (for CLI tools)

### Building the Wallet

```bash
cd wallet
cargo build
```

### Running the Runtime

```bash
cd runtime
cargo run -- --help
```

### Running the Development Network

```bash
./scripts/run_icn_devnet.sh
```

## Architecture

The ICN system consists of several key components:

1. **Identity System** - DID-based identity with cryptographic verification
2. **DAG System** - Directed Acyclic Graph for state management
3. **Federation Protocol** - For node synchronization and consensus
4. **Governance Kernel** - For community governance and decision making

Please refer to the documentation in `docs/` for more detailed information.

## Documentation

For detailed information about ICN, refer to these documents:

- [Architecture Overview](docs/ARCHITECTURE.md) - System architecture and components
- [DAG Structure](docs/DAG_STRUCTURE.md) - Technical details of the DAG implementation
- [Governance System](docs/GOVERNANCE_SYSTEM.md) - Federation governance mechanisms
- [Economic System](docs/ECONOMICS.md) - Token economics and resource metering
- [Security](docs/SECURITY.md) - Security model and threat mitigations
- [Trust Model](docs/TRUST_MODEL.md) - Trust relationships and federation design
- [Integration Guide](docs/INTEGRATION_GUIDE.md) - Guide for developers and federation operators

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under dual MIT/Apache-2.0 license. 