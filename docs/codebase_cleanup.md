# ICN Codebase Cleanup & Refactoring Guide

## Background

Our codebase review identified several structural issues that need to be addressed:

1. **Duplicate crates** with the same functionality in different locations
2. **Nested Rust workspaces** causing build conflicts
3. **Non-incremental database migrations** that would cause data loss during upgrades
4. **Infrastructure and script inconsistencies**

This document outlines our plan to address these issues.

## 1. Duplicate Crates Resolution

### Identified Duplicates

| Duplicate Name | Paths | Resolution |
|----------------|-------|------------|
| wallet-ffi | `runtime/crates/wallet-ffi/` and `wallet/crates/ffi/` (now `wallet-ffi/`) | Keep `wallet/crates/wallet-ffi` (named `icn-wallet-ffi`) |
| wallet-agent | `runtime/crates/wallet-agent/` and `wallet/crates/wallet-agent/` | Keep `wallet/crates/wallet-agent` (named `icn-wallet-agent`) |
| wallet-core | `runtime/crates/wallet-core/` and `wallet/crates/wallet-core/` | Keep `wallet/crates/wallet-core` (named `icn-wallet-core`) |
| wallet-sync | `runtime/crates/wallet-sync/` and `wallet/crates/sync/` | Keep `wallet/crates/sync` when ready |

### Resolution Steps

1. **Update Root Cargo.toml**: Exclude duplicate directories (✅ Done)
2. **Run Cleanup Script**: Execute `scripts/cleanup_duplicate_crates.sh` to backup and remove duplicates
3. **Adjust Dependencies**: Ensure all crates refer to the correct dependencies

### Naming Conventions

- Use consistent prefixes for related crates:
  - `icn-*` - Public-facing crates that might be published
  - `wallet-*` - Internal wallet-related crates
  - `runtime-*` - Internal runtime-related crates

## 2. Workspace Structure Cleanup

### Current State

- Root workspace in `/Cargo.toml`
- Nested workspaces in:
  - `runtime/Cargo.toml`
  - `wallet/Cargo.toml`
  - `tools/Cargo.toml`

### Target State

**Option A: Flatten to Single Workspace** (Recommended)
- Keep only the root workspace definition
- Remove all nested `[workspace]` sections
- Reference all crates directly from the root

**Option B: Separate Namespaces**
- Keep nested workspaces but ensure crate names don't conflict
- Use distinct prefixes: `icn-wallet-*`, `icn-runtime-*`, etc.
- Each sub-workspace manages its own dependencies

Steps for Option A:
1. Remove `[workspace]` section from nested Cargo.toml files
2. Update root workspace members
3. Fix relative paths in dependency references

## 3. Database Migration Strategy

### Current State

- Initial schema: `20240101000000_init.sql`
- Redesign schema: `20240701000000_redesign.sql` (recreates tables)

### Implementation

✅ Created incremental migration `20240702000000_incremental_update.sql` that:
- Backs up existing data
- Alters existing tables to add new columns
- Updates types (TEXT → UUID) with safe conversions
- Creates new tables without dropping existing ones
- Migrates data between old and new structures

### Migration Testing

1. Create a test database with the initial schema
2. Insert sample data
3. Run incremental migration
4. Verify all data is preserved and accessible

## 4. Infrastructure Updates

### CI/CD

✅ Enhanced `.github/workflows/check-workspace.yml` to:
- Detect duplicate crates more thoroughly
- Warn about nested workspaces
- Check all feature flag combinations
- Enforce semantic patterns to avoid future duplicates

### Dev Environment

1. Update Docker configurations to reference correct paths
2. Ensure scripts reference the correct crate locations
3. Update environment variable examples

## Implementation Checklist

1. ✅ Update root Cargo.toml to exclude duplicates
2. ✅ Create cleanup script for duplicate crates
3. ✅ Create incremental database migration
4. ✅ Enhance CI workflow checks
5. [ ] Run cleanup script (requires approval)
6. [ ] Test build after cleanup
7. [ ] Test database migration
8. [ ] Update Docker and development scripts
9. [ ] Document workspace structure in README

## Future Work

1. Complete the workspace flattening process
2. Standardize on consistent naming across the codebase
3. Add versioning for CCL templates as recommended
4. Implement end-to-end tests for Wallet → AgoraNet → Runtime cycle
5. Add secret scanning to CI

## References

- [Cargo Workspaces Documentation](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [SQLx Migration Guide](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#migrations) 