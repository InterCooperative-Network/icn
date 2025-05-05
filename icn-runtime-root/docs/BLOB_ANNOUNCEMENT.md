# Blob Announcement via Kademlia Provider Records

## Overview

The ICN Runtime uses content-addressed storage for blobs, where each blob is identified by a CID (Content IDentifier) derived from the content's hash. To make these blobs discoverable by other nodes in the network, we've implemented a Kademlia provider record announcement mechanism.

When a blob is stored locally, the node announces itself as a provider for that CID to the network. This allows other nodes to discover and retrieve the content when needed.

## Implementation Details

### Components

1. **InMemoryBlobStore** (`crates/storage/src/lib.rs`)
   - Added a `kad_announcer` field to hold an optional channel for sending CIDs to be announced
   - When a blob is stored via `put_blob()`, the CID is sent through this channel if available
   - New constructor methods added: `with_announcer()` and `with_max_size_and_announcer()`

2. **FederationManager** (`crates/federation/src/lib.rs`)
   - Creates a channel pair for blob announcements during initialization
   - Returns the sender half to be used by the storage implementation
   - Added a new message type `AnnounceBlob` to the `FederationManagerMessage` enum
   - Added a direct-call method `announce_blob()` to announce a CID programmatically

3. **Event Loop Processing** (`crates/federation/src/lib.rs`)
   - Updated the event loop to accept the blob announcement channel receiver
   - Added a branch in the `tokio::select!` macro to process incoming CID announcements
   - Implemented the `announce_as_provider()` helper function to handle the Kademlia announcement

### Communication Flow

1. The Distributed Storage layer stores a blob locally
2. The Storage layer sends the CID through the `kad_announcer` channel
3. The Federation Manager's event loop receives the CID from the channel
4. The event loop calls `swarm.behaviour_mut().kademlia.start_providing(cid_bytes.into())`
5. The Kademlia DHT publishes the provider record to the network

## Usage

### Initialization

```rust
// Create a Federation Manager with a storage backend
let (federation_manager, blob_announcer) = FederationManager::start_node(
    config,
    storage_backend
).await?;

// Create a blob store with the announcer channel
let blob_store = InMemoryBlobStore::with_announcer(blob_announcer);

// Use the blob store for storing content
// The blob_store will automatically announce stored blobs to the network
```

### Storing and Announcing Blobs

```rust
// Store a blob - this will automatically trigger an announcement
let content = b"Example blob content".to_vec();
let cid = blob_store.put_blob(&content).await?;

// Announcement happens automatically in the background
// Other nodes can now discover that this node has the content for this CID
```

### Programmatic Announcement

```rust
// For existing blobs or manual control
let cid = /* Some existing CID */;
federation_manager.announce_blob(cid).await?;
```

## Future Improvements

1. Add diagnostic/metrics tracking for announcements
2. Implement re-announcement strategy for high-value blobs
3. Add provider record TTL handling and refreshes
4. Extend federation message handlers to process "find provider" requests 