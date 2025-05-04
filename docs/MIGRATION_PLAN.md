# ICN Monorepo Migration Plan

Based on our analysis, we need to restructure the ICN monorepo to improve clarity, modularity, and build hygiene. This document outlines a step-by-step plan to migrate from the current structure to the target structure.

## Current Structure Issues

1. Duplicated modules (wallet-ffi in both runtime/ and wallet/)
2. Overlapping folders (multiple health_check roots)
3. Inconsistent separation of concerns between runtime/, wallet/, agoranet/, and dashboard/
4. Scattered scripts and documentation files

## Target Structure

```
icn/
├── agoranet/            # Deliberation layer with clean APIs
│   ├── crates/          # API implementation crates
│   ├── migrations/      # Database migration scripts
│   └── src/             # Main agoranet source code
├── docs/                # Centralized documentation
│   ├── runtime/         # Runtime-specific documentation
│   ├── wallet/          # Wallet-specific documentation
│   └── agoranet/        # Agoranet-specific documentation
├── frontend/            # Frontend applications
│   ├── dashboard/       # Main ICN dashboard (React/TS)
│   └── agoranet-ui/     # Agoranet-specific dashboard
├── runtime/             # Core Rust logic
│   ├── crates/          # Core runtime crates
│   ├── examples/        # Example code and applications 
│   └── tests/           # Integration and unit tests
├── scripts/             # Utility scripts
├── tools/               # Standalone tools and utilities
│   ├── health_check/    # Health check service (consolidated)
│   └── icn-verifier/    # Bundle verification tool
└── wallet/              # Identity and sync agent
    ├── crates/          # Wallet component crates
    └── examples/        # Example code for wallet usage
```

## Migration Steps

### 1. Create Directory Structure

```bash
# Create main directory structure
mkdir -p runtime/crates
mkdir -p wallet/crates
mkdir -p agoranet/crates
mkdir -p frontend
mkdir -p docs/{runtime,wallet,agoranet}
mkdir -p scripts
mkdir -p tools/{health_check,icn-verifier}
```

### 2. Consolidate Wallet Components

1. Move all wallet-related crates under wallet/crates/:
   - Move wallet-ffi to wallet/crates/wallet-ffi
   - Move wallet-core to wallet/crates/wallet-core
   - Move wallet-agent to wallet/crates/wallet-agent

2. Update Cargo.toml files to reference the new locations

### 3. Consolidate Health Check

1. Move the root health_check.rs into tools/health_check/src/main.rs
2. Move agoranet/health_check functionality to tools/health_check if unique
3. Create a unified Cargo.toml for tools/health_check

### 4. Reorganize Dashboard

1. Move dashboard/ to frontend/dashboard/
2. Move agoranet/dashboard/ to frontend/agoranet-dashboard/

### 5. Move Verification Tool

1. Move icn-verifier/ to tools/icn-verifier/
2. Update its Cargo.toml to reference the new dependency locations

### 6. Centralize Documentation

1. Move runtime/*.md to docs/runtime/
2. Move wallet/*.md to docs/wallet/
3. Move agoranet-redesign/ to docs/agoranet/
4. Create docs/REPO_STRUCTURE.md documenting the layout

### 7. Gather Scripts

1. Move *.sh files to the scripts/ directory

### 8. Update Workspace Configuration

1. Update the top-level Cargo.toml to reference the new structure

### 9. Verify

1. Run `cargo check` to verify the build integrity
2. Run `cargo test` to ensure functionality is preserved

## Notes on Dependency Management

- Use workspace dependencies where possible
- Ensure path references are correct after moving crates

## Future Improvements

After the migration, consider:

1. Further consolidation of similar functionality
2. Standardization of crate naming conventions
3. Improved documentation of module relationships
4. Development of comprehensive integration tests across the restructured components 