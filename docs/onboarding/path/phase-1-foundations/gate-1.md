# Gate 1: Becoming a Reader

> **Status**: Entry gate to **Reader** tier  
> **Required**: Complete Layers 01-03  
> **Time estimate**: 1-2 weeks (Foundations track) or 2-3 days (Accelerated track)

## What You've Learned

In Phase 1, you built **foundational fluency**:
- Environment: Build, test, lint without friction
- Rust: Ownership/borrowing as system invariants, not abstract rules
- Visibility: Errors, tracing, metrics as first-class concerns

You can now **read ICN code**, trace flows, and explain behavior.

## Requirements

Complete **all four** artifacts below. Each produces a concrete deliverable.

### Artifact 1: System Map Diagram

**What**: Draw the ICN architecture showing kernel/app/domain crate boundaries.

**Requirements**:
- Three distinct regions: Kernel, Apps, Domain
- Label at least 5 crates in each region (15 total)
- Show dependency flow: Apps import both Kernel and Domain, Kernel never imports Domain
- Hand-drawn sketch (photo) or digital diagram (Figma, Excalidraw, etc.)

**Example structure**:
```
┌─────────────────────────────────────┐
│         KERNEL (generic)            │
│  icn-core, icn-net, icn-gossip,    │
│  icn-store, icn-obs, icn-kernel-api │
└─────────────────────────────────────┘
         ↑           ↑           ↑
         │           │           │
┌────────┴───────────┴───────────┴────┐
│            APPS (oracles)            │
│  apps/trust, apps/ledger,            │
│  apps/governance                     │
└──────────────────────────────────────┘
         ↓           ↓           ↓
┌────────┴───────────┴───────────┴────┐
│      DOMAIN (data structures)        │
│  icn-trust, icn-ledger,              │
│  icn-governance, icn-ccl             │
└──────────────────────────────────────┘
```

**Verification**: Can you explain why `icn-core` cannot import `icn-ledger`?

---

### Artifact 2: Tooling Fluency

**What**: Prove you can navigate the build/test cycle without friction.

**Tasks**:
1. Clone the ICN repo (if not already done)
2. Run `cd icn && cargo build` — must succeed
3. Run `cargo test --workspace --lib` — must complete (some tests may fail, that's OK)
4. Introduce a deliberate compiler error (e.g., remove a semicolon)
5. Run `cargo build`, read the error, fix it
6. Screenshot or copy/paste the error + your fix

**Verification**: You understand compiler errors and can resolve them systematically.

---

### Artifact 3: Error/Tracing Proof

**What**: Demonstrate understanding of ICN's error handling and observability.

**Tasks**:
1. Locate `icn/crates/icn-core/src/error.rs` and explain the thiserror pattern used
2. Explain the difference between `#[error("...")]` and `#[error(transparent)]`
3. Add a new tracing span to your lab-02 workspace code
4. Run the code, capture the tracing output, highlight the span you added

**Verification**: You can instrument code for visibility.

---

### Artifact 4: Tiny PR

**What**: Make a small, meaningful contribution to ICN.

**Options** (choose one):
- Fix a typo in documentation
- Add a tracing span to improve observability
- Improve a test description or add a missing test case
- Add a code comment explaining a non-obvious invariant

**Requirements**:
- PR must follow commit format: `type(scope): description` (see `CONTRIBUTING.md`)
- PR description explains **why** the change improves the codebase
- Code must pass `cargo fmt` and `cargo clippy`

**Note**: This is a *tiny* PR — 1-10 lines changed. The goal is to practice the contribution workflow, not to ship a feature.

**Verification**: You can navigate the GitHub PR workflow and pass CI checks.

---

## After Passing

**Congratulations!** You are now a **Reader**.

You can:
- ✅ Run tests and trace execution flows
- ✅ Read Rust code and identify invariants
- ✅ Explain error handling patterns
- ✅ File well-structured bug reports with repro steps
- ✅ Navigate the contribution workflow

**Next step**: Proceed to **Phase 2: Architecture** to become a **Fixer**.

---

## Submission

Submit your four artifacts to your mentor or in the #onboarding channel:
1. System map diagram (image)
2. Compiler error trace + fix (screenshot or code block)
3. Tracing span proof (code + output)
4. Link to merged or open PR

Once reviewed and approved, you'll receive the **Reader** badge and can continue to Phase 2.

---

## Need Help?

- **Stuck on builds?** → `reference/module-00-setup.md` has troubleshooting steps
- **Rust confusion?** → `reference/module-01-rust-fundamentals.md` has deep dives
- **Tracing unclear?** → `reference/module-12-observability.md` has examples

Don't hesitate to ask questions in #onboarding or open a discussion thread!
