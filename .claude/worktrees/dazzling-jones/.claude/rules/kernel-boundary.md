---
paths:
  - "icn/crates/icn-gossip/**"
  - "icn/crates/icn-net/**"
  - "icn/crates/icn-ledger/**"
  - "icn/crates/icn-core/**"
  - "icn/crates/icn-kernel-api/**"
  - "icn/crates/icn-store/**"
---

# Kernel Crate Boundary Rules

These crates are KERNEL crates. The meaning firewall applies strictly.

## Forbidden

- **NEVER** import domain crates: `icn-trust`, `icn-governance`, `icn-ccl`, `icn-coop`, `icn-community`
- **NEVER** reference domain types: `TrustClass`, `TrustGraph`, `GovernanceRole`, `MembershipTier`
- **NEVER** hardcode domain thresholds: `if score >= 0.7`, `if tier == "premium"`
- **NEVER** reconstruct domain semantics from `ConstraintSet` fields
- **NEVER** create bridge/adapter layers between constraints and domain types

## Required

- Use `PolicyOracle::evaluate()` for authorization decisions
- Use `ConstraintSet` fields directly (not mapped to domain enums)
- Use `ErrCode` for protocol-level rejections
- Test with `AllowAllOracle`/`DenyAllOracle`/custom test oracles

## Correct Pattern

```rust
let decision = oracle.evaluate(&request);
let value = decision.constraints()
    .and_then(|c| c.specific_constraint)
    .unwrap_or(sensible_default);
```

## Quick Verification

```bash
# Should produce NO output:
grep -rn 'use icn_trust::' icn/crates/icn-{net,gateway,gossip,ledger,core}/src/
grep -rn 'TrustClass\|TrustGraph' icn/crates/icn-{gossip,net,gateway,ledger,core}/src/
```
