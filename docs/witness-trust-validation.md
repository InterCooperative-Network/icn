# Trust-Graph Integration for Witness Validation

## Overview

This document describes the trust-graph integration for witness validation in ICN's ledger system, implemented to address [Issue #826](https://github.com/InterCooperative-Network/icn/issues/826).

## Background

PR #826 added witness signatures for `HandoffProcedure` completion and high-value transactions. Initially, any DID in the `witnesses` list could attest to completion. This enhancement integrates ICN's trust graph to add trust-based requirements for witnesses, preventing collusion with unknown or untrusted entities.

## Features

### 1. Minimum Trust Score Requirements

Witnesses must have a minimum trust score with all parties involved in the transaction (author and counterparties). This is configured via the `min_witness_trust` parameter in `WitnessConfig`:

```rust
use icn_ledger::WitnessConfig;

// Require witnesses to have at least partner-level trust (0.4)
let config = WitnessConfig::counterparty_with_trust(
    threshold_value,  // Value above which witnesses are required
    0.4              // Minimum trust score (Partner trust class)
);

ledger.set_witness_config(config);
```

### 2. Trust Score Thresholds

Trust scores are in the range [0.0, 1.0] and correspond to ICN's trust classes:

- **0.0 - 0.1**: Isolated (not yet evaluated)
- **0.1 - 0.4**: Known (known but not trusted)
- **0.4 - 0.7**: Partner (trusted partner) ← Recommended minimum
- **0.7 - 1.0**: Federated (federated peer)

### 3. Quorum with Trust Requirements

For quorum-based witness policies, trust requirements apply to all designated witnesses:

```rust
let witnesses = vec![did1, did2, did3];
let config = WitnessConfig::quorum_with_trust(
    2,          // Require 2 signatures
    witnesses,  // From these designated witnesses
    0.4        // Each must have minimum trust score of 0.4
);
```

## API Reference

### WitnessConfig Methods

```rust
// Existing methods (backward compatible)
pub fn counterparty_above(threshold: u64) -> Self
pub fn quorum(required: u32, witnesses: Vec<Did>) -> Self

// New methods with trust requirements
pub fn counterparty_with_trust(threshold: u64, min_trust: f64) -> Self
pub fn quorum_with_trust(required: u32, witnesses: Vec<Did>, min_trust: f64) -> Self
```

### WitnessConfig Fields

```rust
pub struct WitnessConfig {
    pub default_policy: WitnessPolicy,
    pub threshold: Option<u64>,
    pub collection_timeout_secs: u64,
    
    // New field (optional, backward compatible)
    pub min_witness_trust: Option<f64>,
}
```

## Implementation Details

### Trust Validation Flow

1. **Signature Validation**: Ed25519 signature verification (existing)
2. **Timestamp Validation**: Clock skew and expiration checks (existing)
3. **Trust Validation** (new):
   - Extract all transaction parties (author + counterparties)
   - For each witness, compute trust score from each party's perspective
   - Reject if witness has insufficient trust with any party
   - Skip if trust graph not available (logs warning)

### Trust Score Computation

Trust scores are computed from the trust graph owner's perspective using the combined score from all three trust dimensions:

- **Social**: 60% direct, 40% transitive
- **Economic**: 80% direct, 20% transitive  
- **Technical**: 90% direct, 10% transitive

The combined score uses weights: 50% social + 30% economic + 20% technical.

### Async Implementation

Trust validation is implemented as an async operation to support non-blocking trust graph lookups:

```rust
async fn validate_witness_signatures(
    ledger: &Ledger,
    witnessed: &WitnessedEntry,
) -> Result<()>
```

This requires all callers to await the validation:

```rust
ledger.validate_witness_signatures(&witnessed).await?;
```

## Backward Compatibility

Trust validation is **fully backward compatible**:

1. **Default Behavior**: `min_witness_trust` defaults to `None`, disabling trust validation
2. **Existing Configs**: All existing `WitnessConfig` instances continue to work unchanged
3. **Opt-in**: Trust requirements must be explicitly configured
4. **Graceful Degradation**: If trust graph not available, validation is skipped with a warning

## Usage Examples

### Example 1: Basic Counterparty with Trust

```rust
use icn_ledger::{Ledger, WitnessConfig};

let mut ledger = Ledger::new(store)?;

// Set trust graph (required for trust validation)
ledger.set_trust_graph(trust_graph);

// Require counterparty signature for entries >= 100 hours
// Counterparty must have partner-level trust (0.4)
let config = WitnessConfig::counterparty_with_trust(100, 0.4);
ledger.set_witness_config(config);
```

### Example 2: Quorum for High-Value Transfers

```rust
// Require 2-of-3 designated witnesses for high-value transfers
// Each witness must have minimum trust of 0.5
let witnesses = vec![witness1_did, witness2_did, witness3_did];
let config = WitnessConfig::quorum_with_trust(2, witnesses, 0.5);
ledger.set_witness_config(config);
```

### Example 3: Backward Compatible (No Trust Validation)

```rust
// Existing code continues to work - no trust validation
let config = WitnessConfig::counterparty_above(100);
ledger.set_witness_config(config);
```

## Testing

Comprehensive test coverage is provided in `icn/crates/icn-ledger/tests/witness_trust.rs`:

- **test_witness_trust_validation_sufficient_trust**: Witnesses with sufficient trust accepted
- **test_witness_trust_validation_insufficient_trust**: Witnesses with insufficient trust rejected
- **test_witness_trust_validation_unknown_witness**: Unknown witnesses (trust = 0.0) rejected
- **test_witness_trust_validation_backward_compatible**: Existing configs work unchanged
- **test_witness_trust_validation_no_trust_graph**: Validation skipped if trust graph unavailable
- **test_witness_trust_validation_quorum_with_trust**: Quorum with trust requirements
- **test_witness_trust_validation_quorum_insufficient_trusted_witnesses**: Quorum rejects insufficient trust

Run tests with:
```bash
cd icn
cargo test -p icn-ledger witness_trust
```

## Security Considerations

### Benefits

1. **Prevents Sybil Attacks**: Unknown witnesses cannot attest to transactions
2. **Prevents Collusion**: Witnesses must have established trust with all parties
3. **Community-Specific**: Each cooperative can set its own trust thresholds
4. **Defense in Depth**: Adds trust layer on top of cryptographic signatures

### Limitations

1. **Trust Graph Required**: Validation only works if trust graph is configured
2. **Cold Start Problem**: New members may not have sufficient trust edges initially
3. **Trust Computation**: Current implementation uses trust graph owner's perspective

### Best Practices

1. **Set Appropriate Thresholds**: Use 0.4 (Partner) for most high-value transactions
2. **Monitor Trust Edges**: Ensure critical witnesses maintain trust relationships
3. **Grace Periods**: Allow time for witnesses to build trust in new cooperatives
4. **Combine with Value Thresholds**: Only require trusted witnesses above certain values

## Future Enhancements

Potential improvements for future iterations:

1. **Per-Party Trust Lookups**: Compute trust from each transaction party's perspective
2. **Trust Caching**: Cache trust scores to reduce graph traversal overhead
3. **Governance Integration**: Allow cooperatives to set default witness policies via governance
4. **Trust Decay Handling**: Handle trust score changes over time
5. **Multi-Graph Support**: Use different trust dimensions for different witness requirements

## Related Documentation

- [Trust Graph Architecture](trust-multi-graph-migration.md)
- [Governance Integration](dev-journal/2025-01-17-governance-ledger-integration.md)
- [Trust Threshold Configuration](trust-threshold-configuration.md)
- [Production Hardening](production-hardening.md)

## References

- Issue: [feat(ledger): Add trust-graph integration for witness validation](https://github.com/InterCooperative-Network/icn/issues/XXX)
- PR: [feat(ledger): Add witness signatures for HandoffProcedure completion](https://github.com/InterCooperative-Network/icn/pull/826)
