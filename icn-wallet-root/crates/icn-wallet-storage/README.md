# ICN Wallet Storage

The `icn-wallet-storage` crate provides robust, secure, and flexible data persistence capabilities for the ICN Wallet ecosystem. It implements various storage strategies to handle different types of wallet data with appropriate security guarantees.

## Features

- **Multiple Storage Types**: Key-value, document, binary, and DAG storage implementations
- **Secure Storage**: Encrypted storage for sensitive data like private keys
- **Versioned Storage**: Track changes to documents with full version history
- **Searchable Indexes**: Secure indexing for efficient data retrieval
- **Lifecycle Management**: Handle different wallet states (active, locked, background)
- **Storage Namespacing**: Organize data efficiently with namespaced storage

## Storage Types

- **Key-Value Storage**: Simple storage for configuration and settings
- **Document Storage**: JSON document storage with collection-based organization
- **Binary Storage**: Efficient storage for binary blobs like credential proofs
- **DAG Storage**: Specialized storage for DAG nodes with parent-child relationships
- **Secure Storage**: Encrypted storage for sensitive information
- **Versioned Storage**: Track document and DAG node history with metadata

## Usage

### Adding as a Dependency

```toml
[dependencies]
icn-wallet-storage = { version = "0.1.0", path = "../icn-wallet-storage" }
```

### Basic Example

```rust
use icn_wallet_storage::StorageManager;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct UserProfile {
    name: String,
    email: String,
}

async fn storage_example() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the storage manager
    let storage = StorageManager::new("wallet_data").await?;
    
    // Store application settings
    storage.store_setting("app_theme", &"dark").await?;
    
    // Store a user profile in a collection
    let profile = UserProfile {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };
    storage.store_object("profiles", "alice", &profile).await?;
    
    // Retrieve the profile
    let retrieved: UserProfile = storage.get_object("profiles", "alice").await?;
    println!("Retrieved profile: {} ({})", retrieved.name, retrieved.email);
    
    // Store a versioned document
    let version = storage.store_object_versioned(
        "settings", 
        "network", 
        &serde_json::json!({
            "endpoint": "https://node.example.com",
            "timeout": 30
        }),
        Some("admin")
    ).await?;
    println!("Created settings version: {}", version);
    
    // Store sensitive data securely
    storage.store_secret("api_key", &"secret-api-key-value").await?;
    
    Ok(())
}
```

### Secure Indexing Example

```rust
use icn_wallet_storage::{StorageManager, indexing::TermsExtraction};
use serde_json::json;

async fn secure_index_example() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize storage with secure indexing
    let mut storage = StorageManager::new("wallet_data").await?;
    storage.init_secure_indexing().await?;
    
    // Store a document with indexing
    storage.store_secret_with_indexing(
        "contact:1",
        &json!({
            "name": "John Smith",
            "email": "john@example.com",
            "phone": "+1234567890"
        }),
        "contacts",
        Some("Personal contact"),
        TermsExtraction::Values
    ).await?;
    
    // Search for contacts
    let results = storage.search_secrets("contacts", "john", 10).await?;
    
    for result in results {
        println!("Found: {} (score: {})", result.key, result.score);
    }
    
    Ok(())
}
```

## Integration with ICN Wallet

This crate is fundamental to the ICN wallet ecosystem and is used by:

- `icn-wallet-core`: For persisting wallet state
- `icn-wallet-identity`: For storing identity information
- `icn-wallet-actions`: For tracking action history
- `icn-wallet-sync`: For caching synchronized data

## License

This crate is part of the ICN Wallet project and is licensed under the same terms as the parent project. 