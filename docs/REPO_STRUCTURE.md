# ICN Monorepo Structure

This document outlines the organization of the ICN monorepo, describing the purpose and relationships between different modules.

## Top-Level Structure

```
icn/
├── agoranet/            # Deliberation layer with clean APIs, backend code, and DB migrations
├── docs/                # Documentation for all components
├── frontend/            # Frontend applications
├── runtime/             # Core Rust logic for federation governance, execution, DAG, economics, and storage
├── scripts/             # Utility scripts for development, testing, and deployment
├── tools/               # Standalone tools and utilities
└── wallet/              # Mobile-first identity and sync agent
```

## Core Component Details

### runtime/

The canonical home for all Rust logic related to federation governance, execution, DAG, economics, and storage.

```
runtime/
├── bin/                 # Binary executable entry points
├── crates/              # Core runtime crates
│   ├── common/          # Shared utilities and types
│   ├── core-vm/         # Virtual machine implementation
│   ├── dag/             # Directed acyclic graph implementation
│   ├── economics/       # Economic models and incentives
│   ├── federation/      # Federation management
│   ├── governance-kernel/ # Governance mechanisms
│   └── storage/         # Storage implementations
├── docs/                # Runtime-specific documentation
├── examples/            # Example code and applications 
├── tests/               # Integration and unit tests
└── config/              # Configuration templates and examples
```

### wallet/

The mobile-first identity and sync agent.

```
wallet/
├── crates/              # Wallet component crates
│   ├── actions/         # User action implementation
│   ├── api/             # API interface definitions
│   ├── ffi/             # Foreign function interface for mobile integration
│   ├── identity/        # Identity management components
│   ├── storage/         # Storage mechanism
│   ├── sync/            # Synchronization functionality
│   ├── wallet-agent/    # Agent implementation
│   ├── wallet-core/     # Core wallet functionality
│   ├── wallet-ffi/      # FFI implementation for mobile platforms
│   └── wallet-types/    # Type definitions
├── docs/                # Wallet-specific documentation
├── examples/            # Example code for wallet usage
└── src/                 # Wallet crate root source
```

### agoranet/

The deliberation layer exposing clean APIs.

```
agoranet/
├── crates/              # Agoranet component crates
│   └── agoranet-api/    # API implementation
├── migrations/          # Database migration scripts
└── src/                 # Main agoranet source code
```

### frontend/

Frontend applications.

```
frontend/
├── dashboard/           # Main ICN dashboard (React/TS)
└── agoranet-dashboard/  # Agoranet-specific dashboard
```

### tools/

Standalone utilities and tools.

```
tools/
├── health_check/        # Health check service (consolidated)
└── icn-verifier/        # Bundle verification tool
```

### scripts/

Utility scripts for development, testing, and deployment.

```
scripts/
├── deployment/          # Deployment scripts
├── development/         # Development utilities
└── testing/             # Testing scripts
```

### docs/

Central documentation repository.

```
docs/
├── REPO_STRUCTURE.md          # This document
├── MIGRATION_PLAN.md          # Migration plan for restructuring
├── runtime/                   # Runtime-specific documentation
├── wallet/                    # Wallet-specific documentation
└── agoranet/                  # Agoranet-specific documentation
```

## Module Relationships

- `runtime` provides the core federation logic that `wallet` and `agoranet` build upon
- `wallet` handles identity and synchronization of user data
- `agoranet` provides the deliberation layer which connects to both `runtime` and `wallet`
- `frontend` applications consume APIs from all core components
- `tools` provide standalone utilities that work with various components

## Build System

The monorepo uses Cargo workspaces to manage the Rust crates. The top-level Cargo.toml defines the workspace and common dependencies, while individual components have their own Cargo.toml files for component-specific dependencies.

The frontend applications use npm/yarn for dependency management.

## Guidelines for Future Development

1. **Module Placement**: Place new code in the appropriate module based on functionality:
   - Federation logic, execution, DAG, economics, storage → `runtime/`
   - Identity and data synchronization → `wallet/`
   - Deliberation and voting → `agoranet/`
   - User interfaces → `frontend/`
   - Standalone tools → `tools/`

2. **Dependency Management**:
   - Prefer workspace dependencies for common libraries
   - Minimize direct dependencies between major modules
   - Define clear interfaces between components

3. **Naming Conventions**:
   - Use consistent prefixing for related crates (e.g., `wallet-*`)
   - Follow Rust naming conventions for crates and modules
   - Document APIs and module boundaries clearly

4. **Testing**:
   - Place unit tests alongside code
   - Place integration tests in dedicated test directories
   - Ensure cross-module tests validate component relationships 