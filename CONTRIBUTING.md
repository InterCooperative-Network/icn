# Contributing to ICN

## Branch Workflow

### Branch Naming Convention

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
