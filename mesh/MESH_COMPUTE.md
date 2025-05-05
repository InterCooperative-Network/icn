# ICN Mesh Compute Overlay

The Mesh Compute overlay provides distributed WebAssembly (WASM) execution capabilities to the Intercooperative Network (ICN). It allows federations to share computational resources and maintain verifiable execution guarantees through a decentralized network of worker nodes.

## Key Concepts

### 1. Task Distribution and Execution

The Mesh Compute overlay manages the lifecycle of computational tasks:

- **Task Intent**: A request to execute a WASM module with specific input data
- **Worker Selection**: Economic mechanisms to select appropriate worker nodes
- **Verified Execution**: Cryptographic proofs ensuring computational integrity
- **Result Consensus**: Multi-party verification of execution results

### 2. Architecture Components

The system consists of several key components:

- **mesh-types**: Core data structures for the Mesh Compute system
- **mesh-net**: P2P networking layer for task distribution and worker coordination
- **mesh-execution**: WASM execution and verification engine
- **mesh-runtime**: Integration with ICN runtime for credential verification

### 3. Trust Model

The Mesh Compute overlay leverages the ICN's federation-based trust model:

- Nodes establish reputation through correct task execution and verification
- Federation lineage provides identity continuity during federation lifecycle events
- Economic incentives align worker behavior with system integrity

## System Flow

### Task Publication

1. A publisher creates a TaskIntent with:
   - WASM module (identified by CID)
   - Input data (identified by CID)
   - Execution parameters (fee, verifier count, etc.)
   
2. The TaskIntent is published to the Mesh network via gossipsub

### Worker Selection

1. Workers submit execution offers with:
   - Cost estimates
   - Available capacity
   - Estimated completion time
   
2. The publisher (or automated system) selects worker(s) based on:
   - Reputation
   - Cost
   - Federation membership
   - Capacity

### Task Execution

1. Selected worker(s) execute the WASM module with the provided input
2. Workers measure execution metrics (time, fuel consumed, etc.)
3. Workers generate and sign ExecutionReceipts with results

### Result Verification

1. Verifier nodes re-execute the task to validate results
2. Verifiers generate VerificationReceipts (approving or rejecting execution)
3. Consensus is reached when sufficient verifiers agree on result

### Reward Distribution

1. Workers receive payment for correct execution
2. Verifiers receive payment for correct verification
3. Penalties apply for incorrect execution/verification

## Execution Model

The Mesh Compute overlay implements a purpose-bound execution model:

- **Capability Scope**: Tasks define their exact resource requirements using CapabilityScope:
  - Memory capacity (MB)
  - CPU cycles
  - GPU FLOPS (for accelerated tasks)
  - I/O bandwidth (MB)

- **Hardware Selection**: Workers publish their available hardware capabilities, and only workers with sufficient resources are selected

- **Execution Summary**: Detailed resource usage is tracked and reported for accountability:
  - Actual resources consumed
  - Contribution score for quality assessment
  - Verification confirmation

- **Deterministic Verification**: Verifiers confirm outputs through independent re-execution

## Economics

The Mesh Compute overlay uses an escrow-based economic model:

1. **Task Creation**:
   - Publisher defines a total reward amount
   - Tokens are locked in an escrow contract

2. **Worker Compensation**:
   - Successful workers receive ~90% of reward
   - Reward proportional to execution quality and timeliness

3. **Verifier Compensation**:
   - Verifiers share ~10% of reward
   - Distribution weighted by verifier reputation

4. **Dispute Resolution**:
   - Disputed executions trigger multi-party verification
   - Invalid executions result in escrow refund
   - Malicious participants face reputation penalties

5. **Reward Scaling**:
   - Rewards scale with task complexity (capability scope)
   - Market dynamics adjust pricing through worker competition

## Governance

The Mesh Compute overlay is governed through a federation-based policy mechanism:

### Mesh Policy

The `MeshPolicy` structure defines all configurable parameters of the mesh network:

- **Reputation Parameters**: Weights for different aspects of reputation scoring (α, β, γ) and decay rate (λ).
- **Reward Settings**: Distribution percentages between workers, verifiers, and platform.
- **Bonding Requirements**: Minimum stake amount, lock periods, and allowed token types.
- **Scheduling Parameters**: Queue management, task timeouts, and priority boost factors.
- **Verification Quorum**: Required consensus levels for task verification.
- **Capability Requirements**: Baseline hardware requirements for participation.

Each federation has its own active policy, which can be updated through governance proposals.

### Policy Update Process

1. **Proposal Creation**:
   - Federation members can propose policy updates by creating a `MeshPolicyFragment`.
   - The fragment contains only the parameters that need to be changed.
   - Proposals are submitted through the `mesh-policy-update.ccl` contract.

2. **Voting**:
   - Federation members vote on proposals using the standard ICN governance process.
   - Votes are recorded in the DAG with `MeshPolicyVote` events.
   - Quorum requirements are defined by the federation's governance rules.

3. **Policy Activation**:
   - When a proposal reaches approval quorum, the new policy is activated.
   - The update is anchored to the DAG with a `MeshPolicyActivated` event.
   - Previous policy versions are preserved for audit purposes.

4. **Policy Application**:
   - All mesh components query the active policy at runtime.
   - Components adapt to policy changes without requiring restarts.
   - Workers and verifiers automatically adjust their behavior based on the active policy.

### CLI Management

The wallet CLI provides commands for managing mesh policies:

```bash
# View the current active policy
icn-wallet mesh policy view

# Create a policy update proposal
icn-wallet mesh policy propose --update-file policy_update.json --description "Increase worker rewards"

# List policy proposals
icn-wallet mesh policy list

# Vote on a policy proposal
icn-wallet mesh policy vote --policy-cid bafy... --approve
```

### Policy Updates via JSON

Policy updates are defined in JSON format:

```json
{
  "reward_settings": {
    "worker_percentage": 75,
    "verifier_percentage": 20,
    "platform_fee_percentage": 5
  },
  "verification_quorum": {
    "required_percentage": 66,
    "minimum_verifiers": 3
  }
}
```

## Implementation Details

### Data Structures

- **TaskIntent**: Specification for a computational task
- **ExecutionReceipt**: Proof of task execution with results
- **VerificationReceipt**: Validation of an execution receipt
- **ComputeOffer**: Worker bid to execute a task
- **PeerInfo**: Information about a mesh network participant
- **ReputationSnapshot**: Point-in-time reputation data for a peer

### Network Protocol

The mesh-net crate implements a libp2p-based networking layer with:

- **Gossipsub**: Task distribution and result sharing
- **Kademlia DHT**: Content-addressable storage for WASM/data
- **mDNS**: Local peer discovery
- **Identify**: Peer metadata exchange

### Future Enhancements

- **Trusted Execution Environments**: Support for SGX/TrustZone execution
- **Zero-Knowledge Proofs**: Privacy-preserving computation verification
- **Federated Learning**: Distributed machine learning capabilities
- **Cross-Federation Compute Markets**: Economic marketplace for computational resources

## Getting Started

### Running a Mesh Node

```bash
# Start a mesh node
cargo run --bin mesh-cli -- start --listen /ip4/0.0.0.0/tcp/9000

# Publish a task
cargo run --bin mesh-cli -- publish-task --wasm-cid <CID> --input-cid <CID> --fee 100

# Offer to execute a task
cargo run --bin mesh-cli -- offer-execution --task-cid <CID> --cost 80

# List active peers
cargo run --bin mesh-cli -- list-peers
```

### Integration with ICN

The Mesh Compute overlay integrates with the core ICN runtime through:

- Federation identity verification
- Credential attestation for task execution
- Economic ledger for task payments

## Security Considerations

- **Sybil Resistance**: Federation attestation prevents identity spoofing
- **DoS Protection**: Economic mechanisms prevent resource exhaustion
- **Data Privacy**: Content-addressable storage with optional encryption
- **Execution Isolation**: WASM sandboxing for secure computation 