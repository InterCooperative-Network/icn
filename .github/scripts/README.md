# GitHub Scripts

CI enforcement scripts for the ICN repository.

## firewall_denylist.py

**Purpose**: Enforces the Meaning Firewall contract by checking that kernel crates do not depend on domain/app crates.

**Tracking Issue**: [#1007](https://github.com/InterCooperative-Network/icn/issues/1007) (Wave 1 Firewall Contract)

**Documentation**: See `docs/architecture/KERNEL_APP_SEPARATION.md` for the full Firewall Contract specification.

### Usage

```bash
cd /path/to/icn
python3 .github/scripts/firewall_denylist.py
```

### Exit Codes

- `0`: No violations detected (firewall is intact)
- `1`: Violations detected (kernel imports forbidden domain crates)
- `2`: Script error (cargo metadata failed, JSON parsing error, etc.)

### How It Works

1. Runs `cargo metadata --format-version=1` to get dependency graph
2. For each kernel crate (`icn-core`, `icn-kernel-api`, `icn-net`, `icn-gossip`, `icn-store`):
   - Computes transitive dependency closure
   - Checks if any denylisted crates appear in the closure
3. Reports violations with clear remediation guidance

### Kernel Crates (Must Be Pure)

These crates provide enforcement mechanisms and MUST NOT understand domain semantics:

- `icn-core` - Runtime, supervisor, actor lifecycle
- `icn-kernel-api` - Trait definitions, PolicyOracle interface
- `icn-net` - QUIC/TLS transport
- `icn-gossip` - Topic-based replication
- `icn-store` - Generic KV/Log/Blob storage

### Denylisted Crates (Domain/App Logic)

Kernel crates MUST NOT depend on:

- `icn-trust` - Trust graph computation, attestations
- `icn-governance` - Governance rules, proposals, voting
- `icn-ledger` - Mutual credit ledger internals
- `icn-ccl` - Contract language interpreter
- `icn-compute` - Distributed compute coordination
- `icn-entity` - Entity management
- `icn-community` - Community structures
- `icn-federation` - Inter-cooperative coordination
- `icn-steward` - SDIS steward network
- `icn-coop` - Cooperative lifecycle

### Testing the Enforcement

**Test 1: Verify script detects existing violations**

```bash
python3 .github/scripts/firewall_denylist.py
# Expected: Exit code 1, lists 10 violations from icn-core
```

**Test 2: Simulate a clean state (for future)**

Temporarily edit the script to use an empty `KERNEL_CRATES` list:

```python
KERNEL_CRATES = []  # Simulate no kernel crates to check
```

Run the script:

```bash
python3 .github/scripts/firewall_denylist.py
# Expected: Exit code 0, "No firewall violations detected!"
```

**Test 3: Add a new violation (demonstration)**

To verify the script catches new violations:

1. Add a denylisted dependency to a kernel crate's `Cargo.toml`:
   ```toml
   # In icn/crates/icn-kernel-api/Cargo.toml
   [dependencies]
   icn-trust = { path = "../icn-trust" }  # VIOLATION
   ```

2. Run the script:
   ```bash
   python3 .github/scripts/firewall_denylist.py
   # Expected: Exit code 1, reports icn-kernel-api -> icn-trust
   ```

3. Revert the change:
   ```bash
   git checkout icn/crates/icn-kernel-api/Cargo.toml
   ```

### CI Integration

The script runs in the `firewall-contract` job in `.github/workflows/ci.yml`:

```yaml
firewall-contract:
  name: Firewall Contract Enforcement
  runs-on: ubuntu-latest
  continue-on-error: true  # Non-blocking until migrations complete
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
    - name: Run firewall denylist check
      run: python3 .github/scripts/firewall_denylist.py
```

**Current Status**: Non-blocking (continue-on-error: true) until Waves 2-6 complete.

**After Migrations**: Remove `continue-on-error: true` to make violations fail CI.

### Expected Violations (As of Wave 1)

The script currently detects 10 violations, all from `icn-core`:

- icn-core -> icn-ccl
- icn-core -> icn-community
- icn-core -> icn-compute
- icn-core -> icn-coop
- icn-core -> icn-entity
- icn-core -> icn-federation
- icn-core -> icn-governance
- icn-core -> icn-ledger
- icn-core -> icn-steward
- icn-core -> icn-trust

These will be resolved in **Waves 2-6** by:
1. Migrating hardcoded policy values to PolicyOracle
2. Replacing direct domain type usage with ConstraintSet
3. Moving domain-specific initialization out of supervisor
4. Using ServiceRegistry pattern for loose coupling

### Remediation Patterns

When the script reports a violation like `icn-core -> icn-trust`:

**❌ Before (Violation)**:
```rust
// In icn-core/src/supervisor/replication.rs
use icn_trust::TrustGraph;

fn select_replicas(&self) -> Vec<Did> {
    self.peers.iter()
        .filter(|peer| {
            let score = self.trust_graph.compute_trust_score(peer);
            score >= 0.4  // Kernel "knows" what 0.4 means
        })
        .collect()
}
```

**✅ After (Compliant)**:
```rust
// In icn-core/src/supervisor/replication.rs
// No icn_trust import!

fn select_replicas(&self, constraints: &ConstraintSet) -> Vec<Did> {
    let min_score = constraints.custom.get("min_replication_score")
        .and_then(|v| v.as_float())
        .unwrap_or(0.0);
    
    self.peers.iter()
        .filter(|peer| {
            let score = self.trust_service.trust_score(peer);
            score >= min_score  // Kernel enforces threshold, doesn't know WHY
        })
        .collect()
}
```

See `docs/architecture/KERNEL_APP_SEPARATION.md` for comprehensive migration examples.

### Maintenance

**When adding a new kernel crate**: Add to `KERNEL_CRATES` list in the script.

**When adding a new domain/app crate**: Add to `DENYLISTED_CRATES` list in the script.

**When all migrations complete**:
1. Verify script exits with code 0
2. Remove `continue-on-error: true` from CI workflow
3. Update this README to remove "Expected Violations" section
