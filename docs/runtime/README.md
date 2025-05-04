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

We welcome contributions from everyone who shares our vision of democratic, cooperative technology. Please see our [contribution guidelines](docs/CONTRIBUTING.md) for more information.

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

## Phase 2: Federation Mechanics

The ICN Runtime now includes Phase 2 functionality, implementing robust federation mechanics for trust, replication, and synchronization:

### TrustBundle Synchronization

The federation protocol now supports epoch-aware TrustBundle synchronization:

- Runtime nodes automatically discover and exchange TrustBundles using the `/icn/trustbundle/1.0.0` protocol
- TrustBundles contain DAG roots, attestations, and federation membership information
- Bundles are verified using quorum signatures before being accepted and stored
- Epochs ensure consistent progression of federation state

Wallet clients can now sync with federation nodes using the `SyncClient`:

```rust
// Create a federation client connected to runtime nodes
let mut federation_client = SyncClient::federation_client("my-wallet-did");

// Add federation nodes to connect to
federation_client.add_federation_node(FederationNodeAddress {
    http_url: "http://localhost:8080",
    p2p_addr: Some("/ip4/127.0.0.1/tcp/4001"),
    node_id: None,
});

// Get the latest trust bundle
let bundle = federation_client.get_latest_trust_bundle().await?;
println!("Got trust bundle for epoch {}", bundle.epoch);

// Subscribe to trust bundle updates
let mut subscription = federation_client.subscribe_to_trust_bundles();
tokio::spawn(async move {
    while let Some(bundle) = subscription.next().await {
        println!("New trust bundle received: epoch {}", bundle.epoch);
    }
});
```

### Blob Replication Protocol

Content-addressed blobs are now replicated across the federation:

- Pinned blobs trigger the replication protocol according to policy
- Replication policies can specify factor, specific peers, or no replication
- Replication status is tracked and verified
- The protocol handles content discovery, transfer, and integrity validation

Runtime API for blob replication:

```rust
// Pin a blob (triggers replication)
let cid = blob_store.put_blob(&content).await?;
blob_store.pin_blob(&cid).await?;

// Explicitly control replication
let policy = ReplicationPolicy::Factor(3); // Replicate to 3 peers
federation_manager
    .identify_replication_targets(cid, policy, context_id)
    .await?;
```

Wallet API for blob retrieval:

```rust
// Fetch a blob by CID
let cid = "bafybeihcqkmk7dqtvcf...";
let blob_data = federation_client.get_blob(cid).await?;
```

### Federation Health and Discovery

Health endpoints provide detailed federation status:

- REST API endpoint at `/api/v1/federation/health`
- Reports on epoch status, peer connectivity, and replication health
- Includes quorum diagnostics showing federation composition

A diagnostic dashboard is available at `/api/v1/federation/diagnostics` with:

- Detailed peer information
- DAG consistency checks
- Blob replication statistics
- Detected inconsistencies or issues

### Testing with Multiple Nodes

A Docker Compose configuration for testing federation with multiple nodes is provided:

1. Configuration includes genesis, validator, guardian, and observer nodes
2. Each node has different roles and permissions in the federation
3. Automatic bootstrap and peer discovery is configured

To start the test environment:

```bash
cd runtime
docker-compose -f docker-compose.integration.yml up -d
```

Monitor federation status:
- Federation dashboard: http://localhost:3002
- Metrics: http://localhost:3001 (Grafana)

### Configuration

Federation behavior can be configured in `runtime-config.toml`:

```toml
[federation]
bootstrap_period_sec = 30
peer_sync_interval_sec = 60
trust_bundle_sync_interval_sec = 300
max_peers = 25
default_replication_factor = 3
```

See [FEDERATION_PROTOCOL.md](docs/FEDERATION_PROTOCOL.md) and [BLOB_REPLICATION.md](docs/BLOB_REPLICATION.md) for detailed documentation. 