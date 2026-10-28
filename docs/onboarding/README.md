# Learn ICN's internals (optional curriculum)

This is the **deeper developer learning path**, not the entrance exam for contributing to ICN. You can report a bug, improve documentation, test an interface, discuss a cooperative use case, or submit a small PR without completing a course or earning a tier.

**New here and want to help right away? Start with [Getting Started](../GETTING_STARTED.md).** It offers useful first tasks for developers and non-developers, a focused technical setup, and a way to share results.

Use this curriculum if you want to understand why the system works as it does, practice Rust and ICN's architectural boundaries, or grow into more substantial implementation and review work.

## Choose a learning route

- **I want to make a first contribution:** go back to [Getting Started](../GETTING_STARTED.md). Pick a small task before diving into the architecture.
- **I'm new to Rust or distributed systems:** begin with [Phase 1: environment and repository navigation](path/phase-1-foundations/01-environment.md). Use the reference material as needed.
- **I know Rust and want ICN's architecture:** read the [Systems Manual](manual.md), then work through [Phase 2: the Meaning Firewall](path/phase-2-architecture/05-the-meaning-firewall.md), consulting the current code and domain owners.
- **I want a practical exercise:** choose a [lab](#hands-on-labs). Labs are learning sandboxes; they are not automatically outstanding project issues.
- **I need help with setup:** use the main [technical quickstart](../GETTING_STARTED.md#3-technical-path-get-a-development-environment-working) and [setup troubleshooting reference](reference/module-00-setup.md).

**Important:** Older course examples are teaching material, not proof of the current implementation. Check the live checkout, tests, [AGENTS.md](../../AGENTS.md), and relevant truth owners before treating an example as a current architectural or operational claim.

## The learning path

The four phases offer progressively deeper *practice*, not access levels or permissions:

| Phase | Main question | Example outcome |
| --- | --- | --- |
| [1. Foundations](path/phase-1-foundations/01-environment.md) — Reader | Can I build, inspect, and trace one behavior? | Small system sketch, focused test or useful bug report |
| [2. Architecture](path/phase-2-architecture/05-the-meaning-firewall.md) — Fixer | What boundaries must my change preserve? | Bounded fix with a regression test and rationale |
| [3. Systems](path/phase-3-systems/07-identity-and-crypto.md) — Builder | How do interacting components establish evidence and authority? | Scoped implementation or integration exercise |
| [4. Ownership](path/phase-4-ownership/10-federation-and-ops.md) — Maintainer | Can I safely review and maintain a subsystem? | Review, operations, or maintenance exercise |

The checkpoints in `path/` are optional self-study exercises. **Nobody needs to pass a gate, submit artifacts to a mentor, or receive a badge to open a contribution.** The right to participate in the project is not conferred by this course. Maintainer responsibilities and merge decisions follow the actual repository policies and project governance.

For deeper practice, the [Foundations track](tracks/foundations.md), [Accelerated track](tracks/accelerated.md), and [Contributor ladder](tracks/contributor-ladder.md) provide additional structure. Their pacing estimates are study plans, not required time commitments.

## Three kinds of material

### The path: what you follow

The [`path/`](path/) lessons connect concepts to code-reading and checkpoints:

- Phase 1: environment, Rust, errors and tracing
- Phase 2: actors, Meaning Firewall, persistence
- Phase 3: identity, networking, governance
- Phase 4: federation, operations, reviewing and maintenance

Start with [01: Environment Setup & Repository Navigation](path/phase-1-foundations/01-environment.md), or skip to the topic you need.

### Reference: what you look up

The [`reference/`](reference/) modules cover setup troubleshooting, Rust fundamentals, architecture, runtime actors, identity, trust, gossip, ledger, gateway/SDK, UI, operations, security, and governance.

Use the [reading map](reading-map.md) to locate relevant source files. If a code path has moved, prefer the current repository over an older file reference and report the stale pointer.

### Hands-on labs: what you build

The [`labs/`](labs/) directories contain separate practice exercises:

- Workspace setup, structured errors, and actors
- Meaning Firewall / policy-oracle boundaries
- A small ledger and signed envelopes
- Gossip synchronization and governance flows

The [Meaning Firewall lab](labs/lab-04-firewall-oracle/) is a useful deeper exercise, but not a prerequisite for opening a first PR. These labs do not automatically represent production-ready behavior.

## Getting a real contribution reviewed

Course exercises and repository contributions are related but distinct.

For a real change, start from a current issue or a verified gap, keep scope bounded, use the correct source-of-truth documents, and run the checks appropriate to the change. Follow [CONTRIBUTING.md](../../CONTRIBUTING.md) and [AGENTS.md](../../AGENTS.md). The main [Getting Started guide](../GETTING_STARTED.md#4-turn-the-result-into-a-contribution) walks through the first submission.

For an exploratory architecture question or real cooperative scenario, use [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions). For an informal question, use [Discord](https://discord.gg/sAEFCnEahn).

## Keeping this curriculum useful

If you discover a stale example, missing prerequisite, wrong command, or an idealized architecture claim presented as implemented behavior, that is valuable feedback. Open a discussion, a scoped issue, or a small correction PR. Keep this curriculum aligned with the living code and its truth owners instead of copying version numbers and runtime claims into multiple guides.

Other retained materials, including [the syllabus](syllabus.md), [assessments](assessments.md), [workshops](workshops/), and [capstone](capstone.md), are supplemental or historical learning resources. They do not override current contribution policies.

**You can help build ICN before you have learned all of ICN.**
