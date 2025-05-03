# wallet-types

Shared type definitions for the ICN Wallet and Runtime integration.

This crate provides common data structures, conversion utilities, and error types used by both the ICN Wallet and Runtime components.

## Overview

The `wallet-types` crate serves as the bridge between the wallet and runtime components of the InterCooperative Network (ICN). It ensures consistent data representation and reliable conversion between the two systems.

## Key Components

### DagNode

The `DagNode` structure represents a node in the directed acyclic graph (DAG) used for storing data in the ICN.

```rust
pub struct DagNode {
    pub cid: String,
    pub parents: Vec<String>,
    pub issuer: String,
    pub timestamp: SystemTime,
    pub signature: Vec<u8>,
    pub payload: Vec<u8>,
    pub metadata: DagNodeMetadata,
}
```

#### Binary Payload Handling

The `payload` field of `DagNode` is a binary vector (`Vec<u8>`) that can contain any arbitrary data:

- **JSON Data**: Most commonly, payload contains serialized JSON data, which can be accessed using the `payload_as_json()` method
- **Binary Data**: The payload can also contain raw binary data that is not valid JSON or UTF-8
- **Empty Data**: The payload can be empty (zero bytes)
- **Large Data**: The payload can handle large blobs of data

When working with binary payloads:

1. **Do not assume UTF-8 encoding**: Always handle the payload as raw bytes
2. **Try JSON parsing with fallback**: Use the pattern below when attempting to interpret the payload

```rust
// Example of safely handling a payload
match node.payload_as_json() {
    Ok(json_value) => {
        // Handle JSON data
        println!("JSON data: {}", json_value);
    },
    Err(_) => {
        // Handle as binary data
        println!("Binary data, {} bytes", node.payload.len());
    }
}
```

3. **Preserve binary data exactly**: When converting between wallet and runtime components, ensure binary payloads are preserved byte-for-byte

### NodeSubmissionResponse

The `NodeSubmissionResponse` structure represents the response received after submitting a node to the ICN network.

```rust
pub struct NodeSubmissionResponse {
    pub id: String,
    pub timestamp: SystemTime,
    pub block_number: Option<u64>,
    pub status: RequestStatus,
    pub error: Option<String>,
    pub metadata: HashMap<String, String>,
}
```

Example usage:

```rust
// Create a success response
let response = NodeSubmissionResponse::success(
    "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
    SystemTime::now()
);

// Add metadata
let response = response
    .with_block_number(12345)
    .with_metadata("key", "value");

// Check response status
if response.is_success() {
    println!("Node submission successful: {}", response.id);
}
```

### Error Handling

The `WalletError` enum provides a unified error type used across wallet components:

```rust
pub enum WalletError {
    IoError(io::Error),
    SerializationError(String),
    DagError(String),
    IdentityError(String),
    ValidationError(String),
    AuthenticationError(String),
    ResourceNotFound(String),
    ConnectionError(String),
    TimeoutError(String),
    StorageError(String),
    RuntimeError(String),
    GenericError(String),
}
```

The `FromRuntimeError` trait provides a convenient way to convert runtime errors to wallet errors:

```rust
// Example of error conversion
let result = runtime_function().convert_runtime_error()?;
```

## Runtime Compatibility

When the `runtime-compat` feature is enabled, additional functionality is provided for converting between wallet and runtime data structures:

```rust
// Converting from wallet to runtime
let runtime_node = wallet_types::dag::runtime_compat::to_runtime_dag_node(&wallet_node)?;

// Converting from runtime to wallet
let wallet_node = wallet_types::dag::runtime_compat::from_runtime_dag_node(&runtime_node, cid)?;
```

## Time Utilities

The crate provides utilities for converting between different time representations:

```rust
// Convert SystemTime to DateTime<Utc>
let datetime = time::system_time_to_date_time(system_time);

// Convert DateTime<Utc> to SystemTime
let system_time = time::date_time_to_system_time(datetime);
```

## Examples

### Working with JSON Payloads

```rust
use wallet_types::DagNode;
use serde_json::json;

// Create a DagNode with JSON payload
let json_value = json!({
    "title": "Test Node",
    "content": "This is a test node with JSON payload",
    "tags": ["test", "json", "example"]
});

let mut node = DagNode::new(
    "test-cid".to_string(),
    vec![],
    "did:icn:user123".to_string(),
    std::time::SystemTime::now(),
    vec![1, 2, 3, 4], // signature
    vec![], // empty payload for now
    None, // default metadata
);

// Set JSON payload
node.set_payload_from_json(&json_value).unwrap();

// Later, retrieve the JSON payload
let retrieved_json = node.payload_as_json().unwrap();
assert_eq!(retrieved_json["title"], "Test Node");
```

### Working with Binary Payloads

```rust
use wallet_types::DagNode;

// Create binary data (e.g., image bytes)
let binary_data = vec![0xFF, 0xD8, 0xFF, 0xE0, /* ...more image bytes... */];

let node = DagNode::new(
    "binary-cid".to_string(),
    vec![],
    "did:icn:user123".to_string(),
    std::time::SystemTime::now(),
    vec![1, 2, 3, 4], // signature
    binary_data.clone(), // binary payload
    None, // default metadata
);

// Binary data is preserved exactly
assert_eq!(node.payload, binary_data);

// Attempting to parse as JSON will fail
let json_result = node.payload_as_json();
assert!(json_result.is_err());
```

### Runtime Conversion with Binary Data

```rust
#[cfg(feature = "runtime-compat")]
fn convert_binary_node() {
    use wallet_types::dag::runtime_compat::{to_runtime_dag_node, from_runtime_dag_node};
    use cid::Cid;
    
    // Create a wallet node with binary payload
    let binary_data = vec![0x01, 0x02, 0x03, 0xFF];
    let wallet_node = DagNode::new(
        "test-cid".to_string(),
        vec![],
        "did:icn:user123".to_string(),
        std::time::SystemTime::now(),
        vec![1, 2, 3, 4], // signature
        binary_data.clone(), // binary payload
        None, // default metadata
    );
    
    // Convert to runtime node
    let runtime_node = to_runtime_dag_node(&wallet_node).unwrap();
    
    // In runtime, binary data is preserved as Ipld::Bytes
    
    // Convert back to wallet node
    let cid = Cid::try_from("test-cid").unwrap();
    let wallet_node2 = from_runtime_dag_node(&runtime_node, cid).unwrap();
    
    // Binary data is preserved in round-trip conversion
    assert_eq!(wallet_node.payload, wallet_node2.payload);
}
``` 