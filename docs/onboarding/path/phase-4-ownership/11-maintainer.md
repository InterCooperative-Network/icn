# 11: Maintainer Skills — Reshaping Architecture Safely

> **Phase**: 4 | **Tier**: Maintainer  
> **Patterns introduced**: None (mastery phase)  
> **Prerequisite**: 10-federation-and-ops.md

## Why This Matters

Maintainers make architectural decisions, review boundary-crossing PRs, and mentor contributors. This layer teaches advanced Rust, subsystem ownership, and safe refactoring.

## What You'll Learn

### 1. Advanced Rust

**When lifetimes matter**:
- Struct lifetimes for borrowed data
- Lifetime elision rules
- `'static` vs bounded lifetimes

**Trait objects vs generics**:
- `Arc<dyn PolicyOracle>` (runtime polymorphism, different types)
- `impl PolicyOracle` (compile-time polymorphism, single type)

**Async trait patterns**:
- `#[async_trait]` macro for trait methods
- Pin and Unpin for self-referential structs

### 2. Subsystem Ownership

**Maintainer checklist**:
- API stability: Avoid breaking changes
- Performance: Profile hot paths
- Security: Review boundary crossings
- Documentation: Keep specs in sync with code

### 3. Hot Path Analysis

**Files to profile**:
- `icn-gossip/src/gossip.rs` (message processing loop)
- `icn-ledger/src/ledger.rs` (validation chain)
- `icn-net/src/actor.rs` (network I/O)

**Tools**: `cargo flamegraph`, `perf`, `tokio-console`

### 4. Load Testing

**File**: `icn/tests/load_tests.rs`

Simulate 100+ nodes, 1000+ messages/sec, measure:
- Gossip convergence time
- Ledger throughput
- Memory usage

### 5. Maintainer Review Rubric

**For every PR**:
- [ ] Meaning Firewall respected (no kernel → domain imports)
- [ ] Error handling (no panics in protocol paths)
- [ ] Observability (tracing + metrics added)
- [ ] Tests (unit + integration, edge cases covered)
- [ ] Docs (specs updated if semantics changed)

## Checkpoint

You've completed this layer when you can:

1. **Review boundary PRs**: Identify firewall violations, suggest fixes
2. **Profile hot paths**: Use flamegraph to find bottlenecks
3. **Mentor contributors**: Guide a contributor from issue to merged PR
4. **Design refactors**: Propose architectural changes with migration plan

**Artifact**: Review 3 PRs using the maintainer rubric, mentor 1 contributor.

## Deep Reference

→ `reference/module-01-rust-fundamentals.md` § "Advanced Topics"  
→ `AGENTS.md` — Maintainer coding standards  
→ [Rust Performance Book](https://nnethercote.github.io/perf-book/)  
→ [tokio-console](https://github.com/tokio-rs/console) — Async profiling
