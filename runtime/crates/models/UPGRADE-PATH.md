# Models Crate Upgrade Path

This document outlines the steps to complete the migration of shared types between `icn-dag` and `icn-storage` into this central `icn-models` crate.

## Current Status

The `icn-models` crate has been created with:

1. Core DAG types (`DagNode`, `DagNodeMetadata`, `DagNodeBuilder`, `DagType`, `DagCodec`)
2. Storage interfaces (`StorageBackend`, `BasicStorageManager`, `DagStorageManager`)
3. Common error types (`StorageError`, `ModelError`) and result types

However, there are still some workspace configuration issues that prevent a clean build in the current repository structure.

## Next Steps

### 1. Update Common Dependencies

- Ensure all common dependencies use direct version specifications, not workspace references
- Add any necessary [features] section to Cargo.toml

### 2. Update icn-dag Crate

- Remove dependency on `icn-storage`
- Implement the interfaces defined in `icn-models` for DAG-related types
- Export compatible types through re-export from `icn-models`
- Update any code that references `DagNode` or related types to use them from `icn-models`

### 3. Update icn-storage Crate

- Remove direct dependency on `icn-dag`
- Implement storage traits defined in `icn-models`
- Use `DagNode` and related types from `icn-models` 
- Provide compatibility layer for existing code that uses the old interfaces

### 4. Update Downstream Crates

- Update `core-vm` and other crates to use the types from `icn-models` where appropriate
- Ensure that the circular dependency is fully resolved throughout the codebase

## Implementation Details

### DAG Node Factory Function

```rust
// In icn-dag/src/lib.rs
use icn_models::{DagNode, DagNodeBuilder, DagNodeMetadata};

pub struct ConcreteDagNodeBuilder {
    // implementation details
}

impl DagNodeBuilder for ConcreteDagNodeBuilder {
    // implementation of the DagNodeBuilder trait
}

// Factory function to create a new builder
pub fn new_dag_node_builder() -> impl DagNodeBuilder {
    ConcreteDagNodeBuilder::new()
}
```

### Storage Manager Implementation

```rust
// In icn-storage/src/lib.rs
use icn_models::{DagStorageManager, BasicStorageManager, DagNode, DagNodeBuilder};

pub struct InMemoryStorageManager {
    // implementation details
}

#[async_trait]
impl BasicStorageManager for InMemoryStorageManager {
    // implementation of BasicStorageManager
}

#[async_trait]
impl DagStorageManager for InMemoryStorageManager {
    // implementation of DagStorageManager
}
```

## Testing

Once the migration is complete, the following tests should pass:

1. Building each crate independently with `cargo build`
2. Running tests for each crate with `cargo test`
3. Ensuring that the root project builds with `cargo build` in the root directory

## Benefits

This refactoring will:

1. Break the circular dependency between `icn-dag` and `icn-storage`
2. Create clearer boundaries between components
3. Make it easier to reason about the codebase
4. Improve build times by reducing unnecessary rebuilds
5. Make it possible to use each component independently 