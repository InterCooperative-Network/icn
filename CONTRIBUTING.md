# Contributing to ICN

Thank you for your interest in contributing to the Intercooperative Network! This guide will help you get started.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

---

## How Can I Contribute?

### Reporting Bugs

**Before submitting a bug report:**
- Check the [existing issues](https://github.com/InterCooperative-Network/icn/issues) to avoid duplicates
- Collect information about the bug (logs, steps to reproduce, environment details)

**How to submit a good bug report:**
1. Use a clear, descriptive title
2. Describe the exact steps to reproduce the problem
3. Provide specific examples (minimal reproducible test case)
4. Describe the observed behavior and expected behavior
5. Include logs, screenshots, or error messages
6. Specify your environment (OS, Rust version, ICN version)

**Template:**
```markdown
**Description**
A clear description of the bug.

**Steps to Reproduce**
1. Run `icnctl...`
2. Observe...

**Expected Behavior**
What you expected to happen.

**Actual Behavior**
What actually happened.

**Environment**
- OS: Linux 6.6.87 (WSL2)
- Rust version: 1.75.0
- ICN version: 0.1.0

**Logs**
```
[paste logs here]
```
```

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. When creating an enhancement suggestion:

1. **Use a clear, descriptive title**
2. **Provide a detailed description** of the proposed enhancement
3. **Explain why this enhancement would be useful** to ICN users
4. **List examples** of other projects with similar features (if applicable)
5. **Consider the scope**: Does it fit ICN's mission as a substrate for cooperatives?

**ICN's Scope:**
- ✅ Core infrastructure (identity, trust, ledger, gossip, governance)
- ✅ Security and privacy primitives
- ✅ P2P networking and coordination
- ❌ User-facing applications (those should use the Gateway API)
- ❌ Specific cooperative workflows (those should use CCL contracts)

### Contributing Code

#### Development Setup

1. **Install Rust** (1.70+ required):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone the repository**:
   ```bash
   git clone https://github.com/InterCooperative-Network/icn.git
   cd icn
   ```

3. **Use DevContainer (recommended)**: Open in VS Code and select "Reopen in Container"
   - Pre-configured Rust, Node.js, and extensions
   - See [.devcontainer/devcontainer.json](.devcontainer/devcontainer.json)

4. **Build the project** (manual setup):
   ```bash
   cd icn  # Workspace is in icn/ subdirectory
   cargo build
   ```

5. **Run tests**:
   ```bash
   cargo test
   cargo test --test '*'  # Integration tests only
   ```

6. **Check code quality**:
   ```bash
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

#### Development Workflow

1. **Fork the repository** on GitHub
2. **Create a branch** from `main`:
   ```bash
   git checkout -b feature/my-awesome-feature
   ```
3. **Make your changes** (see Code Style below)
4. **Write tests** for your changes
5. **Run tests** and ensure they pass
6. **Commit your changes** with clear messages:
   ```bash
   git commit -m "feat(ledger): add credit limit override API

   - Add set_credit_limit() method to Ledger
   - Add tests for limit enforcement
   - Update docs/economic-safety.md

   Closes #123"
   ```
7. **Push to your fork**:
   ```bash
   git push origin feature/my-awesome-feature
   ```
8. **Open a Pull Request** on GitHub

#### Code Style

**Rust Style:**
- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Run `cargo fmt` before committing (enforces rustfmt style)
- Run `cargo clippy` and fix warnings
- Use meaningful variable names (avoid single-letter except in iterators/closures)
- Prefer `?` over `.unwrap()` in production code (tests are OK)
- Add doc comments (`///`) to all public items

**Example:**
```rust
/// Computes the trust score for a DID based on the transitive trust graph.
///
/// # Arguments
/// * `did` - The DID to compute trust for
/// * `depth` - Maximum graph traversal depth (default: 3)
///
/// # Returns
/// Trust score from 0.0 (no trust) to 1.0 (full trust)
///
/// # Example
/// ```
/// let score = trust_graph.compute_trust(&alice_did, 3)?;
/// assert!(score >= 0.0 && score <= 1.0);
/// ```
pub fn compute_trust(&self, did: &Did, depth: usize) -> Result<f64> {
    // Implementation...
}
```

**Commit Messages:**
- Use [Conventional Commits](https://www.conventionalcommits.org/):
  - `feat(scope): add new feature`
  - `fix(scope): fix bug`
  - `docs(scope): update documentation`
  - `refactor(scope): refactor code`
  - `test(scope): add tests`
  - `chore(scope): update dependencies`
- Scopes: `ledger`, `trust`, `gossip`, `identity`, `net`, `rpc`, `gateway`, `cli`, etc.
- Keep first line under 72 characters
- Add detailed description in body if needed
- Reference issues: `Closes #123` or `Fixes #456`

**Testing:**
- Write tests for all new features
- Aim for 80%+ code coverage
- Use descriptive test names: `test_credit_limit_enforced_on_negative_balance()`
- Add integration tests for multi-component features
- Use `icn-testkit` helpers for multi-node scenarios

**Example Test:**
```rust
#[tokio::test]
async fn test_gossip_anti_entropy_converges() {
    // Arrange
    let node_a = TestNode::new("alice").await;
    let node_b = TestNode::new("bob").await;
    node_a.connect_to(&node_b).await;

    // Act
    node_a.publish_entry("test:topic", b"hello".to_vec()).await;
    tokio::time::sleep(Duration::from_secs(2)).await; // Wait for gossip

    // Assert
    let entries_b = node_b.get_entries("test:topic").await;
    assert_eq!(entries_b.len(), 1);
    assert_eq!(entries_b[0].data, b"hello");
}
```

#### Documentation

- Add doc comments to all public APIs
- Update relevant documentation in `docs/` when changing behavior
- Add examples to doc comments
- Update `CHANGELOG.md` with your changes

---

## Project Structure

```
icn/
├── bins/              # Binaries
│   ├── icnd/         # ICN daemon
│   ├── icnctl/       # CLI tool
│   └── icn-console/  # TUI dashboard
├── crates/           # Library crates
│   ├── icn-core/     # Runtime & supervisor
│   ├── icn-identity/ # DID & keystore
│   ├── icn-trust/    # Trust graph
│   ├── icn-net/      # QUIC networking
│   ├── icn-gossip/   # Gossip protocol
│   ├── icn-ledger/   # Mutual credit ledger
│   ├── icn-ccl/      # Contract language
│   ├── icn-gateway/  # REST + WebSocket API
│   ├── icn-governance/ # Governance primitives
│   ├── icn-compute/  # Distributed compute
│   ├── icn-rpc/      # gRPC server
│   ├── icn-obs/      # Metrics & logging
│   ├── icn-store/    # Persistent storage
│   ├── icn-snapshot/ # State snapshots
│   └── icn-testkit/  # Test utilities
└── docs/             # Documentation
```

**Key Files:**
- `CLAUDE.md` - Guidance for Claude Code (comprehensive project guide)
- `README.md` - Project overview
- `ROADMAP.md` - Development roadmap
- `CHANGELOG.md` - User-facing changelog
- `docs/ARCHITECTURE.md` - System architecture
- `docs/GETTING_STARTED.md` - User guide

---

## Architecture Overview

ICN uses an **actor-based runtime** with Tokio:

1. **Supervisor** (`icn-core/src/supervisor.rs`) spawns and manages actors
2. **Actors** communicate via:
   - Message passing (mpsc channels)
   - Shared state (Arc<RwLock<T>>)
   - Callbacks (Arc<dyn Fn()>)
3. **Gossip** bridges components (ledger sync, governance, compute)

**Adding a new feature:**
1. Identify which crate it belongs to
2. Add types and logic to the crate
3. Expose via RPC (if daemon API)
4. Add CLI commands (if user-facing)
5. Add tests (unit + integration)
6. Document in `docs/`

See `docs/ARCHITECTURE.md` for details.

---

## Testing Philosophy

- **Unit tests**: Test individual functions and modules
- **Integration tests**: Test multiple components together
- **Test names**: Describe the scenario being tested
- **Test isolation**: Each test should be independent
- **Fast tests**: Keep tests under 10 seconds when possible
- **Deterministic**: Tests should not be flaky

**Running specific tests:**
```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p icn-gossip

# Run a specific test
cargo test test_two_node_convergence

# Run integration tests only
cargo test --test '*'

# Run with logs
RUST_LOG=debug cargo test -- --nocapture
```

---

## Pull Request Process

1. **Ensure all tests pass**: `cargo test` and `cargo clippy`
2. **Update documentation**: If behavior changes, update `docs/`
3. **Add changelog entry**: Update `CHANGELOG.md` under "Unreleased"
4. **Fill out PR template**: Describe changes, motivation, testing
5. **Request review**: Tag relevant maintainers
6. **Address feedback**: Make requested changes
7. **Squash commits** (optional): Keep history clean
8. **Merge**: Maintainer will merge after approval

**PR Template:**
```markdown
## Description
Brief description of changes.

## Motivation
Why are these changes needed?

## Changes
- Added X
- Modified Y
- Fixed Z

## Testing
- Added test_foo()
- Verified with manual testing

## Checklist
- [ ] Tests pass (`cargo test`)
- [ ] Clippy clean (`cargo clippy`)
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
```

---

## Release Process

(For maintainers)

1. Update `CHANGELOG.md` (move "Unreleased" to version)
2. Update version in all `Cargo.toml` files
3. Create git tag: `git tag -a v0.2.0 -m "Release 0.2.0"`
4. Push tag: `git push origin v0.2.0`
5. Build release binaries: `cargo build --release`
6. Create GitHub release with binaries
7. Publish crates (if public): `cargo publish`

---

## Communication

- **GitHub Issues**: Bug reports, feature requests, discussions
- **Pull Requests**: Code contributions
- **Roadmap**: See `ROADMAP.md` for planned work
- **Documentation**: `docs/` directory for detailed guides

---

## Recognition

Contributors are recognized in:
- `CHANGELOG.md` (credited for changes)
- GitHub contributors page
- Release notes

---

## Questions?

- Read the [Getting Started Guide](docs/GETTING_STARTED.md)
- Check [ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Browse [docs/](docs/) directory
- Open a [GitHub issue](https://github.com/InterCooperative-Network/icn/issues)

---

**Thank you for contributing to the cooperative internet!** 🎉
