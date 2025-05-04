# ICN Architecture Overview

## Core Components

The ICN platform consists of three main layers:

1. **Wallet Layer** - Identity, storage, and user agent
2. **AgoraNet Layer** - Deliberation, API, and federation communication
3. **Runtime Layer** - CoVM execution environment, DAG, and federation logic

```
├─ agoranet/          ← deliberation & API layer
├─ runtime/           ← CoVM V3, CCL toolchain, federation logic
├─ wallet/            ← mobile‑first agent + storage/sync crates
├─ frontend/          ← React dashboard prototype
├─ devnet/            ← Docker‑compose demo federation
├─ docs/ & scripts/   ← extensive design notes + helper tooling
└─ tools/ & tests/    ← misc. utilities and integration suites
```

## Layer Responsibilities

### Wallet Layer

The Wallet layer is responsible for:
- User identity management (DIDs)
- Local storage and synchronization
- Credential management
- Mobile interface via FFI bindings

Key components:
- `wallet-identity`: DID creation and management
- `wallet-storage`: Local secure storage
- `wallet-ffi`: Foreign function interface for mobile integration

### AgoraNet Layer

The AgoraNet layer is responsible for:
- API endpoints for wallet interaction
- Federation consensus coordination
- Message and thread management
- Persistence of federation state

Key components:
- `agoranet-api`: REST API for wallet interactions
- `agoranet`: Core deliberation logic

### Runtime Layer

The Runtime layer is responsible for:
- Contract execution via CoVM (Collaborative Virtual Machine)
- DAG-based state recording
- Federation validation and consensus
- Identity verification

Key components:
- `core-vm`: Contract execution environment
- `dag`: Directed acyclic graph implementation
- `federation`: Federation management logic
- `identity`: DID/VC implementation for the runtime

## Cross-Layer Communication

1. **Wallet → AgoraNet**: The wallet connects to AgoraNet via REST API endpoints
2. **AgoraNet → Runtime**: AgoraNet submits execution requests to the Runtime via internal APIs
3. **Runtime → AgoraNet**: Execution receipts flow back to AgoraNet for distribution

## Future Development

1. Complete the migration of shared functionality to dedicated crates
2. Standardize the FFI interfaces for mobile wallet integration
3. Implement enhanced security measures for key management
4. Refine the federation protocol for larger-scale deployments

## Migration Status

This document supersedes individual refactoring reports found in:
- `/refactoring-report.md`
- `/runtime/REFACTORING.md`
- `/wallet/MIGRATION_PLAN.md`

Future architectural changes will be documented here to maintain a single source of truth. 