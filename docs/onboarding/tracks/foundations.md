# Foundations Track (8 weeks)

For developers new to Rust or those wanting comprehensive ICN mastery. Focus on fundamentals while learning ICN systems through the **Meaning Firewall** narrative.

## Prerequisites
- Familiarity with at least one programming language
- Comfort with command line basics
- Willingness to learn Rust through production code (not toy examples)

## Learning Path

### Phase 1: Foundations (Weeks 1-2) → Reader Tier

**Goal**: Build, test, trace flows without friction.

**Path files**:
1. `path/phase-1-foundations/01-environment.md` — Workspace navigation, build fluency
2. `path/phase-1-foundations/02-rust-through-icn.md` — Ownership/borrowing as system invariants
3. `path/phase-1-foundations/03-errors-and-tracing.md` — Distributed visibility
4. **Gate 1** — 4 artifacts (system map, tooling, errors, tiny PR)

**Labs**: lab-01-workspace, lab-02-error-receipt

**Extra focus**: Spend more time on Rust concepts. Read annotations in Layer 02 multiple times. Complete additional Rust exercises from *The Rust Book* chapters 4, 6, 10.

---

### Phase 2: Architecture (Weeks 3-5) → Fixer Tier

**Goal**: Master the Meaning Firewall — ICN's central design principle.

**Path files**:
1. `path/phase-2-architecture/04-actors-and-concurrency.md` — Actor pattern, shutdown coordination
2. `path/phase-2-architecture/05-the-meaning-firewall.md` — **KEYSTONE** — PolicyOracle boundary
3. `path/phase-2-architecture/06-persistence-and-ledger.md` — Storage as time-communication
4. **Gate 2** — 4 artifacts (firewall lab, trace diagram, boundary review, lock rationale)

**Labs**: lab-03-mini-actor, **lab-04-firewall-oracle** (critical), lab-05-mini-ledger

**Extra focus**: Lab-04 is the keystone. Spend extra time understanding compile-time boundary enforcement. Redo the lab from scratch if needed.

---

### Phase 3: Systems (Weeks 6-7) → Builder Tier

**Goal**: Integrate multiple subsystems in features.

**Path files**:
1. `path/phase-3-systems/07-identity-and-crypto.md` — Practical cryptography
2. `path/phase-3-systems/08-network-and-gossip.md` — Eventual consistency
3. `path/phase-3-systems/09-governance-and-contracts.md` — Democratic rule changes
4. **Gate 3** — 3 artifacts (real PR, peer review, teaching artifact)

**Labs**: lab-06-signed-envelope, lab-07-gossip-sync, lab-08-governance-flow

**Extra focus**: Ship your first real PR. Ask for review feedback early. Iterate based on maintainer comments.

---

### Phase 4: Ownership (Week 8) → Maintainer Tier

**Goal**: Review boundary PRs, reshape architecture, mentor.

**Path files**:
1. `path/phase-4-ownership/10-federation-and-ops.md` — Production deployment
2. `path/phase-4-ownership/11-maintainer.md` — Advanced Rust, hot path analysis
3. **Capstone** — 5 artifacts (deployment, ledger, governance, metrics, persistence)

**Labs**: Integrate all previous labs into deployed system.

**Extra focus**: Demonstrate end-to-end mastery. This is your portfolio piece.

---

## Weekly Schedule

| Week | Phase | Layers | Labs | Gate | Hours/week |
|------|-------|--------|------|------|------------|
| 1 | Phase 1 | 01-02 | lab-01 | — | 8-10 |
| 2 | Phase 1 | 03 + Gate 1 | lab-02 | ✓ | 8-10 |
| 3 | Phase 2 | 04 | lab-03 | — | 10-12 |
| 4 | Phase 2 | 05 | lab-04 | — | 12-15 |
| 5 | Phase 2 | 06 + Gate 2 | lab-05 | ✓ | 10-12 |
| 6 | Phase 3 | 07-08 | lab-06, lab-07 | — | 10-12 |
| 7 | Phase 3 | 09 + Gate 3 | lab-08 | ✓ | 10-12 |
| 8 | Phase 4 | 10-11 + Capstone | — | ✓ | 12-15 |

**Total**: 80-100 hours over 8 weeks

---

## Rust Learning Checkpoints

- **Week 1**: Understand ownership, borrowing, basic error handling
- **Week 2**: Write small CLI utilities, use `Result<T, E>` consistently
- **Week 3**: Understand `Arc`, `RwLock`, async/await basics
- **Week 4**: Reason about trait objects (`dyn Trait`), lifetimes in signatures
- **Week 5**: Implement actors with message passing
- **Week 6**: Read and write production-quality async Rust
- **Week 7**: Mentor others on Rust basics
- **Week 8**: Review Rust PRs for correctness and style

---

## Support Resources

- **Stuck on Rust?** → `reference/module-01-rust-fundamentals.md`
- **Need examples?** → `patterns.md` has code templates
- **Questions?** → #onboarding channel, ask early and often
- **Pair programming**: Request pairing sessions with Builders/Maintainers

---

## Next Steps

1. Read `docs/onboarding/manual.md` for system-first narrative
2. Start with `path/phase-1-foundations/01-environment.md`
3. Complete labs as you go, don't skip ahead
4. Submit gate artifacts for review before moving to next phase

Welcome to ICN! 🚀
