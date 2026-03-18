# Accelerated Track (4 weeks)

For developers already comfortable with Rust and async systems. Focus on ICN architecture, the **Meaning Firewall**, and rapid PR velocity.

## Prerequisites
- Solid Rust fundamentals (ownership, lifetimes, async/await)
- Familiarity with Tokio or async runtimes
- Experience reading multi-crate Rust projects
- Can debug compiler errors independently

## Learning Path

### Phase 1: Foundations (Days 1-3) → Reader Tier

**Goal**: Prove Rust fluency, understand repo structure.

**Path files**:
1. **Skim** `path/phase-1-foundations/01-environment.md` — Build, test, lint (if already Rust-fluent, skip to checkpoint)
2. **Skim** `path/phase-1-foundations/02-rust-through-icn.md` — Read ownership traces, skip Rust tutorial sections
3. **Read carefully** `path/phase-1-foundations/03-errors-and-tracing.md` — ICN-specific patterns
4. **Gate 1** — 4 artifacts (fast-track: submit system map + tiny PR, skip tooling/errors if confident)

**Labs**: lab-01 (optional if Rust-fluent), lab-02 (recommended for ICN error patterns)

**Time**: 2-3 days

---

### Phase 2: Architecture (Week 1) → Fixer Tier

**Goal**: Master the Meaning Firewall — this is the critical week.

**Path files**:
1. `path/phase-2-architecture/04-actors-and-concurrency.md` — ICN's actor pattern (likely familiar, focus on ICN specifics)
2. **CRITICAL** `path/phase-2-architecture/05-the-meaning-firewall.md` — PolicyOracle boundary (this is new, read deeply)
3. `path/phase-2-architecture/06-persistence-and-ledger.md` — Quarantine pattern (ICN-specific)
4. **Gate 2** — 4 artifacts (all required, no shortcuts)

**Labs**: lab-03 (skip if actor-fluent), **lab-04** (REQUIRED), lab-05 (recommended)

**Time**: 5-7 days

---

### Phase 3: Systems (Weeks 2-3) → Builder Tier

**Goal**: Ship real PRs, integrate subsystems.

**Path files**:
1. `path/phase-3-systems/07-identity-and-crypto.md` — Replay protection, keystore
2. `path/phase-3-systems/08-network-and-gossip.md` — Eventual consistency, vector clocks
3. `path/phase-3-systems/09-governance-and-contracts.md` — Governance flow
4. **Gate 3** — 3 artifacts (ship 2+ PRs if possible, demonstrate velocity)

**Labs**: lab-06, lab-07, lab-08 (do all, they're fast for experienced devs)

**Time**: 10-14 days

---

### Phase 4: Ownership (Week 4) → Maintainer Tier

**Goal**: Demonstrate production readiness.

**Path files**:
1. `path/phase-4-ownership/10-federation-and-ops.md` — Production deployment (likely familiar territory)
2. `path/phase-4-ownership/11-maintainer.md` — Review rubric, profiling
3. **Capstone** — 5 artifacts (this is your portfolio piece, take time to do it well)

**Labs**: Integrate previous labs into deployed system

**Time**: 7-10 days

---

## Weekly Schedule

| Week | Phase | Layers | Focus | Hours/week |
|------|-------|--------|-------|------------|
| 1 | Phase 1-2 | 01-06 + Gate 1-2 | Meaning Firewall mastery | 15-20 |
| 2 | Phase 3 | 07-08 + labs | Crypto, gossip, ship PRs | 15-20 |
| 3 | Phase 3 | 09 + Gate 3 | Governance, more PRs | 15-20 |
| 4 | Phase 4 | 10-11 + Capstone | Deployment, demonstrate mastery | 15-20 |

**Total**: 60-80 hours over 4 weeks

---

## Key Differences from Foundations Track

1. **Skim Phase 1**: If Rust-fluent, you can fast-track through Layers 01-02. But don't skip Layer 03 (ICN error patterns).
2. **Focus on Layer 05**: The Meaning Firewall is ICN-specific. Even if you understand trait objects and dependency injection, this pattern is new. Spend time here.
3. **Lab-04 is non-negotiable**: Compile-time boundary enforcement is the keystone. Do it from scratch, don't copy/paste.
4. **Ship multiple PRs**: Gate 3 asks for one PR, but demonstrate velocity by shipping 2-3 if possible.
5. **Capstone as portfolio**: Make this production-quality. This is what maintainers see when evaluating your tier.

---

## Entry Criteria

Can you answer these confidently?

- [ ] Explain Rust ownership and borrowing
- [ ] Write async Rust with Tokio
- [ ] Debug a trait bound error
- [ ] Understand `Arc<dyn Trait>` vs `impl Trait`
- [ ] Read a 50-file Rust codebase and trace execution

**If yes**: Start with Phase 1, skim to Phase 2.  
**If no**: Use Foundations Track instead.

---

## Support Resources

- **Architecture questions?** → `docs/architecture/KERNEL_APP_SEPARATION.md`
- **Rust advanced topics?** → `reference/module-01-rust-fundamentals.md` § Advanced
- **Fast feedback?** → Request reviews in #development, not #onboarding

---

## Next Steps

1. Read `docs/onboarding/manual.md` (45 min, high-density)
2. Skim Phase 1, enter at Phase 2 if Rust-fluent
3. Spend quality time on Layer 05 (Meaning Firewall)
4. Ship PRs early, get feedback, iterate fast

Welcome to ICN! 🚀
