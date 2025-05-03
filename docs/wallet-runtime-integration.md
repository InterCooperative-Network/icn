# ICN Wallet ↔ Runtime Integration

This document describes the integration points between the ICN Wallet and Runtime components. It serves as a guide for understanding how data flows between these two critical systems.

## Architecture Overview

The ICN system consists of two primary components:

1. **Runtime**: The core consensus and execution engine that processes DAG nodes, manages federation operations, and runs governance proposals.
2. **Wallet**: The user-facing component for identity management, credential storage, and participation in governance.

These components communicate through well-defined interfaces with standardized data structures.

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

## Integration Points

### 1. wallet-types

The `wallet-types` crate serves as the shared type definition library between the Wallet and Runtime components. Key types include:

- **DagNode**: The core data structure representing a node in the directed acyclic graph
- **NodeSubmissionResponse**: Response structure for node submission operations
- **WalletError**: Common error type used across wallet components
- **FromRuntimeError**: Trait for converting runtime errors to wallet errors

### 2. Node Submission Flow

```
┌──────────────────┐    1. Submit     ┌──────────────────┐
│                  │    DagNode       │                  │
│     Wallet       │─────────────────▶│     Runtime      │
│                  │                  │                  │
└──────────────────┘                  └──────────────────┘
         ▲                                     │
         │                                     │
         │                                     │
         │       2. NodeSubmissionResponse     │
         └─────────────────────────────────────┘
```

1. The wallet creates a `DagNode` with the necessary payload and metadata.
2. The node is submitted to the runtime using the wallet-sync API.
3. The runtime processes the node, adds it to the DAG, and returns a `NodeSubmissionResponse`.

### 3. Binary Data Handling

When binary data flows between the wallet and runtime:

1. In the wallet, binary data is stored as `Vec<u8>` in the `DagNode.payload` field.
2. During conversion, the runtime attempts to parse this as JSON.
3. If JSON parsing fails, the data is treated as raw binary and stored as `Ipld::Bytes`.
4. The conversion is fully reversible - binary data is preserved exactly in both directions.

### 4. Error Handling

Error propagation between the wallet and runtime:

```
┌──────────────────┐                  ┌──────────────────┐
│                  │                  │                  │
│     Wallet       │                  │     Runtime      │
│                  │                  │                  │
└──────────────────┘                  └──────────────────┘
         ▲                                     │
         │                                     │
         │     RuntimeError -> WalletError     │
         │           (conversion)              │
         └─────────────────────────────────────┘
```

1. Runtime errors are converted to wallet errors using the `FromRuntimeError` trait.
2. Specific error types (DagError, StorageError, etc.) are mapped to the appropriate WalletError variant.
3. This ensures consistent error handling and proper context preservation.

### 5. TrustBundle Verification

TrustBundle verification flow:

```
┌──────────────────┐    1. Get        ┌──────────────────┐
│                  │    TrustBundle   │                  │
│     Wallet       │─────────────────▶│     Runtime      │
│                  │                  │                  │
└──────────────────┘                  └──────────────────┘
         │                                     │
         │ 2. Verify                           │
         │ Attestations                        │
         ▼                                     │
┌──────────────────┐    3. Submit     ┌──────────────────┐
│                  │    only if       │                  │
│     Wallet       │─────────────────▶│     Runtime      │
│                  │    trusted       │                  │
└──────────────────┘                  └──────────────────┘
```

1. The wallet retrieves the latest TrustBundle from the runtime federation.
2. The wallet verifies attestations and signatures in the TrustBundle.
3. The wallet only submits nodes from issuers listed in the trusted DIDs list.

## Testing Integration

Integration tests cover:

1. **Full Governance Cycle**: Testing proposal creation, voting, finalization, and execution.
2. **Binary Data Handling**: Testing various binary payload edge cases including empty data, large data, and non-UTF8 content.
3. **Error Propagation**: Ensuring runtime errors are properly converted to wallet errors.
4. **TrustBundle Verification**: Testing verification failures and quorum requirements.

## Future Integration Plans

Phase 2 (Federation Mechanics) will focus on enhancing the integration with:

1. **TrustBundle Replication**: Ensuring all federation nodes have consistent TrustBundles
2. **Blob Storage**: Distributed storage for large binary data
3. **Federation Identity**: Unified identity management across the federation
4. **Quorum Verification**: Mechanisms for verifying quorum across federation nodes 