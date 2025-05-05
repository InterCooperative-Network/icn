# ICN Federation Protocol

This document describes the Federation Protocol used by the ICN Runtime for peer discovery, TrustBundle synchronization, and distributed content management.

## Overview

The Federation Protocol is a critical component of the ICN Runtime that facilitates coordination between nodes in a federation. It enables:

1. **Network Formation**: Peer discovery and maintenance of the peer-to-peer network
2. **Trust Management**: Synchronization of TrustBundle data that defines the current set of trusted nodes
3. **Guardian Operations**: Distribution of guardian mandates and quorum decisions
4. **Content Distribution**: Replication and retrieval of content blobs according to federation policies

The protocol is implemented using libp2p, providing a modular, extensible framework for peer-to-peer networking.

## Network Architecture

The ICN Federation uses a decentralized mesh topology where each node maintains connections to multiple peers:

```
           ┌─────────┐
           │ Node A  │
           └────┬────┘
                │
    ┌───────────┼────────────┐
    │           │            │
┌───┴───┐   ┌───┴───┐    ┌───┴───┐
│ Node B│   │ Node C│    │ Node D│
└───┬───┘   └───┬───┘    └───┬───┘
    │           │            │
    └───────────┼────────────┘
                │
           ┌────┴────┐
           │ Node E  │
           └─────────┘
```

## Protocol Components

### 1. Federation Manager

The `FederationManager` is the central component responsible for coordinating all federation-related activities. It:

- Initializes the networking layer
- Manages peer connections
- Handles protocol message routing
- Coordinates TrustBundle synchronization
- Processes blob announcements and replication

### 2. Trust Bundles

A TrustBundle is a signed collection of nodes and their roles within the federation for a specific epoch. It serves as the source of truth for:

- Which nodes are part of the federation
- What roles each node fulfills (Validator, Guardian, Observer, etc.)
- The current federation epoch

#### TrustBundle Structure

```json
{
  "epochId": 42,
  "timestamp": 1671234567,
  "validFrom": 1671234567,
  "validUntil": 1671320967,
  "nodes": [
    {
      "id": "did:icn:node1",
      "role": "Validator",
      "publicKey": "base64-encoded-public-key",
      "endpoints": ["ip4/10.0.0.1/tcp/4001"]
    },
    {
      "id": "did:icn:node2",
      "role": "Guardian",
      "publicKey": "base64-encoded-public-key",
      "endpoints": ["ip4/10.0.0.2/tcp/4001"]
    }
  ],
  "proof": {
    "type": "QuorumSignature2023",
    "created": "2023-12-15T12:34:56Z",
    "verificationMethod": "did:icn:federation#quorum",
    "proofValue": "base64-encoded-signature",
    "proofQuorum": "67%",
    "signers": ["did:icn:signer1", "did:icn:signer2"]
  }
}
```

### 3. Network Protocols

The Federation Protocol implements several sub-protocols using libp2p:

| Protocol | Purpose |
|----------|---------|
| `/icn/discovery/1.0.0` | Node discovery and peer metadata exchange |
| `/icn/trust-bundle/1.0.0` | TrustBundle request and synchronization |
| `/icn/mandate/1.0.0` | Guardian mandate dissemination |
| `/icn/blob/1.0.0` | Content blob discovery and exchange |

## TrustBundle Synchronization Protocol

TrustBundle synchronization is a critical process that ensures all nodes maintain a consistent view of the federation membership.

### TrustBundle Request/Response Protocol

#### Request Format

```json
{
  "type": "TrustBundleRequest",
  "epochId": 42,
  "requester": "did:icn:requesting-node"
}
```

#### Response Format

```json
{
  "type": "TrustBundleResponse",
  "status": "Success",
  "epochId": 42,
  "trustBundle": {
    // Complete TrustBundle structure as shown above
  }
}
```

#### Error Response Format

```json
{
  "type": "TrustBundleResponse",
  "status": "Error",
  "errorCode": "EPOCH_NOT_FOUND",
  "errorMessage": "The requested epoch 42 is not available",
  "latestAvailableEpoch": 41
}
```

### Synchronization Process

1. **Automatic Epoch Discovery**:
   - Nodes periodically query peers for their latest known epoch
   - If a node discovers a higher epoch than it currently knows, it initiates a sync

2. **TrustBundle Request**:
   - Node A sends a TrustBundleRequest to Node B for a specific epoch
   - Node B responds with either the requested TrustBundle or an error

3. **Validation**:
   - Upon receiving a TrustBundle, the node verifies:
     - The quorum signature is valid
     - The bundle has not expired
     - The signers have appropriate authorization
     - The epoch ID is greater than or equal to the current one

4. **Storage and Propagation**:
   - Valid bundles are stored locally
   - Nodes update their latest known epoch metadata
   - Nodes advertise their latest epoch to peers

### Example Sync Flow

```
Node A                              Node B
  |                                   |
  |-- DiscoverLatestEpoch ----------->|
  |<- LatestEpochResponse (epoch=42) -|
  |                                   |
  |-- TrustBundleRequest(epoch=42) -->|
  |<- TrustBundleResponse ------------|
  |                                   |
  |-- [Validate TrustBundle] ---------|
  |                                   |
  |-- [Store TrustBundle] ------------|
  |                                   |
  |-- [Update Latest Epoch] ----------|
  |                                   |
```

## SyncClient Implementation for Wallet Integration

The `SyncClient` is a lightweight implementation of the Federation Protocol designed for wallets and other clients that need to retrieve TrustBundles but don't participate as full federation nodes.

### SyncClient Interface

```rust
/// Client for synchronizing with ICN federation nodes
pub struct SyncClient {
    // Private fields
}

impl SyncClient {
    /// Create a new sync client with the provided identity
    pub fn new(identity: Identity) -> Self;
    
    /// Add a federation node to connect to
    pub fn add_federation_node(&mut self, address: Multiaddr);
    
    /// Retrieve the latest known TrustBundle
    pub async fn get_latest_trust_bundle(&self) -> Result<TrustBundle, SyncError>;
    
    /// Retrieve a specific TrustBundle by epoch ID
    pub async fn get_trust_bundle(&self, epoch_id: u64) -> Result<TrustBundle, SyncError>;
    
    /// Listen for new TrustBundle announcements
    pub async fn subscribe_to_trust_bundles(&self) -> Result<TrustBundleSubscription, SyncError>;
}
```

### SyncClient Usage Example

```rust
// Initialize the sync client with wallet identity
let identity = wallet.get_identity();
let mut sync_client = SyncClient::new(identity);

// Add known federation nodes (bootstrap peers)
sync_client.add_federation_node("/ip4/10.0.0.1/tcp/4001".parse()?);
sync_client.add_federation_node("/ip4/10.0.0.2/tcp/4001".parse()?);

// Get the latest TrustBundle
let latest_bundle = sync_client.get_latest_trust_bundle().await?;
println!("Latest epoch: {}", latest_bundle.epoch_id);

// Subscribe to new TrustBundle announcements
let mut subscription = sync_client.subscribe_to_trust_bundles().await?;

tokio::spawn(async move {
    while let Some(bundle) = subscription.next().await {
        println!("New TrustBundle received for epoch {}", bundle.epoch_id);
        // Process the new TrustBundle (e.g., update local state)
    }
});
```

## Blob Storage and Replication

The Federation Protocol also handles content blob storage and replication:

### 1. Blob Announcement

When a node stores a new blob, it announces it to the network:

```json
{
  "type": "BlobAnnouncement",
  "cid": "bafyrei...",
  "size": 1024,
  "replicationPolicy": "Federation",
  "contextId": "did:icn:context-specific-id"
}
```

### 2. Blob Replication Request

```json
{
  "type": "BlobReplicationRequest",
  "cid": "bafyrei...",
  "requester": "did:icn:requesting-node"
}
```

### 3. Blob Replication Response

```json
{
  "type": "BlobReplicationResponse",
  "status": "Success",
  "cid": "bafyrei...",
  "data": "base64-encoded-blob-data"
}
```

## Security Considerations

The Federation Protocol implements several security mechanisms:

1. **Transport Security**: All communications use libp2p's noise protocol for encrypted transport
2. **Identity Verification**: Nodes verify each other's identities using public key cryptography
3. **TrustBundle Validation**: TrustBundles require quorum signatures from authorized guardians
4. **Epoch Versioning**: Each TrustBundle has a unique epoch ID that must increase monotonically
5. **Bounded Resources**: The protocol implements resource limiting to prevent DoS attacks

## Configuration Parameters

The Federation Protocol's behavior can be configured with the following parameters:

| Parameter | Description | Default Value |
|-----------|-------------|---------------|
| `bootstrap_period` | Period between bootstrap attempts | 30 seconds |
| `peer_sync_interval` | Interval between peer discovery | 60 seconds |
| `trust_bundle_sync_interval` | Interval between TrustBundle sync attempts | 300 seconds |
| `max_peers` | Maximum number of peer connections | 25 |
| `bootstrap_peers` | List of bootstrap peers to connect to | [] |
| `listen_addresses` | Addresses to listen on for connections | ["/ip4/0.0.0.0/tcp/0"] |
| `gossipsub_heartbeat_interval` | Interval for gossipsub heartbeats | 1 second |
| `gossipsub_validation_mode` | Validation mode for gossipsub messages | Strict |

## Implementation Notes

The current implementation uses:

- `libp2p` for peer-to-peer networking
- `libp2p-kad` for DHT and content discovery
- `libp2p-gossipsub` for efficient message propagation
- `libp2p-request-response` for request/response patterns

## Example: Starting a Federation Node

```rust
// Create federation manager configuration
let config = FederationManagerConfig {
    bootstrap_period: Duration::from_secs(30),
    peer_sync_interval: Duration::from_secs(60),
    trust_bundle_sync_interval: Duration::from_secs(300),
    max_peers: 25,
    bootstrap_peers: vec![
        "/ip4/10.0.0.1/tcp/4001".parse().unwrap(),
        "/ip4/10.0.0.2/tcp/4001".parse().unwrap(),
    ],
    listen_addresses: vec!["/ip4/0.0.0.0/tcp/4001".parse().unwrap()],
    ..Default::default()
};

// Initialize storage backend
let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));

// Initialize node identity
let identity = create_node_identity();

// Start the federation manager
let (federation_manager, blob_sender, fed_cmd_sender) = 
    FederationManager::start_node(config, storage.clone(), identity).await?;

// Request the latest TrustBundle
let latest_epoch = federation_manager.get_latest_known_epoch().await?;
let trust_bundle = federation_manager.request_trust_bundle(latest_epoch).await?;

println!("Connected to federation, latest epoch: {}", latest_epoch);
```

## See Also

- [Governance Kernel](./GOVERNANCE_KERNEL.md)
- [DAG System](./DAG_SYSTEM.md)
- [Events and Credentials](./EVENTS_CREDENTIALS.md) 