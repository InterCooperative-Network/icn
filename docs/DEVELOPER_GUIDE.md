# ICN Developer Guide

## Quick Start

ICN now provides a simplified development setup using Make commands:

```bash
# Start the complete development environment
make dev

# Start individual components
make runtime     # Start just the runtime
make agoranet    # Start just the AgoraNet API
make wallet      # Build the wallet
make frontend    # Start the dashboard frontend
```

The `make dev` command will:
1. Start PostgreSQL in Docker if not running
2. Run database migrations
3. Start the ICN Runtime
4. Start AgoraNet API
5. Start the Dashboard Frontend

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

## Web Framework Standardization

All API endpoints in AgoraNet now use **Axum** as the web framework. Previous Actix Web implementations have been migrated to ensure consistency and avoid dependency conflicts.

## Database Migrations

SQLx migrations are now organized sequentially with clear timestamps:

- `20240101000000_initial_schema.sql` - Initial database schema
- `20240101000001_add_thread_creator.sql` - Add creator field to threads
- `20240701000000_dag_redesign.sql` - DAG-based schema redesign
- `20240702000000_incremental_data_migration.sql` - Data migration and updates

To verify migrations can be applied to a clean database:

```bash
# Set your database URL
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/icn
./scripts/check_migrations.sh
```

## Federation Configuration

### Bootstrap Nodes

Federation bootstrap nodes are now configurable through:

1. A configuration file: `config/bootstrap_nodes.toml`
2. Environment variables: `ICN_BOOTSTRAP_CONFIG` and `ICN_BOOTSTRAP_NODES`

Example bootstrap node configuration:

```toml
# In config/bootstrap_nodes.toml
[[bootstrap_nodes]]
addr = "/ip4/54.165.32.146/tcp/4001/p2p/QmeLw3P7f1eTXMgMAW9VhvXgvCYrv7VypnEmqw7VJU4pYH"
name = "ICN Main Federation Node 1"
federation_id = "icn-main"
node_did = "did:icn:yK7vdAzCXPP3DZSz2oMKWGU4YUV9GQ4xVfDZnNKj5Y7"
fingerprint = "sha256:f83d6d68965a13d5760b4609fe97c38f2f34a0ad7702c31dfbfda4addaf27638"
```

Environment variable format:
```
ICN_BOOTSTRAP_NODES="/ip4/1.2.3.4/tcp/4001/p2p/QmNodeId1;Node 1;icn-main,/ip4/4.3.2.1/tcp/4001/p2p/QmNodeId2;Node 2;icn-test"
```

### Federation Synchronization

Thread and message synchronization have been fully implemented. The sync logic:

1. Retrieves thread/message data from the database
2. Builds sync message payload
3. Broadcasts to connected peers
4. Handles incoming sync messages with validation

## Development Practices

### Adding New Crates

When adding a new crate to the wallet ecosystem:

1. Follow the naming convention: `icn-wallet-<component>`
2. Place it in `wallet/crates/icn-wallet-<component>`
3. Add it to the workspace in the root `Cargo.toml`
4. Use workspace dependencies where possible
5. Carefully consider its position in the dependency hierarchy

### No Backup Directories

The codebase now prohibits `*backup*` directories through CI validation. Use Git branches for backup purposes instead.

```bash
# CI will fail if backup directories are detected
./scripts/validate_crate_naming.sh
```

## CI Validation

The CI pipeline includes validation to ensure adherence to our naming and structure conventions:

- Directory names should match their package names (enforced by `validate_crate_naming.sh`)
- No duplicate crates should exist in the repository
- All wallet crates should use the `icn-wallet-` prefix
- All runtime crates should use the `icn-` prefix
- No backup directories should exist in the repository (enforced by `validate-no-backups.yml`)

To test these validations locally:

```bash
# Validate crate naming
./scripts/validate_crate_naming.sh 1  # The 1 parameter causes it to exit with an error on failures

# Check for backup directories
find . -type d -name "*backup*" -not -path "*/\.*" | grep -v "node_modules"
``` 