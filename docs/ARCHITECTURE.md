# Intercooperative Network (ICN) Architecture

## Overview

The Intercooperative Network (ICN) is a decentralized governance and economic coordination system designed to facilitate federated decision-making and resource allocation. Built as a Rust monorepo, the ICN system consists of three primary components:

1. **Runtime (CoVM v3)**: A WebAssembly execution environment that processes governance operations, enforces economic policies, and maintains the federated state.

2. **Wallet**: A secure, mobile-first client agent that manages user identity, credentials, and local state.

3. **AgoraNet**: A REST API server and deliberation engine that facilitates inter-federation communication and user interaction.

The system uses a Directed Acyclic Graph (DAG) as its foundational data structure, enabling non-blocking concurrent operations while maintaining causal relationships. Identity and trust are managed through DIDs (Decentralized Identifiers) and VCs (Verifiable Credentials), with a federation-based trust model.

## Component Map

### Runtime (CoVM v3)

| Crate | Path | Description | Key Dependencies |
|-------|------|-------------|------------------|
| icn-core-vm | runtime/crates/core-vm | WASM execution environment | wasmer, wasmer-wasi |
| icn-host-abi | runtime/crates/host-abi | Host functions exposed to WASM modules | icn-core-vm |
| icn-storage | runtime/crates/storage | Persistent storage interface | cid, multihash |
| icn-identity | runtime/crates/identity | Identity management and verification | ssi, did-method-key |
| icn-economics | runtime/crates/economics | Economic policy enforcement | icn-core-vm, icn-storage |
| icn-governance-kernel | runtime/crates/governance-kernel | Core governance logic | icn-core-vm, icn-identity |
| icn-federation | runtime/crates/federation | Federation management | icn-identity, icn-storage |
| icn-ccl-compiler | runtime/crates/ccl-compiler | Cooperative Coordination Language compiler | WASM target |
| icn-execution-tools | runtime/crates/execution-tools | Utilities for runtime execution | icn-core-vm |

**Responsibilities:**
- Execute governance operations in sandboxed WASM environment
- Enforce economic policies and resource allocation
- Maintain and verify the federation state DAG
- Issue and validate credentials
- Process proposals, votes, and appeals
- Implement federation bootstrapping protocol

### Wallet

| Crate | Path | Description | Key Dependencies |
|-------|------|-------------|------------------|
| wallet-core | wallet/crates/wallet-core | Core wallet functionality | wallet-types, wallet-storage |
| wallet-sync | wallet/crates/sync | DAG synchronization | wallet-types, reqwest |
| wallet-agent | wallet/crates/wallet-agent | User agent implementation | wallet-core, wallet-api |
| wallet-types | wallet/crates/wallet-types | Shared type definitions | serde, chrono |
| wallet-ffi | wallet/crates/wallet-ffi | Foreign function interface | wallet-core, uniffi |
| wallet-storage | wallet/crates/storage | Secure data storage | wallet-types |
| wallet-identity | wallet/crates/identity | Identity management | wallet-types, did-method-key |
| wallet-api | wallet/crates/api | API client for AgoraNet | wallet-types, reqwest |
| wallet-actions | wallet/crates/actions | Action processing | wallet-core, wallet-api |

**Responsibilities:**
- Manage user identity (DIDs and key material)
- Store and sync DAG threads
- Securely store and selectively share credentials
- Resolve conflicts in local DAG state
- Synchronize with federation nodes
- Provide unified bindings for mobile platforms

### AgoraNet

| Crate | Path | Description | Key Dependencies |
|-------|------|-------------|------------------|
| agoranet-api | agoranet/crates/api | REST API server | axum, sqlx |
| agoranet-auth | agoranet/crates/auth | Authentication middleware | jwt, icn-identity |
| agoranet-dashboard | agoranet/crates/dashboard | Admin dashboard | axum, tower-http |
| agoranet-db | agoranet/crates/db | Database interface | sqlx, postgres |
| agoranet-message | agoranet/crates/message | Messaging protocol | serde, tokio |

**Responsibilities:**
- Provide REST API for thread/message CRUD operations
- Handle user authentication and authorization
- Facilitate inter-federation communication
- Implement deliberation thread logic
- Forward governance operations to runtime
- Serve federation status information

## Data Flow Diagram

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│                 │     │                 │     │                 │
│  Mobile Wallet  │     │  Desktop Wallet │     │   CLI Wallet    │
│                 │     │                 │     │                 │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                          wallet-core                            │
│                                                                 │
└─────────────────────────────┬───────────────────────────────────┘
                              │
                              │ HTTP/REST
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                          AgoraNet API                           │
│                                                                 │
└─────────────────────────────┬───────────────────────────────────┘
                              │
         ┌───────────────────┴────────────────────┐
         │                                        │
         │                                        │
         ▼                                        ▼
┌─────────────────────┐              ┌─────────────────────────┐
│                     │              │                         │
│  Federation Node 1  │◄────────────►│    Federation Node 2    │
│                     │    P2P       │                         │
└─────────┬───────────┘              └─────────────┬───────────┘
          │                                        │
          │                                        │
          ▼                                        ▼
┌─────────────────────┐              ┌─────────────────────────┐
│                     │              │                         │
│     Runtime VM      │              │      Runtime VM         │
│                     │              │                         │
└─────────────────────┘              └─────────────────────────┘
```

## DAG Lifecycle

The Directed Acyclic Graph (DAG) is the core data structure of the ICN system, representing the causal relationships between operations:

1. **Creation**: A wallet creates a DAG node containing a payload (proposal, vote, etc.) and references to parent nodes.

2. **Signing**: The node is signed using the user's private key, establishing authorship and authorization.

3. **Local Validation**: The wallet validates the node structure, signature, and causal relationships.

4. **Submission**: The signed node is submitted to the network through AgoraNet.

5. **Federation Validation**: Federation nodes verify the node's signature, structure, and authorization.

6. **Execution**: The Runtime processes the node's payload, updating the federation state.

7. **Consensus**: Federation nodes reach consensus on the validity and ordering of nodes.

8. **Anchoring**: Periodically, the federation state is anchored to provide finality.

9. **Synchronization**: Other wallets and federation nodes sync the updated state.

10. **Conflict Resolution**: If conflicting nodes are detected, resolution is applied based on predefined rules.

## Trust Model

The ICN trust model is built upon several cryptographic primitives and federation-based validation:

### Decentralized Identifiers (DIDs)
- Each participant has a DID that serves as their persistent, verifiable identity
- DIDs are controlled by cryptographic key pairs
- Different key types are supported (Ed25519, RSA, etc.)
- DID documents describe verification methods and services

### Verifiable Credentials (VCs)
- Credentials are issued by authorized entities
- Credentials contain claims about subjects
- Credentials can be selectively disclosed
- Credentials are cryptographically verifiable

### Trust Bundles
- Federation nodes maintain a bundle of trusted issuers
- Trust bundles contain DIDs of trusted credential issuers
- Trust bundles are updated through governance processes
- Trust bundles establish the root of trust for the federation

### DAG Anchoring
- Critical federation state is anchored periodically
- Anchoring provides a consensus snapshot of the federation
- Anchors can be cross-verified between federations
- Anchors establish checkpoints for conflict resolution

### Scoped Authorization
- Operations are authorized based on credential scope
- Different operations require different credential types
- Credential scopes limit the actions a participant can perform
- Scopes are enforced by the Runtime during execution

## Integration Paths

### Wallet ↔ AgoraNet
- HTTP REST API for thread/message operations
- WebSocket for real-time updates
- Authentication via JWT tokens
- Selective disclosure of credentials

### AgoraNet ↔ Runtime
- Direct library calls for synchronous operations
- Message queue for asynchronous operations
- Shared storage for large data
- Event callbacks for state updates

### Wallet ↔ Runtime
- Indirect integration through AgoraNet
- Direct P2P connection for critical operations
- Shared wallet-types crate for type compatibility
- DAG node submission and validation

### Federation ↔ Federation
- Authenticated P2P communication
- Cross-federation credential verification
- Trust bundle exchange protocol
- State synchronization through DAG exchange

## Development Status

As of May 2025, the ICN codebase is in active development with the following status:

### Completed Components
- Runtime (CoVM v3): Feature-complete with green tests
- Governance kernel: Fully implemented with support for proposals, voting, and appeals
- CCL → WASM compiler: Operational with support for all core primitives
- Identity and trust validation: Complete implementation with DID and VC support
- Economic primitives: Fully implemented with resource allocation and accounting
- Federation bootstrap protocol: Complete implementation of phases 1-6
- Docker dev-net: Operational with support for local development

### Active Development
- Monorepo consolidation: Eliminating duplicate crates and nested workspaces
- Wallet ↔ Runtime interface: Resolving circular dependencies in icn-identity and agoranet
- Natural-language CCL: Developing a more human-readable layer for bylaws & policies
- Guardian system: Downscoping from a headline feature to an optional quorum role
- Mobile wallet UI: React Native implementation in progress

### Planned Improvements
- Installer & Dev UX: Creating a streamlined installer for test federations
- End-to-end federation testing: Comprehensive testing across federation boundaries
- Documentation expansion: Developer onboarding guides and glossary
- Public test federation: Deployment for early adopters

### Known Issues
- Duplicate crates in wallet and runtime directories
- Circular dependencies in identity and messaging components
- Configuration path inconsistencies across components
- Grammar tweaks needed for natural-language CCL

## Conclusion

The ICN architecture provides a robust foundation for decentralized governance and economic coordination through a federation-based model. By combining WebAssembly execution, DAG-based state management, and cryptographic trust primitives, the system enables secure, transparent, and efficient collective decision-making.

The modular design of the codebase facilitates ongoing development and extension, while the shared type system ensures compatibility across components. As development continues, the focus remains on creating a user-friendly experience while maintaining the security and integrity required for cooperative governance. 