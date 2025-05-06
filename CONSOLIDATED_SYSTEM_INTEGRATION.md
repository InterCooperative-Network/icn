# ICN System Integration Guide

This document provides a comprehensive guide to the integration between the various components of the ICN system: Runtime, Wallet, AgoraNet, and Mesh Compute. It serves as a reference for understanding how data and control flows across system boundaries.

## Overview

The ICN system consists of four primary components that work together to provide a complete decentralized governance and computation system:

```
┌────────────────┐        ┌────────────────┐        ┌────────────────┐
│                │        │                │        │                │
│    Wallet      │◄─────► │    AgoraNet    │◄─────► │    Runtime     │
│                │        │                │        │                │
└───────┬────────┘        └────────────────┘        └──────┬─────────┘
        │                                                  │
        │                                                  │
        │                                                  │
        │                                                  │
        │                 ┌────────────────┐               │
        │                 │                │               │
        └────────────────►│     Mesh       │◄──────────────┘
                          │                │
                          └────────────────┘
```

Each component has specific responsibilities:

- **Runtime**: Core execution engine for governance operations
- **Wallet**: User identity and state management
- **AgoraNet**: Deliberation and user interface
- **Mesh**: Distributed computation resources

## Shared Data Structures

### Core Types

These fundamental types are shared across all components:

| Type | Description | Used By |
|------|-------------|---------|
| DagNode | Core data structure representing a DAG node | All components |
| Did | Decentralized identifier string | All components |
| Cid | Content identifier for addressing data | All components |
| VerifiableCredential | Cryptographic credential structure | All components |

### Message Types

These types facilitate communication between components:

| Type | Description | Used For |
|------|-------------|----------|
| NodeSubmissionResponse | Response to DAG node submission | Wallet ↔ Runtime |
| ExecutionReceipt | Result of operation execution | Runtime ↔ Mesh |
| MeshPolicyFragment | Partial mesh policy update | Wallet ↔ Mesh ↔ Runtime |
| FederationEvent | Federation state change notification | Runtime ↔ AgoraNet |

## Wallet ↔ Runtime Integration

### Data Flow Diagram

```
┌──────────────────┐    1. Submit     ┌──────────────────┐
│                  │    DagNode       │                  │
│     Wallet       │─────────────────▶│     Runtime      │
│                  │                  │                  │
└──────────────────┘                  └──────────────────┘
         ▲                                     │
         │                                     │
         │       2. NodeSubmissionResponse     │
         └─────────────────────────────────────┘
```

### Integration Points

1. **Type Sharing**: The `icn-wallet-types` crate defines shared structures
2. **Node Submission**: The wallet submits DAG nodes to the runtime
3. **Data Sync**: The wallet synchronizes state from the runtime
4. **Trust Verification**: The wallet verifies trust bundles from the runtime

### Implementation Example

```rust
// In wallet code
async fn submit_proposal(&self, proposal: Proposal) -> Result<ProposalId> {
    // Create DAG node with proposal payload
    let node = DagNode {
        parents: self.get_dag_tips()?,
        payload: serde_json::to_vec(&proposal)?,
        metadata: NodeMetadata {
            node_type: NodeType::Proposal,
            // ...other metadata
        },
        // ...other fields
    };
    
    // Sign the node
    let signed_node = self.identity.sign_node(node)?;
    
    // Submit to runtime
    let response = self.runtime_client.submit_node(signed_node).await?;
    
    // Process response
    if response.accepted {
        Ok(proposal_id_from_response(response))
    } else {
        Err(anyhow!("Proposal rejected: {}", response.rejection_reason))
    }
}
```

## AgoraNet ↔ Runtime Integration

### Data Flow Diagram

```
┌──────────────────┐    1. Node       ┌──────────────────┐
│                  │    Updates       │                  │
│    AgoraNet      │◀────────────────▶│     Runtime      │
│                  │                  │                  │
└──────────────────┘    2. Commands   └──────────────────┘
         ▲                                     ▲
         │                                     │
         │                                     │
         │                                     │
         │                                     │
         │                                     │
         │                                     │
┌──────────────────┐    3. DAG        │
│                  │    Nodes         │
│     Wallet       │─────────────────▶│
│                  │                  │
└──────────────────┘                  │
```

### Integration Points

1. **State Synchronization**: AgoraNet retrieves federation state from Runtime
2. **Node Submission**: AgoraNet forwards nodes from Wallet to Runtime
3. **Event Subscription**: AgoraNet receives state change notifications from Runtime
4. **Execution Requests**: AgoraNet requests operation execution from Runtime

### Implementation Example

```rust
// In AgoraNet code
async fn handle_proposal_submission(&self, proposal: web::Json<ProposalSubmission>) -> Result<HttpResponse> {
    // Authenticate user
    let user_id = self.authenticate_request(&req)?;
    
    // Create DAG node
    let node = create_dag_node_from_proposal(proposal.0, user_id)?;
    
    // Submit to runtime
    let result = self.runtime_client.submit_node(node).await?;
    
    // Return response
    match result.status {
        SubmissionStatus::Accepted => {
            Ok(HttpResponse::Created().json(json!({
                "proposal_id": result.node_id,
                "status": "accepted"
            })))
        },
        SubmissionStatus::Rejected => {
            Ok(HttpResponse::BadRequest().json(json!({
                "error": result.rejection_reason
            })))
        }
    }
}
```

## Mesh ↔ Runtime Integration

### Data Flow Diagram

```
┌──────────────────┐    1. Host ABI   ┌──────────────────┐
│                  │    Functions     │                  │
│     Runtime      │◀────────────────▶│      Mesh        │
│                  │                  │                  │
└──────────────────┘    2. Events     └──────────────────┘
         ▲                                     ▲
         │                                     │
         │                                     │
         │                                     │
         │                                     │
         │                                     │
         │                                     │
┌──────────────────┐    3. Task       ┌──────────────────┐
│                  │    Publication   │                  │
│     Wallet       │─────────────────▶│    MeshCLI       │
│                  │                  │                  │
└──────────────────┘                  └──────────────────┘
```

### Integration Points

1. **Host ABI Functions**: Runtime exposes functions for mesh operations
2. **DAG Event System**: Mesh events are stored in the DAG
3. **Policy Governance**: Mesh policy is managed through governance system
4. **Economic System**: Task rewards use the Runtime's economic system

### Host ABI Example

```rust
// In Runtime code (host_abi.rs)
fn host_lock_tokens_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    task_cid_ptr: u32, task_cid_len: u32,
    amount_ptr: u32,
    escrow_cid_out_ptr: u32, escrow_cid_out_max_len: u32,
) -> Result<i32, Trap> {
    // Read inputs from WASM memory
    let task_cid_bytes = safe_read_bytes(&mut caller, task_cid_ptr, task_cid_len)?;
    let amount = safe_read_u64(&mut caller, amount_ptr)?;
    
    // Parse CID
    let task_cid = Cid::read_bytes(Cursor::new(task_cid_bytes))?;
    
    // Execute escrow operation
    let env = caller.data_mut();
    let escrow_cid = env.lock_tokens(&task_cid, amount).await?;
    
    // Write result back to WASM memory
    let escrow_cid_bytes = escrow_cid.to_bytes();
    safe_write_bytes(&mut caller, &escrow_cid_bytes, escrow_cid_out_ptr, escrow_cid_out_max_len)
}
```

## Wallet ↔ Mesh Integration

### Data Flow Diagram

```
┌──────────────────┐    1. Mesh       ┌──────────────────┐
│                  │    Commands      │                  │
│     Wallet       │─────────────────▶│      Mesh        │
│                  │                  │                  │
└──────────────────┘                  └──────────────────┘
         │                                     │
         │                                     │
         │                                     │
         │       2. Task Status Updates        │
         └─────────────────────────────────────┘
```

### Integration Points

1. **CLI Interface**: Wallet provides commands for mesh operations
2. **Task Management**: Wallet can publish and monitor tasks
3. **Policy Management**: Wallet can propose and vote on policy changes
4. **Reputation Tracking**: Wallet can view reputation of mesh participants

### CLI Example

```rust
// In wallet-agent/src/commands/mesh.rs
pub async fn handle_task_publish(
    wasm_cid: String,
    input_cid: String,
    fee: u64,
    verifiers: u32,
    mem_mb: u32,
    cpu_cycles: u32,
    gpu_flops: u32,
    io_mb: u32,
) -> Result<()> {
    // Parse CIDs
    let wasm_cid = Cid::try_from(wasm_cid)?;
    let input_cid = Cid::try_from(input_cid)?;
    
    // Create capability scope
    let capability_scope = mesh_types::CapabilityScope {
        mem_mb,
        cpu_cycles,
        gpu_flops,
        io_mb,
    };
    
    // Publish task
    let task_cid = mesh_types::publish_computation_task(
        &wasm_cid,
        &input_cid,
        fee,
        verifiers,
        capability_scope,
        24, // 24 hour expiry
    ).await?;
    
    println!("Computation task published: {}", task_cid);
    Ok(())
}
```

## Full System Integration Flow

### Governance Example: Mesh Policy Update

This example illustrates how all components interact during a mesh policy update:

1. **Proposal Creation**:
   - User initiates a policy update in the Wallet
   - Wallet creates a DAG node with a MeshPolicyFragment
   - Wallet signs and submits the node via AgoraNet to Runtime

2. **Voting Process**:
   - Other users view the proposal in AgoraNet UI
   - Votes are submitted through Wallet → AgoraNet → Runtime
   - Runtime validates votes and updates federation state

3. **Policy Activation**:
   - When quorum is reached, Runtime executes the policy update
   - Policy activation is recorded in the DAG
   - MeshPolicyActivated event is generated

4. **Mesh Application**:
   - Mesh nodes observe the policy activation event
   - Nodes update their behavior based on new policy
   - Subsequent tasks use new policy parameters

### Computation Example: Task Execution Flow

This example shows how a computation task flows through the system:

1. **Task Creation**:
   - User prepares a WASM module and input data
   - Wallet uploads data and receives CIDs
   - Wallet submits TaskIntent with fee to Mesh

2. **Worker Selection**:
   - Workers submit ComputeOffers
   - Publisher (or automated system) selects worker(s)
   - Selection criteria uses Runtime's identity verification

3. **Task Execution**:
   - Worker executes WASM module with input
   - Worker creates signed ExecutionReceipt
   - Receipt is published to Mesh network

4. **Result Verification**:
   - Verifier nodes re-execute the task
   - Verification results are published
   - Consensus is reached on correct result

5. **Reward Distribution**:
   - Mesh calls Runtime's host_release_tokens function
   - Rewards are distributed according to policy
   - All operations are recorded in the DAG

## Error Handling and Recovery

### Error Propagation

Errors are consistently handled across component boundaries:

1. **Runtime → Wallet**: Runtime errors are converted to wallet errors using the `FromRuntimeError` trait
2. **Runtime → AgoraNet**: Runtime errors are mapped to HTTP status codes
3. **Mesh → Runtime**: Mesh errors are captured in event records with appropriate status

### Recovery Mechanisms

The system provides multiple recovery mechanisms:

1. **Conflict Resolution**:
   - DAG structure allows detecting and resolving conflicts
   - Resolution rules are specific to operation types

2. **Retry Logic**:
   - Components implement exponential backoff for transient errors
   - Persistent operations resume after connection disruptions

3. **State Reconstruction**:
   - DAG anchors enable rebuilding system state
   - Components can sync from last known valid state

## Future Integration Enhancements

Planned improvements to system integration:

1. **Unified Event Bus**:
   - Implement a shared event publication/subscription mechanism
   - Standardize event formats across all components
   - Enable real-time notifications across system boundaries

2. **Cross-Federation Integration**:
   - Enhance support for operations spanning multiple federations
   - Implement federation-to-federation authentication
   - Support cross-federation mesh computation

3. **Enhanced Integration Testing**:
   - Expand test coverage of component boundaries
   - Implement integration test harness
   - Add chaos testing for resilience verification

4. **Consolidated API Layer**:
   - Create a unified API gateway
   - Standardize authentication across components
   - Implement consistent error handling

## Conclusion

The integration of Wallet, Runtime, AgoraNet, and Mesh components creates a powerful system for decentralized governance and computation. By using consistent data structures, well-defined interfaces, and standardized communication patterns, the ICN system achieves both modularity and coherence.

Future development will focus on tightening these integrations while maintaining the clear separation of concerns that allows each component to excel in its domain of responsibility. 