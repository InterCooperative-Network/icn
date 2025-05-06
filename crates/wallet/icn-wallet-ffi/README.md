# ICN Wallet FFI

The `icn-wallet-ffi` crate provides Foreign Function Interface (FFI) bindings for the ICN Wallet, allowing it to be used from programming languages other than Rust, such as JavaScript, Swift, Kotlin, or C/C++.

## Features

- **Cross-Language Support**: Use ICN Wallet from multiple programming languages
- **Safe API Design**: Memory-safe interface with proper error handling
- **Comprehensive Coverage**: Access to all core wallet functionality
- **Mobile Integration**: Optimized for use in mobile applications
- **Automatic Memory Management**: Proper cleanup of resources
- **Type Mapping**: Bidirectional conversion between Rust and foreign types

## Supported Languages

- **JavaScript/TypeScript**: For web and Node.js applications
- **Swift**: For iOS and macOS applications
- **Kotlin/Java**: For Android applications
- **C/C++**: For native applications and custom integrations

## Usage

### Adding as a Dependency

```toml
[dependencies]
icn-wallet-ffi = { version = "0.1.0", path = "../icn-wallet-ffi" }
```

### Building the FFI Library

```bash
# Build dynamic library
cargo build --release --package icn-wallet-ffi

# Build for Android
cargo ndk -t armeabi-v7a -t arm64-v8a -o ./android/src/main/jniLibs build --release

# Build for iOS
cargo lipo --release
```

### Example: C/C++ Integration

```c
#include "icn_wallet.h"
#include <stdio.h>

int main() {
    // Initialize the wallet
    WalletHandle* wallet = icn_wallet_init("wallet_data");
    if (!wallet) {
        printf("Failed to initialize wallet\n");
        return 1;
    }
    
    // Create an identity
    char* did = icn_wallet_create_identity(wallet);
    if (did) {
        printf("Created identity: %s\n", did);
        
        // Free string allocated by the Rust code
        icn_wallet_free_string(did);
    }
    
    // Clean up
    icn_wallet_free(wallet);
    
    return 0;
}
```

### Example: JavaScript Integration

```javascript
// Using the generated bindings
import { ICNWallet } from 'icn-wallet';

async function walletExample() {
  // Initialize the wallet
  const wallet = await ICNWallet.init('wallet_data');
  
  try {
    // Create a new identity
    const did = await wallet.createIdentity();
    console.log(`Created identity: ${did}`);
    
    // Issue a credential
    const credential = await wallet.issueCredential({
      issuer: did,
      subject: 'did:icn:recipient',
      type: 'TestCredential',
      claims: {
        name: 'Test User',
        role: 'Developer'
      }
    });
    
    console.log(`Issued credential: ${credential.id}`);
  } finally {
    // Clean up
    await wallet.close();
  }
}

walletExample().catch(console.error);
```

### Example: Swift Integration

```swift
import ICNWallet

func walletExample() {
    // Initialize the wallet
    guard let wallet = ICNWallet(path: "wallet_data") else {
        print("Failed to initialize wallet")
        return
    }
    
    // Create an identity
    do {
        let did = try wallet.createIdentity()
        print("Created identity: \(did)")
        
        // Create a proposal
        let proposal = try wallet.createProposal(
            title: "Test Proposal",
            content: [
                "action": "add_member",
                "member": "did:icn:new_member"
            ]
        )
        
        print("Created proposal: \(proposal.id)")
    } catch {
        print("Error: \(error.localizedDescription)")
    }
    
    // Wallet is automatically cleaned up when it goes out of scope
}
```

## Integration with ICN Wallet

This crate provides foreign language bindings to the core ICN wallet components:

- `icn-wallet-core`: The main functionality exposed via FFI
- `icn-wallet-identity`: Identity operations exposed to foreign languages
- `icn-wallet-storage`: Storage operations with safe memory handling
- `icn-wallet-types`: Type definitions mapped to foreign language equivalents

## License

This crate is part of the ICN Wallet project and is licensed under the same terms as the parent project. 