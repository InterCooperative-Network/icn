# ICN Wallet Sync

This crate provides functionality for synchronizing data between ICN wallets and runtime nodes, enabling secure and consistent communication across the ICN network.

## Features

- **DAG Node Compatibility Layer**: Convert between wallet and runtime DAG node representations
- **Federation Synchronization**: Fetch credentials and other data from federation endpoints
- **Type-Safe Conversion**: Ensure data integrity when moving between wallet and runtime
- **Credential Management**: Store and verify credentials

## Usage

### Setup

```rust
use icn_wallet_sync::{WalletSync, federation::{FederationEndpoint, FederationSyncClientConfig}};
use icn_storage::Storage;
use std::sync::{Arc, Mutex};

// Create a storage backend
let storage = Arc::new(Mutex::new(my_storage_impl));

// Create a wallet sync instance
let wallet_sync = WalletSync::new(storage);

// Configure federation endpoints
let endpoint = FederationEndpoint {
    federation_id: "main-federation".to_string(),
    base_url: "https://icn-federation.example.com".to_string(),
    last_sync: None,
    auth_token: None,
};

// Start synchronization
let config = FederationSyncClientConfig {
    endpoints: vec![endpoint],
    ..Default::default()
};

// Create a federation client
let federation_client = FederationSyncClient::new(
    store,
    config
);
```

### Converting Between Wallet and Runtime DAG Nodes

```rust
use icn_wallet_sync::compat::{WalletDagNode, wallet_to_runtime, runtime_to_wallet};

// Create a wallet DAG node
let wallet_node = WalletDagNode {
    // ... node properties ...
};

// Convert to runtime format
let runtime_node = wallet_to_runtime(&wallet_node)?;

// ... submit to runtime ...

// Convert back to wallet format
let converted_wallet_node = runtime_to_wallet(&runtime_node)?;
```

### Synchronization

```rust
// Fetch credentials from a federation
let credentials = federation_client.sync_credentials(
    "main-federation",
    &[SyncCredentialType::ExecutionReceipt],
    from_timestamp
).await?;

// Process the credentials
for credential in credentials {
    // Handle each credential
}
```

## Architecture

### Components

- **compat**: Handles conversion between wallet and runtime data structures
- **federation**: Manages communication with federation endpoints
- **credentials**: Provides storage and verification for credentials

### Data Flow

1. Wallet creates a local DAG node
2. Node is converted to runtime format
3. Node is submitted to the runtime
4. Runtime processes the node and adds it to the DAG
5. Other wallets synchronize with the runtime, retrieving the new node
6. Nodes are converted back to wallet format for local storage

## Integration with Other Components

- **Wallet**: Uses wallet-sync to communicate with the runtime
- **Runtime**: Processes DAG nodes submitted via wallet-sync
- **Federation**: Distributes DAG nodes and credentials across the network

## Extensibility

The wallet-sync crate is designed to be extensible:

- Add new credential types by extending the `SyncCredentialType` enum
- Implement custom storage backends by implementing the `CredentialStore` trait
- Create custom federation synchronization strategies for specific use cases

## License

This crate is part of the InterCooperative Network (ICN) project and follows the same licensing terms. 