# ICN Runtime (CoVM V3)

The Intercooperative Network Runtime (CoVM V3) is a constitutional engine for cooperative and community governance. It provides a secure, verifiable, and participatory infrastructure for post-capitalist coordination.

## Vision

The ICN Runtime serves as a constitutional substrate, enabling Cooperatives, Communities, Federations, and Individuals to operate within a shared framework of:

- Participatory governance with verifiable processes
- Non-extractive, commons-based economics
- Scoped identity with contextual reputation
- Restorative justice through deliberative processes
- Federation-scale coordination with local autonomy

Unlike traditional blockchain systems, ICN focuses on constitutionally-bound political and social primitives, building cooperation rather than competition into its core architecture.

## Key Components

### Governance Kernel
The heart of the system, providing:
- Constitutional Cooperative Language (CCL) interpretation
- Core Law Modules (Civic, Contract, Justice)
- Proposal processing and execution
- Democratic decision-making primitives

### CCL to WASM Compilation
A powerful bridge between governance rules and execution:
- Transform declarative CCL templates into executable WASM
- Domain-specific language (DSL) inputs for action parameterization
- Verifiable, deterministic execution of governance rules
- Integration with the VM for resource-controlled execution

### DAG System
A verifiable, append-only data structure supporting:
- Immutable operation history
- Merkle-based integrity verification
- Lineage attestations
- Forkless by design through constitutional processes

### Identity System
A comprehensive identity framework with:
- Scoped DIDs (Cooperative, Community, Individual)
- Verifiable Credentials with selective disclosure
- Trust Bundles for federation-wide verification
- Guardian roles for constitutional enforcement

### Economic System
A non-extractive resource system enabling:
- Scoped Resource Tokens (not speculative currency)
- Participatory Budgeting primitives
- Metering for resource usage tracking
- Multi-dimensional value accounting

### Federation System
Tools for cross-community coordination:
- Trust Bundle synchronization
- Quorum-based decision making
- Guardian mandates with federation oversight
- Resource sharing with constitutional constraints

### Distributed Storage
A robust data storage system providing:
- Content-addressable blob storage
- Replication with governance-defined policies
- Access control through identity verification
- Federation-wide data availability

## Getting Started

### Prerequisites
- Rust 1.70 or later
- Cargo and standard Rust tooling

### Building from Source

```bash
# Clone the repository
git clone https://github.com/intercooperative-network/icn-covm-v3.git
cd icn-covm-v3

# Build the project
cargo build --release

# Run tests
cargo test --workspace
```

### Using the CLI

The CoVM CLI provides access to all runtime functionality:

```bash
# Register a new identity
./target/release/covm identity register --scope cooperative --name "My Cooperative"

# Compile a CCL template with DSL input into a WASM module
./target/release/covm compile --ccl-template examples/cooperative_bylaws.ccl --dsl-input examples/dsl/propose_join.dsl --output proposal.wasm --scope cooperative

# Create a proposal using a CCL template
./target/release/covm propose --ccl-template examples/cooperative_bylaws.ccl --dsl-input my_params.json --identity did:icn:my-identity

# Vote on a proposal
./target/release/covm vote --proposal-id <CID> --vote approve --reason "Aligns with our values" --identity did:icn:my-identity

# Execute an approved proposal
./target/release/covm execute --proposal-payload proposal.wasm --constitution examples/cooperative_bylaws.ccl --identity did:icn:my-identity --scope cooperative

# Export a verifiable credential
./target/release/covm export-vc --credential-id <CID> --output credential.json
```

## Documentation

Comprehensive documentation is available in the `docs/` directory:

- [Governance Kernel](docs/GOVERNANCE_KERNEL.md)
- [CCL to WASM Compilation](docs/CCL_TO_WASM.md)
- [DAG System](docs/DAG_SYSTEM.md)
- [Identity System](docs/IDENTITY_SYSTEM.md)
- [Economic System](docs/ECONOMIC_SYSTEM.md)
- [Distributed Storage](docs/DISTRIBUTED_STORAGE.md)
- [Development Roadmap](docs/ROADMAP.md)

## Development Status

This project is currently in early development. See the [roadmap](docs/ROADMAP.md) for detailed development plans.

## Contributing

We welcome contributions from everyone who shares our vision of democratic, cooperative technology. Please see our [contribution guidelines](CONTRIBUTING.md) for more information.

## License

This project is licensed under [LICENSE_TBD] - a cooperative-compatible license that ensures the software remains in the commons while allowing for cooperative use and modification.

## Acknowledgements

The ICN Runtime builds on years of research and development in cooperative technology, drawing inspiration from:
- Democratic governance systems
- Commons-based resource management
- Distributed systems and content-addressed storage
- Self-sovereign identity frameworks
- Cooperative economic models

## Integration Testing

The ICN Runtime now supports automated integration testing with improved stability, state verification mechanisms, and predictable interaction patterns.

### Key Features

- **Stabilized Docker Configuration**: Reliable container setup with health checks, fixed ports, and predictable volumes
- **Debug API**: Read-only API endpoints under `/api/v1/debug` for state inspection and verification
- **Structured Logging**: JSON-formatted logs for easier parsing and analysis
- **Event Monitoring**: WebSocket monitoring tools to verify event emission
- **State Reset**: Utilities to reset runtime state between test runs

See the [integration testing documentation](tests/README.md) for detailed information on how to use these features for automated testing. 