# ICN: Intercooperative Network Architecture

## System Overview

The Intercooperative Network (ICN) is a decentralized governance and economic coordination system designed to facilitate federated decision-making and resource allocation. Built as a Rust monorepo, the ICN system consists of four primary components:

1. **Runtime (CoVM v3)**: A WebAssembly execution environment that processes governance operations, enforces economic policies, and maintains federated state.

2. **Wallet**: A secure, mobile-first client agent that manages user identity, credentials, and local state.

3. **AgoraNet**: A REST API server and deliberation engine that facilitates inter-federation communication and user interaction.

4. **Mesh**: A distributed computation overlay that enables federations to share computational resources with verifiable execution guarantees.

The system uses a Directed Acyclic Graph (DAG) as its foundational data structure, enabling non-blocking concurrent operations while maintaining causal relationships. Identity and trust are managed through DIDs (Decentralized Identifiers) and VCs (Verifiable Credentials), with a federation-based trust model.

## Monorepo Structure

```
icn/
├── agoranet/            # Deliberation layer and federation communication
├── docs/                # Documentation for all components
├── icn-runtime-root/    # Runtime system (execution, DAG, governance)
├── icn-wallet-root/     # Wallet components (identity, storage, sync)
├── mesh/                # Mesh computation system
├── scripts/             # Utility scripts
├── tools/               # Standalone tools
├── Cargo.toml           # Workspace configuration
└── README.md            # Project overview
```

## Component Details

### Runtime System

The runtime provides the core federation logic for governance, execution, and state management.

| Crate | Description | Key Dependencies |
|-------|-------------|------------------|
| icn-core-vm | WASM execution environment | wasmer, wasmer-wasi |
| icn-dag | DAG implementation for federation state | cid, multihash |
| icn-identity | Identity management and verification | ssi, did-method-key |
| icn-economics | Economic policy enforcement | icn-core-vm, icn-storage |
| icn-governance-kernel | Governance logic and proposal processing | icn-core-vm, icn-identity |
| icn-federation | Federation management and lifecycle | icn-identity, icn-storage |
| icn-ccl-compiler | Cooperative Coordination Language compiler | WASM target |

**Responsibilities:**
- Execute governance operations in a sandboxed WASM environment
- Enforce economic policies and resource allocation
- Maintain and verify the federation state DAG
- Issue and validate credentials
- Process proposals, votes, and appeals
- Implement federation bootstrapping protocol

### Wallet System

The wallet provides identity management, credential storage, and governance participation.

| Crate | Description | Key Dependencies |
|-------|-------------|------------------|
| icn-wallet-core | Core wallet functionality | icn-wallet-types, icn-wallet-storage |
| icn-wallet-sync | DAG synchronization | icn-wallet-types, reqwest |
| icn-wallet-agent | User agent implementation | icn-wallet-core, icn-wallet-api |
| icn-wallet-types | Shared type definitions | serde, chrono |
| icn-wallet-ffi | Foreign function interface | icn-wallet-core, uniffi |
| icn-wallet-storage | Secure data storage | icn-wallet-types |
| icn-wallet-identity | Identity management | icn-wallet-types, did-method-key |

**Responsibilities:**
- Manage user identity (DIDs and key material)
- Store and sync DAG threads
- Securely store and share credentials
- Resolve conflicts in local DAG state
- Synchronize with federation nodes
- Provide unified bindings for mobile platforms

### AgoraNet System

AgoraNet provides the deliberation layer and user-facing APIs.

| Crate | Description | Key Dependencies |
|-------|-------------|------------------|
| agoranet-api | REST API server | axum, sqlx |
| agoranet-auth | Authentication middleware | jwt, icn-identity |
| agoranet-db | Database interface | sqlx, postgres |
| agoranet-message | Messaging protocol | serde, tokio |

**Responsibilities:**
- Provide REST API for thread/message operations
- Handle user authentication and authorization
- Facilitate inter-federation communication
- Implement deliberation thread logic
- Forward governance operations to runtime
- Serve federation status information

### Mesh Compute System

The Mesh Compute overlay enables distributed resource sharing.

| Crate | Description | Key Dependencies |
|-------|-------------|------------------|
| mesh-types | Core data structures | serde, cid |
| mesh-net | P2P networking layer | libp2p, tokio |
| mesh-reputation | Reputation tracking | mesh-types |
| mesh-escrow | Economic token management | icn-economics |

**Responsibilities:**
- Manage distributed task execution lifecycle
- Enable verifiable WebAssembly computation
- Implement economic incentives for honest computation
- Provide resource discovery and allocation
- Ensure secure and private execution environments

## Data Flow and Integration Points

### Wallet ↔ Runtime Integration

```
┌──────────────────────┐                  ┌──────────────────────┐
│                      │                  │                      │
│      ICN Wallet      │◀────────────────▶│     ICN Runtime      │
│                      │   wallet-types   │                      │
└───────────┬──────────┘                  └──────────┬───────────┘
            │                                        │
            │                                        │
            ▼                                        ▼
┌──────────────────────┐                  ┌──────────────────────┐
│                      │                  │                      │
│   Credential Store   │                  │   Federation Logic   │
│                      │                  │                      │
└──────────────────────┘                  └──────────────────────┘
            │                                        │
            │                                        │
            ▼                                        ▼
┌──────────────────────┐                  ┌──────────────────────┐
│                      │                  │                      │
│   Wallet-Sync API    │◀────────────────▶│   Governance Kernel  │
│                      │                  │                      │
└──────────────────────┘                  └──────────────────────┘
```

- Shared type definitions via `icn-wallet-types`
- DAG node submission for governance operations
- TrustBundle verification for credential validation
- Binary data handling and error propagation

### AgoraNet ↔ Runtime Integration

- Direct library calls for synchronous operations
- Message queue for asynchronous operations
- Shared storage for large data
- Event callbacks for state updates

### Mesh ↔ Runtime Integration

- Host ABI functions for policy management and escrow
- Federation identity verification
- DAG event system for tracking mesh operations
- Economic token management for task rewards

## Current Consolidation Status

The ICN codebase is currently undergoing consolidation to eliminate duplicate crates and resolve circular dependencies:

### Completed Tasks
- [x] Identify duplicate crates across components
- [x] Standardize naming conventions for primary crates

### In-Progress Tasks
- [ ] Merge functionality of duplicate wallet crates
- [ ] Implement proper abstraction layers between components
- [ ] Resolve circular dependencies in identity and messaging

### Upcoming Tasks
- [ ] Update root Cargo.toml to include all active crates
- [ ] Remove exclusion entries that reference resolved duplicates
- [ ] Finalize integration tests across component boundaries

## Future Architecture Direction

1. **Event Bus Integration**: Implement a unified event system for cross-component communication
2. **Unified API Layer**: Create a consistent API exposure model for all components
3. **Shared Identity System**: Consolidate identity management across wallet, runtime and mesh
4. **Cross-Federation Mechanisms**: Enhance support for operations spanning multiple federations
5. **Unified CLI Interface**: Provide a consistent command-line experience across all components

## Conclusion

The ICN architecture provides a robust foundation for decentralized governance and economic coordination through a federation-based model. By combining WebAssembly execution, DAG-based state management, cryptographic trust primitives, and distributed computation, the system enables secure, transparent, and efficient collective decision-making.

The modular design of the codebase facilitates ongoing development and extension, while the shared type system ensures compatibility across components. The consolidation work currently underway will further strengthen the system's coherence and maintainability. 