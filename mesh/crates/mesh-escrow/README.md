# Mesh Escrow

This crate implements the Escrow & Reward-Payout system for the ICN Mesh Compute overlay.

## Overview

The Mesh Escrow system handles token management for mesh compute tasks:

1. **Token Locking**: Locks tokens in escrow when a task is published
2. **Contract Management**: Manages the lifecycle of escrow contracts
3. **Reward Distribution**: Distributes rewards to workers and verifiers
4. **Dispute Resolution**: Handles disputes over task execution

## Core Components

- **EscrowContract**: Smart contract controlling token escrow for task execution
- **TokenLock**: Represents tokens locked in escrow
- **RewardDistribution**: Manages reward distribution to workers and verifiers
- **PaymentInterface**: Interface for token payment systems

## Integration

Mesh Escrow integrates with other components of the ICN system:

- **Mesh Types**: Uses task intents, execution receipts, and verification receipts
- **Mesh Reputation**: Uses reputation scores for weighted reward distribution
- **ICN DAG**: Anchors escrow events and transactions to the DAG

## Usage Example

Here's a simple example of creating an escrow contract:

```rust
// Create reputation system
let reputation = Arc::new(ReputationSystem::new(policy));

// Create escrow system
let escrow = EscrowSystem::new(reputation.clone());

// Create reward settings
let reward_settings = RewardSettings {
    worker_percentage: 70,
    verifier_percentage: 20,
    platform_fee_percentage: 10,
    use_reputation_weighting: true,
    platform_fee_address: "did:icn:platform".to_string(),
};

// Create escrow contract for a task
let contract = escrow.create_contract(&task, reward_settings).await?;

// Lock tokens in escrow
let amount = TokenAmount::new(task.fee, 6);
escrow.lock_tokens(&contract.id, amount).await?;

// Process task execution receipt
escrow.process_execution(&contract.id, &execution_receipt).await?;

// Process verification receipt
escrow.process_verification(&contract.id, &verification_receipt).await?;
```

## Additional Features

- **Reputation-Weighted Rewards**: Rewards can be distributed based on verifier reputation
- **Token Locks**: Tokens are locked in escrow until task completion or failure
- **Dispute Handling**: Provides mechanisms for creating and resolving disputes
- **DAG Anchoring**: All key events are anchored to the ICN DAG

## License

Licensed under MIT or Apache-2.0, at your option. 