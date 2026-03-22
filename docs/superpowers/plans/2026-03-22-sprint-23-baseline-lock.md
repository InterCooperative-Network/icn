# Sprint 23 — Baseline Lock + Narrative Surface — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore operational truth across repo, board, and narrative state — closing Sprint 22, resolving CI noise, removing stale artifacts, closing P0 residue, and publishing three canonical reference documents.

**Architecture:** Three tracks run in parallel (Track A: operations closure, Track B: P0 residue, Track C: narrative synthesis), with Track C gated on Track A/B outputs. The sprint has no new Rust features — work is diagnosis, triage, specification, and documentation. Track B implementation tasks are P2 and most likely to be explicitly deferred; the plan covers both paths.

**Tech Stack:** Rust 1.88.0 (workspace), GitHub Actions / cargo-tarpaulin (CI), git worktrees, JSON (sprint state), Markdown (docs)

**Spec:** `docs/superpowers/specs/2026-03-22-sprint-23-baseline-lock-design.md`

**Governing rule:** A task is done when repo state, board state, and narrative state agree.

---

## Pre-flight

Before starting any task, run from `icn/`:

```bash
git branch --show-current           # Must be: main
git status --short                  # One untracked entry: docs/development/sessions/2026-03/
cargo check --workspace             # Must pass
```

---

## File Map

| Task | Creates / Modifies |
|------|--------------------|
| s23-t1 | `.github/workflows/ci.yml`, `ops/state/ci-exceptions.md` (create if needed) |
| s23-t2 | `docs/development/sessions/2026-03/2026-03-22-sprint22-close.md` (commit) |
| s23-t3 | `icn-wt/1310-execution-receipt-gate/` (delete directory) |
| s23-t4 | `ops/state/sprint/current.json` |
| s23-t5 | Likely: `ops/state/sprint/current.json` (deferral note), or `icn/crates/icn-kernel-api/src/coord.rs` + new impl crate |
| s23-t6 | `icn/crates/icn-kernel-api/src/coord.rs` (ContainerRuntime trait) OR deferral note |
| s23-t7 | `docs/state/storage-governance-spec.md` (new), verify `icn/crates/icn-kernel-api/src/storage.rs` |
| s23-t8 | `docs/state/ICN-Platform-Baseline-2026-03.md` (new) |
| s23-t9 | `docs/strategy/ICN-Roadmap-Live.md`, `ops/state/sprint/current.json` (Sprint 23 tasks) |
| s23-t10 | `docs/state/demo-path-2026-03.md` (new) |

---

## Task 1 (s23-t1): Fix Test Coverage CI failure on `main`

**Acceptance:** `main` passes all non-observational gates, or remaining failures are explicitly classified in `ops/state/ci-exceptions.md`. ("Non-observational" = gates with `GATE_RATCHET_PHASE_*: blocking` or `warning`; excludes `observational`, flaky preview deployments, advisory jobs.)

**Files:**
- Investigate: `.github/workflows/ci.yml`
- Possibly modify: `.github/workflows/ci.yml`
- Create if needed: `ops/state/ci-exceptions.md`

### Step 1.1: Fetch the failing CI log

```bash
gh run view 23399051170 --log-failed 2>&1 | grep -A 60 "Generate coverage"
```

Look for the actual tarpaulin error — it will be one of:
- **Disk space**: `No space left on device`
- **Compilation failure**: `error[E...]`
- **Timeout**: `timed out after 300s`
- **Toolchain mismatch**: `error: toolchain '1.88.0' is not installed`

- [ ] **Step 1.1: Identify the failure category from CI logs**

Run the command above. Record the failure type before doing anything else.

- [ ] **Step 1.2: Apply fix based on failure category**

**If disk space:** Increase cleanup aggressiveness in the coverage job:
```yaml
# In .github/workflows/ci.yml, under "Pre-tarpaulin cleanup":
- name: Pre-tarpaulin cleanup
  run: |
    rm -rf ~/.cargo/registry/cache || true
    rm -rf ~/.cargo/git/db || true
    rm -rf ~/.rustup/toolchains/*/lib/rustlib/src || true
    sudo apt-get clean || true
    df -h /
```

**If compilation failure:** The failure is in a specific crate. Check which crate with:
```bash
gh run view 23399051170 --log-failed 2>&1 | grep "^error\[" | head -5
```
Then fix the compilation error in that crate, following the standard Rust fix workflow.

**If timeout:** Scope tarpaulin to non-slow crates only:
```yaml
- name: Generate coverage
  run: |
    cargo tarpaulin --workspace \
      --exclude icn-testkit \
      --timeout 300 --out Xml --output-dir ./coverage
  working-directory: ./icn
```

**If toolchain mismatch:** The coverage job uses `dtolnay/rust-toolchain@stable`. Pin it to match the workspace:
```yaml
# Replace:
- uses: dtolnay/rust-toolchain@stable
# With:
- uses: dtolnay/rust-toolchain@1.88.0
```
> Note: Do NOT change `rust-toolchain.toml`. Only fix the coverage job's action.

**If the fix is non-trivial and cannot land in one PR:** Document classification instead:

```bash
# Create ops/state/ci-exceptions.md if it doesn't exist:
cat > /home/ubuntu/projects/icn/ops/state/ci-exceptions.md << 'EOF'
# CI Exception Log

Non-blocking CI failures that are explicitly classified and documented.

## 2026-03-22: Test Coverage (observational gate)

**Job:** Test Coverage
**Gate level:** observational (`GATE_RATCHET_PHASE_COVERAGE=observational`)
**Failure reason:** [paste actual reason from CI log]
**Impact:** Zero — this gate is advisory only. It does not block merges.
**Tracking:** [link to follow-up issue or note if deferred to Sprint 24]
EOF
```

- [ ] **Step 1.3: Push the fix (or classification) on a branch**

```bash
git checkout -b fix/s23-t1-ci-coverage
git add .github/workflows/ci.yml   # or ops/state/ci-exceptions.md
git commit -m "fix(ci): resolve Test Coverage failure on main (s23-t1)"
gh pr create --title "fix(ci): resolve Test Coverage failure (s23-t1)" \
  --body "Fixes the failing Test Coverage job on main. See Sprint 23 s23-t1."
```

- [ ] **Step 1.4: Confirm CI passes on the PR, then merge**

```bash
gh pr merge --auto --squash <PR_NUMBER>
git checkout main && git pull
```

- [ ] **Step 1.5: Verify main is green**

```bash
gh run list --branch main --limit 3 --json conclusion,name,status
# All required gates must show "success" or the CI exception must be documented
```

---

## Task 2 (s23-t2): Resolve dirty file in `icn/`

**Acceptance:** `git status` clean on `main`. Stash is not acceptable.

**Files:**
- `docs/development/sessions/2026-03/2026-03-22-sprint22-close.md` — commit this.

The untracked file is a session log created during Sprint 22 work. It belongs in the repo.

- [ ] **Step 2.1: Inspect the file**

```bash
cat /home/ubuntu/projects/icn/docs/development/sessions/2026-03/2026-03-22-sprint22-close.md
```

Confirm it's a session record (not credentials or generated data).

- [ ] **Step 2.2: Commit it**

```bash
cd /home/ubuntu/projects/icn
git add docs/development/sessions/2026-03/2026-03-22-sprint22-close.md
git commit -m "docs(sessions): add Sprint 22 close session log (s23-t2)"
```

- [ ] **Step 2.3: Verify clean**

```bash
git status --short
# Expected: nothing shown
```

> **Alternative:** If the file should not be tracked (e.g., it's auto-generated or private), add a gitignore pattern instead:
> ```bash
> echo "docs/development/sessions/" >> .gitignore
> git add .gitignore
> git commit -m "chore: gitignore development session logs (s23-t2)"
> ```

---

## Task 3 (s23-t3): Remove stale `1310-execution-receipt-gate` worktree

**Acceptance:** `icn-wt/` contains no detached or branchless entries.

The directory `icn-wt/1310-execution-receipt-gate` exists on disk but is NOT registered in git's worktree list (already de-registered). It is a stale directory.

- [ ] **Step 3.1: Confirm git has no record of it**

```bash
git -C /home/ubuntu/projects/icn worktree list
# Expected: only the main worktree at /home/ubuntu/projects/icn
```

- [ ] **Step 3.2: Check if branch still exists**

```bash
git -C /home/ubuntu/projects/icn branch --list "*1310*"
```

- [ ] **Step 3.3: Delete the stale directory**

```bash
rm -rf /home/ubuntu/projects/icn-wt/1310-execution-receipt-gate
```

- [ ] **Step 3.4: Delete the branch if it still exists locally**

```bash
# Only if step 3.2 showed a local branch:
git -C /home/ubuntu/projects/icn branch -d feat/1310-execution-receipt-gate 2>/dev/null || true
# If -d fails (unmerged), use -D only after confirming the branch work is abandoned:
# git -C /home/ubuntu/projects/icn branch -D feat/1310-execution-receipt-gate
```

- [ ] **Step 3.5: Verify clean**

```bash
ls /home/ubuntu/projects/icn-wt/
git -C /home/ubuntu/projects/icn worktree prune
git -C /home/ubuntu/projects/icn worktree list
```

---

## Task 4 (s23-t4): Formally close Sprint 22

**Acceptance:** Sprint 22 state reconciled across `ops/state/sprint/current.json` + GitHub issue disposition. All s22 tasks marked done. Sprint board closed. No "in-review" state for already-merged PRs.

**Files:**
- `ops/state/sprint/current.json`

The sprint board currently shows all 5 Sprint 22 tasks as "in-review" but all 5 PRs are merged. This task closes them correctly.

- [ ] **Step 4.1: Verify all Sprint 22 PRs are merged**

```bash
gh pr view 1389 --json state,mergedAt | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['state'], d.get('mergedAt',''))"
gh pr view 1390 --json state,mergedAt | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['state'], d.get('mergedAt',''))"
gh pr view 1391 --json state,mergedAt | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['state'], d.get('mergedAt',''))"
gh pr view 1392 --json state,mergedAt | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['state'], d.get('mergedAt',''))"
# All should show: MERGED <date>
```

- [ ] **Step 4.2: Archive Sprint 22 board — write the closed sprint JSON**

Create `ops/state/sprint/sprint-22-closed.json` as a record:

```bash
cp /home/ubuntu/projects/icn/ops/state/sprint/current.json \
   /home/ubuntu/projects/icn/ops/state/sprint/sprint-22-closed.json
```

Then edit the archived copy to set status `"done"` for all tasks.

- [ ] **Step 4.3: Write Sprint 23 initial board to `current.json`**

```bash
cat > /home/ubuntu/projects/icn/ops/state/sprint/current.json << 'ENDJSON'
{
  "sprint": 23,
  "name": "Sprint 23 — Baseline Lock + Narrative Surface",
  "started": "2026-03-22",
  "goals": [
    "Restore operational truth: CI green, repo clean, worktrees pruned, Sprint 22 formally closed",
    "Close P0 residue: resolve #1095, #1096, #1131 — merged, deferred, or decomposed with zero ambiguity",
    "Publish baseline: three canonical reference documents that make current ICN state portable"
  ],
  "tasks": [
    {
      "id": "s23-t1",
      "title": "Fix Test Coverage CI failure on main",
      "status": "done",
      "assignee": null,
      "epic": "baseline-lock",
      "artifact": "ops/state/ci-exceptions.md or ci.yml fix"
    },
    {
      "id": "s23-t2",
      "title": "Resolve dirty file in icn/ (from #1394 merge)",
      "status": "done",
      "assignee": null,
      "epic": "baseline-lock",
      "artifact": "docs/development/sessions/2026-03/2026-03-22-sprint22-close.md"
    },
    {
      "id": "s23-t3",
      "title": "Remove stale 1310-execution-receipt-gate worktree",
      "status": "done",
      "assignee": null,
      "epic": "baseline-lock"
    },
    {
      "id": "s23-t4",
      "title": "Formally close Sprint 22",
      "status": "done",
      "assignee": null,
      "epic": "baseline-lock",
      "artifact": "ops/state/sprint/current.json"
    },
    {
      "id": "s23-t5",
      "title": "#1095 CRDT OrSet + LwwRegister — resolve disposition",
      "status": "pending",
      "assignee": null,
      "epic": "p0-residue",
      "pr": null
    },
    {
      "id": "s23-t6",
      "title": "#1096 ContainerRuntime trait interface — resolve disposition",
      "status": "pending",
      "assignee": null,
      "epic": "p0-residue",
      "pr": null
    },
    {
      "id": "s23-t7",
      "title": "#1131 Storage Specification — verify + document",
      "status": "pending",
      "assignee": null,
      "epic": "p0-residue",
      "artifact": "docs/state/storage-governance-spec.md"
    },
    {
      "id": "s23-t8",
      "title": "Publish current platform baseline",
      "status": "pending",
      "assignee": null,
      "epic": "narrative",
      "artifact": "docs/state/ICN-Platform-Baseline-2026-03.md"
    },
    {
      "id": "s23-t9",
      "title": "Roadmap + sprint-state refresh",
      "status": "pending",
      "assignee": null,
      "epic": "narrative",
      "artifact": "docs/strategy/ICN-Roadmap-Live.md"
    },
    {
      "id": "s23-t10",
      "title": "Validate and document canonical demo path",
      "status": "pending",
      "assignee": null,
      "epic": "narrative",
      "artifact": "docs/state/demo-path-2026-03.md"
    }
  ]
}
ENDJSON
```

> Update task statuses in this file as you complete each task — that is how the board stays honest.

- [ ] **Step 4.4: Commit**

```bash
cd /home/ubuntu/projects/icn
git add ops/state/sprint/current.json ops/state/sprint/sprint-22-closed.json
git commit -m "chore(sprint): close Sprint 22, initialize Sprint 23 board (s23-t4)"
```

---

## Task 5 (s23-t5): `#1095` CRDT OrSet + LwwRegister — resolve disposition

**Acceptance:** `#1095` (CRDT OrSet + LwwRegister) is merged, explicitly deferred with documented rationale, or decomposed into bounded child tasks with named ownership and acceptance criteria.

**Context:** The `CoordState` trait and `CrdtType::OrSet`/`LwwRegister` variants already exist in `icn-kernel-api/src/coord.rs`. What's missing are concrete implementations. Issue priority is **P2 (post-demo)**. Sprint 23 spine is legitimacy, not feature expansion.

**Recommended path: Explicit deferral to Sprint 24.** The implementations are non-trivial (deterministic merge semantics, fuzz tests) and are not needed for the demo flows or narrative docs.

### Path A: Explicit Deferral (recommended)

- [ ] **Step 5A.1: Comment on GitHub issue with deferral rationale**

```bash
gh issue comment 1095 --body "Deferring to Sprint 24 per Sprint 23 scope discipline.

Sprint 23 spine is baseline stabilization, not feature expansion. The CRDT implementations (OrSet, LwwRegister, governance integration) are P2 post-demo work and will land cleanly on a stable baseline.

Sprint 24 will open around Commons Compute expansion — this issue fits naturally in that wave alongside #925, #947, #964.

Acceptance criteria for Sprint 24 entry: OrSet merge semantics proven deterministic via fuzz test; LwwRegister last-write semantics proven deterministic; integration into governance vote counting with proof export."
```

- [ ] **Step 5A.2: Update sprint board status**

In `ops/state/sprint/current.json`, update s23-t5:
```json
{
  "id": "s23-t5",
  "status": "done",
  "resolution": "deferred",
  "deferred_to": "sprint-24",
  "rationale": "P2 post-demo; Sprint 23 is stabilization-only. See #1095 comment 2026-03-22."
}
```

- [ ] **Step 5A.3: Commit**

```bash
git add ops/state/sprint/current.json
git commit -m "chore(sprint): defer #1095 CRDT implementations to Sprint 24 (s23-t5)"
```

### Path B: Implement (if decided)

Only follow this path if explicitly asked to implement.

**Files:**
- Modify: `icn/crates/icn-kernel-api/src/coord.rs` (add OrSet + LwwRegister concrete structs)
- Create: `icn/crates/icn-kernel-api/src/crdt/or_set.rs`
- Create: `icn/crates/icn-kernel-api/src/crdt/lww_register.rs`

- [ ] **Step 5B.1: Write failing tests first**

In `icn/crates/icn-kernel-api/src/coord.rs` test module, add:
```rust
#[test]
fn test_or_set_merge_is_deterministic() {
    // Two nodes add the same element independently
    // Merge result must be identical regardless of merge order
    // Use fixed byte values (not random) so test is deterministic
    let a = OrSet::new();
    let b = OrSet::new();
    // ... (implement once struct exists)
    todo!("implement after OrSet struct")
}

#[test]
fn test_lww_register_last_write_wins() {
    // Two writes at different timestamps
    // Higher timestamp wins regardless of application order
    todo!("implement after LwwRegister struct")
}
```

- [ ] **Step 5B.2: Implement `OrSet`**

```rust
// icn/crates/icn-kernel-api/src/crdt/or_set.rs
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Observed-Remove Set CRDT. Supports add + remove with causal ordering.
/// Merge is commutative, associative, idempotent — and therefore deterministic.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OrSet {
    /// element bytes → set of unique tags (each add gets a unique tag)
    entries: HashMap<Vec<u8>, std::collections::HashSet<u64>>,
    /// removed tags (tombstone set)
    tombstones: std::collections::HashSet<u64>,
    next_tag: u64,
}

impl OrSet {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, element: Vec<u8>) {
        let tag = self.next_tag;
        self.next_tag += 1;
        self.entries.entry(element).or_default().insert(tag);
    }

    pub fn remove(&mut self, element: &[u8]) {
        if let Some(tags) = self.entries.get(element) {
            for &tag in tags { self.tombstones.insert(tag); }
        }
    }

    pub fn contains(&self, element: &[u8]) -> bool {
        self.entries.get(element)
            .map(|tags| tags.iter().any(|t| !self.tombstones.contains(t)))
            .unwrap_or(false)
    }

    /// Merge another OrSet into self. Commutative and idempotent.
    pub fn merge(&mut self, other: &OrSet) {
        for (elem, tags) in &other.entries {
            self.entries.entry(elem.clone()).or_default().extend(tags);
        }
        self.tombstones.extend(&other.tombstones);
        self.next_tag = self.next_tag.max(other.next_tag);
    }
}
```

- [ ] **Step 5B.3: Run tests, confirm they pass**

```bash
cd /home/ubuntu/projects/icn/icn
cargo test -p icn-kernel-api -- or_set 2>&1 | tail -5
```

- [ ] **Step 5B.4: Implement `LwwRegister`** (similar pattern — last timestamp wins on merge)

- [ ] **Step 5B.5: Commit and open PR**

```bash
git add icn/crates/icn-kernel-api/src/crdt/
git commit -m "feat(coord): implement OrSet and LwwRegister CRDTs (#1095, s23-t5)"
gh pr create --title "feat(coord): CRDT OrSet + LwwRegister implementation (#1095)"
```

---

## Task 6 (s23-t6): `#1096` ContainerRuntime trait interface — resolve disposition

**Acceptance:** `#1096` (ContainerRuntime trait) is merged, explicitly deferred, or decomposed into bounded child tasks.

**Context:** No OCI code exists. Issue asks for a kernel/app boundary trait in `icn-kernel-api`. Priority **P2 (phase 2 work)**. The trait definition is small (~40 lines).

**Recommended path: Implement** — this is a pure interface definition with a mock, no runtime dependency. It's bounded, safe, and closes the issue cleanly.

**Files:**
- Modify: `icn/crates/icn-kernel-api/src/lib.rs` (add `pub mod container;`)
- Create: `icn/crates/icn-kernel-api/src/container.rs`

- [ ] **Step 6.1: Write failing test first**

```rust
// In icn/crates/icn-kernel-api/src/container.rs, add a test module:
#[cfg(test)]
mod tests {
    use super::*;

    struct MockRuntime;
    impl ContainerRuntime for MockRuntime {
        fn pull(&self, _image: &str, _digest: &str) -> Result<(), ContainerError> { Ok(()) }
        fn run(&self, _config: ContainerConfig) -> Result<ContainerId, ContainerError> {
            Ok(ContainerId("mock-container-1".to_string()))
        }
        fn stop(&self, _id: &ContainerId) -> Result<(), ContainerError> { Ok(()) }
        fn status(&self, _id: &ContainerId) -> Result<ContainerStatus, ContainerError> {
            Ok(ContainerStatus::Running)
        }
    }

    #[test]
    fn test_mock_runtime_satisfies_trait() {
        let rt = MockRuntime;
        let id = rt.run(ContainerConfig {
            image: "icn/worker:latest".to_string(),
            digest: "sha256:abc123".to_string(),
            signer_did: "did:icn:abc".to_string(),
            cpu_limit_millicores: 500,
            memory_limit_bytes: 128 * 1024 * 1024,
            env: vec![],
        }).unwrap();
        assert_eq!(id.0, "mock-container-1");
        assert_eq!(rt.status(&id).unwrap(), ContainerStatus::Running);
        assert!(rt.stop(&id).is_ok());
    }
}
```

- [ ] **Step 6.2: Run test to confirm it fails (trait doesn't exist yet)**

```bash
cd /home/ubuntu/projects/icn/icn
cargo test -p icn-kernel-api 2>&1 | grep "cannot find\|not found\|error"
```

- [ ] **Step 6.3: Implement the trait**

```rust
// icn/crates/icn-kernel-api/src/container.rs
//! ContainerRuntime kernel/app boundary (PR11, #1096)
//!
//! Defines the interface for OCI container execution without importing
//! any runtime dependency. Concrete implementations live in app crates.

use serde::{Deserialize, Serialize};

/// Opaque identifier for a running container.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContainerId(pub String);

/// Configuration for launching a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// OCI image reference (e.g. "icn/worker:latest")
    pub image: String,
    /// Content digest for attestation (sha256:...)
    pub digest: String,
    /// DID of the entity that signed the image
    pub signer_did: String,
    /// CPU limit in millicores (e.g. 500 = 0.5 CPU)
    pub cpu_limit_millicores: u32,
    /// Memory limit in bytes
    pub memory_limit_bytes: u64,
    /// Environment variables
    pub env: Vec<(String, String)>,
}

/// Observable lifecycle states of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerStatus {
    Running,
    Stopped,
    Failed,
}

/// Errors from container operations.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("image pull failed: {0}")]
    PullFailed(String),
    #[error("container launch failed: {0}")]
    LaunchFailed(String),
    #[error("container not found: {0}")]
    NotFound(String),
    #[error("runtime unavailable: {0}")]
    Unavailable(String),
}

/// Kernel/app boundary for OCI container execution.
///
/// The kernel treats this trait as opaque — it dispatches work and reads
/// status, but never understands what is inside the container. Domain logic
/// (attestation, policy) lives in the app that implements this trait.
///
/// # Invariants
/// - `pull` must verify `digest` matches image content before returning Ok.
/// - `run` must not return Ok until the container is in the Running state.
/// - All methods are synchronous for now; async variants can be added later.
pub trait ContainerRuntime: Send + Sync {
    /// Pull an image and verify its digest. Idempotent.
    fn pull(&self, image: &str, digest: &str) -> Result<(), ContainerError>;

    /// Launch a container from a previously pulled image.
    fn run(&self, config: ContainerConfig) -> Result<ContainerId, ContainerError>;

    /// Signal a container to stop. Idempotent after first call.
    fn stop(&self, id: &ContainerId) -> Result<(), ContainerError>;

    /// Query current container status.
    fn status(&self, id: &ContainerId) -> Result<ContainerStatus, ContainerError>;
}
```

- [ ] **Step 6.4: Export the module from `lib.rs`**

Add `pub mod container;` to `icn/crates/icn-kernel-api/src/lib.rs`.

- [ ] **Step 6.5: Run tests**

```bash
cd /home/ubuntu/projects/icn/icn
cargo test -p icn-kernel-api -- container 2>&1 | tail -5
cargo clippy -p icn-kernel-api -- -D warnings 2>&1 | tail -5
```

Expected: test passes, no clippy warnings.

- [ ] **Step 6.6: Commit and open PR**

```bash
git add icn/crates/icn-kernel-api/src/container.rs icn/crates/icn-kernel-api/src/lib.rs
git commit -m "feat(kernel-api): add ContainerRuntime trait interface (#1096, s23-t6)"
gh pr create --title "feat(kernel-api): ContainerRuntime trait interface (#1096)"
```

---

## Task 7 (s23-t7): `#1131` Storage Specification — verify + document

**Acceptance:** Written spec merged to `docs/`, issue closed or explicitly deferred with rationale committed. Independent of t5/t6.

**Context:** The `StorageClass` enum and `DataLocality` type **already exist** and are fully implemented in `icn-kernel-api/src/storage.rs` (563 lines with tests). Issue #1131 asked to add them — they were added. What remains is verification and documentation.

**Files:**
- Read: `icn/crates/icn-kernel-api/src/storage.rs`
- Create: `docs/state/storage-governance-spec.md`

- [ ] **Step 7.1: Verify the implementation satisfies the issue DoD**

```bash
grep -n "StorageClass\|DataLocality\|allowed_scopes\|validate" \
  /home/ubuntu/projects/icn/icn/crates/icn-kernel-api/src/storage.rs | head -20
cargo test -p icn-kernel-api -- storage 2>&1 | tail -5
```

Expected: `StorageClass` enum with `Canonical`, `ServiceState`, `Blob` variants exists; tests pass.

- [ ] **Step 7.2: Write the specification document**

Create `docs/state/storage-governance-spec.md`:

```markdown
# Storage Governance Specification

**Status:** Implemented
**Implementation:** `icn-kernel-api/src/storage.rs`
**Issue:** #1131 — Storage is Governance
**Sprint:** 23

## Three-Tier Storage Model

Storage in ICN is not a utility layer — it is a governance artifact. Where data lives
determines who can read it, replicate it, and what trust is required to access it.

### Storage Classes

| Class | Mutability | Replication | Examples |
|-------|-----------|-------------|---------|
| `Canonical` | Immutable | Required, cooperative-wide | Ledger entries, governance receipts, trust edges |
| `ServiceState` | Mutable | Cell-local, optional mirrors | App state, calendars, schedules |
| `Blob` | Immutable | Scope-constrained | Documents, media, WASM modules |

### Data Locality

| Locality | Federation Access | Description |
|----------|------------------|-------------|
| `CellLocal` | No | Must stay in originating cell |
| `CoopReplicated` | No | Replicated within cooperative |
| `FederationMirrored` | Yes | Cross-entity interoperability |
| `CommonsPublic` | Yes | Public indices and shared libraries |

## Enforcement Invariants

1. A `Canonical` task must only produce `Canonical` storage outputs.
2. A task cannot access data with `DataLocality` more restrictive than the task's own locality.
3. `CellLocal` data cannot be sent to federation-scope nodes (enforced at placement).

## Current Implementation Status

- [x] `StorageClass` enum — `icn-kernel-api/src/storage.rs`
- [x] `DataLocality` enum with `allowed_scopes()` — same file
- [x] `StorageSpec` struct linking class + locality + constraints
- [x] `validate_storage_access()` function enforcing invariants
- [x] Serde roundtrip tests + enforcement tests (563 lines total)

## Recovery Semantics

`Canonical` data is recovered from gossip replication on restart (Sled-backed, replicated).
`ServiceState` is recovered from local Sled KV store; mirrors are advisory, not authoritative.
`Blob` data is content-addressed; recovery = re-fetch from providers via `BlobStore`.
See `icn-snapshot` crate for graceful restart protocol.
```

- [ ] **Step 7.3: Commit**

```bash
mkdir -p /home/ubuntu/projects/icn/docs/state
git add docs/state/storage-governance-spec.md
git commit -m "docs(storage): add storage governance spec for #1131 (s23-t7)"
```

- [ ] **Step 7.4: Close the issue with a comment**

```bash
gh issue comment 1131 --body "Implementation complete — \`StorageClass\`, \`DataLocality\`, \`StorageSpec\`, and \`validate_storage_access()\` all exist in \`icn-kernel-api/src/storage.rs\` (563 lines, fully tested).

Specification document published at \`docs/state/storage-governance-spec.md\`.

Closing as done."
gh issue close 1131
```

---

## Task 8 (s23-t8): Publish current platform baseline

**Depends on:** s23-t1, s23-t2, s23-t3, s23-t4 (all Track A must be complete first)

**Acceptance:** `docs/state/ICN-Platform-Baseline-2026-03.md` exists and answers: what ICN is now, what is complete, what is in progress, what is next. A new contributor can orient from this document without oral tradition.

**Files:**
- Create: `docs/state/ICN-Platform-Baseline-2026-03.md`

Before writing, gather ground truth:

```bash
# How many crates?
ls /home/ubuntu/projects/icn/icn/crates/ | wc -l

# How many LOC (approx)?
find /home/ubuntu/projects/icn/icn/crates -name "*.rs" | xargs wc -l | tail -1

# How many tests?
cargo test --workspace 2>&1 | grep "test result" | awk '{sum += $4} END {print sum " tests"}'

# What's currently deployed?
kubectl get pods -n icn 2>/dev/null | grep Running | wc -l
```

- [ ] **Step 8.1: Run the ground-truth commands above and record results**

- [ ] **Step 8.2: Write the baseline document**

```markdown
# ICN Platform Baseline — March 2026

**Date:** 2026-03-22
**Sprint:** 23 (Baseline Lock + Narrative Surface)
**Status:** Stable. Operationally deployed. No active incidents.

---

## What ICN Is

ICN (InterCooperative Network) is a peer-to-peer coordination substrate for
cooperatives, communities, and federations. It is not a blockchain, not a payment
network, and not a messaging app. It is the infrastructure layer that lets
cooperative entities coordinate without central servers.

The architectural spine is a **constraint enforcement system**:
- Apps (PolicyOracles) translate domain semantics (trust scores, governance rules)
  into generic constraints.
- The kernel enforces constraints blindly — it never knows what the constraints mean.
- This separation is the "meaning firewall" — apps carry meaning, the kernel carries
  mechanics.

---

## Current State (March 2026)

### Scale

| Metric | Value |
|--------|-------|
| Rust crates | [N from ground-truth] |
| Lines of Rust | ~[N from ground-truth] |
| Tests | [N from ground-truth] |
| Deployed nodes | K3s cluster, 3 workers |

### What Is Complete

**Meaning Firewall (Sprints 19–22 — fully closed):**
All hardcoded policy constants extracted to typed configuration structs:
- `MisbehaviorThresholdsConfig` in `icn-security`
- `ComputePolicyConfig` in `icn-compute`
- `CreditPolicyConfig` / `NewMemberPolicyConfig` in `icn-ledger`
- `ContributionAttestationConfig` in `icn-obs`

**Pilot Flows (fully closed):**
- Flow A: WASM distribution — upload, distribute via gossip, execute by hash
- Flow B: Service discovery — DID-authenticated query, naming service, gateway aggregation
- Flow C: Treasury governance — proposal → vote → receipt → ledger settlement
- All three flows run on a 3-node devnet with `make devnet-up`

**Vertical Slice (fully closed):**
- Identity → Standing → Governance → Allocations → Workloads → Receipts → Audit
- Integration test passes in CI

**Kernel/App Separation (fully closed):**
- All kernel crates are domain-agnostic
- Forbidden-deps CI gate prevents semantic imports into kernel
- PolicyOracle pattern implemented across trust, governance, membership, charter apps

**Infrastructure:**
- K3s cluster (3 workers + control) — deployed Dec 2025, migrated VLAN 30 Feb 2026
- Registry at 10.8.30.40:30500, gateway at :30080, Pilot UI at :30030
- Prometheus + Grafana at :30090 / :30300
- Backup CronJobs running (etcd-snapshot + ICN state backup)

### What Is In Progress (Sprint 23)

- CI stabilization (Test Coverage gate, observational)
- ContainerRuntime trait interface (#1096)
- Storage governance spec documentation (#1131)

### What Is Next (Sprint 24 candidates)

- Commons Compute expansion: unaffiliated node participation (#947), resource pool accounting (#925), stale participant expiry (#964)
- CRDT implementations: OrSet + LwwRegister (#1095)
- Phase 7: Naming Primitive (#862), Federation Agreements (#863)

---

## How to Orient as a New Contributor

1. **Architecture:** Read `docs/ARCHITECTURE.md` — it's authoritative and current
2. **Quick start:** `docs/GETTING_STARTED.md`
3. **Crate map:** `docs/planning/` contains the crate reference
4. **Current sprint:** `ops/state/sprint/current.json`
5. **Run a demo:** See `docs/state/demo-path-2026-03.md`

---

## Known Gaps (not blocking, tracked)

- Villain CI: broken since Mar 8 (unrelated repo)
- Paperless-NGX: Cloudflare Access not configured
- TL-SG108E: port 5 VLAN 100 DHCP leak risk (tracked, not urgent)
```

- [ ] **Step 8.3: Commit**

```bash
mkdir -p /home/ubuntu/projects/icn/docs/state
git add docs/state/ICN-Platform-Baseline-2026-03.md
git commit -m "docs(baseline): publish ICN platform baseline March 2026 (s23-t8)"
```

---

## Task 9 (s23-t9): Roadmap + sprint-state refresh

**Depends on:** s23-t4 (Sprint 22 closed), s23-t7 (P0 tail disposition known)

**Acceptance:** `ICN-Roadmap-Live.md` updated to reflect post-Sprint-22 state. `ops/state/sprint/current.json` reflects Sprint 23 tasks. Sprint 24 candidates listed with title + one-line scope + rationale only (no full decomposition).

**Files:**
- Modify: `docs/strategy/ICN-Roadmap-Live.md`
- Modify: `ops/state/sprint/current.json` (Sprint 23 task statuses updated to current)

- [ ] **Step 9.1: Read the current roadmap**

```bash
head -100 /home/ubuntu/projects/icn/docs/strategy/ICN-Roadmap-Live.md
```

- [ ] **Step 9.2: Add a Sprint 22/23 completion section**

Find the appropriate section in `ICN-Roadmap-Live.md` and update it to reflect completion. Add:

```markdown
## Sprint 22 — Meaning Firewall Completion (CLOSED 2026-03-22)

All hardcoded policy constants extracted to typed config structs. Fully merged.
PRs: #1389 (security), #1390 (obs), #1391 (compute), #1392 (ledger).

## Sprint 23 — Baseline Lock + Narrative Surface (ACTIVE 2026-03-22)

Theme: Make reality, status, and story converge.
Goals: CI green, repo clean, board honest, P0 tail closed, three reference docs published.

## Sprint 24 — Commons Compute Hardening (PLANNED)

Candidates (title + scope + rationale only — full decomposition deferred):

- **#947 Unaffiliated Node Participation:** Define protocol for nodes contributing compute
  outside formal cooperative membership. Rationale: unlocks decentralized resource pools.
- **#925 Commons Resource Pool Accounting:** Implement pool contribution + draw accounting.
  Rationale: foundational for commons compute economic model.
- **#964 Stale Commons Pool Participant Expiry:** Expire inactive participants from pool.
  Rationale: prevents pool state drift, required for fair accounting.
- **#1095 CRDT OrSet + LwwRegister:** Implement coordination primitives for distributed
  vote convergence. Rationale: deferred from Sprint 23 (P2, post-demo).
```

- [ ] **Step 9.3: Update Sprint 23 task statuses in `current.json`**

As tasks complete, update their `"status"` fields. At the end of Sprint 23, all should be `"done"`.

- [ ] **Step 9.4: Commit**

```bash
git add docs/strategy/ICN-Roadmap-Live.md ops/state/sprint/current.json
git commit -m "docs(roadmap): update roadmap through Sprint 23, list Sprint 24 candidates (s23-t9)"
```

---

## Task 10 (s23-t10): Validate and document canonical demo path

**Depends on:** s23-t5, s23-t6 (disposition known — not because they're on the demo path, but so their status can be noted in the scope note)

**Acceptance:** `docs/state/demo-path-2026-03.md` documents exact commands to run the demo on current `main`. Tested against reality.

**Files:**
- Create: `docs/state/demo-path-2026-03.md`

- [ ] **Step 10.1: Start the devnet and verify it boots**

```bash
cd /home/ubuntu/projects/icn
make devnet-up 2>&1 | tail -20
# Wait for all services healthy
curl -sf http://localhost:30080/v1/health | python3 -m json.tool
```

Record actual output. If anything fails, fix it before writing the doc.

- [ ] **Step 10.2: Run Flow A (WASM)**

```bash
# Per the existing demo script:
bash demo/scripts/present-governance.sh --port 9080
# Or the individual flow commands — run them, record exact output
```

- [ ] **Step 10.3: Run Flow B (Discovery)**

Run service discovery demo, record exact commands and expected output.

- [ ] **Step 10.4: Run Flow C (Treasury governance)**

Run treasury governance demo, record exact commands and expected output.

- [ ] **Step 10.5: Write the document with exactly what you ran**

```markdown
# ICN Canonical Demo Path — March 2026

**Date:** 2026-03-22
**Branch:** main
**Cluster:** Local devnet (localhost) OR K3s (10.8.30.40)

> This document is tested against reality. Commands are exact.
> If something fails, update the doc — do not leave aspirational commands.

## Prerequisites

- Docker + Docker Compose installed
- Ports 9080, 30080 available (or check `make devnet-up` output)
- Built: `cargo build --release` from `icn/icn/`

## Scope Note

#1095 (CRDT OrSet + LwwRegister) and #1096 (ContainerRuntime) are not on the
critical path for Flows A, B, or C. They are deferred to Sprint 24.
Current demo coverage is unaffected by their disposition.

## Start the Devnet

[exact commands from step 10.1]

## Flow A: WASM Distribution

[exact commands from step 10.2]

## Flow B: Service Discovery

[exact commands from step 10.3]

## Flow C: Treasury Governance

[exact commands from step 10.4]

## Tear Down

[exact commands to stop devnet]
```

- [ ] **Step 10.6: Commit**

```bash
git add docs/state/demo-path-2026-03.md
git commit -m "docs(demo): add canonical demo path validated against main (s23-t10)"
```

---

## Sprint Close Checklist

When all 10 tasks are complete:

- [ ] All `current.json` task statuses set to `"done"`
- [ ] `git status --short` clean on `main`
- [ ] `gh run list --branch main --limit 1 --json conclusion` shows `"success"` (or exceptions documented)
- [ ] `docs/state/` contains three new files: `ICN-Platform-Baseline-2026-03.md`, `storage-governance-spec.md`, `demo-path-2026-03.md`
- [ ] `docs/strategy/ICN-Roadmap-Live.md` updated
- [ ] GitHub issues #1095, #1096, #1131 have disposition comments
- [ ] Governing rule holds: **repo state, board state, and narrative state agree**
