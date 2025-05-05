# ICN Wallet Architecture

This document describes the architecture of the reengineered ICN Wallet, designed to address the issues in the previous implementation and provide a clean, modular, and maintainable foundation.

## Design Goals

1. **Clean Separation of Concerns**: Clear module boundaries with well-defined interfaces
2. **Consistent Error Handling**: Unified error types and propagation strategy
3. **Mobile-First Design**: Optimized for mobile platforms with efficient FFI bindings
4. **Scalable Architecture**: Designed to grow with the ICN ecosystem
5. **Testable Components**: Architecture that facilitates unit and integration testing

## Architecture Overview

The wallet is organized into six primary modules with clearly defined responsibilities:

```
┌─────────────────────────────────────────────────────────────────┐
│                        Wallet API (api)                         │
└───────────┬─────────────────┬────────────────┬─────────────┬────┘
            │                 │                │             │     
┌───────────▼─────┐ ┌─────────▼──────┐ ┌───────▼───────┐ ┌──▼───────────┐
│  Identity (id)  │ │ Actions (act)  │ │ Sync (sync)  │ │ Storage (st) │
└─────────────────┘ └────────────────┘ └───────────────┘ └──────────────┘
                                                    
┌─────────────────────────────────────────────────────────────────┐
│                       FFI Interface (ffi)                       │
└─────────────────────────────────────────────────────────────────┘
```

### Module Responsibilities

#### 1. Identity Module (`identity`)

Responsible for all identity-related operations:
- DID creation and management
- Cryptographic key operations (signing, verification)
- Credential management (storage, validation)
- Identity scope handling (Individual, Community, Cooperative)

#### 2. Storage Module (`storage`)

Handles all persistent storage concerns:
- File storage for keys, credentials, and settings
- Secure storage for sensitive data
- DAG node persistence and retrieval
- Caching and optimization

#### 3. Sync Module (`sync`)

Manages synchronization with ICN Runtime nodes:
- TrustBundle synchronization
- DAG synchronization
- Federation protocol implementation
- Network status monitoring

#### 4. Actions Module (`actions`)

Manages all user-initiated actions:
- Action queue management
- DAG node creation and submission
- Action state management (pending, completed, failed)
- Action execution processing

#### 5. API Module (`api`)

Provides a high-level API for applications:
- Unified interface to all wallet functionality
- Event notifications for wallet state changes
- Settings management
- Task scheduling and lifecycle management

#### 6. FFI Module (`ffi`)

Provides foreign function interfaces for mobile integration:
- Unified FFI layer using UniFFI
- Platform-specific bindings (Swift, Kotlin)
- Async-to-sync bridging
- Error handling translation

## Error Handling Strategy

The wallet implements a consistent error handling strategy:

1. Each module defines its own error types
2. Errors implement the standard `Error` trait
3. Module APIs return `Result<T, ModuleError>` types
4. A top-level `WalletError` enum provides translation from module errors
5. FFI layer translates Rust errors to platform-specific error types

## Data Flow

### Authentication Flow

```
Mobile App → FFI → API → Identity → Storage
                    ↓
                  Sync
```

### Action Submission Flow

```
Mobile App → FFI → API → Actions → Identity (signing)
                           ↓
                         Sync (submission)
                           ↓
                        Storage (recording)
```

### TrustBundle Synchronization Flow

```
Scheduled Task → API → Sync → Storage
```

### DAG Synchronization Flow

```
Scheduled Task → API → Sync → Actions (validation) → Storage
```

## Dependencies

The wallet has minimal external dependencies:

- `tokio` for async runtime
- `serde` for serialization
- `ed25519-dalek` for cryptography
- `uniffi` for FFI bindings
- `reqwest` for HTTP client functionality

## Security Considerations

1. **Key Management**: Private keys never leave the device
2. **Credential Security**: Credentials are stored encrypted at rest
3. **Network Security**: All communication uses TLS
4. **Identity Verification**: All DAG nodes are signed by the identity
5. **Zero Knowledge Proofs**: Support for selective disclosure

## Testing Strategy

1. **Unit Tests**: Each module has comprehensive unit tests
2. **Integration Tests**: Cross-module functionality is tested at boundaries
3. **Mocking**: Network and storage layers can be mocked for reliable testing
4. **Fuzz Testing**: Key cryptographic and serialization code is fuzz tested
5. **End-to-End Tests**: Full system tests using simulated nodes

## Implementation Plan

1. **Core Infrastructure**: Identity, Storage, and basic API modules
2. **Sync Capabilities**: Federation protocol implementation
3. **Action Framework**: Queue system and processing logic
4. **FFI Layer**: Mobile bindings and platform optimization
5. **Advanced Features**: Zero knowledge proofs, advanced governance

## Migration Strategy

For users of the existing wallet:
1. Export identity and credentials from old wallet
2. Import into new wallet with verification
3. Validate DAG sync with federation nodes
4. Resume normal operations with enhanced features 