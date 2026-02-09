# ICN Developer Onboarding Curriculum

Welcome to the InterCooperative Network (ICN) developer onboarding program. This
structured curriculum will take you from zero to productive ICN contributor,
whether you're new to Rust or an experienced systems programmer.

## What is ICN?

ICN is a **decentralized coordination substrate for cooperative organizations**.
Unlike blockchains that focus on trustless consensus, ICN focuses on:

- **Trust-based coordination**: Social relationships inform system behavior
- **Cooperative economics**: Mutual credit, clearing, and fair value exchange
- **Federation**: Independent cooperatives working together across boundaries
- **Privacy and sovereignty**: Members control their own data and identity

The system is built in Rust using an actor-based architecture, with components
for identity, trust, networking, gossip synchronization, ledger accounting,
smart contracts, and inter-cooperative federation.

## Start Here

### 1. Read the ICN Systems Manual

If you want a comprehensive, system-first explanation of ICN (how and why it
works), start with:

→ **`docs/onboarding/manual.md`**

### 2. Follow the Path Curriculum

The **`path/`** directory contains the primary learning path — phase-gated,
artifact-driven curriculum organized around the **Meaning Firewall** as its
narrative spine.

→ **Start here**: `path/phase-1-foundations/01-environment.md`

## Three Content Types

### **`path/`** — What You Follow

Phase-gated curriculum with concrete checkpoints:

- **Phase 1: Foundations** → Reader tier (build, test, trace)
- **Phase 2: Architecture** → Fixer tier (master the Meaning Firewall)
- **Phase 3: Systems** → Builder tier (ship features)
- **Phase 4: Ownership** → Maintainer tier (reshape architecture)

Each phase ends with a **gate checkpoint** requiring concrete artifacts, not self-assessment.

### **`reference/`** — What You Consult

Deep-dive technical references (formerly `modules/`):

- `reference/module-00-setup.md` — Full toolchain setup, troubleshooting
- `reference/module-01-rust-fundamentals.md` — Ownership, lifetimes, async
- `reference/module-02-architecture-overview.md` — System architecture
- `reference/module-03-runtime-actors.md` — Actor runtime, supervisor
- `reference/module-04-identity-trust.md` — DIDs, keystore, trust graph
- `reference/module-05-network-gossip.md` — QUIC, mDNS, gossip protocol
- `reference/module-06-ledger-contracts.md` — Ledger, Merkle-DAG, CCL
- `reference/module-07-gateway-sdk.md` — REST API, WebSocket, SDK
- `reference/module-08-web-ui.md` — Pilot UI, data flow
- `reference/module-09-ops-deploy.md` — Deployment, monitoring
- `reference/module-10-contributor-workflow.md` — Tests, CI, PR workflow
- `reference/module-11-federation.md` — Inter-coop coordination
- `reference/module-12-observability.md` — Metrics, tracing, logging
- `reference/module-13-security-privacy.md` — Security patterns, threat model
- `reference/module-14-governance-ccl-deep-dive.md` — Governance, CCL semantics

### **`labs/`** — What You Build

Hands-on exercises with "Done when" acceptance criteria:

- `labs/lab-01-workspace/` — Build a 2-crate workspace
- `labs/lab-02-error-receipt/` — Structured errors and tracing
- `labs/lab-03-mini-actor/` — Actor with bounded mailbox
- **`labs/lab-04-firewall-oracle/`** — **KEYSTONE LAB** — Meaning Firewall boundary
- `labs/lab-05-mini-ledger/` — Double-entry ledger with quarantine
- `labs/lab-06-signed-envelope/` — Cryptographic signatures + replay protection
- `labs/lab-07-gossip-sync/` — Vector clocks + anti-entropy
- `labs/lab-08-governance-flow/` — Proposal → vote → constraint update

## Contributor Ladder

ICN onboarding follows a **ladder model**: each tier unlocks new responsibilities.

| Tier | Phase | Skills | Gate Artifacts |
|------|-------|--------|----------------|
| **Reader** | Phase 1 | Build, test, trace flows | 4 artifacts |
| **Fixer** | Phase 2 | Fix bugs, respect boundaries | 4 artifacts |
| **Builder** | Phase 3 | Add features, design changes | 3 artifacts |
| **Maintainer** | Phase 4 | Review PRs, mentor, own subsystems | 5 artifacts (capstone) |

→ See `tracks/contributor-ladder.md` for full details.

## Choose Your Track

### Foundations Track (8 weeks)
For developers new to Rust or those wanting comprehensive mastery.

→ `tracks/foundations.md`

**Pacing**: 8-10 hours/week, 80-100 hours total

### Accelerated Track (4 weeks)
For developers already comfortable with Rust and async systems.

→ `tracks/accelerated.md`

**Pacing**: 15-20 hours/week, 60-80 hours total

## File Structure

```
docs/onboarding/
├── README.md           # This file — start here
├── manual.md           # System-first textbook manual
├── patterns.md         # Common code patterns reference
├── syllabus.md         # Course outline (legacy, superseded by path/)
├── assessments.md      # Quick knowledge checks
├── capstone.md         # Final integrative project (legacy, see path/phase-4-ownership/capstone.md)
├── reading-map.md      # Module-to-code cross-references
├── path/               # PRIMARY: Phase-gated curriculum
│   ├── phase-1-foundations/
│   │   ├── 01-environment.md
│   │   ├── 02-rust-through-icn.md
│   │   ├── 03-errors-and-tracing.md
│   │   └── gate-1.md
│   ├── phase-2-architecture/
│   │   ├── 04-actors-and-concurrency.md
│   │   ├── 05-the-meaning-firewall.md  ← KEYSTONE
│   │   ├── 06-persistence-and-ledger.md
│   │   └── gate-2.md
│   ├── phase-3-systems/
│   │   ├── 07-identity-and-crypto.md
│   │   ├── 08-network-and-gossip.md
│   │   ├── 09-governance-and-contracts.md
│   │   └── gate-3.md
│   └── phase-4-ownership/
│       ├── 10-federation-and-ops.md
│       ├── 11-maintainer.md
│       └── capstone.md
├── labs/               # Hands-on exercises
│   ├── lab-01-workspace/
│   ├── lab-02-error-receipt/
│   ├── lab-03-mini-actor/
│   ├── lab-04-firewall-oracle/  ← KEYSTONE LAB
│   ├── lab-05-mini-ledger/
│   ├── lab-06-signed-envelope/
│   ├── lab-07-gossip-sync/
│   └── lab-08-governance-flow/
├── reference/          # Deep-dive technical references
│   ├── module-00-setup.md
│   ├── module-01-rust-fundamentals.md
│   ├── ...
│   └── module-14-governance-ccl-deep-dive.md
├── workshops/          # Legacy workshop materials (retained)
└── tracks/             # Track-specific guidance
    ├── contributor-ladder.md
    ├── foundations.md
    └── accelerated.md
```

## Quick Start Commands

If you are ready to build and run the codebase, use these. Otherwise, start
with `manual.md` and the path curriculum.

```bash
# Clone and build
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build

# Run tests
cargo test --workspace --lib

# Start the daemon (requires initialized identity)
export ICN_PASSPHRASE="your-passphrase"
./target/debug/icnctl id init
./target/debug/icnd

# Format and lint before contributing
cargo fmt --all
cargo clippy --workspace
```

## Getting Help

- **Issues**: Open a GitHub issue for bugs or questions
- **Discussions**: Use GitHub Discussions for design conversations
- **Contributing**: See `CONTRIBUTING.md` for PR guidelines
- **Onboarding chat**: #onboarding channel (if available)

## Contributor Notes

If you update this curriculum:
- Keep lessons aligned with the current codebase
- Ensure all code snippets compile and run
- Test workshops on a fresh environment
- Update `reading-map.md` when file paths change
- Follow the template in `path/` files for new layers
- Document changes in `update-process.md`

---

**Welcome to ICN!** Start with `manual.md` for the system narrative, then proceed to `path/phase-1-foundations/01-environment.md` to begin your journey. 🚀
