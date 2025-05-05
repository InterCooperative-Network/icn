# DAG Anchor Module

## Overview

The DAG Anchor module is responsible for creating and verifying anchors in the Directed Acyclic Graph (DAG) of a federation. Anchors are special nodes that provide cryptographic proofs of federation state at specific points in time.

## Key Components

- **GenesisAnchor**: A structure representing the first anchor in a federation's DAG, containing references to the DAG root, trust bundle, and federation identity.

- **Anchor Creation**: Functions for creating cryptographically signed anchors that can be used to verify the integrity of federation state.

- **Anchor Verification**: Functions for validating the authenticity and integrity of existing anchors.

- **Merkle Root Calculation**: Utility functions for computing content-addressable identifiers (CIDs) for DAG anchors.

## Primary Interfaces

```rust
// Create a new genesis anchor
pub async fn create_genesis_anchor(
    trust_bundle: &GenesisTrustBundle,
    keypair: &KeyPair,
    federation_did: &str,
) -> FederationResult<GenesisAnchor>

// Verify a genesis anchor
pub async fn verify_genesis_anchor(
    anchor: &GenesisAnchor,
    trust_bundle: &GenesisTrustBundle,
) -> FederationResult<bool>

// Convert an anchor to a DAG payload for storage
pub fn to_dag_payload(&self) -> serde_json::Value
```

## Usage Context

DAG anchors are essential for:

1. **Federation Genesis**: Establishing the initial state of a new federation
2. **State Verification**: Providing cryptographic proof of federation state at specific points
3. **Cross-Federation Trust**: Enabling verification of foreign federation state
4. **Recovery Procedures**: Supporting disaster recovery and state reconstruction

## Development Status

This module is complete and has full test coverage. Future enhancements may include:

- Support for different anchor types beyond genesis anchors
- Optimized Merkle proof generation for partial state verification
- Enhanced anchor chain validation for faster state replay 