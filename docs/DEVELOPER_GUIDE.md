# ICN Developer Guide

## Crate Structure and Naming Conventions

The ICN project follows strict naming conventions for crates to ensure consistency and avoid dependency conflicts. All crates are prefixed with `icn-` followed by their component area.

### Wallet Crates

The wallet component is organized into multiple crates with a standardized naming convention:

| Crate Name | Path | Description |
|------------|------|-------------|
| `icn-wallet-root` | `wallet/` | Root crate for the wallet component |
| `icn-wallet-types` | `wallet/crates/icn-wallet-types` | Common data structures and error types |
| `icn-wallet-storage` | `wallet/crates/icn-wallet-storage` | Persistent storage functionality |
| `icn-wallet-identity` | `wallet/crates/icn-wallet-identity` | Identity and key management |
| `icn-wallet-actions` | `wallet/crates/icn-wallet-actions` | Action execution and handling |
| `icn-wallet-api` | `wallet/crates/icn-wallet-api` | API interfaces for wallet services |
| `icn-wallet-sync` | `wallet/crates/icn-wallet-sync` | Sync with network and other nodes |
| `icn-wallet-agent` | `wallet/crates/icn-wallet-agent` | Agent for distributed operations |
| `icn-wallet-core` | `wallet/crates/icn-wallet-core` | Core wallet functionality |
| `icn-wallet-ffi` | `wallet/crates/icn-wallet-ffi` | Foreign Function Interface for mobile |

### Dependency Structure

The wallet crates follow a specific dependency hierarchy to avoid circular dependencies:

```
icn-wallet-types
    ↑
icn-wallet-storage
    ↑
icn-wallet-identity
    ↑
icn-wallet-sync -------→ icn-wallet-types
    ↑
icn-wallet-actions
    ↑
icn-wallet-api
    ↑
icn-wallet-core
    ↑
icn-wallet-agent
    ↑
icn-wallet-ffi
```

### Runtime Integration

The wallet components integrate with the runtime through well-defined interfaces:

- `runtime/crates/core-vm` depends on `icn-wallet-sync` for certain operations
- The CLI tools in `runtime/cli` use `icn-wallet-types` for shared data structures
- The verifier tools depend on `icn-wallet-agent` and `icn-wallet-core`

## Development Practices

### Adding New Crates

When adding a new crate to the wallet ecosystem:

1. Follow the naming convention: `icn-wallet-<component>`
2. Place it in `wallet/crates/icn-wallet-<component>`
3. Add it to the workspace in the root `Cargo.toml`
4. Use workspace dependencies where possible
5. Carefully consider its position in the dependency hierarchy

### Migrating Existing Code

When migrating code into the wallet structure:

1. Ensure it follows the naming convention
2. Update all dependencies to point to the correct paths
3. Run the validation scripts: `./scripts/validate_crate_naming.sh`
4. Run the test suite to ensure compatibility

## Compatibility Layer

For backward compatibility with existing code, especially in the `icn-wallet-sync` crate, we maintain compatibility layers:

- `compat.rs` modules provide translation between different data structures
- Helper methods like `content_as_json()` maintain API compatibility
- Field mappings (e.g., `issuer` → `creator`, `payload` → `content`) ensure data model compatibility

## Running the CI Validation

The CI pipeline includes validation to ensure adherence to our naming and structure conventions:

- Directory names should match their package names (enforced by `validate_crate_naming.sh`)
- No duplicate crates should exist in the repository
- All wallet crates should use the `icn-wallet-` prefix
- All runtime crates should use the `icn-` prefix

To test these validations locally:

```bash
chmod +x ./scripts/validate_crate_naming.sh
./scripts/validate_crate_naming.sh 1  # The 1 parameter causes it to exit with an error on failures
``` 