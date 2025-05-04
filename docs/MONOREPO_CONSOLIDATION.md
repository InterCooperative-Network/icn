# Monorepo Consolidation Plan

This document outlines the tasks required to complete the consolidation of the ICN monorepo, eliminating duplicate crates and resolving circular dependencies.

## Current Issues

1. **Duplicate Crates**: Several crates exist in multiple locations with different paths but similar or identical functionality
2. **Circular Dependencies**: Component boundaries have circular dependencies, particularly in the wallet ↔ runtime interface
3. **Inconsistent Naming**: Some crates use the `icn-` prefix while others don't, leading to confusion
4. **Excluded Directories**: Several directories are currently excluded from the workspace in Cargo.toml

## Consolidation Tasks

### 1. Wallet-FFI Consolidation

- [x] Identify differences between `runtime/crates/wallet-ffi` and `wallet/crates/wallet-ffi`
- [ ] Merge functionality into unified `wallet/crates/wallet-ffi` implementation
- [ ] Update dependent crates to use the consolidated implementation
- [ ] Remove the duplicate crate from `runtime/crates/wallet-ffi`
- [ ] Update Cargo.toml to remove exclusion

### 2. Wallet Core Consolidation

- [x] Identify differences between `runtime/crates/wallet-core` and `wallet/crates/wallet-core`
- [ ] Migrate any runtime-specific functionality to appropriate crates
- [ ] Update dependent crates to use the consolidated implementation
- [ ] Remove the duplicate crate from `runtime/crates/wallet-core`
- [ ] Update Cargo.toml to remove exclusion

### 3. Wallet Agent Consolidation

- [x] Identify differences between `runtime/crates/wallet-agent` and `wallet/crates/wallet-agent`
- [ ] Merge functionality into unified `wallet/crates/wallet-agent` implementation
- [ ] Update dependent crates to use the consolidated implementation
- [ ] Remove the duplicate crate from `runtime/crates/wallet-agent`
- [ ] Update Cargo.toml to remove exclusion

### 4. Wallet Sync Consolidation

- [x] Identify differences between `runtime/crates/wallet-sync` and `wallet/crates/sync`
- [ ] Normalize the naming convention (decide on `wallet-sync` or `sync`)
- [ ] Merge functionality into the chosen implementation
- [ ] Update dependent crates to use the consolidated implementation
- [ ] Remove the duplicate crate
- [ ] Update Cargo.toml to remove exclusion

### 5. AgoraNet Integration Cleanup

- [ ] Assess current state of `runtime/crates/agoranet-integration`
- [ ] Determine if it should be migrated to `agoranet/crates/`
- [ ] Implement proper abstraction layer for runtime ↔ agoranet communication
- [ ] Remove circular dependencies between components
- [ ] Update Cargo.toml to remove exclusion

### 6. Frontend Directory Organization

- [ ] Identify active frontend components in `frontend/`
- [ ] Determine whether to include in workspace or maintain as separate project
- [ ] Update Cargo.toml workspace configuration accordingly

## Naming Standardization

- [ ] Adopt consistent naming scheme for all crates (prefix with `icn-` or not)
- [ ] Update references in all import statements to match new convention
- [ ] Update Cargo.toml workspace members list
- [ ] Update documentation to reflect standardized naming

## Workspace Cleanup

- [ ] Update root Cargo.toml to include all active crates
- [ ] Remove all exclusion entries that reference resolved duplicates
- [ ] Ensure proper version management for all dependencies
- [ ] Add comments explaining any remaining necessary exclusions

## Testing Strategy

1. After each consolidation step:
   - Run all tests to verify functionality is preserved
   - Check for broken imports or compilation errors
   - Verify runtime operation in dev environment

2. After full consolidation:
   - Run integration tests across all components
   - Verify cross-component communication
   - Test federation bootstrapping end-to-end

## Timeline

| Task | Estimated Time | Dependencies | Assignee |
|------|----------------|--------------|----------|
| Wallet-FFI Consolidation | 2 days | None | TBD |
| Wallet Core Consolidation | 3 days | Wallet-FFI | TBD |
| Wallet Agent Consolidation | 2 days | Wallet Core | TBD |
| Wallet Sync Consolidation | 2 days | Wallet Agent | TBD |
| AgoraNet Integration | 3 days | All wallet consolidation | TBD |
| Frontend Organization | 1 day | None | TBD |
| Naming Standardization | 1 day | All consolidation | TBD |
| Workspace Cleanup | 1 day | All above tasks | TBD |

## Release Strategy

Once consolidation is complete:

1. Tag the repository with a minor version bump (e.g., `v0.9.0-consolidated`)
2. Update the ARCHITECTURE.md to reflect the consolidated structure
3. Remove this tracking document or transition it to a historical note 