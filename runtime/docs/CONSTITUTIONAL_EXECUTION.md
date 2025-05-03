# Constitutional Execution and Economic Enforcement

This document outlines the design and implementation of the constitutional execution and economic enforcement system for the ICN Runtime. This system ensures that governance actions are verifiable, economically metered, and permanently recorded in the DAG.

## Overview

The ICN's constitutional execution system operates on three fundamental principles:

1. **Verifiability** - All governance actions must be cryptographically verifiable and anchored in immutable data structures
2. **Economic Scoping** - All actions must be metered and tracked through an economic accounting system
3. **Transparent Governance** - Authority to perform privileged actions is scoped and validated through identity roles

## CCL Compiler Enhancements

The Constitutional Cooperative Language (CCL) compiler has been extended with new DSL commands:

- `anchor_data(key, value)` - Stores verifiable data to the DAG, anchoring it with a secure content identifier (CID)
- `perform_metered_action(resource_type, amount)` - Provides explicit metering for resource usage
- `mint_token(resource_type, recipient, amount)` - Issues new tokens of a specific resource type (Guardian-only)
- `transfer_resource(from, to, amount)` - Transfers resources between identities, with authorization checks

These commands are compiled into WASM modules that enforce scoped resource usage and authorization.

## Economic Enforcement

The economic enforcement system ensures that:

1. All resource usage is authorized before execution through the `host_check_resource_authorization` function
2. All resource consumption is tracked through `host_record_resource_usage`
3. All token issuance is restricted to Guardian-level identities
4. All economic activity is anchored to the DAG for transparency and auditing

### Resource Types

The system recognizes the following resource types:

- **Compute** - Processing resources used during execution
- **Storage** - Data storage capacity in the system
- **Network** - Data transfer and communication resources
- **Token** - General-purpose tokenized assets

### Authorization Model

Resource authorization follows a hierarchy:

1. **Identity Scope** - Determines permissions (e.g., Guardian can mint tokens)
2. **Resource Scoping** - Limits on specific resource types
3. **DAG Anchoring** - Records authorizations and usage for transparency

## DAG Anchoring System

All governance actions and economic activities are anchored in a Directed Acyclic Graph (DAG) to provide:

1. **Immutability** - Once recorded, data cannot be modified
2. **Verification** - Cryptographic proofs of actions
3. **History** - Complete timeline of governance activities

The anchoring system uses content-addressable storage with CIDs that are deterministically generated from the content.

### Key Structures

When anchoring data to the DAG, the system creates several related data structures:

- **Content Blob** - The raw data being anchored
- **DAG Node** - A reference structure that points to the content and metadata
- **Key Mapping** - A lookup record for easy reference by key

## Implementation Details

### Host Functions

The system exposes the following host functions to WASM modules:

- `host_check_resource_authorization` - Verifies if a requested resource amount is authorized
- `host_record_resource_usage` - Records consumption of resources 
- `host_anchor_to_dag` - Anchors data to the DAG with metadata
- `host_mint_token` - Issues new tokens to a recipient (Guardian-only)
- `host_transfer_resource` - Transfers resources between identities

### Execution Flow

1. **Compilation** - CCL configuration and DSL inputs are compiled to WASM
2. **Authorization** - Before execution, resource requirements are checked
3. **Execution** - WASM module runs with resource constraints
4. **Recording** - All activities are recorded to the DAG
5. **Verification** - Results can be cryptographically verified

## Integration with Governance Systems

The constitutional execution system is tightly integrated with the governance system:

- **Proposal Workflow** - Constitutional proposals are metered and anchored
- **Mandate Execution** - Guardian mandates are enforced through the economic system
- **Federation Parameters** - Economic parameters can be updated through governance

## Verifiable Credentials

All governance outcomes generate Verifiable Credentials that can be:

1. Anchored to the DAG
2. Verified cryptographically
3. Used for authorization in other systems

## Security Model

The system enforces several security principles:

1. **Least Privilege** - Actions are limited to the minimum necessary permissions
2. **Resource Scoping** - Economic resources constrain excessive execution
3. **Role Separation** - Guardian-level operations require specific identity scope
4. **Auditability** - All actions are recorded for verification

## Future Enhancements

- **Proof Generation** - Cryptographic proofs of inclusion for governance actions
- **Multi-party Authorization** - Require multiple signatures for privileged operations
- **Revocation Mechanisms** - Support for credential revocation in the DAG 