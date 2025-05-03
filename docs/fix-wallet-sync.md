# Fixing the wallet-sync crate

After investigating the wallet-sync crate, we've found several issues that need to be addressed to make it compatible with the runtime codebase:

## Version Conflicts

1. **multihash version conflict**: There are two different versions of the multihash crate being used:
   - The `multihash` in the workspace (version 0.18.1)
   - The `multihash` crate directly used in wallet-sync (version 0.16.3)

   This causes types to be incompatible between code that uses one version and code that uses the other.

2. **backoff error handling**: The backoff error handling is not correctly propagating errors of type `SyncError` through the backoff structure.

## Type Mismatches

1. **DagNode.data type**: 
   - Expected: `serde_json::Value`
   - Found: `Vec<u8>`

2. **DagNode.created_at type**:
   - Expected: `chrono::DateTime<Utc>`
   - Found: `Option<SystemTime>`

3. **NodeSubmissionResponse fields**:
   - `cid` field doesn't exist, should be using `id` instead
   - Missing other required fields: timestamp, block_number, data

## Compatibility with wallet-core and runtime types

The wallet-sync crate needs to be updated to match the types used in both wallet-core and the runtime components.

## Recommended Fixes

1. **Update Cargo.toml to use correct dependencies**:
   ```toml
   # Use a renamed multihash to avoid version conflicts
   multihash-0_16_3 = { package = "multihash", version = "=0.16.3", features = ["sha2"] }
   # Pin backoff to exactly match the workspace version 
   backoff = { version = "=0.4.0", features = ["futures", "tokio"] }
   # Add hex dependency for CID generation
   hex = "0.4"
   ```

2. **Fix error handling in backoff callbacks**:
   - Make sure error converters properly handle the `backoff::Error` types
   - Create a custom conversion between `backoff::Error<SyncError>` and `SyncError`

3. **Fix the DagNode and TrustBundle structure compatibility**:
   - Update `to_dag_node()` implementation to create data as `serde_json::Value`
   - Fix timestamp handling using `chrono::DateTime<Utc>` instead of `Option<SystemTime>`
   - Update the method signatures to match between components

4. **Fix CID handling**:
   - Use a consistent approach to CID generation without depending on multihash directly
   - Possibly use simplified mocks for CID when running tests

5. **Update synchronization points**:
   - Ensure NodeSubmissionResponse uses the right field names
   - Modify validation methods to use the correct timestamp comparisons

The current implementation has too many version conflicts to fix completely without a more comprehensive refactoring, but these steps would allow the code to compile and run basic tests. 