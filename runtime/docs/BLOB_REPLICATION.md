# Blob Replication via Kademlia Peer Discovery

## Overview

The ICN Runtime enables content-addressed storage with replication capabilities to ensure data availability across the network. When a blob is pinned by a node, it initiates the replication process based on configurable policies.

## Implementation Details

### Components

1. **InMemoryBlobStore** (`crates/storage/src/lib.rs`)
   - Added a `fed_cmd_sender` field to hold a channel for federation commands
   - When a blob is pinned via `pin_blob()`, it sends a replication request command if newly pinned
   - New constructor methods added for federation integration: `with_federation()` and `with_max_size_and_federation()`

2. **ReplicationPolicy Enum** (`crates/storage/src/lib.rs`)
   - `Factor(u32)`: Replicate to N peers
   - `Peers(Vec<String>)`: Replicate to specific peers
   - `None`: No replication required

3. **FederationCommand Enum** (`crates/storage/src/lib.rs`)
   - `AnnounceBlob(Cid)`: Announce a CID via Kademlia
   - `IdentifyReplicationTargets { cid, policy, context_id }`: Identify replication targets for a blob

4. **Replication Module** (`crates/federation/src/replication.rs`)
   - `identify_target_peers`: Filters and selects suitable replication targets based on policy
   - `replicate_to_peers`: Logs replication intent for future implementation of the replication protocol

5. **Federation Event Loop** (`crates/federation/src/lib.rs`)
   - Handles Kademlia queries for closest peers to identify replication candidates
   - Processes responses from Kademlia to select suitable replication targets
   - Currently logs replication intent; actual data transfer will be implemented later

6. **Governance Integration** (`crates/federation/src/roles.rs`)
   - Added `get_replication_policy` to look up policies from governance configurations
   - Similar pattern to how guardian roles are looked up

### Process Flow

1. A blob is pinned on a node via `InMemoryBlobStore.pin_blob()`
2. If newly pinned, a `FederationCommand::IdentifyReplicationTargets` command is sent
3. The Federation event loop receives the command and:
   - If a context ID is provided, looks up the replication policy from governance
   - Otherwise, uses the provided default policy
4. Based on the policy, a Kademlia query is initiated to find the closest peers
5. When the query completes, the `replication::identify_target_peers` function selects suitable targets
6. The `replication::replicate_to_peers` function is called to initiate replication
   - Currently this just logs the intent; actual data transfer will be implemented later

## Usage

### Pinning and Auto-Replication

```rust
// Create a Federation Manager with federation channels
let (federation_manager, blob_sender, fed_cmd_sender) = FederationManager::start_node(
    config,
    storage_backend
).await?;

// Create a blob store with the federation command sender
let blob_store = InMemoryBlobStore::with_federation(blob_sender, fed_cmd_sender);

// Store a blob
let content = b"Example blob content".to_vec();
let cid = blob_store.put_blob(&content).await?;

// Pin the blob - this will trigger replication
blob_store.pin_blob(&cid).await?;

// Replication process happens automatically in the background
```

### Manual Replication

```rust
// For explicit control over replication
let policy = ReplicationPolicy::Factor(3); // Replicate to 3 peers
let target_peers = federation_manager
    .identify_replication_targets(cid, policy, Some("my-federation".to_string()))
    .await?;

// Access the selected target peers if needed
for peer in target_peers {
    println!("Selected peer for replication: {}", peer);
}
```

## Future Improvements

1. Implement the actual peer-to-peer blob transfer protocol
2. Add replication status tracking
3. Add prioritization and queuing for replication tasks
4. Implement intelligent peer selection based on:
   - Geographical distribution
   - Network latency/bandwidth
   - Peer reputation
   - Resource availability
5. Add verification of successful replication
6. Add support for repair/re-replication when peers disappear 