# Module Splitting Implementation Guide

**Related**: [Module Splitting Analysis](module-splitting-analysis.md)  
**Status**: Ready for Implementation  
**Priority**: LOW (Tech Debt)

## Quick Start

This guide provides step-by-step instructions for implementing the module splits identified in the analysis document.

## Prerequisites

Before starting any module split:
1. Create a feature branch: `git checkout -b refactor/split-<module-name>`
2. Ensure all tests pass: `cd icn && cargo test -p <crate-name>`
3. Backup the current file: `cp src/file.rs src/file.rs.backup`

## Implementation: ledger.rs Split

### Phase 1: Extract Query Operations (Lowest Risk)

**Estimated Time**: 2-3 hours  
**Risk**: 🟢 Low

#### Step 1.1: Create Module Directory

```bash
cd icn/crates/icn-ledger/src
mkdir -p ledger
```

#### Step 1.2: Create queries.rs

```bash
touch ledger/queries.rs
```

Add this content:

```rust
//! Query operations for ledger entries
//!
//! This module contains read-only operations that query the ledger
//! without modifying state.

use crate::error::Result;
use crate::types::{ContentHash, JournalEntry};
use anyhow::Context;
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use tracing::warn;

// Re-export pagination types
pub use crate::ledger::PaginationCursor;

// Constants (copy from ledger.rs)
const JOURNAL_PREFIX: &str = "ledger:journal:";
const JOURNAL_TS_PREFIX: &str = "ledger:journal_ts:";
const ARCHIVE_PREFIX: &str = "ledger:archive:";

/// Query operations for the ledger
pub struct LedgerQueries {
    store: Arc<dyn Store>,
}

impl LedgerQueries {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }
    
    // TODO: Move query functions here:
    // - get_entry()
    // - get_all_entries()
    // - count_entries()
    // - get_entries_paginated()
    // - get_entries_paginated_asc()
    // - get_entries_filtered_paginated()
    // - get_archived_entries()
    // - list_rollback_timestamps()
}
```

#### Step 1.3: Move Functions

Copy these functions from `ledger.rs` to `ledger/queries.rs`:
- Lines 2184-2195: `get_entry()`
- Lines 2198-2212: `get_all_entries()`
- Lines 2218-2221: `count_entries()`
- Lines 2237-2281: `get_entries_paginated()`
- Lines 2297-2328: `get_entries_paginated_asc()`
- Lines 2358-2495: `get_entries_filtered_paginated()`
- Lines 3511-3522: `get_archived_entries()`
- Lines 3525-3545: `list_rollback_timestamps()`

**Important**: 
- Change `&self` to use `self.store` for the moved functions
- Update function signatures to be standalone or methods on `LedgerQueries`
- Keep doc comments

#### Step 1.4: Update ledger.rs

In `ledger.rs`, add at the top of the file (after existing module declarations):

```rust
mod ledger;
pub use ledger::queries::LedgerQueries;
```

Then in the `impl Ledger` block, replace the moved functions with delegation:

```rust
impl Ledger {
    // ... existing code ...
    
    /// Get a journal entry by its hash
    pub fn get_entry(&self, hash: &ContentHash) -> Result<Option<JournalEntry>> {
        let queries = LedgerQueries::new(self.store.clone());
        queries.get_entry(hash)
    }
    
    // ... repeat for all query functions ...
}
```

#### Step 1.5: Create ledger/mod.rs

```bash
touch ledger/mod.rs
```

Add this content:

```rust
//! Ledger submodules

pub mod queries;

// Re-export commonly used types
pub use queries::LedgerQueries;
```

#### Step 1.6: Test

```bash
cd ../../../  # Back to icn/ directory
cargo test -p icn-ledger
cargo clippy -p icn-ledger
cargo fmt -p icn-ledger
```

#### Step 1.7: Commit

```bash
git add .
git commit -m "refactor(ledger): Extract query operations to submodule

- Move read-only query functions to ledger/queries.rs
- Maintain backward compatibility via delegation
- No API changes, all tests pass"
```

### Phase 2: Extract Balance Operations

**Estimated Time**: 2-3 hours  
**Risk**: 🟡 Medium

Follow similar pattern as Phase 1, extracting:
- `get_balance()`
- `get_account_balances()`
- `get_all_balances()`
- `total_cleared_by()`
- `recompute_balances()`
- `recompute_balances_with_retry()`
- `load_cached_balances()`
- `save_cached_balances()`
- `load_cleared_volume_index()`
- `save_cleared_volume_index()`

Create `ledger/balances.rs` and follow steps 1.2-1.7.

### Phase 3: Extract Fork Operations

**Estimated Time**: 2-3 hours  
**Risk**: 🟢 Low

Extract fork-related functions to `ledger/fork_ops.rs`:
- `detect_forks()`
- `has_fork()`
- `detect_and_resolve_forks()`
- `quarantine_forked_entry()`
- `get_fork_stats()`
- `rebuild_fork_index()`
- `ensure_timestamp_index()`

### Phase 4: Extract Freeze Operations

**Estimated Time**: 2 hours  
**Risk**: 🟢 Low

Extract freeze-related functions to `ledger/freeze_ops.rs`:
- `freeze_member()`
- `freeze_member_with_metadata()`
- `unfreeze_member()`
- `unfreeze_member_with_metadata()`
- `is_member_frozen()`
- `get_freeze_record()`
- `list_frozen_members()`
- `frozen_member_count()`
- `cleanup_expired_freezes()`

### Phase 5: Extract Witness Operations

**Estimated Time**: 2-3 hours  
**Risk**: 🟡 Medium

Extract witness-related functions to `ledger/witness_ops.rs`:
- `requires_witnesses()`
- `effective_witness_policy()`
- `validate_witness_signatures()`
- `store_witness_signatures()`
- `load_witness_signatures()`
- `count_entry_signers()`
- `calculate_entry_value()`

### Phase 6: Final Validation

**Estimated Time**: 1 hour  
**Risk**: 🟢 Low

1. Run full test suite:
   ```bash
   cargo test -p icn-ledger
   cargo test --workspace  # Full integration tests
   ```

2. Run linters:
   ```bash
   cargo clippy --workspace -- -D warnings
   cargo fmt --all --check
   ```

3. Check API compatibility:
   ```bash
   cargo doc -p icn-ledger --no-deps
   # Verify all public APIs still available
   ```

4. Performance check (optional):
   ```bash
   cargo bench -p icn-ledger
   # Compare with baseline
   ```

## Alternative Approach: Incremental PR Strategy

If preferred, each phase can be a separate PR:

1. **PR 1**: Extract queries (least risky)
2. **PR 2**: Extract balances
3. **PR 3**: Extract forks
4. **PR 4**: Extract freeze
5. **PR 5**: Extract witness

This allows for:
- Easier code review
- Faster feedback cycles
- Incremental value delivery
- Reduced merge conflict risk

## Rollback Plan

If a split causes issues:

1. **Immediate Rollback**:
   ```bash
   git revert <commit-hash>
   ```

2. **Restore from Backup**:
   ```bash
   cp src/file.rs.backup src/file.rs
   ```

3. **Cherry-pick Fix**:
   ```bash
   git cherry-pick <fix-commit>
   ```

## Success Criteria

✅ All tests pass  
✅ No public API changes  
✅ Code compiles without warnings  
✅ Documentation builds successfully  
✅ No performance regressions  
✅ Improved code navigation (smaller files)  
✅ Clearer module boundaries

## Common Pitfalls

### Pitfall 1: Circular Dependencies

**Problem**: New module needs to reference parent  
**Solution**: Use trait objects or pass dependencies explicitly

### Pitfall 2: Lost Documentation

**Problem**: Doc comments not moved with functions  
**Solution**: Always move complete doc blocks with functions

### Pitfall 3: Broken Re-exports

**Problem**: Public API not accessible after split  
**Solution**: Add `pub use` statements in parent module

### Pitfall 4: Test Failures

**Problem**: Tests can't find moved functions  
**Solution**: Update test imports to use new paths

## Verification Checklist

Before merging each phase:

- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] No clippy warnings
- [ ] Code formatted with rustfmt
- [ ] Documentation builds without errors
- [ ] Public API unchanged (no breaking changes)
- [ ] PR description includes:
  - [ ] Rationale for split
  - [ ] Before/after file sizes
  - [ ] Test results
  - [ ] Performance impact (if any)

## Getting Help

If you encounter issues:
1. Check the [Module Splitting Analysis](module-splitting-analysis.md) for context
2. Review git history: `git log --oneline docs/`
3. Search for similar refactorings: `git log --grep="refactor.*modul"`
4. Ask in team chat/issue tracker

## Additional Resources

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Cargo Book - Package Layout](https://doc.rust-lang.org/cargo/guide/project-layout.html)
- Prior modularization: `docs/dev-journal/2025-12-13-phase4-nat-traversal-supervisor-modularization.md`

---

**Last Updated**: 2026-01-23  
**Author**: Analysis by GitHub Copilot
