# Mesh Escrow

The `mesh-escrow` crate provides secure token escrow functionality for the ICN Mesh Compute overlay. It handles locking, releasing, and refunding tokens based on the results of distributed computation tasks.

## Features

- **Escrow Contract Management**: Create and manage escrow contracts for compute tasks
- **Secure Token Operations**: Lock, release, and refund tokens with proper authorization
- **DAG Integration**: All escrow events are anchored to the ICN DAG for transparency and auditability
- **Verification-based Release**: Token release requires verification quorum
- **Fair Distribution**: Proportional reward distribution between workers and verifiers

## Usage

```rust
use mesh_escrow::{claim_reward, refund};
use cid::Cid;

// After successful task verification with quorum
async fn handle_successful_execution(escrow_cid: &Cid, worker_did: &str, reward_amount: u64) {
    if let Err(e) = claim_reward(escrow_cid, worker_did, reward_amount).await {
        eprintln!("Failed to claim reward: {}", e);
    }
}

// If verification fails
async fn handle_failed_execution(escrow_cid: &Cid) {
    if let Err(e) = refund(escrow_cid).await {
        eprintln!("Failed to refund tokens: {}", e);
    }
}
```

## CCL Contract

The escrow functionality is implemented using a CCL contract. Here's the template:

```ccl
contract compute_escrow(cid escrow_cid, TokenAmount total_reward) {
    action lock_tokens { host_lock_tokens(escrow_cid, total_reward) }
    action release(worker_did, TokenAmount reward) { host_release_tokens(escrow_cid, worker_did, reward) }
    action refund() { host_refund_tokens(escrow_cid) }
}
```

See `policies/compute_escrow.ccl` for the full implementation.

## Integration with Mesh Compute

The escrow crate integrates with the broader Mesh Compute system:

1. When a `ParticipationIntent` is created, an escrow contract is deployed
2. The escrow CID is included in the intent broadcast to worker nodes
3. Upon successful execution and verification, the escrow releases tokens to workers and verifiers
4. If execution is rejected by verifiers, tokens are refunded to the original publisher

## License

Same license as the ICN project 