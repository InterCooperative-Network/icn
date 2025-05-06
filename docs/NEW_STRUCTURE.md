# ICN Project Structure

This document describes the new organizational structure of the Internet Cooperation Network (ICN) project after the cleanup and reorganization process.

## Overview

The ICN project has been reorganized into a standardized monorepo structure with all components consolidated under a single workspace. This improves build consistency, dependency management, and code organization.

## Directory Structure

```
icn/
├── crates/               # All Rust crates organized by component
│   ├── runtime/          # Runtime/CoVM components
│   ├── wallet/           # Wallet components
│   ├── agoranet/         # AgoraNet components
│   ├── mesh/             # Mesh compute components
│   └── common/           # Shared utilities and libraries
├── docs/                 # Documentation
├── scripts/              # Build and development scripts
└── Cargo.toml            # Workspace definition
```

## Component Organization

### Common Libraries

The `crates/common` directory contains shared libraries used across multiple components:

- `common-types`: Core type definitions, error handling, identity mechanisms, and networking primitives
- `icn-common`: Legacy common utilities (to be gradually merged into common-types)

### Runtime Components

The `crates/runtime` directory contains the core VM and governance components:

- `icn-core-vm`: WebAssembly execution environment
- `icn-governance-kernel`: Core governance logic implementation
- `icn-dag`: Directed acyclic graph implementation for state storage
- `icn-federation`: Federation management and coordination
- `icn-economics`: Economic policy implementation
- `icn-storage`: Storage subsystem
- `icn-identity`: Identity and authentication
- Other supporting modules

### Wallet Components

The `crates/wallet` directory contains wallet-related components:

- `icn-wallet-core`: Core wallet functionality
- `icn-wallet-api`: API for interfacing with the wallet
- `icn-wallet-storage`: Storage mechanisms for the wallet
- `icn-wallet-identity`: Identity management specific to the wallet
- `icn-wallet-sync`: Synchronization between wallet and network
- `icn-wallet-agent`: CLI agent for wallet interactions
- Other supporting modules

### AgoraNet Components

The `crates/agoranet` directory contains network and deliberation components:

- `agoranet-core`: Core networking and deliberation engine
- `agoranet-api`: API interfaces for AgoraNet

### Mesh Components

The `crates/mesh` directory contains mesh compute components:

- `mesh-types`: Core types for the mesh compute system
- `mesh-net`: Networking layer for mesh compute
- `mesh-reputation`: Reputation system for mesh nodes
- `mesh-escrow`: Payment escrow for compute tasks
- `meshctl`: Command-line tool for mesh management

## Integration Points

The major components interact through well-defined integration points:

1. **Wallet ↔ Runtime**: Wallet components use common types and the wallet sync module to interact with the runtime.

2. **Runtime ↔ AgoraNet**: The icn-agoranet-integration module handles the integration between the runtime and AgoraNet.

3. **Wallet ↔ Mesh**: The wallet-agent provides mesh-specific commands to interact with the mesh compute network.

4. **Mesh ↔ Runtime**: Mesh components interact with the runtime through standardized interfaces.

## Build System

A unified build system has been implemented with shared scripts:

- `scripts/build.sh`: Main build script that can target specific components or the entire system

## Future Improvements

This structure allows for:

1. Gradual deduplication of code between components
2. Clearer responsibility boundaries
3. Easier onboarding for new developers
4. Consistent dependency management through workspace-level specifications
5. Simpler CI/CD pipeline configuration 