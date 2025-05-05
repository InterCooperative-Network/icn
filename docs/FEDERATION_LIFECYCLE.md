# Federation Lifecycle in the Intercooperative Network

This document outlines the federation lifecycle management in the Intercooperative Network (ICN), focusing on federation merges and splits, which enable flexible governance structures while maintaining cryptographic verifiability and trust continuity.

## Table of Contents

1. [Overview](#overview)
2. [Federation Merge Process](#federation-merge-process)
3. [Federation Split Process](#federation-split-process)
4. [Trust Continuity](#trust-continuity)
5. [DAG-Based Lineage](#dag-based-lineage)
6. [Economic Continuity](#economic-continuity)
7. [Challenge Windows](#challenge-windows)
8. [Sequence Diagrams](#sequence-diagrams)
9. [Error Recovery](#error-recovery)

## Overview

Federation lifecycle management in ICN allows for the evolution of governance structures through two primary operations:

1. **Federation Merges**: Combining two existing federations into a new one
2. **Federation Splits**: Dividing an existing federation into two new ones

Both operations preserve the history, lineage, and trust relationships established in the source federations, ensuring that credentials and governance actions remain valid across federation transitions.

## Federation Merge Process

When two federations decide to merge, they follow this process:

### Phase 1: Preparation

1. Each federation creates and approves a merge proposal through their respective governance processes
2. The proposal includes:
   - Source federation DIDs
   - Metadata for the new federation (name, description, etc.)
   - Quorum configuration for the new federation
   - Challenge window duration

### Phase 2: Trust Mapping

1. A trust mapping is created between the federations, defining:
   - DID mappings between source and target federations
   - Role assignments in the new federation
   - Credential validation rules across federations

### Phase 3: Bundle Creation

1. A `PreMergeBundle` is assembled containing:
   - DAG roots of both source federations
   - Metadata for the new federation
   - Lineage attestation documenting the relationship
   - Cryptographic proofs of approval from both federations

### Phase 4: Execution

1. The merge is executed by a quorum of authorized signers
2. A new federation DID is created if needed
3. A Genesis DAG node is created for the new federation
4. Lineage attestation is anchored in the DAG
5. A MergeBridge node is created linking the federations
6. Execution receipts are issued to all signers

### Phase 5: Challenge Window

1. The merge enters a challenge window where it can be contested
2. If challenges are raised, they must be resolved through governance
3. If no challenges are raised after the window expires, the merge is finalized

### Phase 6: Finalization

1. Once finalized, the new federation becomes fully operational
2. The source federations may be archived or continue as components of the new one

## Federation Split Process

When a federation decides to split, it follows this process:

### Phase 1: Preparation

1. The parent federation creates and approves a split proposal
2. The proposal includes:
   - Parent federation DID
   - Partition map CID defining how members and resources are divided
   - Quorum configuration for the resulting federations
   - Challenge window duration

### Phase 2: Partition Mapping

1. A partition map is created detailing:
   - Member assignments to each resulting federation
   - Resource allocations across federations
   - Ledger balances for each federation

### Phase 3: Bundle Creation

1. A `SplitBundle` is assembled containing:
   - DAG root of the parent federation
   - Partition map
   - Lineage attestation documenting the relationship
   - Cryptographic proof of approval from the parent federation

### Phase 4: Execution

1. The split is executed by a quorum of authorized signers
2. New federation DIDs are created if needed
3. Genesis DAG nodes are created for both new federations
4. Lineage attestation is anchored in the DAG
5. SplitBridge nodes are created linking the federations
6. Execution receipts are issued to all signers

### Phase 5: Challenge Window

1. The split enters a challenge window where it can be contested
2. If challenges are raised, they must be resolved through governance
3. If no challenges are raised after the window expires, the split is finalized

### Phase 6: Finalization

1. Once finalized, both new federations become fully operational
2. The parent federation may be archived

## Trust Continuity

A critical aspect of federation lifecycle management is maintaining trust continuity:

1. **Lineage Attestations**: Cryptographically signed records documenting federation relationships
2. **Trust Bridges**: DAG nodes linking federations for verifiable proof paths
3. **Credential Continuity**: Rules for validating credentials across federation transitions
4. **Historical Verifiability**: Ability to verify past operations through federation transitions

This ensures that credentials issued by source federations remain valid and verifiable in the resulting federations, preserving the integrity of the governance system.

## DAG-Based Lineage

The ICN uses a Directed Acyclic Graph (DAG) to represent federation lineage:

```
┌─────────────┐     ┌─────────────┐
│ Federation  │     │ Federation  │
│    Alpha    │     │    Beta     │
└──────┬──────┘     └──────┬──────┘
       │                   │
       └───────┬───────────┘
               ▼
       ┌───────────────────┐
       │    Federation     │
       │      Gamma        │
       └──────────┬────────┘
                  │
     ┌────────────┴────────────┐
     │                         │
     ▼                         ▼
┌─────────────┐         ┌─────────────┐
│ Federation  │         │ Federation  │
│    Delta    │         │   Epsilon   │
└─────────────┘         └─────────────┘
```

Each arrow represents a lineage attestation with:
- Parent federation(s)
- Child federation(s)
- Attestation type (merge or split)
- Quorum proof of approval
- Timestamp and metadata

## Economic Continuity

Federation lifecycle operations must maintain economic balance:

1. **Merge Operations**: The union of ledgers must preserve account balances
2. **Split Operations**: The sum of balances in resulting ledgers must equal the original
3. **Resource Allocation**: Resources must be fully accounted for across federation transitions
4. **Balance Verification**: Economic consistency checks are performed during execution

## Challenge Windows

Challenge windows provide a safety mechanism for federation lifecycle operations:

1. A time period after execution during which the operation can be contested
2. Challenges can be raised through governance mechanisms
3. If valid challenges are raised, the operation can be rolled back or modified
4. Challenge windows help prevent malicious or unauthorized federation changes

The duration of challenge windows is specified in the proposal and can vary based on the scope and impact of the operation.

## Sequence Diagrams

### Federation Merge

```
┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐
│ Federation │    │ Federation │    │            │    │ Federation │
│    Alpha   │    │    Beta    │    │  Executor  │    │   Gamma    │
└──────┬─────┘    └──────┬─────┘    └──────┬─────┘    └──────┬─────┘
       │                 │                 │                 │
       │ Create Proposal │                 │                 │
       ├────────────────►│                 │                 │
       │                 │                 │                 │
       │ Approve Proposal│                 │                 │
       ├─────────────────┼────────────────►│                 │
       │                 │                 │                 │
       │                 │ Approve Proposal│                 │
       │                 ├────────────────►│                 │
       │                 │                 │                 │
       │                 │                 │ Execute Merge   │
       │                 │                 ├────────────────►│
       │                 │                 │                 │
       │                 │                 │ Create Genesis  │
       │                 │                 │ Create Lineage  │
       │                 │                 │ Create Bridge   │
       │                 │                 │                 │
       │                 │                 │ Challenge Window│
       │                 │                 │                 │
       │                 │                 │                 │
       │                 │                 │   Finalization  │
       │                 │                 │                 │
       │                 │                 │                 │
┌──────┴─────┐    ┌──────┴─────┐    ┌──────┴─────┐    ┌──────┴─────┐
│ Federation │    │ Federation │    │            │    │ Federation │
│    Alpha   │    │    Beta    │    │  Executor  │    │   Gamma    │
└────────────┘    └────────────┘    └────────────┘    └────────────┘
```

### Federation Split

```
┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐
│ Federation │    │            │    │ Federation │    │ Federation │
│   Gamma    │    │  Executor  │    │   Delta    │    │  Epsilon   │
└──────┬─────┘    └──────┬─────┘    └──────┬─────┘    └──────┬─────┘
       │                 │                 │                 │
       │ Create Proposal │                 │                 │
       ├────────────────►│                 │                 │
       │                 │                 │                 │
       │ Create Partition│                 │                 │
       │      Map        │                 │                 │
       ├────────────────►│                 │                 │
       │                 │                 │                 │
       │ Approve Split   │                 │                 │
       ├────────────────►│                 │                 │
       │                 │                 │                 │
       │                 │ Execute Split   │                 │
       │                 ├────────────────►│                 │
       │                 ├─────────────────┼────────────────►│
       │                 │                 │                 │
       │                 │ Create Genesis  │ Create Genesis  │
       │                 │ Create Lineage  │ Create Lineage  │
       │                 │ Create Bridge   │ Create Bridge   │
       │                 │                 │                 │
       │                 │ Challenge Window│                 │
       │                 │                 │                 │
       │                 │                 │                 │
       │                 │   Finalization  │                 │
       │                 │                 │                 │
       │                 │                 │                 │
┌──────┴─────┐    ┌──────┴─────┐    ┌──────┴─────┐    ┌──────┴─────┐
│ Federation │    │            │    │ Federation │    │ Federation │
│   Gamma    │    │  Executor  │    │   Delta    │    │  Epsilon   │
└────────────┘    └────────────┘    └────────────┘    └────────────┘
```

## Error Recovery

Federation lifecycle operations include robust error recovery mechanisms:

1. **Pre-execution Validation**: Extensive validation before execution
2. **Transaction Atomicity**: Operations are atomic (all-or-nothing)
3. **Rollback Capability**: Ability to roll back failed operations
4. **Challenge Resolution**: Process for resolving challenges during the window
5. **Audit Trails**: Comprehensive logging and receipts for debugging

In case of issues, operations can be:
- Retried with corrections
- Rolled back through governance actions
- Manually reconciled by federation administrators 