# Storage Crate Refactoring Plan

## Problem Statement

During the audit and refactoring of the storage crate, we've identified a circular dependency between the following crates:

1. `icn-storage` depends on `icn-dag` for DAG-related types like `DagNode` and `DagNodeBuilder`
2. `icn-dag` depends on `icn-storage` for storage-related functionality

This circular dependency prevents compilation and needs to be resolved.

## Refactoring Approach

### Step 1: Create a shared models crate

Create a new crate `icn-models` that will contain shared data structures used by both `icn-storage` and `icn-dag`:

```
runtime/crates/models/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── dag.rs       # Contains core DAG types
    └── storage.rs   # Contains storage interfaces
```

This crate will define the essential types without implementation details:
- `DagNode` and `DagNodeBuilder` interfaces
- Basic trait definitions for storage

### Step 2: Refactor `icn-dag` to depend on `icn-models`

- Remove dependency on `icn-storage`
- Add dependency on `icn-models`
- Implement the interfaces defined in `icn-models`
- Export the types from `icn-models` as needed

### Step 3: Refactor `icn-storage` to depend on `icn-models`

- Remove direct dependency on `icn-dag` 
- Add dependency on `icn-models`
- Implement storage traits defined in `icn-models`
- Use the DAG types from `icn-models` instead of directly from `icn-dag`

### Step 4: Create clean implementation boundaries

Make both crates independent of each other:

- `icn-dag` should focus on DAG structure and operations
- `icn-storage` should focus on persistence and retrieval 
- Business logic involving both DAG and storage should be in higher-level crates

## Testing and Validation

1. Create unit tests for each crate independently
2. Create integration tests that use both crates together
3. Ensure all existing functionality is preserved

## Implementation Plan

1. First create the `icn-models` crate with minimal shared interfaces
2. Update the storage crate to use these interfaces
3. Update the dag crate to use these interfaces
4. Gradually refine the interfaces as needed

This refactoring will result in a cleaner architecture with clear boundaries between components.

## Current Status

We have attempted to break the circular dependency by making the `icn-dag` dependency optional in the storage crate, but this is not sufficient as the `icn-dag` crate still depends on `icn-storage`. The proper solution is to extract shared interfaces to a separate crate. 