# Gate 3: Becoming a Builder

> **Status**: Promotion gate to **Builder** tier  
> **Required**: Complete Layers 07-09  
> **Time estimate**: 2-3 weeks (Foundations track) or 1-2 weeks (Accelerated track)

## What You've Learned

In Phase 3, you mastered **ICN's subsystems**:
- Identity & Crypto: Practical cryptography, replay protection
- Network & Gossip: Eventual consistency, vector clocks
- Governance & Contracts: Democratic rule changes, bounded execution

You can now **build features**, implement new actors/oracles, and design subsystem changes.

## Requirements

Complete **all three** artifacts below. Each demonstrates systems-level understanding.

### Artifact 1: Real PR Shipped

**What**: Ship a meaningful pull request touching multiple crates.

**Requirements**:
- Bug fix, test improvement, or small feature
- Touches at least 2 crates (e.g., actor + storage, or API + domain)
- Includes tracing spans and metrics
- Includes tests proving correctness
- Passes CI (fmt, clippy, tests)
- Follows commit format and PR conventions

**Examples**:
- Fix a race condition in gossip sync (icn-gossip + tests)
- Add new metric to ledger validation (icn-ledger + icn-obs)
- Improve error messages in governance (icn-governance + icn-api)

**Verification**: PR merged or approved by maintainer.

---

### Artifact 2: Peer Review

**What**: Review another contributor's PR using the boundary-safety rubric.

**Requirements**:
- Choose a PR that touches kernel or app crates
- Check for Meaning Firewall violations (kernel importing domain)
- Check error handling (no unwrap/expect in protocol paths)
- Check observability (tracing spans, metrics)
- Check tests (coverage, edge cases)
- Leave constructive feedback

**Verification**: Your review comment demonstrating the rubric checks.

---

### Artifact 3: Teaching Artifact

**What**: Create one reusable diagram, checklist, or mini-lesson for future contributors.

**Options**:
- Diagram: Actor message flow (e.g., Gossip ↔ Network communication)
- Checklist: "Adding a new actor" step-by-step
- Mini-lesson: "When to use parking_lot vs tokio::sync locks"
- Debugging guide: "How to trace a missing gossip entry"

**Requirements**:
- Practical, specific, actionable
- Includes code examples or real file paths
- Submitted as markdown in `docs/onboarding/resources/` or in #onboarding

**Verification**: Your teaching artifact helps another contributor solve a problem.

---

## After Passing

**Congratulations!** You are now a **Builder**.

You can:
- ✅ Add features and new subsystems
- ✅ Implement new actors and oracles
- ✅ Design subsystem changes respecting boundaries
- ✅ Mentor junior contributors
- ✅ Review PRs for architectural correctness

**Next step**: Proceed to **Phase 4: Ownership** to become a **Maintainer**.

---

## Submission

Submit your three artifacts to your mentor or in the #onboarding channel:
1. Link to merged or approved PR
2. Link to your peer review comment
3. Your teaching artifact (diagram, checklist, or guide)

Once reviewed and approved, you'll receive the **Builder** badge and can continue to Phase 4.

---

## Need Help?

- **Feature unclear?** → Ask in #development before starting
- **Review confused?** → `CONTRIBUTING.md` has review guidelines
- **Teaching artifact ideas?** → Look at `docs/onboarding/patterns.md` for inspiration

You're now contributing meaningfully — keep going!
