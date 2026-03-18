# Workshop 1: Rust Fundamentals in Practice

## Goal
Apply Rust ownership, error handling, and async patterns by tracing real ICN code
and writing small exercises.

## Prerequisites
- Completed Module 1 reading
- ICN repo cloned and buildable

## Estimated time
2-3 hours

## Part 1: Error Propagation Walkthrough

### Steps
1. Open `icn/bins/icnd/src/main.rs`
2. Find the `main()` function and locate all uses of `?`
3. For each `?`, identify:
   - What type is being propagated?
   - Where does the error eventually get handled?

### Expected findings
You should find `?` used for:
- Config loading (`Config::from_file`)
- Keystore operations
- Runtime initialization

### Checkpoint
- [ ] You can trace an error from config loading to process exit
- [ ] You understand how `anyhow::Context` adds diagnostic info

## Part 2: Ownership Pattern Analysis

### Steps
1. Open `icn/crates/icn-core/src/supervisor/mod.rs`
2. Find the `Supervisor` struct definition
3. List all fields that use `Arc<...>`
4. For each `Arc` field, explain:
   - Why is shared ownership needed?
   - What synchronization primitive is used (`RwLock`, `Mutex`, or none)?

### Code to examine
Look for patterns like:
```rust
pub struct Supervisor {
    config: Config,
    trust_graph: Arc<RwLock<TrustGraph>>,
    gossip_handle: Arc<RwLock<GossipActor>>,
    // ...
}
```

### Checkpoint
- [ ] You can identify at least 3 `Arc`-wrapped fields
- [ ] You can explain why the trust graph needs `Arc<RwLock<...>>`

## Part 3: Async Task Spawning

### Steps
1. Open `icn/crates/icn-gossip/src/gossip.rs`
2. Search for `tokio::spawn`
3. For each spawn, identify:
   - What task is being created?
   - Why is it spawned instead of awaited directly?

### Expected findings
Spawned tasks typically handle:
- Background timers (anti-entropy intervals)
- IO operations that shouldn't block the main loop
- Parallel message processing

### Checkpoint
- [ ] You found at least one `tokio::spawn` usage
- [ ] You can explain the difference between `spawn` and direct `await`

## Part 4: Hands-on Exercise

### Exercise A: Write a function with Result
Create a new file `scratch/ownership_exercise.rs` and implement:

```rust
use anyhow::{Context, Result};

/// Load a config value from environment or return a default
fn get_config_value(key: &str, default: &str) -> Result<String> {
    // EXERCISE: Implement this function (intentional TODO for learners)
    // Hints:
    // - Use std::env::var and handle the error
    // - If the var is missing, return the default
    // - If the var exists but is empty, return an error
    todo!("Implement this function")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_var_uses_default() {
        std::env::remove_var("TEST_MISSING_VAR");
        let result = get_config_value("TEST_MISSING_VAR", "fallback");
        assert_eq!(result.unwrap(), "fallback");
    }

    #[test]
    fn test_present_var_is_returned() {
        std::env::set_var("TEST_PRESENT_VAR", "custom_value");
        let result = get_config_value("TEST_PRESENT_VAR", "fallback");
        assert_eq!(result.unwrap(), "custom_value");
    }
}
```

### Exercise B: Shared ownership mini-task
Explain in writing (2-3 sentences) why the `GossipActor` is wrapped in
`Arc<RwLock<...>>` rather than just `RwLock<...>`.

## Troubleshooting

### "Cannot find struct" errors
Make sure you're looking at the correct file path. The supervisor module
was refactored; check `icn/crates/icn-core/src/supervisor/mod.rs`.

### "Borrow checker errors" in exercises
Review the ownership diagram in Module 1. Remember:
- `&T` is an immutable borrow
- `&mut T` is a mutable borrow
- Only one mutable borrow OR multiple immutable borrows at a time

## Summary
After completing this workshop you should be able to:
- Trace error propagation through `?` in async code
- Identify and explain shared ownership patterns with `Arc`
- Understand when to use `tokio::spawn` vs direct await
- Write functions that return `Result` with proper error context

## Next steps
Proceed to Module 2: Architecture Overview
