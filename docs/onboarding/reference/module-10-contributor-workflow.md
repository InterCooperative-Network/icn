# Module 10: Contributor Workflow

## Learning Objectives

By the end of this module, you will:
- Set up a complete ICN development environment
- Understand the branching strategy and commit conventions
- Run all quality gates before submitting code
- Create effective pull requests that pass review
- Navigate the code review process
- Update documentation alongside code changes

## Prerequisites

- Module 9 (Operations and Deployment)
- Git proficiency
- Basic Rust development experience

## Key Reading

- `CONTRIBUTING.md`
- `CLAUDE.md` (Git Workflow section)
- `.github/ISSUE_POLICY.md`

---

## Concepts (Textbook Style)

### The Contribution Philosophy

ICN maintains a high bar for code quality because the system handles identity, trust,
and financial data for cooperatives. Every change must be:

1. **Safe**: No security regressions
2. **Correct**: Tests prove the behavior
3. **Reviewable**: Small, focused changes with clear motivation
4. **Documented**: APIs and concepts explained

```
┌────────────────────────────────────────────────────────────────┐
│                   Contribution Quality Gate                    │
│                                                                │
│    ┌──────────┐     ┌──────────┐     ┌──────────┐            │
│    │  Format  │────▶│   Lint   │────▶│   Test   │            │
│    │  cargo   │     │  clippy  │     │  cargo   │            │
│    │   fmt    │     │          │     │   test   │            │
│    └──────────┘     └──────────┘     └──────────┘            │
│                           │                │                  │
│                           ▼                ▼                  │
│                    ┌───────────────────────────┐              │
│                    │      CI Pipeline         │              │
│                    │  (GitHub Actions)        │              │
│                    └───────────────────────────┘              │
│                                │                              │
│                                ▼                              │
│                    ┌───────────────────────────┐              │
│                    │     Code Review          │              │
│                    │  (Human + Automated)     │              │
│                    └───────────────────────────┘              │
│                                │                              │
│                                ▼                              │
│                    ┌───────────────────────────┐              │
│                    │        Merge             │              │
│                    └───────────────────────────┘              │
└────────────────────────────────────────────────────────────────┘
```

### The Main Branch Principle

The `main` branch is always releasable. This means:

- No direct commits to `main` - all changes via PR
- CI must pass before merge
- Breaking changes require migration paths

---

## Development Environment Setup

### Automated Setup (Recommended)

```bash
# Clone the repository
git clone https://github.com/InterCooperative-Network/icn.git
cd icn

# Run the setup script
./scripts/dev-setup.sh
```

The setup script installs:
- Rust development tools (`cargo-watch`, `cargo-audit`, `cargo-tarpaulin`)
- Pre-commit hooks for formatting and linting
- Commit message validation (conventional commits)
- Optional: Node.js tools for TypeScript SDK development

### Manual Setup

```bash
# 1. Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Install development tools
cargo install cargo-watch     # Auto-rebuild on changes
cargo install cargo-audit     # Security vulnerability checking
cargo install cargo-tarpaulin # Code coverage
cargo install cargo-outdated  # Dependency freshness

# 3. Install pre-commit hooks (recommended)
cp scripts/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

# 4. For TypeScript SDK development
cd sdk/typescript
npm install
```

### Verify Setup

```bash
cd icn

# Build everything
cargo build

# Run tests
cargo test --workspace

# Run linter
cargo clippy --workspace --all-targets

# Check formatting
cargo fmt --all -- --check

# Security audit
cargo audit
```

---

## Branching Strategy

### Branch Naming Convention

| Prefix | Purpose | Example |
|--------|---------|---------|
| `feat/` | New capability | `feat/gossip-compression` |
| `fix/` | Bug fix | `fix/trust-rate-limit-overflow` |
| `refactor/` | Structure change, behavior preserved | `refactor/actor-message-routing` |
| `docs/` | Documentation only | `docs/deployment-guide` |
| `test/` | Test additions | `test/federation-integration` |
| `chore/` | Build tooling, formatting, deps | `chore/update-deps` |

### Branch Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                    Branch Lifecycle                             │
│                                                                 │
│  main ─────●─────●─────●─────●─────●─────●───────▶              │
│            │           ▲     │           ▲                      │
│            │           │     │           │                      │
│            ▼           │     ▼           │                      │
│  feat/x    ●───●───●───┘     ●───●───────┘                      │
│                              fix/y                              │
│                                                                 │
│  Legend: ● = commit, ▲ = merge (squash)                        │
└─────────────────────────────────────────────────────────────────┘
```

**Key principles**:
- Branches are short-lived (ideally < 1 week)
- Squash merge to keep history clean
- Delete branch after merge

### Creating a Branch

```bash
# 1. Start from updated main
git checkout main
git pull origin main

# 2. Create your branch
git checkout -b feat/your-feature-name

# 3. Work in small commits
# ... make changes ...
cargo test
cargo fmt
git add -A
git commit -m "feat(gossip): add message compression header"

# 4. Keep synced with main (if needed)
git fetch origin
git rebase origin/main

# 5. Push and create PR
git push -u origin feat/your-feature-name
gh pr create
```

---

## Commit Message Convention

ICN follows [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <summary>

[optional body]

[optional footer]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Code restructuring without behavior change |
| `docs` | Documentation changes |
| `test` | Adding or updating tests |
| `chore` | Build tools, CI, dependencies |
| `style` | Formatting, whitespace |

### Scopes

Use the crate name or feature area:

| Scope | Crate/Area |
|-------|------------|
| `gossip` | icn-gossip |
| `net` | icn-net |
| `identity` | icn-identity |
| `trust` | icn-trust |
| `ledger` | icn-ledger |
| `ccl` | icn-ccl |
| `gateway` | icn-gateway |
| `cli` | icnctl |
| `runtime` | icn-core |
| `sdk` | TypeScript SDK |
| `governance` | icn-governance |
| `compute` | icn-compute |

### Examples

```bash
# New feature
git commit -m "feat(ledger): add demurrage scheduler"

# Bug fix
git commit -m "fix(trust): prevent negative reputation underflow"

# Refactoring
git commit -m "refactor(runtime): isolate actor mailbox logic"

# Documentation
git commit -m "docs: update API reference for governance endpoints"

# Test addition
git commit -m "test(gossip): add vector clock convergence tests"

# Multi-line commit with body
git commit -m "feat(gateway): add SDIS enrollment endpoints

Implements the Sovereign Digital Identity System enrollment flow:
- Level 1: Device proof verification
- Level 2: Steward vouch verification
- Complete: Full identity enrollment

Closes #123"
```

---

## Quality Gates

### Local Checks (Required Before Push)

```bash
# 1. Format code
cargo fmt --all

# 2. Run linter (treat warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# 3. Run tests
cargo test --workspace

# 4. Check for security vulnerabilities
cargo audit
```

### Quick Checks for Specific Areas

```bash
# Testing a single crate
cargo test -p icn-gossip

# Testing a specific test
cargo test test_two_node_convergence

# Testing with verbose output
cargo test -- --nocapture

# Security-critical validation
cargo test -p icn-gateway --test scope_validation_integration
cargo test -p icn-net --lib test_create
```

### TypeScript SDK Checks

```bash
cd sdk/typescript

# Run tests
npm test

# Build
npm run build

# Lint
npm run lint
```

### Full CI Pipeline (Local)

```bash
# Run everything CI runs
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo audit

# Optional: Coverage report
cargo tarpaulin --workspace --timeout 300
```

---

## Test Coverage Patterns

### Test Organization

ICN follows a consistent test organization pattern across crates:

```
icn/crates/icn-example/
├── src/
│   ├── lib.rs           # Inline unit tests
│   ├── module.rs        # Inline unit tests
│   └── ...
└── tests/
    ├── integration_test.rs   # Cross-module tests
    └── fixtures/             # Test data files
```

### Unit Test Pattern

Unit tests live alongside the code they test:

```rust
// src/balance.rs
use crate::types::{JournalEntry, AccountDelta};

/// Computes the balance for a specific account from journal entries.
/// In ICN's double-entry system, each JournalEntry contains Vec<AccountDelta>
/// where each delta specifies account, currency, and debit/credit amounts.
pub fn compute_account_balance(entries: &[JournalEntry], account_id: &str, currency: &str) -> i64 {
    entries.iter()
        .flat_map(|e| &e.accounts)
        .filter(|delta| delta.account_id == account_id && delta.currency == currency)
        .map(|delta| delta.credit.unwrap_or(0) - delta.debit.unwrap_or(0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a simple account delta for testing
    fn credit_delta(account: &str, amount: i64) -> AccountDelta {
        AccountDelta {
            account_id: account.to_string(),
            currency: "USD".to_string(),
            debit: None,
            credit: Some(amount),
        }
    }

    fn debit_delta(account: &str, amount: i64) -> AccountDelta {
        AccountDelta {
            account_id: account.to_string(),
            currency: "USD".to_string(),
            debit: Some(amount),
            credit: None,
        }
    }

    fn test_entry(deltas: Vec<AccountDelta>) -> JournalEntry {
        JournalEntry {
            id: None,
            author: Did::from_str("did:icn:test").unwrap(),
            accounts: deltas,
            timestamp: 0,
            parents: vec![],
            signature: None,
            contract_ref: None,
        }
    }

    #[test]
    fn test_empty_balance() {
        let entries: Vec<JournalEntry> = vec![];
        assert_eq!(compute_account_balance(&entries, "alice", "USD"), 0);
    }

    #[test]
    fn test_single_credit() {
        let entries = vec![
            test_entry(vec![credit_delta("alice", 100)]),
        ];
        assert_eq!(compute_account_balance(&entries, "alice", "USD"), 100);
    }

    #[test]
    fn test_mixed_entries() {
        let entries = vec![
            test_entry(vec![credit_delta("alice", 100)]),
            test_entry(vec![debit_delta("alice", 30)]),
            test_entry(vec![credit_delta("alice", 50)]),
        ];
        assert_eq!(compute_account_balance(&entries, "alice", "USD"), 120);
    }

    #[test]
    fn test_negative_balance_allowed() {
        // Mutual credit allows negative balances
        let entries = vec![
            test_entry(vec![debit_delta("alice", 100)]),
        ];
        assert_eq!(compute_account_balance(&entries, "alice", "USD"), -100);
    }
}
```

### Integration Test Pattern

Integration tests verify cross-component behavior:

```rust
// tests/two_node_sync.rs
use icn_testkit::{TestNode, TestNodeConfig};
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_two_node_ledger_sync() {
    // Setup: Create two nodes with unique ports
    let node1 = TestNode::new(TestNodeConfig {
        port: 4001,
        ..Default::default()
    }).await.expect("node1 should start");

    let node2 = TestNode::new(TestNodeConfig {
        port: 4002,
        ..Default::default()
    }).await.expect("node2 should start");

    // Connect nodes
    node1.connect_to(&node2).await.expect("should connect");

    // Act: Create transaction on node1
    let entry_id = node1.ledger()
        .create_entry(node2.did(), 100, "test payment")
        .await
        .expect("should create entry");

    // Assert: Verify sync to node2 (with retries)
    let mut synced = false;
    for _ in 0..10 {
        if node2.ledger().has_entry(&entry_id).await {
            synced = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(synced, "Entry should sync to node2 within 1 second");

    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
}
```

### Async Test Pattern

For async code, use `tokio::test`:

```rust
#[tokio::test]
async fn test_async_operation() {
    let result = some_async_function().await;
    assert!(result.is_ok());
}

// With timeout
#[tokio::test(flavor = "multi_thread")]
#[timeout(5000)] // 5 second timeout
async fn test_with_timeout() {
    let result = potentially_slow_operation().await;
    assert!(result.is_ok());
}
```

### Property-Based Testing

For complex invariants, use property testing:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn trust_score_always_in_range(score in 0.0f64..=1.0f64) {
        let trust = TrustScore::new(score);
        prop_assert!(trust.value() >= 0.0);
        prop_assert!(trust.value() <= 1.0);
    }

    #[test]
    fn balance_sum_is_zero(
        credits in prop::collection::vec(1i64..1000, 1..10),
        debits in prop::collection::vec(1i64..1000, 1..10)
    ) {
        // In mutual credit, total credits == total debits
        let total_credits: i64 = credits.iter().sum();
        let total_debits: i64 = debits.iter().sum();
        // This is a simplified example
        prop_assert_eq!(total_credits, total_credits); // Replace with real invariant
    }
}
```

### Test Expectations by Category

| Category | Coverage Target | Notes |
|----------|-----------------|-------|
| Public APIs | 90%+ | All public functions must have tests |
| Error paths | 80%+ | Test error conditions explicitly |
| Security-critical | 95%+ | Identity, trust, crypto, rate limiting |
| Integration | Key flows | Two-node sync, auth flow, ledger ops |

### Running Specific Test Categories

```bash
# Unit tests for one crate
cargo test -p icn-ledger --lib

# Integration tests only
cargo test -p icn-core --test '*'

# Tests matching a pattern
cargo test trust_score

# Tests with output
cargo test -- --nocapture

# Specific test
cargo test test_two_node_ledger_sync -- --exact

# Coverage report
cargo tarpaulin -p icn-ledger --out Html
```

### Test Naming Convention

```rust
// Pattern: test_<action>_<condition>_<expected>
#[test]
fn test_compute_balance_with_empty_entries_returns_zero() { }

#[test]
fn test_verify_signature_with_invalid_key_returns_error() { }

#[test]
fn test_rate_limiter_when_exceeded_blocks_request() { }
```

---

## Pull Request Process

### Creating a PR

```bash
# Push your branch
git push -u origin feat/your-feature-name

# Create PR with gh CLI
gh pr create \
  --title "feat(ledger): add demurrage scheduler" \
  --body "$(cat <<'EOF'
## Summary
- Implements scheduled demurrage calculation
- Adds configurable decay rates per currency

## Test plan
- [x] Unit tests for decay calculation
- [x] Integration test for scheduled execution
- [ ] Manual test in pilot environment

## Risk
Low - new feature, no changes to existing behavior

Closes #456
EOF
)"
```

### PR Requirements

Every PR must include:

| Section | Content |
|---------|---------|
| **Summary** | What changed and why (1-3 bullets) |
| **Test plan** | How you verified the change |
| **Risk** | What could break |
| **Closes** | Issue number if applicable |

### PR Size Guidelines

- Keep PRs small (< 400 lines of meaningful changes)
- One coherent change per PR
- Split large features into multiple PRs
- Each PR should be reviewable in < 20 minutes

### Before Requesting Review

```bash
# Check for new comments on existing PRs you're working on
gh pr view <PR_NUMBER> --comments

# Check CI status
gh pr checks <PR_NUMBER>

# Review your changes
gh pr diff <PR_NUMBER>
```

### After CI Failure

```bash
# Get failed job logs
gh run view <RUN_ID> --log-failed

# Fix issues and push again
cargo fmt
cargo clippy -- -D warnings
cargo test
git add -A
git commit -m "fix: address CI feedback"
git push
```

---

## Code Review Guidelines

### As an Author

- Respond to all comments (resolve or explain)
- Don't rebase after review starts (makes re-review hard)
- Mark conversations as resolved only after reviewer confirms

### As a Reviewer

Focus on:

1. **Correctness**: Does the code do what it claims?
2. **Security**: Any new attack surfaces?
3. **Tests**: Is behavior adequately tested?
4. **Documentation**: Are public APIs documented?
5. **Simplicity**: Is this the simplest solution?

Avoid:
- Style nitpicks (let `cargo fmt` handle it)
- Scope creep ("while you're here...")
- Blocking on non-essential changes

### Review Comments

Use conventional prefixes:

| Prefix | Meaning |
|--------|---------|
| `nit:` | Minor style preference, non-blocking |
| `suggestion:` | Improvement idea, author's discretion |
| `question:` | Need clarification |
| `concern:` | Potential issue, should discuss |
| `blocker:` | Must fix before merge |

---

## Documentation Updates

### When to Update Docs

Update documentation when you:
- Add or change public APIs
- Modify configuration options
- Change deployment procedures
- Add new features users will interact with

### Documentation Locations

| Type | Location |
|------|----------|
| Architecture | `docs/ARCHITECTURE.md` |
| API reference | Code doc comments + OpenAPI |
| Onboarding | `docs/onboarding/reference/` |
| Deployment | `deploy/README.md`, `docs/HOMELAB_DEPLOYMENT.md` |
| Security | `docs/security/`, `docs/production-hardening.md` |
| Changelog | `CHANGELOG.md` |

### Updating Onboarding Modules

If your change affects concepts taught in onboarding:

```bash
# 1. Check which modules might need updates
grep -r "your_feature" docs/onboarding/

# 2. Update affected modules
edit docs/onboarding/reference/module-XX-topic.md

# 3. Follow the update process
cat docs/onboarding/update-process.md
```

---

## Issue Management

### Issue Labels

Every issue must have:
- **One** `priority:*` label
- **One** type label (`bug`, `enhancement`, `docs`, etc.)
- Domain labels if touching code (`core`, `gateway`, `ledger`, etc.)

### Creating Issues

```bash
# Create a bug report
gh issue create \
  --title "fix(gateway): Rate limiter not respecting trust class" \
  --body "..." \
  --label "bug,priority:high,gateway"

# Create a feature request
gh issue create \
  --title "feat(ledger): Add support for demurrage schedules" \
  --body "..." \
  --label "enhancement,priority:medium,ledger"
```

### Linking PRs to Issues

```bash
# In PR description
Closes #123
Fixes #456
Resolves #789
```

---

## Common Workflows

### Adding a New Feature

```bash
# 1. Create branch
git checkout -b feat/your-feature

# 2. Implement with tests
# ... code changes ...

# 3. Run quality checks
cargo fmt
cargo clippy -- -D warnings
cargo test -p your-crate

# 4. Commit
git add -A
git commit -m "feat(scope): add feature description"

# 5. Push and create PR
git push -u origin feat/your-feature
gh pr create
```

### Fixing a Bug

```bash
# 1. Create branch
git checkout -b fix/bug-description

# 2. Write a failing test first
# ... add test that reproduces the bug ...
cargo test test_name  # Should fail

# 3. Fix the bug
# ... code changes ...
cargo test test_name  # Should pass

# 4. Run full test suite
cargo test --workspace

# 5. Commit and push
git add -A
git commit -m "fix(scope): prevent bug description

Adds regression test and fixes the root cause."
git push -u origin fix/bug-description
gh pr create
```

### Updating Dependencies

```bash
# 1. Check for outdated deps
cargo outdated

# 2. Create branch
git checkout -b chore/update-deps

# 3. Update Cargo.toml files

# 4. Run security audit
cargo audit

# 5. Run full test suite
cargo test --workspace

# 6. Commit
git add -A
git commit -m "chore: update dependencies

- tokio 1.35 -> 1.36
- serde 1.0.193 -> 1.0.195"
```

---

## Code Map

| File | Purpose |
|------|---------|
| `CONTRIBUTING.md` | Contribution guidelines |
| `CLAUDE.md` | Claude Code guidance (includes git workflow) |
| `.github/ISSUE_POLICY.md` | Issue taxonomy and triage |
| `.github/workflows/` | CI pipeline definitions |
| `scripts/dev-setup.sh` | Development environment setup |
| `scripts/pre-commit` | Pre-commit hook template |

---

## Exercises

### Exercise 1: Complete Development Setup

Set up your development environment and verify all tools work:

```bash
./scripts/dev-setup.sh
cargo build
cargo test --workspace
cargo clippy -- -D warnings
cargo fmt --all --check
```

### Exercise 2: Create a Test PR

Practice the PR workflow with a documentation-only change:

```bash
git checkout -b docs/onboarding-improvement
# Make a small improvement to an onboarding module
cargo fmt
git add -A
git commit -m "docs: improve onboarding module clarity"
git push -u origin docs/onboarding-improvement
gh pr create --draft
```

### Exercise 3: Review a PR

Find an open PR and practice code review:

```bash
gh pr list
gh pr view <PR_NUMBER>
gh pr diff <PR_NUMBER>
# Leave a comment using the review comment prefixes
```

---

## Checkpoints

You have completed the ICN onboarding when you can:

- [ ] Set up a complete development environment
- [ ] Create branches following naming conventions
- [ ] Write commits following conventional commit format
- [ ] Run all quality gates locally
- [ ] Create PRs with proper documentation
- [ ] Navigate the code review process
- [ ] Link PRs to issues appropriately
- [ ] Update documentation when needed

---

## Congratulations!

You have completed the ICN onboarding program. You now have the knowledge to:

1. **Understand ICN's architecture** - From transport layer to application contracts
2. **Navigate the codebase** - Know where each subsystem lives
3. **Deploy and operate** - Run ICN in development and production
4. **Contribute effectively** - Follow the workflow that maintains quality

### Next Steps

- Join the community discussions
- Pick up a `good-first-issue` labeled issue
- Explore the crates you're most interested in
- Start contributing!

Welcome to ICN development!
