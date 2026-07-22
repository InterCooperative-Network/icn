---
paths:
  - "icn/crates/icn-gossip/**"
  - "icn/crates/icn-net/**"
  - "icn/crates/icn-gateway/**"
  - "icn/crates/icn-ledger/**"
  - "icn/crates/icn-core/**"
  - "icn/crates/icn-kernel-api/**"
  - "icn/crates/icn-store/**"
---

# Kernel Crate Boundary Rules

Kernel crates are `icn-core`, `icn-net`, `icn-gossip`, `icn-store`,
`icn-kernel-api` — the class of record is `scripts/firewall-taxonomy.toml`
(single source of truth; sync-checked in CI). `icn-gateway`/`icn-rpc`/`icn-api`
are API-SHELL class (pinned, shrink-only domain coupling); `icn-ledger` is
DOMAIN class (2026-07-22 reclassification). The meaning firewall applies
strictly to kernel crates.

## Forbidden

- **NEVER** import domain crates (full list in the taxonomy): `icn-trust`, `icn-governance`, `icn-ledger`, `icn-ccl`, `icn-compute`, `icn-entity`, `icn-community`, `icn-federation`, `icn-steward`, `icn-coop`, `icn-commons`, `icn-zkp`
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
# Honest, taxonomy-driven check (fails on violations):
bash scripts/check-meaning-firewall.sh

# Raw grep equivalent — should produce NO output (kernel class only;
# meaning_firewall.rs is excluded because it holds pattern literals):
grep -rn --exclude=meaning_firewall.rs 'use icn_trust::' icn/crates/icn-{net,gossip,store,kernel-api,core}/src/
```

Note: the pre-2026-07-22 version of this block grepped icn-gateway and
icn-ledger as kernel — icn-gateway has 3 real `use icn_trust::` imports
(api-shell class, pinned in `strict_shell_import_violations`), so that
"should produce NO output" claim was failing the whole time.
