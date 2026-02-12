# Workshop 10: Contributor Workflow Practice

## Goal
Practice the ICN contribution workflow by running local checks, understanding
test scopes, and preparing a sample PR.

## Prerequisites
- Completed Module 10 reading
- ICN repo cloned with toolchain installed
- Rust toolchain with rustfmt and clippy

## Estimated time
1.5-2 hours

## Part 1: Run Local Quality Checks

### Steps
1. Navigate to the ICN workspace:
   ```bash
   cd icn
   ```

2. Check formatting (should pass if no changes):
   ```bash
   cargo fmt --all -- --check
   ```

3. Run clippy:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```

4. Run the test suite:
   ```bash
   cargo test --workspace --lib
   ```

### Expected results
- Format check: No output means success
- Clippy: Warnings/errors only if there are issues
- Tests: All tests should pass on a clean checkout

### Record your findings
| Check | Result | Time |
|-------|--------|------|
| `cargo fmt --check` | Pass/Fail | ___ sec |
| `cargo clippy` | ___ warnings | ___ sec |
| `cargo test --lib` | ___ passed | ___ sec |

### Checkpoint
- [ ] All local checks pass
- [ ] You understand what each check validates

## Part 2: Understand Test Scopes

### Steps
1. List available tests for icn-core:
   ```bash
   cargo test -p icn-core --lib -- --list 2>&1 | head -30
   ```

2. List available tests for icn-gateway:
   ```bash
   cargo test -p icn-gateway --lib -- --list 2>&1 | head -30
   ```

3. Identify integration tests:
   ```bash
   ls icn/crates/*/tests/*.rs 2>/dev/null
   ```

### Questions to answer
1. What's the difference between `--lib` and `--test`?
2. Which crates have integration tests?
3. How do you run tests for a specific function?

### Checkpoint
- [ ] You can list tests for a specific crate
- [ ] You understand the test scope options

## Part 3: Test Scope Decision Exercise

### Scenario
You're making changes to the following areas. Decide which tests to run:

| Change | Minimum Test Command |
|--------|---------------------|
| Bug fix in `icn/crates/icn-ledger/src/ledger.rs` | ? |
| New endpoint in `icn-gateway/src/api/` | ? |
| Change to `icn/crates/icn-trust/src/types.rs` | ? |
| Modify `icn/crates/icn-core/src/supervisor/mod.rs` | ? |

### Hints
- Unit tests: `cargo test -p <crate> --lib`
- Integration tests: `cargo test -p <crate> --test <name>`
- Full workspace: `cargo test --workspace`

### Expected answers
1. `cargo test -p icn-ledger --lib`
2. `cargo test -p icn-gateway` (includes integration tests)
3. `cargo test -p icn-trust --lib`
4. `cargo test -p icn-core` (full crate tests)

### Checkpoint
- [ ] You can determine the right test scope for a change

## Part 4: PR Checklist Review

### Steps
1. Read `CONTRIBUTING.md`
2. Find the PR checklist
3. List the requirements for a PR

### Expected checklist items
- [ ] Tests pass locally
- [ ] Code is formatted (`cargo fmt`)
- [ ] Clippy passes
- [ ] Documentation updated (if needed)
- [ ] Commit messages follow convention
- [ ] PR description explains the change

### Questions to answer
1. What is the commit message format?
2. When should you update documentation?
3. What should a PR description include?

### Checkpoint
- [ ] You can list PR requirements from memory
- [ ] You understand the commit message convention

## Part 5: Simulate a Contribution

### Exercise
Create a small documentation improvement (do not commit):

1. Find a typo or unclear sentence in any doc file
2. Note the file path and line number
3. Write down:
   - What change you would make
   - Which tests you would run
   - What your commit message would be

### Example
```
File: docs/ARCHITECTURE.md:42
Change: Fix typo "coordinaton" → "coordination"
Tests: None needed (docs only)
Commit: docs: fix typo in architecture overview
```

### Checkpoint
- [ ] You identified a potential improvement
- [ ] You determined the correct test scope (none for docs)
- [ ] You wrote a proper commit message

## Part 6: Branch and Commit Conventions

### Steps
1. Review the branch naming conventions in `CLAUDE.md` or `CONTRIBUTING.md`
2. Practice creating a branch name for:
   - A bug fix in the ledger
   - A new feature for governance
   - A documentation update

### Expected branch names
- `fix/ledger-balance-calculation`
- `feat/governance-delegation`
- `docs/update-architecture-overview`

### Commit message format
```
<type>(<scope>): <summary>

<optional body>

Co-Authored-By: Your Name <email>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

### Checkpoint
- [ ] You can create proper branch names
- [ ] You understand the commit message format

## Summary
After completing this workshop you should be able to:
- Run all local quality checks
- Determine the correct test scope for changes
- Follow the PR checklist
- Use proper branch and commit conventions

## Quick Reference

### Common Commands
```bash
# Format check
cargo fmt --all -- --check

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# All tests
cargo test --workspace

# Single crate tests
cargo test -p icn-<crate> --lib

# Run specific test
cargo test -p icn-<crate> test_function_name
```

### PR Workflow
```bash
# Create branch
git checkout -b <type>/<short-description>

# Make changes, then:
cargo fmt --all
cargo clippy --workspace
cargo test --workspace

# Commit
git add -A
git commit -m "<type>(<scope>): <summary>"

# Push and create PR
git push -u origin <branch>
gh pr create --title "<title>" --body "<description>"
```

## Next steps
You've completed the onboarding curriculum! Consider:
- Working on a "good first issue"
- Reviewing the capstone project in `capstone.md`
- Joining the community discussions
