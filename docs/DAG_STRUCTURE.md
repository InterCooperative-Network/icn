# ICN Directed Acyclic Graph (DAG) Structure

## Introduction

This document provides a technical overview of the Directed Acyclic Graph (DAG) implementation in the Intercooperative Network (ICN). The DAG serves as the foundational data structure for governance operations, trust anchoring, and federated state coordination across the entire ICN ecosystem.

## Diagram

```
                            ┌─────────────┐
                            │  TrustBundle │
                            │    Node     │
                            └──────┬──────┘
                                   │
                                   ▼
┌─────────────┐            ┌─────────────┐            ┌─────────────┐
│   Proposal  │            │   Anchor    │            │  Federation  │
│    Node     │◄───────────┤    Node     │────────────►   Config    │
└──────┬──────┘            └──────┬──────┘            └─────────────┘
       │                          │
       │                          │
┌──────▼──────┐            ┌──────▼──────┐
│    Vote     │            │    Vote     │
│    Node     │            │    Node     │
└──────┬──────┘            └──────┬──────┘
       │                          │
       │                          │
┌──────▼──────┐            ┌──────▼──────┐
│   Receipt   │            │   Appeal    │
│    Node     │            │    Node     │
└─────────────┘            └──────┬──────┘
                                  │
                                  │
                           ┌──────▼──────┐
                           │   Receipt   │
                           │    Node     │
                           └─────────────┘

                   DAG Structure Example
```

## Glossary

| Term | Definition |
|------|------------|
| **Anchor** | A special DAG node that captures the state of the federation at a point in time with Merkle proofs |
| **CID** | Content Identifier - a cryptographic hash that uniquely identifies a DAG node |
| **DAG** | Directed Acyclic Graph - a data structure with directed edges and no cycles |
| **DID** | Decentralized Identifier - a self-sovereign identifier controlled by a private key |
| **Federation** | A collection of nodes operating under shared governance rules |
| **IPLD** | InterPlanetary Linked Data - a data model for distributed systems |
| **Merkle Tree** | A tree data structure where each leaf node is labeled with a cryptographic hash |
| **Node** | An operation in the DAG with metadata, payload, and references to parent nodes |
| **Quorum** | The minimum number of participants required to make a valid decision |
| **TrustBundle** | A collection of trusted credential issuers and verification keys |
| **VC** | Verifiable Credential - a cryptographically verifiable claim about a subject |

## Purpose of the DAG in ICN

The ICN uses a DAG-based approach rather than a linear blockchain or consensus system for several critical reasons:

### Non-blocking Concurrent Operations

Unlike traditional blockchain systems that process transactions sequentially, the DAG allows multiple operations to be proposed and processed simultaneously. This enables:

- Parallel proposal submission from different federation members
- Concurrent voting processes on multiple governance issues
- Independent credential issuance without centralized sequencing

### Causal Relationship Preservation

The DAG structure explicitly captures the causal relationships between operations:

- Each node references its parent nodes, establishing a "happens-after" relationship
- Operations can have multiple parents, representing causal dependencies on multiple prior states
- The structure naturally represents forking and merging of decision paths

### Federation Autonomy with Cross-Verification

The DAG approach enables:

- Local federation autonomy without requiring global consensus
- Cross-federation verification through shared trust anchors
- Partial state sharing without exposing the entire federation history

### Resilience to Network Partitions

The DAG model offers superior resilience compared to linear models:

- Operations can continue during network partitions
- Automatic reconciliation when connectivity is restored
- No single point of failure in the consensus process

## Node Structure

### DAG Node Definition

Each node in the ICN DAG is represented by the `DagNode` structure:

```rust
pub struct DagNode {
    // Content identifier (cryptographic hash of the node)
    pub cid: String,
    
    // References to parent nodes (array of CIDs)
    pub parents: Vec<String>,
    
    // DID of the node issuer
    pub issuer: String,
    
    // UTC timestamp of node creation
    pub timestamp: SystemTime,
    
    // Cryptographic signature of the node by the issuer
    pub signature: Vec<u8>,
    
    // Binary payload data (serialized governance operation)
    pub payload: Vec<u8>,
    
    // Additional metadata about the node
    pub metadata: DagNodeMetadata,
}
```

### Node Metadata Structure

The `DagNodeMetadata` structure contains operational information:

```rust
pub struct DagNodeMetadata {
    // Type of the node (proposal, vote, anchor, etc.)
    pub node_type: NodeType,
    
    // Scope of the operation (federation, global, etc.)
    pub scope: IdentityScope,
    
    // Visibility level of the node
    pub visibility: Visibility,
    
    // Federation ID where the node was created
    pub federation_id: FederationId,
    
    // Optional thread ID for deliberative threads
    pub thread_id: Option<String>,
    
    // Reference to related nodes (e.g., a vote references its proposal)
    pub references: Vec<Reference>,
    
    // Additional free-form metadata as key-value pairs
    pub attributes: HashMap<String, String>,
}
```

### Payload Types

The payload of a DAG node can contain various types of governance operations:

1. **Proposal**: A proposed governance action, policy change, or resource allocation
2. **Vote**: A vote on a proposal, including the decision and optional justification
3. **Appeal**: A formal appeal against a governance decision
4. **Credential**: A verifiable credential issuance or revocation
5. **Receipt**: Confirmation of operation execution with results
6. **Anchor**: A periodic snapshot of federation state with Merkle proofs
7. **TrustBundle**: A collection of trusted credential issuers and verification keys
8. **ConfigChange**: A change to federation configuration parameters

### CID Generation

Content identifiers (CIDs) are generated using a multi-step process:

1. Serialize the DagNode structure (excluding the CID field)
2. Generate a SHA-256 hash of the serialized data
3. Encode the hash using the multibase format with a base58btc encoding
4. Prefix with the IPLD codec identifier (dag-cbor) and hash function identifier (sha2-256)

This approach ensures:
- Content-addressable storage and retrieval
- Cryptographic verification of node integrity
- Compatibility with IPLD and IPFS ecosystems

### Merkle Root Generation

For operations involving multiple nodes (like batch proposals or anchors):

1. Collect all relevant node CIDs
2. Construct a Merkle tree using these CIDs as leaves
3. Generate the Merkle root hash
4. Include the root hash in the anchor node for efficient verification

## DAG Validation Lifecycle

### 1. Node Creation

A node is created when a governance operation is initiated:

```rust
let node = DagNode {
    cid: "", // Initially empty, computed later
    parents: current_tips(), // Get current DAG tips
    issuer: did.to_string(), // DID of the creating entity
    timestamp: SystemTime::now(),
    signature: Vec::new(), // Initially empty, filled after signing
    payload: serialize_operation(operation),
    metadata: metadata_for_operation(operation),
};
```

### 2. Local Signing

The node is cryptographically signed by the creator:

```rust
// Hash the node contents
let node_bytes = serialize_node_for_signing(&node);
let node_hash = sha256(&node_bytes);

// Sign with the issuer's private key
let signature = key_pair.sign(&node_hash);
node.signature = signature;

// Generate the CID
node.cid = generate_cid(&node);
```

### 3. Local Validation

Before submission, the wallet performs local validation:

- Verify the node's structure conforms to the schema
- Check that parent references are valid and accessible
- Validate the payload against the operation schema
- Ensure the issuer has required credentials for the operation
- Verify the signature matches the issuer's DID

### 4. Submission

The node is submitted to the network through an AgoraNet API:

```
POST /api/v1/dag/nodes
Content-Type: application/json

{
  "node": <serialized_dag_node>,
  "credentials": [<supporting_credentials>]
}
```

### 5. Federation Validation

Upon receipt, federation nodes perform thorough validation:

- Cryptographic verification of the node signature
- DID resolution and key verification
- Parent node existence and validity
- Temporal validation (timestamp within acceptable range)
- Authorization validation using the trust model
- Schema compliance for the specific operation type

### 6. Execution

The Runtime processes the node's payload:

1. Deserialize the payload into the specific operation
2. Apply the operation to the federation state
3. Generate a receipt node confirming execution
4. Update the federation DAG with the receipt

```rust
let receipt = execute_operation(node, federation_state);
let receipt_node = create_receipt_node(receipt, &node);
dag_manager.add_node(receipt_node);
```

### 7. Consensus

Federation nodes achieve consensus on node validity:

- P2P propagation of validated nodes
- Quorum-based acceptance of operations
- Eventual consistency across federation nodes
- Optional guardian signatures for critical operations

### 8. Anchoring

Periodically, the federation state is anchored:

1. Generate a Merkle tree of all nodes since the last anchor
2. Create an anchor node containing the Merkle root
3. Sign the anchor with a federation quorum
4. Publish the anchor for cross-federation verification

### 9. Synchronization

Wallets and other federation nodes sync the updated state:

```rust
// In wallet sync process
let new_nodes = wallet_sync.sync_from_federation(federation_endpoint);
for node in new_nodes {
    // Verify node validity
    if wallet.verify_node(&node) {
        // Add to local dag
        wallet.dag.add_node(node);
    }
}
```

### 10. Conflict Resolution

If conflicting nodes are detected, resolution rules are applied based on predefined policies specific to the operation type.

## Concurrency and Causality

### Causal Relationships

The DAG explicitly models causal relationships:

- **Happens-Before**: If node A is an ancestor of node B, A happened before B
- **Concurrent**: If neither A is an ancestor of B nor B is an ancestor of A, they are concurrent
- **Derived-From**: If B directly references A as a parent, B is derived from A

### Parallel Operation Types

The ICN DAG supports several forms of parallel operations:

1. **Independent Proposals**: Multiple proposals can be submitted concurrently
2. **Parallel Voting**: Votes can be cast simultaneously on multiple proposals
3. **Implementation Streams**: Different execution aspects can progress in parallel
4. **Thread-Based Discussion**: Deliberation threads can branch and merge

### Delayed Operations

The DAG structure inherently supports operations with temporal dynamics:

- **Time-Bound Voting**: Votes accepted only within a specified time window
- **Contingent Execution**: Operations that execute only when certain conditions are met
- **Staged Implementation**: Multi-phase proposals with checkpoints
- **Event-Triggered Actions**: Operations that respond to external events

### Partial State Execution

The federation can perform partial state updates:

- Apply parts of a proposal while other parts are still under deliberation
- Execute operations in different scopes concurrently
- Update credential status independently of governance decisions
- Process economic transactions alongside policy changes

## Conflict Detection and Resolution

### Conflict Types

The ICN DAG can experience several types of conflicts:

1. **Double-Spending**: Multiple operations attempting to allocate the same resource
2. **Contradictory Policies**: Operations that create logically inconsistent rules
3. **Role Conflicts**: Multiple credential updates affecting the same role
4. **Authorization Conflicts**: Disputed authority to perform operations
5. **Temporal Conflicts**: Operations with overlapping or contradictory time bounds

### Detection Mechanisms

Conflicts are detected through several mechanisms:

1. **State Invariant Checks**: Validation against defined invariants
2. **Logical Constraint Validation**: Checking for logical contradictions
3. **Resource Allocation Tracking**: Monitoring resource assignment
4. **Authority Graph Analysis**: Checking credential chains for conflicts
5. **Temporal Overlap Detection**: Analyzing time-bound operations

### Resolution Approaches

The federation resolves conflicts through:

1. **Quorum-Based Resolution**: Requiring a supermajority to resolve disputes
2. **Temporal Precedence**: Earlier operations take precedence
3. **Scope-Based Priority**: Operations in narrower scopes have priority
4. **Policy-Defined Rules**: Explicit rules in the governance model
5. **Appeal Process**: Formal mechanism for contesting resolutions

### Scoped Resolution Rules

Resolution is governed by scope-specific rules:

```rust
pub enum ResolutionStrategy {
    // First valid operation takes precedence
    FirstValid,
    
    // Requires quorum approval for resolution
    QuorumApproval(u32),
    
    // Delegates to guardian committee
    GuardianResolution,
    
    // Applies custom logic defined in WASM module
    CustomLogic(WasmModuleRef),
}
```

## DAG Anchors

### Anchor Structure

DAG anchors are special nodes that provide finality and cross-verification:

```rust
pub struct AnchorNode {
    // Standard DAG node fields
    pub base_node: DagNode,
    
    // Merkle root of anchored state
    pub state_root: String,
    
    // Range of nodes included in this anchor
    pub node_range: NodeRange,
    
    // Vector of federation signatures
    pub signatures: Vec<FederationSignature>,
    
    // Compact proof format for external validation
    pub compact_proof: CompactProof,
}
```

### Anchoring Intervals

Anchors are created based on several triggers:

1. **Time-Based**: Regular intervals (e.g., every 24 hours)
2. **Block-Based**: After a certain number of operations
3. **Event-Based**: After critical governance decisions
4. **Quorum-Based**: When requested by a federation quorum

### Anchor Content

Each anchor includes:

1. **State Merkle Root**: Hash of the current federation state
2. **Node Range**: CIDs of the first and last nodes included
3. **Critical Decisions**: Summary of governance decisions
4. **Guardian Signatures**: Cryptographic attestations from guardians
5. **Cross-References**: References to other federation anchors
6. **Bundle Updates**: Changes to the trust bundle

### Merkle Proof Structure

The compact proof in an anchor follows this structure:

```rust
pub struct CompactProof {
    // Root hash of the Merkle tree
    pub root: String,
    
    // Array of proof elements
    pub proof_elements: Vec<ProofElement>,
    
    // Inclusion bitmap for efficient verification
    pub inclusion_bitmap: Vec<u8>,
}
```

### Verification Process

External parties can verify an anchor through:

1. Validating the quorum signatures
2. Reconstructing the Merkle tree
3. Verifying the compact proof
4. Checking cross-references to other anchors

### Use in Replay and Audit

Anchors enable several advanced capabilities:

1. **State Reconstruction**: Rebuild the federation state from anchors
2. **External Auditing**: Verify federation compliance without full access
3. **Cross-Federation Verification**: Validate operations across federations
4. **Snapshot Restoration**: Restore from a specific anchor point
5. **Continuity Verification**: Ensure unbroken governance history

## Federation Replication & Trust Replay

### Node Propagation

DAG nodes propagate through several mechanisms:

1. **Push Propagation**: Nodes are pushed to connected federation members
2. **Pull Synchronization**: Members periodically pull updates
3. **Gossip Protocol**: Nodes spread through peer-to-peer gossip
4. **Targeted Distribution**: Critical nodes sent directly to affected parties

### Federation Boundaries

ICN supports multiple federation boundaries:

1. **Private Federation**: Nodes visible only within a specific federation
2. **Cross-Federation Shared**: Nodes shared between specific federations
3. **Global Public**: Nodes visible to all participants
4. **Selectively Disclosed**: Nodes shared based on credential-based access

### TrustBundle Structure

A TrustBundle enables cross-federation verification:

```rust
pub struct TrustBundle {
    // Bundle identifier
    pub id: String,
    
    // Federation that issued this bundle
    pub issuer_federation: FederationId,
    
    // Valid time range
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    
    // Trusted issuer DIDs and their roles
    pub trusted_issuers: Vec<TrustedIssuer>,
    
    // Verification keys for signatures
    pub verification_keys: Vec<VerificationKey>,
    
    // Cross-federation trust relationships
    pub trusted_federations: Vec<FederationTrust>,
    
    // Federation signature
    pub signature: FederationSignature,
}
```

### Verification Without Global Consensus

TrustBundles enable trustless verification through:

1. **Chain of Trust**: Follow credential issuance authority
2. **Cross-Federation Anchors**: Verify against shared anchor points
3. **Authority Delegation**: Track delegation of authority across boundaries
4. **Bundle Updates**: Timestamp-ordered updates establish current trust state

### Implementation in Code

Federation nodes implement trust verification as follows:

```rust
// Verify operation across federation boundaries
pub fn verify_cross_federation_operation(
    node: &DagNode,
    local_trust_bundle: &TrustBundle,
    external_trust_bundle: &TrustBundle
) -> Result<(), VerificationError> {
    // 1. Verify node signature
    verify_signature(node)?;
    
    // 2. Check if issuer is directly trusted
    if is_directly_trusted(&node.issuer, local_trust_bundle) {
        return Ok(());
    }
    
    // 3. Check if issuer's federation is trusted
    let issuer_federation = resolve_federation(&node.issuer)?;
    if !is_federation_trusted(issuer_federation, local_trust_bundle) {
        return Err(VerificationError::UntrustedFederation);
    }
    
    // 4. Verify against external trust bundle
    if !is_directly_trusted(&node.issuer, external_trust_bundle) {
        return Err(VerificationError::UntrustedIssuer);
    }
    
    // 5. Verify external bundle signature
    verify_bundle_signature(external_trust_bundle)?;
    
    Ok(())
}
```

## Technical Implementation Notes

### Storage Optimization

The ICN DAG implements several optimizations:

1. **Pruned Views**: Wallets maintain pruned DAG views for efficiency
2. **Compressed Node References**: Use of compressed reference encoding
3. **Selective Synchronization**: Sync only relevant DAG subgraphs
4. **Lazy IPLD Loading**: Load node content on demand

### Scalability Considerations

The DAG structure scales through:

1. **Federated Sharding**: Different federations handle different subgraphs
2. **Scoped Operations**: Operations affect only relevant portions of the state
3. **Condensed History**: Use of anchors to reference large historical sections
4. **Multi-tier Storage**: Recent nodes in hot storage, historical in cold storage

### Security Guarantees

The DAG implementation provides:

1. **Tamper Evidence**: Any change to a node invalidates its CID
2. **Nonrepudiation**: Signed nodes cannot be denied by their issuer
3. **History Preservation**: Complete causal history is maintained
4. **Authority Verification**: All operations verify against trust bundles
5. **Temporal Integrity**: Anchors provide temporal proof of existence

### Current Limitations and Mitigations

Current technical limitations include:

1. **Synchronization Lag**: Mitigated through anchor-based catching up
2. **Storage Growth**: Addressed with pruning and archiving strategies
3. **Network Partition Handling**: Resolved through conflict resolution policies
4. **Verification Overhead**: Reduced through cached verification results
5. **Cross-Federation Latency**: Improved with optimistic execution patterns 

## References

- [ICN Architecture](docs/ARCHITECTURE.md) - Overview of the entire ICN system architecture
- [Federation Bootstrap Protocol](docs/FEDERATION_BOOTSTRAP.md) - Details on federation initialization
- [CCL Language Specification](docs/CCL_SPEC.md) - Cooperative Coordination Language specification
- [Wallet Integration Guide](docs/WALLET_INTEGRATION.md) - Guide for wallet developers

---

*DAG_STRUCTURE.md v0.1 – May 2025 – ICN Protocol Team* 