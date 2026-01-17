# Contributing to ICN

Thank you for your interest in contributing to ICN! This document provides guidelines and workflows for contributors.

---

## Getting Started

### 1. Development Environment Setup

**Automated Setup (Recommended):**
```bash
./scripts/dev-setup.sh
```

This script will:
- Install all required Rust development tools (`cargo-watch`, `cargo-audit`, `cargo-tarpaulin`, `cargo-outdated`)
- Install Node.js tools for commit message validation (optional)
- Set up pre-commit hooks (format checking, linting)
- Set up commit-msg validation (conventional commits)
- Create `.envrc` for direnv users (optional)

**Manual Setup:**
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install development tools
cargo install cargo-watch cargo-audit cargo-tarpaulin cargo-outdated

# Install Node.js tools (optional, for commit message validation)
npm install -g @commitlint/cli @commitlint/config-conventional
```

### 2. Build and Test

```bash
# Clone the repository
git clone https://github.com/InterCooperative-Network/icn.git
cd icn

# Build the project
cd icn
cargo build

# Run tests
cargo test --workspace

# Run clippy (linter)
cargo clippy --workspace --all-targets

# Check formatting
cargo fmt --all -- --check
```

### 3. Verify Everything Works

```bash
# Run the full CI pipeline locally
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit  # Check for security vulnerabilities

# Optional: Run benchmarks
cargo bench --workspace

# Optional: Generate coverage report
cargo tarpaulin --workspace --timeout 300
```

---

Use descriptive branch names with prefixes:

| Prefix | Purpose | Example |
|--------|---------|---------|
| `feat/` | New features | `feat/time-sync-tests` |
| `fix/` | Bug fixes | `fix/attestation-expiry` |
| `test/` | Test additions | `test/federation-integration` |
| `docs/` | Documentation | `docs/deployment-guide` |
| `refactor/` | Code refactoring | `refactor/clearing-manager` |
| `chore/` | Maintenance tasks | `chore/update-deps` |

### Development Flow

1. **Create a feature branch**
   ```bash
   git checkout -b feat/your-feature-name
   ```

2. **Make changes and commit**
   ```bash
   # Run tests first
   cargo test
   cargo clippy
   cargo fmt
   
   # Commit with conventional commit message
   git commit -m "feat: add time synchronization tests"
   ```

3. **Push and create PR**
   ```bash
   git push -u origin feat/your-feature-name
   gh pr create --title "feat: Add time sync tests" --body "..."
   ```

4. **After review, merge to main**
   - Squash merge preferred for clean history
   - Delete branch after merge

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
- `feat(federation): add cross-coop registry tests`
- `fix(ccl): correct fuel consumption tracking`
- `test(privacy): add onion routing integration tests`
- `docs: update GAP_ANALYSIS with progress`

### Code Quality Checks

Before submitting a PR:

```bash
# Run all tests
cargo test

# Check for warnings
cargo clippy -- -D warnings

# Format code
cargo fmt

# Run specific package tests
cargo test -p icn-federation
```

### Gap Analysis Workflow

When addressing gaps from `docs/GAP_ANALYSIS.md`:

1. Reference the gap number in branch name: `test/gap-5-federation`
2. Update GAP_ANALYSIS.md marking item as fixed
3. Close related GitHub issue in PR description: `Closes #31`

### Quick Reference

```bash
# Create feature branch
git checkout -b feat/my-feature

# Check status
git status

# Stage and commit
git add .
git commit -m "feat: description"

# Push and create PR
git push -u origin feat/my-feature
gh pr create

# After merge, cleanup
git checkout main
git pull
git branch -d feat/my-feature
```

---

## Related Guides

- [Internationalization Guide](docs/i18n-guide.md) - Adding translations to ICN components
- [Architecture Overview](docs/ARCHITECTURE.md) - System architecture documentation
- [Phase History](docs/PHASE_HISTORY.md) - Completed development phases
- [Production Hardening](docs/production-hardening.md) - Security configuration
