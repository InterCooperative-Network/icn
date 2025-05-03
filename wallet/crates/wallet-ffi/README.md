# Wallet FFI

This crate provides Foreign Function Interface (FFI) bindings for the ICN Wallet core functionality, allowing it to be used from mobile platforms like Android (Kotlin) and iOS (Swift).

## Note on Circular Dependencies

The current codebase has a circular dependency issue between `wallet-agent` and `wallet-sync` crates. This prevents the build process from succeeding. To resolve this issue, the codebase structure should be refactored by:

1. Creating a new `wallet-types` crate that contains shared types like `TrustBundle`, `SyncError`, etc.
2. Moving these shared types from both `wallet-agent` and `wallet-sync` into `wallet-types`
3. Having both `wallet-agent` and `wallet-sync` depend on `wallet-types` instead of each other

Once this refactoring is complete, the wallet-ffi crate can be built successfully.

## Overview

The wallet-ffi crate uses the [UniFFI](https://github.com/mozilla/uniffi-rs) framework to generate language bindings from a Rust API. It exposes core wallet functionality including:

- Identity management
- Action queuing and processing
- Synchronization with federation nodes
- Trust bundle management
- DAG thread operations

## Building

To build the library with FFI bindings (after resolving the circular dependency issue):

```bash
cargo build --package wallet-ffi
```

### Generated Bindings

When building the crate, UniFFI automatically generates:

- Kotlin bindings for Android
- Swift bindings for iOS

The generated files are output in the `target/` directory.

## Using in Android Projects

1. Add the generated `.jar` file from the build output as a dependency
2. Include the compiled native library (`.so`) files for your target architectures
3. Initialize the wallet in your Kotlin code:

```kotlin
import wallet.WalletApi
import wallet.WalletConfig

// Create a default configuration
val config = WalletConfig(
    storagePath = context.filesDir.absolutePath + "/wallet",
    federationUrls = listOf("https://federation.example.com"),
    syncIntervalSeconds = 3600,
    autoSyncOnStartup = true
)

// Initialize the wallet
val wallet = WalletApi(config)

// Create an identity
val metadata = mapOf("displayName" to "User Name")
val identityId = wallet.createIdentity("user", metadata)

// Trigger a sync
wallet.triggerSync()
```

## Using in iOS Projects

1. Add the generated Swift files and module map to your project
2. Include the compiled native library (`.dylib` or `.a`) for your target
3. Initialize the wallet in your Swift code:

```swift
import wallet_ffi

// Create a configuration
let config = WalletConfig(
    storagePath: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!.appendingPathComponent("wallet").path,
    federationUrls: ["https://federation.example.com"],
    syncIntervalSeconds: 3600,
    autoSyncOnStartup: true
)

// Initialize the wallet
let wallet = try WalletApi(config: config)

// Create an identity
let metadata = ["displayName": "User Name"]
let identityId = try wallet.createIdentity(scope: "user", metadata: metadata)

// Get sync status
let syncStatus = try wallet.getSyncStatus()
```

## Data Validation

The library includes robust validation of data received from the network:

- **TrustBundle Validation**: Ensures bundles have valid signatures, proper timestamps, and meet governance requirements
- **DagNode Validation**: Verifies node structure, signatures, timestamps, and parent references

This validation happens automatically within the SyncManager before data is saved to local storage.

## Testing

Run the test suite (after resolving the circular dependency issue):

```bash
cargo test --package wallet-ffi
```

## License

This crate is part of the ICN Wallet project. 