---
name: firewall-check
description: Check for meaning firewall violations - kernel crates must never import domain crates
user-invocable: true
allowed-tools: "Bash, Grep, Glob, Read"
---

Check for meaning firewall violations in the ICN codebase.

## What is the Meaning Firewall?

The kernel enforces constraints WITHOUT understanding their semantic origin. Domain semantics (trust scores, governance rules) stay in apps. Kernel only sees generic `ConstraintSet` and `PolicyDecision`.

## Checks to Run

### 1. Forbidden imports in kernel crates

Kernel crates (`icn-net`, `icn-gateway`, `icn-gossip`, `icn-ledger`, `icn-core`) must NEVER import domain crates.

Search for violations:
```
grep -rn 'use icn_trust::' icn/crates/icn-{net,gateway,gossip,ledger,core}/src/
grep -rn 'use icn_governance::' icn/crates/icn-{net,gateway,gossip,ledger,core}/src/
grep -rn 'use icn_ccl::' icn/crates/icn-{net,gateway,gossip,ledger,core}/src/
grep -rn 'use icn_coop::' icn/crates/icn-{net,gateway,gossip,ledger,core}/src/
grep -rn 'use icn_community::' icn/crates/icn-{net,gateway,gossip,ledger,core}/src/
```

### 2. Domain types in kernel structs

Search for domain type references in kernel code:
```
grep -rn 'TrustClass\|TrustGraph\|GovernanceRole\|MembershipTier' icn/crates/icn-{gossip,net,gateway,ledger,core}/src/
```

### 3. Hardcoded domain thresholds

Search for trust score thresholds in kernel code:
```
grep -rn '0\.7\|0\.4\|0\.1' icn/crates/icn-{gossip,net,gateway,ledger}/src/ | grep -i 'trust\|score\|threshold'
```

### 4. Reverse firewall patterns

Search for constraint-to-domain reconstruction:
```
grep -rn 'match.*constraints\|match.*max_topics\|match.*rate_limit' icn/crates/icn-{gossip,net,gateway,ledger}/src/ | grep -i 'class\|tier\|level'
```

### 5. Cargo.toml dependency check

Verify kernel crate Cargo.toml files don't depend on domain crates:
```
grep -l 'icn-trust\|icn-governance\|icn-ccl' icn/crates/icn-{net,gateway,gossip,ledger,core}/Cargo.toml
```

## Output Format

```
## Meaning Firewall Check

### Import violations: <PASS/FAIL>
<details>

### Domain types in kernel: <PASS/FAIL>
<details>

### Hardcoded thresholds: <PASS/FAIL>
<details>

### Reverse firewall patterns: <PASS/FAIL>
<details>

### Cargo.toml dependencies: <PASS/FAIL>
<details>

### Overall: CLEAN / VIOLATIONS FOUND
```

If violations are found, explain exactly what needs to change and why.
