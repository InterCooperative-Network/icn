# Sprint 21: Meaning Firewall Policy Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make hardcoded governance constants in `icn-compute` and `icn-security` configurable via node config, eliminating the two HIGH-severity Meaning Firewall violations identified in the Phase 2 audit.

**Architecture:** The kernel crates keep their enforcement logic but stop encoding policy values as compiled constants. Thresholds move to config structs that are loaded from `config.toml` at startup. This is Phase 2 (config-driven) on a path toward Phase 3 (per-coop PolicyOracle-driven dynamic values). No new traits needed in Sprint 21.

**Tech Stack:** Rust 1.88.0, serde/serde_json for config, toml crate already in workspace, `icn-core` config module pattern (see `crates/icn-core/src/config/gateway.rs` for how config sections are structured)

**Sprint 20 Context:** Four open PRs (#1380, #1381, #1382, #1383) need to merge before this sprint starts. Sprint board `ops/state/sprint/current.json` still shows Sprint 19 — update it as the first task.

---

## Background: What We're Fixing

The [Meaning Firewall audit](../architecture/meaning-firewall-audit.md) found two HIGH-severity violations:

**icn-compute** (`crates/icn-compute/src/policy.rs`, `commons_pool.rs`):
- `min_standing: 0.3` — hardcoded trust threshold for commons pool access
- `min_trust_score: 0.1` — hardcoded sybil admission threshold
- `estimate_task_cost()` — fuel-to-credits conversion with hardcoded `1000` divisor

**icn-security** (`crates/icn-security/src/misbehavior.rs`):
- `Violation::severity()` returns hardcoded scores: Critical=10, Major=5, Minor=1
- `StorageFailureReason::severity()`: InvalidMerkleProof=8, DataMismatch=5, NoResponse=1
- `apply_penalty()`: `penalty = severity * 0.05` — hardcoded 5% per severity point

These values encode governance decisions that different cooperatives should be able to set. A cooperative running high-stakes compute should be able to set `min_standing: 0.7`. A development cooperative should be able to set `min_standing: 0.1`. None of that is possible today without a code deploy.

---

## Pre-Sprint: Close Sprint 20

### Task 0: Update sprint board and merge Sprint 20 PRs

**Files:**
- Modify: `ops/state/sprint/current.json`
- Modify: `ops/state/sprint/history/sprint-19.json` (create from current sprint 19 data)

- [ ] **Step 1: Check PR merge status**
```bash
gh pr list --repo InterCooperative-Network/icn --state open \
  --json number,title,statusCheckRollup \
  | python3 -c "import json,sys; [print(p['number'], p['title']) for p in json.load(sys.stdin)]"
```
Expected: PRs #1380, #1381, #1382, #1383 visible

- [ ] **Step 2: Merge Sprint 20 PRs (in order)**
```bash
gh pr merge 1382 --squash --delete-branch --repo InterCooperative-Network/icn
gh pr merge 1383 --squash --delete-branch --repo InterCooperative-Network/icn
gh pr merge 1380 --squash --delete-branch --repo InterCooperative-Network/icn
gh pr merge 1381 --squash --delete-branch --repo InterCooperative-Network/icn
```
Use `--admin` if CI is queue-stalled (Benchmark "Compare Against Base" running >1hr).

- [ ] **Step 3: Archive Sprint 19, activate Sprint 20**

On `main` after merges:
```bash
git checkout main && git pull origin main
cp ops/state/sprint/current.json /tmp/sprint-19-backup.json
```

Confirm `current.json` was updated to Sprint 20 by the `feat/1051` merge. If not, update manually (see Sprint 20 content below).

- [ ] **Step 4: Create Sprint 21 board**

Write `ops/state/sprint/current.json` (see Sprint 21 JSON at end of this doc).

```bash
git add ops/state/sprint/current.json
git commit -m "chore(ops): advance sprint board to Sprint 21"
git push
```

---

## Sprint 21 Scope

**Two tasks. Both are config-extraction changes — no new traits, no interface changes, no PolicyOracle wiring (that's Sprint 22).**

| Task | Crate | What moves | Config section |
|------|-------|------------|----------------|
| s21-t1 | `icn-compute` | `min_standing`, `min_trust_score`, fuel cost divisor | `[compute.policy]` |
| s21-t2 | `icn-security` | Severity weights, penalty rate | `[security.reputation]` |

---

## Task 1: Compute Policy Config Extraction (s21-t1)

**Issue:** #1370 (partial — compute violations)

**Files:**
- Modify: `crates/icn-compute/src/policy.rs` — make `CommonsPoolPolicy` defaults configurable
- Modify: `crates/icn-compute/src/commons_pool.rs` — make `SybilPolicy` defaults configurable
- Create: `crates/icn-core/src/config/compute.rs` — new `ComputePolicyConfig` struct
- Modify: `crates/icn-core/src/config/mod.rs` — add `compute: ComputePolicyConfig` field
- Modify: `crates/icn-core/src/supervisor/lifecycle.rs` — pass config to compute components
- Test: `crates/icn-compute/src/policy.rs` — new config-driven default tests

### Step 1.1: Create branch

- [ ] **Create branch**
```bash
git checkout main && git pull origin main
git checkout -b feat/1370-compute-policy-config
```

### Step 1.2: Write failing test for configurable threshold

The test asserts that `CommonsPoolPolicy` can be constructed with a non-default `min_standing`, and that `check_standing()` uses the configured value, not the hardcoded `0.3`.

- [ ] **Write test in `crates/icn-compute/src/policy.rs`** (in existing `#[cfg(test)]` block):
```rust
#[test]
fn test_commonspool_policy_uses_configured_min_standing() {
    let policy = CommonsPoolPolicy {
        min_standing: 0.7, // cooperative set this, not compiled constant
        ..CommonsPoolPolicy::default()
    };
    assert!(policy.check_standing(0.6).is_err(), "0.6 should fail 0.7 threshold");
    assert!(policy.check_standing(0.8).is_ok(), "0.8 should pass 0.7 threshold");
}

#[test]
fn test_sybil_policy_uses_configured_min_trust() {
    let policy = SybilPolicy { min_trust_score: 0.5 };
    assert!(policy.min_trust_score == 0.5);
    // Try to add participant below threshold
    let mut pool = CommonsPool::with_policy(policy);
    let participant = CommonsParticipant {
        did: "did:icn:test".to_string(),
        trust_score: 0.3, // below 0.5 threshold
        // ... other fields from existing test helpers
    };
    assert!(pool.try_add_participant(participant).is_err());
}
```

- [ ] **Run test to verify it fails (or verify it passes if structs already accept these)**
```bash
cd /home/ubuntu/projects/icn/icn
cargo test -p icn-compute -- test_commonspool_policy test_sybil_policy 2>&1 | tail -5
```

### Step 1.3: Create `ComputePolicyConfig`

- [ ] **Create `crates/icn-core/src/config/compute.rs`**:
```rust
//! Compute subsystem policy configuration
use serde::{Deserialize, Serialize};

/// Policy thresholds for the compute subsystem.
///
/// These values are governance decisions — different cooperatives should be
/// able to set them via config without recompiling. Default values match the
/// original hardcoded constants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputePolicyConfig {
    /// Minimum trust standing required to submit to the commons pool (0.0–1.0).
    /// Original hardcoded value: 0.3
    #[serde(default = "default_min_standing")]
    pub min_standing: f64,

    /// Minimum trust score required for sybil admission to commons pool (0.0–1.0).
    /// Original hardcoded value: 0.1
    #[serde(default = "default_min_trust_score")]
    pub min_trust_score: f64,

    /// Fuel units per credit for cost estimation.
    /// Original hardcoded value: 1000 (1 credit per 1000 fuel units)
    #[serde(default = "default_fuel_cost_divisor")]
    pub fuel_cost_divisor: u64,
}

fn default_min_standing() -> f64 { 0.3 }
fn default_min_trust_score() -> f64 { 0.1 }
fn default_fuel_cost_divisor() -> u64 { 1000 }

impl Default for ComputePolicyConfig {
    fn default() -> Self {
        Self {
            min_standing: default_min_standing(),
            min_trust_score: default_min_trust_score(),
            fuel_cost_divisor: default_fuel_cost_divisor(),
        }
    }
}
```

- [ ] **Run `cargo check -p icn-core`**
```bash
cargo check -p icn-core 2>&1 | tail -5
```
Expected: no errors yet (module not wired in)

### Step 1.4: Wire into `NodeConfig`

- [ ] **Read `crates/icn-core/src/config/mod.rs`** and add `compute` field:
```rust
pub mod compute;
pub use compute::ComputePolicyConfig;

// In NodeConfig struct, add:
pub compute: ComputePolicyConfig,

// In NodeConfig::default():
compute: ComputePolicyConfig::default(),
```

- [ ] **Verify compile**
```bash
cargo check -p icn-core 2>&1 | tail -5
```

### Step 1.5: Use config values in `CommonsPoolPolicy` and `SybilPolicy`

The structs already accept configured values — this step ensures lifecycle.rs passes the config to them instead of relying on hardcoded `Default` values.

- [ ] **Modify `crates/icn-core/src/supervisor/lifecycle.rs`** — find where `CommonsPool` or `CommonsPoolPolicy` is constructed and pass config values. Search:
```bash
grep -n "CommonsPool\|CommonsPoolPolicy\|SybilPolicy" \
  crates/icn-core/src/supervisor/lifecycle.rs
```

If construction is inside `icn-compute`, thread config through via a new `init` function that accepts `ComputePolicyConfig`.

- [ ] **Modify `estimate_task_cost` in `crates/icn-compute/src/policy.rs`** — replace hardcoded `1000`:
```rust
// Before:
fn estimate_task_cost(task: &ComputeTask) -> i64 {
    (task.fuel_limit / 1000) as i64  // 1 credit per 1000 fuel
}

// After — add fuel_cost_divisor to CommonsPoolPolicy:
pub fuel_cost_divisor: u64,  // new field, default 1000

fn estimate_task_cost(&self, task: &ComputeTask) -> i64 {
    (task.fuel_limit / self.fuel_cost_divisor) as i64
}
```
Note: `estimate_task_cost` is currently a static method (`fn estimate_task_cost(task: &ComputeTask)`). Change to `&self` to access `fuel_cost_divisor`.

- [ ] **Run all compute tests**
```bash
cargo test -p icn-compute 2>&1 | tail -10
```
Expected: all passing

### Step 1.6: Commit

- [ ] **Verify fmt and clippy**
```bash
cargo fmt --all --check
cargo clippy -p icn-compute -p icn-core --all-targets -- -D warnings 2>&1 | tail -5
```

- [ ] **Commit**
```bash
cargo fmt --all
git add crates/icn-compute/src/policy.rs \
        crates/icn-compute/src/commons_pool.rs \
        crates/icn-core/src/config/compute.rs \
        crates/icn-core/src/config/mod.rs \
        crates/icn-core/src/supervisor/lifecycle.rs
git commit -m "feat(compute): make admission thresholds config-driven (#1370)

Move min_standing, min_trust_score, and fuel_cost_divisor from hardcoded
constants to ComputePolicyConfig in node config. Default values match the
original constants for backward compatibility.

This is Sprint 21 Phase 2 of the Meaning Firewall audit — making governance
choices configurable via config.toml rather than requiring code deploys.
Phase 3 (per-coop PolicyOracle dynamic values) is future work.

Part of #1370."
```

### Step 1.7: Open PR

```bash
git push -u origin feat/1370-compute-policy-config
gh pr create \
  --title "feat(compute): make admission thresholds config-driven (#1370)" \
  --body "Part of #1370 Meaning Firewall Phase 2. Moves min_standing, min_trust_score, fuel_cost_divisor from hardcoded constants to ComputePolicyConfig. Defaults unchanged."
```

---

## Task 2: Reputation Policy Config Extraction (s21-t2)

**Issue:** #1370 (partial — security violations)

**Files:**
- Modify: `crates/icn-security/src/misbehavior.rs` — add `SeverityWeights` and `penalty_rate` to `MisbehaviorThresholds`
- Create: `crates/icn-core/src/config/security.rs` — `ReputationPolicyConfig`
- Modify: `crates/icn-core/src/config/mod.rs` — add `security: ReputationPolicyConfig`
- Modify: `crates/icn-core/src/supervisor/lifecycle.rs` — pass config to `MisbehaviorDetector`
- Test: `crates/icn-security/src/misbehavior.rs` — configurable severity tests

### Step 2.1: Create branch

- [ ] **Create branch** (after t1 is merged, or in parallel with t1 on a separate branch)
```bash
git checkout main && git pull origin main
git checkout -b feat/1370-reputation-policy-config
```

### Step 2.2: Understand current severity structure

- [ ] **Read the current code**
```bash
sed -n '95,145p' crates/icn-security/src/misbehavior.rs
```
Note: `Violation::severity()` returns `u32` and switches on violation type with hardcoded values. `apply_penalty` uses `severity * 0.05`.

### Step 2.3: Write failing test for configurable severity

- [ ] **Write test** (in `crates/icn-security/src/misbehavior.rs` test block):
```rust
#[test]
fn test_apply_penalty_uses_configurable_rate() {
    let mut score = ReputationScore::new();
    let initial = score.score;

    // Default rate is 0.05 (5% per severity point)
    // Critical violation has severity 10 → default penalty = 0.5
    let violation = Violation::ConsistentlyOffline { missed_rounds: 5 };
    score.apply_penalty(&violation, 0.05);
    let default_penalty = initial - score.score;

    // Now use a different rate
    let mut score2 = ReputationScore::new();
    score2.apply_penalty(&violation, 0.10); // 10% per point
    let custom_penalty = initial - score2.score;

    assert!(
        custom_penalty > default_penalty,
        "higher rate should produce larger penalty"
    );
}

#[test]
fn test_severity_weights_are_configurable() {
    // With default weights: Critical=10, Major=5, Minor=1
    let w = SeverityWeights::default();
    let v = Violation::SybilAttack;
    assert_eq!(v.severity_with_weights(&w), 10); // Critical

    // With custom weights
    let custom = SeverityWeights { critical: 20, major: 10, minor: 2 };
    assert_eq!(v.severity_with_weights(&custom), 20);
}
```

- [ ] **Run to confirm failure (method doesn't exist yet)**
```bash
cargo test -p icn-security -- test_apply_penalty_uses test_severity_weights 2>&1 | tail -5
```

### Step 2.4: Add `SeverityWeights` struct and update `apply_penalty`

- [ ] **Modify `crates/icn-security/src/misbehavior.rs`**:

Add after the existing `Violation` impl block:
```rust
/// Configurable weights for violation severity scoring.
///
/// These are governance decisions — different cooperatives may weigh
/// violations differently based on their risk tolerance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityWeights {
    /// Weight for critical violations (default: 10)
    pub critical: u32,
    /// Weight for major violations (default: 5)
    pub major: u32,
    /// Weight for minor violations (default: 1)
    pub minor: u32,
}

impl Default for SeverityWeights {
    fn default() -> Self {
        Self { critical: 10, major: 5, minor: 1 }
    }
}
```

Add `severity_with_weights` method to `Violation`:
```rust
impl Violation {
    // Keep existing severity() for backward compat (uses default weights)
    pub fn severity(&self) -> u32 {
        self.severity_with_weights(&SeverityWeights::default())
    }

    /// Compute severity using provided weights instead of hardcoded values.
    pub fn severity_with_weights(&self, weights: &SeverityWeights) -> u32 {
        match self {
            // Critical violations
            Violation::SybilAttack | Violation::InvalidBlock { .. }
            | Violation::DoubleSigning { .. } => weights.critical,
            // Major violations
            Violation::InvalidProposal { .. } | Violation::FailedStorageChallenge { .. }
            | Violation::ConsistentlyOffline { .. } => weights.major,
            // Minor violations
            Violation::SlowResponse { .. } | Violation::PartialResponse { .. } => weights.minor,
        }
    }
}
```

**Note:** Check the actual variant names in the existing `severity()` match — map each arm to critical/major/minor, preserving the original mapping.

Update `apply_penalty` to accept `decay_rate` as a parameter (already does!) and use `severity_with_weights`:
```rust
pub fn apply_penalty(&mut self, violation: &Violation, decay_rate: f64) {
    self.apply_penalty_with_weights(violation, decay_rate, &SeverityWeights::default())
}

pub fn apply_penalty_with_weights(
    &mut self,
    violation: &Violation,
    decay_rate: f64,
    weights: &SeverityWeights,
) {
    let severity = violation.severity_with_weights(weights);
    let penalty = severity as f64 * decay_rate; // was 0.05 hardcoded
    self.score = (self.score - penalty).max(0.0);
    self.severity_points += severity;
    // ... existing logging
}
```

### Step 2.5: Create `ReputationPolicyConfig`

- [ ] **Create `crates/icn-core/src/config/security.rs`**:
```rust
//! Security subsystem reputation policy configuration
use serde::{Deserialize, Serialize};

/// Reputation policy parameters for the security subsystem.
///
/// These are governance decisions that cooperatives should control.
/// Default values match the original hardcoded constants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationPolicyConfig {
    /// Penalty rate per severity point (e.g. 0.05 = 5% per point).
    /// Original hardcoded value: 0.05
    #[serde(default = "default_penalty_rate")]
    pub penalty_rate: f64,

    /// Severity weight for critical violations.
    /// Original hardcoded value: 10
    #[serde(default = "default_critical_weight")]
    pub critical_weight: u32,

    /// Severity weight for major violations.
    /// Original hardcoded value: 5
    #[serde(default = "default_major_weight")]
    pub major_weight: u32,

    /// Severity weight for minor violations.
    /// Original hardcoded value: 1
    #[serde(default = "default_minor_weight")]
    pub minor_weight: u32,

    /// Maximum violations per hour before auto-quarantine.
    /// Original hardcoded value: 10
    #[serde(default = "default_max_violations_per_hour")]
    pub max_violations_per_hour: usize,

    /// Violation history retention in seconds.
    /// Original hardcoded value: 604800 (7 days)
    #[serde(default = "default_violation_retention_secs")]
    pub violation_retention_secs: u64,
}

fn default_penalty_rate() -> f64 { 0.05 }
fn default_critical_weight() -> u32 { 10 }
fn default_major_weight() -> u32 { 5 }
fn default_minor_weight() -> u32 { 1 }
fn default_max_violations_per_hour() -> usize { 10 }
fn default_violation_retention_secs() -> u64 { 7 * 24 * 3600 }

impl Default for ReputationPolicyConfig {
    fn default() -> Self {
        Self {
            penalty_rate: default_penalty_rate(),
            critical_weight: default_critical_weight(),
            major_weight: default_major_weight(),
            minor_weight: default_minor_weight(),
            max_violations_per_hour: default_max_violations_per_hour(),
            violation_retention_secs: default_violation_retention_secs(),
        }
    }
}

impl ReputationPolicyConfig {
    /// Convert to `SeverityWeights` for use with `Violation::severity_with_weights()`.
    pub fn severity_weights(&self) -> icn_security::misbehavior::SeverityWeights {
        icn_security::misbehavior::SeverityWeights {
            critical: self.critical_weight,
            major: self.major_weight,
            minor: self.minor_weight,
        }
    }
}
```

### Step 2.6: Wire into supervisor

- [ ] **Find where `MisbehaviorDetector` is constructed**:
```bash
grep -n "MisbehaviorDetector::new\|MisbehaviorThresholds" \
  crates/icn-core/src/supervisor/lifecycle.rs
```

- [ ] **Pass config-derived thresholds**:
Replace `MisbehaviorThresholds::default()` with values from config:
```rust
let reputation_config = &config.security;
let thresholds = MisbehaviorThresholds {
    quarantine_threshold: thresholds.quarantine_threshold, // keep existing field
    ban_threshold: thresholds.ban_threshold,
    max_violations_per_hour: reputation_config.max_violations_per_hour,
    decay_rate: thresholds.decay_rate,
    violation_retention_secs: reputation_config.violation_retention_secs,
};
// Pass penalty_rate and severity_weights to detector separately
// or add them to MisbehaviorThresholds as new fields
```

### Step 2.7: Run all security tests

- [ ] **Run tests**
```bash
cargo test -p icn-security 2>&1 | tail -10
```
Expected: all passing

### Step 2.8: Commit and open PR

- [ ] **Verify fmt and clippy**
```bash
cargo fmt --all --check
cargo clippy -p icn-security -p icn-core --all-targets -- -D warnings 2>&1 | tail -5
```

- [ ] **Commit**
```bash
cargo fmt --all
git add crates/icn-security/src/misbehavior.rs \
        crates/icn-core/src/config/security.rs \
        crates/icn-core/src/config/mod.rs \
        crates/icn-core/src/supervisor/lifecycle.rs
git commit -m "feat(security): make severity scoring and penalty rate config-driven (#1370)

Add SeverityWeights struct and severity_with_weights() method to replace
hardcoded 10/5/1 severity scores. Add penalty_rate to ReputationPolicyConfig
to replace hardcoded 0.05 constant. All values wired to node config with
defaults matching original hardcoded values.

Sprint 21 Meaning Firewall Phase 2 — security violations.
Part of #1370."
```

- [ ] **Open PR**
```bash
git push -u origin feat/1370-reputation-policy-config
gh pr create \
  --title "feat(security): make severity scoring and penalty rate config-driven (#1370)" \
  --body "Part of #1370 Meaning Firewall Phase 2. Adds SeverityWeights + configurable penalty_rate. Defaults unchanged."
```

---

## Task 3: Update Audit Doc (Post-Merge)

After both PRs merge, update the audit doc to reflect remediated violations.

- [ ] **Update `docs/architecture/meaning-firewall-audit.md`**
  - Mark compute and security violations as "REMEDIATED in Sprint 21"
  - Update summary table — remaining: icn-ledger (Sprint 22), icn-core/icn-obs (low priority)

- [ ] **Commit on main**
```bash
git add docs/architecture/meaning-firewall-audit.md
git commit -m "docs(arch): update meaning firewall audit — sprint 21 remediations"
```

---

## Sprint 21 Board JSON

Write this to `ops/state/sprint/current.json` when advancing from Sprint 20:

```json
{
  "sprint": 21,
  "name": "Sprint 21 — Meaning Firewall Policy Extraction",
  "started": "2026-03-21",
  "goals": [
    "Close Sprint 20: merge PRs #1380 #1381 #1382 #1383",
    "Make compute admission thresholds config-driven: min_standing, min_trust_score, fuel_cost_divisor",
    "Make reputation severity scoring config-driven: SeverityWeights, penalty_rate",
    "Update meaning firewall audit doc with Sprint 21 remediations"
  ],
  "tasks": [
    {
      "id": "s21-t0",
      "title": "chore(ops): close Sprint 20 — merge PRs and update sprint board",
      "status": "pending",
      "prs": [1380, 1381, 1382, 1383],
      "note": "Merge in order: doc PRs first (1382, 1383), then code PRs (1380, 1381). Use --admin if Benchmark job is stalled."
    },
    {
      "id": "s21-t1",
      "title": "feat(compute): make admission thresholds config-driven (#1370 partial)",
      "status": "pending",
      "pr": null,
      "issue": 1370,
      "note": "ComputePolicyConfig in icn-core/config/compute.rs. min_standing, min_trust_score, fuel_cost_divisor. Wire into lifecycle.rs. Defaults match current hardcoded values."
    },
    {
      "id": "s21-t2",
      "title": "feat(security): make severity scoring config-driven (#1370 partial)",
      "status": "pending",
      "pr": null,
      "issue": 1370,
      "note": "SeverityWeights struct, severity_with_weights() method, penalty_rate in ReputationPolicyConfig. Wire into MisbehaviorDetector via lifecycle.rs. Defaults match current hardcoded values."
    },
    {
      "id": "s21-t3",
      "title": "docs(arch): update meaning firewall audit post-sprint",
      "status": "pending",
      "pr": null,
      "issue": 1370,
      "note": "Update docs/architecture/meaning-firewall-audit.md to mark compute and security violations as remediated. Update summary table."
    }
  ],
  "deferred_to_s22": {
    "note": "icn-ledger CreditPolicy extraction, icn-obs attestation thresholds, full PolicyOracle wiring for per-coop dynamic values",
    "issues": [1370]
  }
}
```

---

## Verification Checklist

After all Sprint 21 PRs merge:

```bash
cd /home/ubuntu/projects/icn/icn

# All tests pass
cargo test --workspace --lib 2>&1 | tail -5
cargo test -p icn-compute 2>&1 | tail -5
cargo test -p icn-security 2>&1 | tail -5

# Meaning firewall CI gate still passes
cargo test -p icn-core --lib meaning_firewall 2>&1 | tail -5

# No format or clippy regressions
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
```

**Manual verification**: Set non-default values in a test `config.toml`:
```toml
[compute.policy]
min_standing = 0.7
min_trust_score = 0.4
fuel_cost_divisor = 500

[security.reputation]
penalty_rate = 0.10
critical_weight = 20
```
Start `icnd` with this config and verify logs reflect the new values.
