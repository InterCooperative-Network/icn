# ICN Wallet Identity

The `icn-wallet-identity` crate provides comprehensive identity management capabilities for the ICN Wallet ecosystem. It enables the creation, management, and verification of decentralized identifiers (DIDs) and verifiable credentials.

## Features

- **DID Management**: Create and manage decentralized identifiers using various methods
- **Key Management**: Generate and securely store cryptographic keys
- **Credential Operations**: Issue, verify, and manage verifiable credentials
- **Selective Disclosure**: Support for selective disclosure and zero-knowledge proofs
- **Signature Capabilities**: Sign and verify messages and data structures
- **Recovery**: Identity recovery and backup mechanisms
- **Standards Compliance**: Follows W3C DID and Verifiable Credentials standards

## Identity Types Supported

- **did:icn**: ICN's native DID method for wallet identities
- **did:key**: Simple method for key-based DIDs
- **did:web**: Web-based DIDs for integration with existing systems
- **Extensible**: Framework for adding new DID methods

## Usage

### Adding as a Dependency

```toml
[dependencies]
icn-wallet-identity = { version = "0.1.0", path = "../icn-wallet-identity" }
```

### Basic Example

```rust
use icn_wallet_identity::{IdentityManager, IdentityOptions};
use icn_wallet_storage::StorageManager;

async fn identity_example() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize storage
    let storage = StorageManager::new("wallet_data").await?;
    
    // Create identity manager
    let identity_manager = IdentityManager::new(storage);
    
    // Create a new DID
    let options = IdentityOptions {
        method: "icn".to_string(),
        key_type: "ed25519".to_string(),
        ..Default::default()
    };
    
    let did = identity_manager.create_identity_with_options(options).await?;
    println!("Created DID: {}", did);
    
    // List all identities
    let identities = identity_manager.list_identities().await?;
    println!("Found {} identities", identities.len());
    
    // Sign data with identity
    let data = "Hello, world!";
    let signature = identity_manager.sign(did.as_str(), data.as_bytes()).await?;
    
    // Verify signature
    let is_valid = identity_manager.verify(
        did.as_str(),
        data.as_bytes(),
        &signature
    ).await?;
    
    assert!(is_valid, "Signature verification failed");
    
    Ok(())
}
```

### Verifiable Credential Example

```rust
use icn_wallet_identity::{IdentityManager, CredentialOptions};
use serde_json::json;

async fn credential_example(identity_manager: &IdentityManager) -> Result<(), Box<dyn std::error::Error>> {
    // Get the issuer DID
    let issuer_did = identity_manager.get_default_identity().await?;
    
    // Create credential subject
    let subject = json!({
        "id": "did:icn:recipient",
        "name": "John Doe",
        "degree": {
            "type": "BachelorDegree",
            "name": "Bachelor of Science"
        }
    });
    
    // Issue a credential
    let credential = identity_manager.issue_credential(
        &issuer_did,
        "did:icn:recipient",
        "UniversityDegree",
        subject,
        None,
    ).await?;
    
    println!("Issued credential with ID: {}", credential.id);
    
    // Verify the credential
    let verification = identity_manager.verify_credential(&credential).await?;
    
    if verification.is_valid {
        println!("Credential verified successfully");
    } else {
        println!("Credential verification failed: {}", verification.reason.unwrap_or_default());
    }
    
    Ok(())
}
```

## Integration with ICN Wallet

This crate is a core component of the ICN wallet ecosystem and integrates with:

- `icn-wallet-storage`: For persisting identity information
- `icn-wallet-core`: For providing identity services to the wallet
- `icn-wallet-sync`: For synchronizing identity data across devices

## License

This crate is part of the ICN Wallet project and is licensed under the same terms as the parent project. 