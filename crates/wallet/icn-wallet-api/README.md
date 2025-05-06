# ICN Wallet API

The `icn-wallet-api` crate provides a comprehensive HTTP API for interacting with the ICN wallet. It enables applications to interact with the wallet's core functionality through a well-defined RESTful interface.

## Features

- **RESTful API**: Provides a clean HTTP interface following REST principles
- **Authentication**: Secure API with proper authentication and authorization
- **Full Wallet Access**: Expose wallet operations including credential management, proposal handling, and DAG operations
- **OpenAPI Documentation**: Auto-generated API documentation with Swagger/OpenAPI
- **Error Handling**: Consistent error responses and status codes

## API Endpoints

The API includes endpoints for:

- **Identity Management**: Create, retrieve, and manage DIDs and other identity objects
- **Credential Operations**: Issue, verify, and manage verifiable credentials
- **Proposal Handling**: Create and vote on governance proposals
- **DAG Operations**: Submit and retrieve DAG nodes
- **Federation Integration**: Interact with federation resources and services
- **Settings Management**: Configure wallet behavior and preferences

## Usage

### Adding as a Dependency

```toml
[dependencies]
icn-wallet-api = { version = "0.1.0", path = "../icn-wallet-api" }
```

### Starting the API Server

```rust
use icn_wallet_api::{WalletApiConfig, WalletApiServer};
use icn_wallet_core::WalletCore;
use std::net::SocketAddr;

async fn start_api_server(wallet: WalletCore) -> Result<(), Box<dyn std::error::Error>> {
    // Configure the API
    let config = WalletApiConfig {
        bind_address: SocketAddr::from(([127, 0, 0, 1], 3000)),
        enable_cors: true,
        enable_docs: true,
        // Other configuration options...
    };
    
    // Create and start the server
    let server = WalletApiServer::new(wallet, config);
    server.start().await?;
    
    Ok(())
}
```

### Client Usage Example

```rust
// Using a HTTP client to interact with the wallet API
async fn api_client_example() -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    
    // Get wallet identity information
    let identity = client.get("http://localhost:3000/api/v1/identity")
        .header("Authorization", "Bearer your-token-here")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    
    println!("Wallet identity: {}", identity);
    
    Ok(())
}
```

## Integration with ICN Wallet

This crate integrates with other components of the ICN wallet ecosystem:

- `icn-wallet-core`: Core wallet logic that the API exposes
- `icn-wallet-identity`: Identity management functionality
- `icn-wallet-storage`: Data persistence for API operations

## License

This crate is part of the ICN Wallet project and is licensed under the same terms as the parent project. 