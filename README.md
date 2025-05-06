# Internet Cooperation Network (ICN)

A decentralized governance and economic coordination system built using Rust. The ICN system facilitates federated decision-making and resource allocation through a WebAssembly execution environment.

## Project Structure

The ICN project has been reorganized into a standardized monorepo structure with all components consolidated under a single workspace:

```
icn/
├── crates/               # All Rust crates organized by component
│   ├── runtime/          # Runtime/CoVM components
│   ├── wallet/           # Wallet components
│   ├── agoranet/         # AgoraNet components
│   ├── mesh/             # Mesh compute components
│   └── common/           # Shared utilities and libraries
├── docs/                 # Documentation
├── scripts/              # Build and development scripts
└── Cargo.toml            # Workspace definition
```

For detailed information about the project structure, see [NEW_STRUCTURE.md](docs/NEW_STRUCTURE.md).

## Components

The ICN system consists of four primary components:

1. **Runtime (CoVM v3)**: A WebAssembly execution environment that processes governance operations, enforces economic policies, and maintains federated state.

2. **Wallet**: A secure, mobile-first client agent that manages user identity, credentials, and local state.

3. **AgoraNet**: A REST API server and deliberation engine that facilitates inter-federation communication and governance processes.

4. **Mesh Compute**: A distributed computation overlay network that enables secure, privacy-preserving task execution.

## Documentation

For detailed documentation on each component, refer to:

- [Architecture Overview](docs/ARCHITECTURE.md)
- [Runtime Overview](docs/RUNTIME_OVERVIEW.md)
- [Wallet Overview](docs/WALLET_OVERVIEW.md)
- [AgoraNet Overview](docs/NETWORKING.md)
- [Mesh Implementation](docs/MESH_IMPLEMENTATION.md)
- [DAG Structure](docs/DAG_STRUCTURE.md)
- [System Integration](CONSOLIDATED_SYSTEM_INTEGRATION.md)
- [Developer Guide](CONSOLIDATED_DEVELOPER_GUIDE.md)

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)
- Cargo
- Docker (for development environments)

### Building the Project

The project now uses a unified build system:

```bash
# Build all components
./scripts/build.sh

# Build specific components
./scripts/build.sh runtime
./scripts/build.sh wallet
./scripts/build.sh agoranet
./scripts/build.sh mesh

# Build in release mode
./scripts/build.sh --release

# Build and run tests
./scripts/build.sh --test
```

### Development Workflow

1. Clone the repository:
   ```bash
   git clone https://github.com/intercoop-network/icn.git
   cd icn
   ```

2. Build the components you want to work on:
   ```bash
   ./scripts/build.sh common
   ./scripts/build.sh runtime
   ```

3. Run tests to ensure everything is working:
   ```bash
   ./scripts/build.sh common --test
   ```

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](docs/CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License or Apache License 2.0, at your option - see the LICENSE files for details. 