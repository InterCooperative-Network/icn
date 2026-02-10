# ICN Contributor Ladder

The ICN onboarding follows a **ladder model**: each tier unlocks new responsibilities and requires demonstrable skills via gate checkpoints.

## Overview

| Tier | Phase | Skills | Artifacts Required |
|------|-------|--------|---------------------|
| **Reader** | Phase 1 | Build, test, trace flows, explain code | 4 artifacts (system map, tooling, errors, tiny PR) |
| **Fixer** | Phase 2 | Fix bugs, write tests, respect boundaries | 4 artifacts (firewall lab, trace diagram, boundary review, lock rationale) |
| **Builder** | Phase 3 | Add features, implement actors/oracles, design changes | 3 artifacts (real PR, peer review, teaching artifact) |
| **Maintainer** | Phase 4 | Review boundary PRs, reshape architecture, mentor | 5 artifacts (deployment, ledger, governance, metrics, persistence) |

---

## Tier 1: Reader

**Goal**: Navigate codebase, trace execution, file good bug reports.

**Skills**:
- Run `cargo build`, `cargo test`, `cargo clippy` without friction
- Read Rust code and identify invariants (ownership, borrowing, error handling)
- Explain error handling patterns (thiserror vs anyhow vs panic)
- Trace execution through tracing output
- Understand kernel/app/domain crate boundaries

**Gate Checkpoint**: Gate 1 (4 artifacts)
1. System map diagram showing kernel/app/domain split
2. Tooling fluency (build → error → fix cycle)
3. Error/tracing proof (explain thiserror pattern, add tracing span)
4. Tiny PR (typo, test, doc, tracing span)

**After passing**: Can read code, trace flows, explain behavior, file bug reports.

---

## Tier 2: Fixer

**Goal**: Fix bugs, write tests, ship bugfix PRs respecting boundaries.

**Skills**:
- Understand actor pattern (mailbox, sequential processing, shutdown)
- **Master the Meaning Firewall**: kernel/app separation via PolicyOracle
- Implement persistence with invariants (double-entry, quarantine)
- Add tracing spans and metrics to improve observability
- Choose between `parking_lot::RwLock` and `tokio::sync::RwLock`

**Gate Checkpoint**: Gate 2 (4 artifacts)
1. Lab-04 complete (firewall oracle with compile-time boundary proof)
2. End-to-end trace diagram (Gateway → Kernel → Oracle → constraints → enforcement)
3. Boundary review exercise (identify and fix firewall violation)
4. Lock choice rationale (when to use parking_lot vs tokio::sync)

**After passing**: Can fix bugs, write tests, add tracing, ship bugfix PRs.

---

## Tier 3: Builder

**Goal**: Add features, implement new actors/oracles, design subsystem changes.

**Skills**:
- Practical cryptography (sign, verify, replay protection)
- Gossip protocol (eventual consistency, vector clocks, anti-entropy)
- Governance (proposals, voting, parameter changes)
- Integrate multiple subsystems in a single feature
- Mentor junior contributors

**Gate Checkpoint**: Gate 3 (3 artifacts)
1. Real PR shipped (touches 2+ crates, includes tracing + tests)
2. Peer review (review another PR using boundary-safety rubric)
3. Teaching artifact (diagram, checklist, or mini-lesson for future contributors)

**After passing**: Can add features, implement actors/oracles, design subsystem changes.

---

## Tier 4: Maintainer

**Goal**: Review boundary PRs, reshape architecture, mentor contributors.

**Skills**:
- Federation (inter-cooperative coordination, clearing/netting)
- Operations (deployment, monitoring, incident response)
- Advanced Rust (lifetimes when they matter, trait objects vs generics)
- Hot path profiling and optimization
- Maintainer review rubric (firewall, errors, observability, tests, docs)

**Gate Checkpoint**: Capstone (5 artifacts)
1. Deployment proof (2-node cluster, metrics exposed)
2. Ledger flow (transaction via Gateway API, replicated)
3. Governance flow (proposal activated, parameter changed, oracle updated)
4. Metrics verification (Prometheus shows activity)
5. Persistence verification (state survives restart)

**After passing**: Can review boundary PRs, reshape architecture, mentor contributors, own subsystems.

---

## Progression Timeline

### Foundations Track (8 weeks total)
- **Phase 1 (Weeks 1-2)**: Reader tier
- **Phase 2 (Weeks 3-5)**: Fixer tier
- **Phase 3 (Weeks 6-7)**: Builder tier
- **Phase 4 (Week 8)**: Maintainer tier

### Accelerated Track (4 weeks total)
- **Phase 1 (Days 1-3)**: Reader tier (skim if Rust-fluent)
- **Phase 2 (Week 1)**: Fixer tier
- **Phase 3 (Weeks 2-3)**: Builder tier
- **Phase 4 (Week 4)**: Maintainer tier

---

## Key Principles

1. **Artifact-driven**: Each gate requires concrete deliverables, not self-assessment.
2. **Boundary-first**: The Meaning Firewall is the central design principle — mastery comes in Phase 2.
3. **Incremental**: Each tier builds on the previous. No skipping gates.
4. **Mentorship**: Builders mentor Fixers, Maintainers mentor Builders.
5. **Onboarding produces PR-capable humans**, not just readers.

---

## Next Steps

- **Start onboarding**: `docs/onboarding/path/phase-1-foundations/01-environment.md`
- **Choose track**: `docs/onboarding/tracks/foundations.md` or `accelerated.md`
- **Read manual**: `docs/onboarding/manual.md` for system-first narrative
- **Ask questions**: #onboarding channel or GitHub Discussions
