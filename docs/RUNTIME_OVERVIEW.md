# ICN Runtime System Overview

This document provides a high-level overview of the ICN Runtime system, explaining the key components and how they interact to enable CCL-based governance, WASM execution, and secure credential verification.

## Architecture Overview

The ICN Runtime system is composed of several interrelated components that work together to provide a secure, flexible, and decentralized execution environment for federation governance. The following diagram illustrates the high-level architecture:

```
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│               │     │               │     │               │
│  CCL Policy   │────▶│  WASM Module  │────▶│  VM Execution │
│               │     │               │     │               │
└───────────────┘     └───────────────┘     └───────────────┘
                                                    │
                                                    ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│               │     │               │     │               │
│  Verification │◀────│  DAG Anchor   │◀────│  Execution    │
│               │     │               │     │  Receipt      │
└───────────────┘     └───────────────┘     └───────────────┘
```

## Key Components

### 1. CCL (Capability Control Language)

CCL is a domain-specific language designed for expressing governance policies and authorization rules in a human-readable format. It defines:

- **Schemas**: Data structures for representing governance objects like proposals
- **Rules**: Conditions and actions that govern how resources are accessed
- **Authorizations**: Permissions granted based on rule conditions

```ccl
schema ProposalSchema {
    title: String,
    description: String,
    action: String
}

rule AddMemberRule {
    description: "Rule for adding new members to the federation"
    when:
        proposal oftype ProposalSchema
        with proposal.action == "add_member"
    then:
        authorize(invoker, "federation:add_member")
}
```

### 2. CCL Compiler

The CCL compiler (`icn-ccl-compiler`) transforms CCL code into WebAssembly (WASM) modules that can be executed by the runtime:

1. Parses CCL code and validates its structure
2. Converts rules and schemas into an intermediate representation
3. Generates WASM code that implements the rules' logic
4. Applies optimizations to the WASM code
5. Outputs a binary WASM module ready for execution

### 3. Core VM

The Core VM (`icn-core-vm`) is responsible for executing WASM modules securely:

- Loads WASM modules into a sandboxed environment
- Provides host functions for the WASM module to interact with the runtime
- Handles execution context, including identity information and resource access
- Returns execution results and derived authorizations

### 4. Execution Receipts

After a WASM module is executed, the runtime generates an Execution Receipt that captures:

- The identity of the invoker and subject
- The WASM module that was executed (reference by CID)
- The input data provided to the execution
- The execution result
- The derived authorizations
- A timestamp and other metadata

This receipt serves as a verifiable record of the execution and its outcomes.

### 5. DAG Storage

The Directed Acyclic Graph (DAG) storage system (`icn-dag`) provides:

- Content-addressable storage for WASM modules, execution receipts, and other data
- Immutable history of all governance actions and decisions
- Verifiable references between related objects (e.g., proposals and their execution receipts)
- Efficient retrieval and verification of historical data

### 6. Verifier Runtime

The Verifier Runtime (`icn-verifier-runtime`) validates execution receipts to ensure they represent authorized and correctly executed operations:

1. Retrieves the WASM module referenced in the receipt
2. Re-executes the module with the original input
3. Compares the re-execution results with those in the receipt
4. Verifies the cryptographic signatures and identity information
5. Confirms that the derived authorizations match the expected pattern

## Execution Flow

### 1. Policy Creation and Compilation

Federation administrators define governance policies using CCL and compile them to WASM:

```rust
let ccl_code = r#"
    schema ProposalSchema { /* ... */ }
    rule AddMemberRule { /* ... */ }
"#;

let wasm_binary = compile(ccl_code, &compilation_options)?;
```

### 2. Proposal Submission

Users submit proposals that will be evaluated against the governance policies:

```rust
let proposal_data = serde_json::json!({
    "title": "Add New Member",
    "description": "Add user1 to the federation",
    "action": "add_member",
    "member_id": "did:icn:user1"
});

let proposal_id = governance_kernel.create_proposal(
    &user_id.to_string(),
    proposal_data.to_string()
).await?;
```

### 3. Proposal Execution

When a proposal is ready to be executed (e.g., after voting):

```rust
// Create VM context with identity information
let identity_context = IdentityContext {
    invoker: federation_id.to_string(),
    subject: Some(target_user_id.to_string()),
    federation: Some(federation_id.to_string()),
};

let vm_context = VMContext::new(storage.clone(), identity_context);

// Load and execute the WASM module
let wasm_module = storage.get_binary(&wasm_cid).await?;
let execution_result = vm_context.execute_wasm(&wasm_module, &proposal_data.to_string()).await?;
```

### 4. Authorization Derivation

The system extracts authorizations from the execution result:

```rust
let authorizations = derive_authorizations(&execution_result)?;

// Example authorization
// ResourceAuthorization { 
//     identity_id: "did:icn:federation", 
//     resource: "federation:add_member",
//     constraints: {...} 
// }
```

### 5. Receipt Generation and Anchoring

An execution receipt is created and anchored in the DAG:

```rust
let execution_receipt = ExecutionReceipt::new(
    ExecutionReceiptSubject::Proposal(proposal_id),
    wasm_cid,
    proposal_data.to_string(),
    execution_result,
    authorizations,
    federation_id.to_string(),
);

// Create a verifiable credential from the receipt
let credential = VerifiableCredential::from_execution_receipt(
    execution_receipt,
    &federation_keypair,
    None,
);

// Anchor in the DAG
let credential_json = serde_json::to_string(&credential)?;
let dag_node = DagNode {
    node_type: DagNodeType::ExecutionReceipt,
    content: credential_json.into_bytes(),
    // ...
};

let node_id = dag_manager.add_node(dag_node).await?;
```

### 6. Verification

Later, the execution receipt can be verified:

```rust
let verification_result = verifier.verify_credential(&credential).await?;
```

## Security Considerations

The ICN Runtime system implements several security measures:

1. **Sandboxed Execution**: WASM modules run in an isolated environment with no direct access to the host system.
2. **Identity Verification**: All operations require valid identity credentials.
3. **Deterministic Execution**: WASM execution is deterministic, allowing for reliable verification.
4. **Immutable Records**: All governance actions are recorded immutably in the DAG.
5. **Cryptographic Verification**: Execution receipts are cryptographically signed and can be verified by any participant.

## Integration Points

The runtime system integrates with other parts of the ICN ecosystem:

1. **Wallet**: For identity management and credential storage
2. **AgoraNet**: For federation communication and proposal discussion
3. **Federation Registry**: For managing federation membership and policies

## Further Resources

- [Integration Testing Guide](./integration_testing.md)
- [CCL Language Reference](./ccl_reference.md)
- [Runtime API Documentation](../icn-runtime-root/README.md) 