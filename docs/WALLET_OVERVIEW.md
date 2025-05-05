# ICN Wallet System Overview

This document provides a high-level overview of the ICN Wallet system, explaining its architecture, components, and key features like DAG thread caching, receipt sharing, and encrypted bundles.

## Architecture Overview

The ICN Wallet serves as the user's interface to the ICN ecosystem, managing identities, credentials, and interactions with federations. Its architecture follows a modular design:

```
┌─────────────────────────────────────────────────────────┐
│                                                         │
│                ICN Wallet Application                   │
│                                                         │
├─────────┬─────────┬─────────┬─────────┬─────────┬───────┤
│         │         │         │         │         │       │
│ Identity│ Storage │  Sync   │  API    │ Actions │ Agent │
│  Module │ Module  │ Module  │ Module  │ Module  │Module │
│         │         │         │         │         │       │
└─────────┴─────────┴─────────┴─────────┴─────────┴───────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
┌─────────────▼─┐  ┌───────▼──────┐  ┌──▼────────────┐
│               │  │              │  │               │
│  Federation   │  │  AgoraNet    │  │  ICN Runtime  │
│  Resources    │  │  Services    │  │  System       │
│               │  │              │  │               │
└───────────────┘  └──────────────┘  └───────────────┘
```

## Key Components

### 1. Identity Module (`icn-wallet-identity`)

The Identity module manages all aspects of the user's decentralized identities (DIDs):

- Creating and managing DIDs
- Securely storing private keys
- Signing and verifying messages
- Managing verifiable credentials
- Selective disclosure of identity data

### 2. Storage Module (`icn-wallet-storage`)

The Storage module provides a robust, secure data persistence layer:

- Multiple storage types (key-value, document, binary, DAG)
- Encrypted storage for sensitive data
- Versioned storage for data integrity
- Secure indexing for efficient lookup
- Lifecycle-aware storage management

### 3. Sync Module (`icn-wallet-sync`)

The Sync module handles synchronization with the ICN network:

- DAG thread caching for offline access
- Peer-to-peer data sharing
- Conflict resolution
- Differential synchronization
- Receipt sharing protocols

### 4. API Module (`icn-wallet-api`)

The API module exposes wallet functionality to other applications:

- RESTful API interface
- Authentication and authorization
- OpenAPI documentation
- Method invocation
- Event streaming

### 5. Actions Module (`icn-wallet-actions`)

The Actions module defines and executes operations within the wallet:

- Creating and tracking wallet operations
- Standardized action types
- Status updates
- Result storage
- Action history

### 6. Agent Module (`icn-wallet-agent`)

The Agent module enables automated background operations:

- Scheduled tasks
- Event-driven processing
- Federation monitoring
- Credential renewals
- Proposal tracking

## Key Features

### DAG Thread Caching

The wallet maintains local cached copies of DAG threads for offline access and improved performance:

1. **Selective Caching**: Only caches threads relevant to the user
2. **Merkle Proofs**: Uses Merkle proofs to validate cached data
3. **Differential Updates**: Only downloads changed parts of threads
4. **Priority-Based Sync**: Prioritizes important threads for sync
5. **Conflict Resolution**: Handles concurrent updates to threads

```rust
// Example: Caching a thread
let thread_id = "thread_123";
let dag_cache = dag_cache_manager.get_or_create_cache(thread_id);

// Fetch latest updates
let updates = sync_service.get_thread_updates(thread_id, dag_cache.latest_version()).await?;

// Apply updates to cache
let new_version = dag_cache.apply_updates(updates).await?;

// Access cached data (works offline)
let thread_data = dag_cache.get_thread_data().await?;
```

### Receipt Sharing

The wallet implements a protocol for securely sharing execution receipts between participants:

1. **P2P Receipt Exchange**: Direct sharing between authorized parties
2. **Federation Broadcast**: Distribution of receipts to federation members
3. **Selective Disclosure**: Sharing only necessary receipt data
4. **Verifiable Forwarding**: Receipts can be forwarded with proofs of authority
5. **Receipt Aggregation**: Combining related receipts for efficiency

```rust
// Example: Sharing a receipt
let receipt_id = "receipt_456";
let receipt = receipt_manager.get_receipt(receipt_id).await?;

// Share with specific participants
let participants = vec!["did:icn:user1", "did:icn:user2"];
receipt_sharing.share_receipt(receipt, participants).await?;

// Or broadcast to federation
let federation_id = "did:icn:federation";
receipt_sharing.broadcast_receipt(receipt, federation_id).await?;
```

### Encrypted Bundles

The wallet uses encrypted bundles to protect sensitive data while allowing controlled sharing:

1. **Content Encryption**: AES-GCM encryption of bundle contents
2. **Key Sharing**: Sharing decryption keys via DH key exchange
3. **Access Control**: Fine-grained control over who can access bundles
4. **Revocable Access**: Ability to revoke access to shared bundles
5. **Versioned Bundles**: Support for updating bundle contents while maintaining history

```rust
// Example: Creating and sharing an encrypted bundle
let sensitive_data = json!({
    "private_info": "confidential content",
    "metadata": {
        "created": "2024-07-15T10:30:00Z"
    }
});

// Create encrypted bundle
let bundle_id = encrypted_bundle_manager
    .create_bundle("Confidential Report", sensitive_data)
    .await?;

// Share with a recipient
encrypted_bundle_manager
    .share_bundle(bundle_id, "did:icn:recipient")
    .await?;

// Recipient can decrypt and access
let bundle = encrypted_bundle_manager.get_bundle(bundle_id).await?;
let decrypted_content = bundle.decrypt().await?;
```

## Wallet Operations

### Identity Creation and Management

```rust
// Create a new identity
let identity_manager = IdentityManager::new(storage.clone());
let did = identity_manager.create_identity().await?;

// Sign data with identity
let message = "Hello, world!";
let signature = identity_manager.sign(&did, message.as_bytes()).await?;

// Verify signature
let valid = identity_manager.verify(&did, message.as_bytes(), &signature).await?;
```

### Credential Management

```rust
// Issue a credential
let credential = identity_manager.issue_credential(
    &issuer_did,
    recipient_did,
    "MembershipCredential",
    json!({
        "name": "John Doe",
        "membershipLevel": "Full",
        "joinDate": "2024-01-15"
    }),
    None,
).await?;

// Verify a credential
let verification = identity_manager.verify_credential(&credential).await?;
```

### Proposal Creation and Tracking

```rust
// Create a proposal
let proposal = wallet.create_proposal(
    federation_id,
    "Add new member",
    json!({
        "description": "Add Jane Doe to the federation",
        "action": "add_member",
        "member_did": "did:icn:jane"
    }),
).await?;

// Vote on a proposal
wallet.vote_on_proposal(
    proposal.id,
    VoteChoice::Approve,
    Some("I support this addition"),
).await?;
```

### Federation Participation

```rust
// Join a federation
let invitation = wallet.get_federation_invitation(invitation_id).await?;
wallet.join_federation(invitation).await?;

// List federations
let federations = wallet.list_federations().await?;

// Get federation updates
let updates = wallet.get_federation_updates(federation_id).await?;
```

## Security Considerations

The ICN Wallet implements several security measures:

1. **Encrypted Storage**: All sensitive data is encrypted at rest
2. **Key Isolation**: Private keys are stored in secure enclaves when available
3. **Capability-Based Security**: Access to wallet functions is controlled by capabilities
4. **Audit Logging**: All significant operations are logged for accountability
5. **Defense in Depth**: Multiple layers of security protect critical functionality

## Integration Points

The wallet integrates with other parts of the ICN ecosystem:

1. **AgoraNet**: For participating in federation discussions
2. **ICN Runtime**: For executing governance operations
3. **Federation Registry**: For managing federation membership
4. **External Applications**: Via the FFI and API interfaces

## Further Resources

- [Wallet API Documentation](../icn-wallet-root/README.md)
- [Security Considerations](./wallet_security.md)
- [Integration Guide](./wallet_integration.md) 