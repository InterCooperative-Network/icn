# Wallet Sync

A synchronization library for the ICN wallet that communicates with the ICN runtime and federation nodes.

## Features

- **DAG Node Synchronization**: Submit and retrieve DAG nodes from the network
- **Trust Bundle Management**: Create and verify trust bundles containing trusted DIDs
- **Federation Discovery**: Discover federation nodes and retrieve peer information
- **Resilient Communication**: Automatic retries with exponential backoff for network operations

## Architecture

The wallet-sync crate provides a clean, type-safe interface for synchronizing wallet data with the ICN network:

- `SyncClient`: Core client for HTTP API communication with ICN nodes
- `SyncService`: High-level service with retry and error handling capabilities
- `TrustManager`: Specialized component for trust bundle synchronization
- Compatible data types that work across the wallet and runtime components

## Usage

### Basic Usage

```rust
use wallet_sync::{SyncClient, SyncService, DagNode};
use serde_json::json;

// Create a client connected to an ICN node
let client = SyncClient::new("http://localhost:8080".to_string());

// Create a sync service with retry capabilities
let sync_service = SyncService::new(client.clone());

// Create a DAG node
let node = DagNode::new(
    "example-id".to_string(),
    json!({ 
        "type": "Example",
        "data": "Some data" 
    }),
    vec![]
);

// Submit the node with automatic retries
let result = sync_service.submit_node_with_retry(&node).await?;
```

### Trust Bundle Management

```rust
use wallet_sync::{SyncClient, TrustManager, TrustBundle};

// Create a client
let client = SyncClient::new("http://localhost:8080".to_string());

// Create a trust manager
let trust_manager = TrustManager::new(client);

// Create a trust bundle
let mut trust_bundle = TrustBundle::new(
    "My Trust Bundle".to_string(),
    "did:icn:issuer".to_string(),
    vec!["did:icn:trusted-1".to_string(), "did:icn:trusted-2".to_string()]
);

// Submit the trust bundle
let bundle_id = trust_manager.submit_trust_bundle(&mut trust_bundle).await?;

// Retrieve a trust bundle
let retrieved_bundle = trust_manager.get_trust_bundle(&bundle_id).await?;
```

### Federation Discovery

```rust
use wallet_sync::SyncClient;

// Create a client
let client = SyncClient::new("http://localhost:8080".to_string());

// Discover federation nodes
let endpoints = client.discover_federation().await?;

// Explore federation info
let federation_info = client.get_federation_info().await?;
```

## Design Decisions

1. **Consistent Type Handling**: All data types use `serde_json::Value` for the `data` field to avoid binary/JSON conversion issues
2. **Chrono for Timestamps**: Uses `chrono::DateTime<Utc>` consistently for timestamp handling
3. **Renamed Dependencies**: Uses renamed dependencies to avoid version conflicts
4. **Error Handling**: Comprehensive error handling with conversion between error types

## Running the Example

```bash
# Set the ICN node URL (optional)
export ICN_NODE_URL=http://localhost:8080

# Run the example
cargo run --example basic_sync
```

## Testing

Run the tests with:

```bash
cargo test
``` 