# ICN Wallet Core

The `icn-wallet-core` crate serves as the central foundation for the ICN Wallet ecosystem. It coordinates all the wallet's primary functionality and provides the core business logic that powers wallet operations.

## Features

- **Identity Management**: Manages DIDs and verifiable credentials
- **Proposal Lifecycle**: Creates, signs, and tracks governance proposals
- **DAG Integration**: Connects with the ICN's Directed Acyclic Graph for data exchange
- **Credential Operations**: Issues, verifies, and manages verifiable credentials
- **Federation Interaction**: Interfaces with federation services and resources
- **Security**: Enforces proper access control and data protection
- **State Management**: Maintains a consistent wallet state across operations

## Architecture

The wallet core implements a modular architecture:

- **Service Layer**: High-level wallet functions exposed to applications
- **Domain Layer**: Core business logic for wallet operations
- **Infrastructure Layer**: Integration with external systems and storage

## Usage

### Adding as a Dependency

```toml
[dependencies]
icn-wallet-core = { version = "0.1.0", path = "../icn-wallet-core" }
```

### Basic Example

```rust
use icn_wallet_core::{WalletCore, WalletConfig};
use icn_wallet_storage::StorageManager;
use icn_wallet_identity::IdentityManager;
use icn_wallet_sync::SyncManager;

async fn wallet_example() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize necessary components
    let storage = StorageManager::new("wallet_data").await?;
    let identity = IdentityManager::new(storage.clone());
    let sync = SyncManager::new("https://node.example.com");
    
    // Configure the wallet
    let config = WalletConfig {
        wallet_name: "My ICN Wallet".to_string(),
        federation_url: "https://federation.example.com".to_string(),
        // Other configuration options...
    };
    
    // Create the wallet core
    let wallet = WalletCore::new(
        config, 
        storage,
        identity,
        sync,
    ).await?;
    
    // Use wallet features
    let did = wallet.identity().create_identity().await?;
    
    // Create a proposal
    let proposal_id = wallet.create_proposal(
        "My Proposal",
        serde_json::json!({
            "description": "A test proposal",
            "action": "add_member",
            "member": "did:icn:new_member"
        }),
    ).await?;
    
    println!("Created proposal: {}", proposal_id);
    
    Ok(())
}
```

## Component Integration

The wallet core integrates with several other ICN wallet components:

- `icn-wallet-storage`: For persistent data storage
- `icn-wallet-identity`: For identity-related operations
- `icn-wallet-sync`: For network synchronization
- `icn-wallet-actions`: For tracking and executing wallet actions

## License

This crate is part of the ICN Wallet project and is licensed under the same terms as the parent project. 