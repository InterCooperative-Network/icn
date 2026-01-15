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

## Who is this curriculum for?

**Foundations Track** (5-6 weeks)
- New to Rust or haven't used it in production
- Familiar with programming concepts from other languages
- Want to learn Rust idioms through real-world ICN code

**Accelerated Track** (3-4 weeks)
- Comfortable with Rust ownership, lifetimes, and async
- Experience with distributed systems or P2P networking
- Want to focus on ICN architecture and contribute quickly

## Curriculum Structure

### Core Modules (0-10)
| Module | Topic | What You'll Learn |
|--------|-------|-------------------|
| 0 | Setup | Build environment, tooling, project structure |
| 1 | Rust Fundamentals | Ownership, error handling, async patterns |
| 2 | Architecture | Layer stack, crate responsibilities, integration points |
| 3 | Runtime & Actors | Supervisor, actor lifecycle, shutdown coordination |
| 4 | Identity & Trust | DIDs, keystore, trust graphs, key rotation |
| 5 | Network & Gossip | QUIC transport, mDNS, topic subscriptions |
| 6 | Ledger & Contracts | Mutual credit, Merkle-DAG, CCL execution |
| 7 | Gateway & SDK | REST API, WebSocket, JWT auth, SDK usage |
| 8 | Web UI | Pilot UI, data flow, session handling |
| 9 | Operations | Deployment, monitoring, production hardening |
| 10 | Contributing | Tests, CI, PR workflow, git conventions |

### Advanced Module
| Module | Topic | What You'll Learn |
|--------|-------|-------------------|
| 11 | Federation | Inter-coop agreements, clearing, netting, attestations |

### Supporting Materials
- **Workshops**: Hands-on exercises for each module with checkpoints
- **Assessments**: Quick knowledge checks to verify understanding
- **Patterns**: Common code patterns used throughout ICN
- **Capstone**: Final project integrating multiple concepts

## Learning Path

```
┌──────────────────────────────────────────────────────────────────┐
│                         FOUNDATIONS                               │
│  Week 1: Setup + Rust    Week 2: Architecture + Runtime          │
│  Week 3: Identity + Net  Week 4: Ledger                          │
│  Week 5: Gateway + UI    Week 6: Ops + Contributing + Capstone   │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                         ACCELERATED                               │
│  Week 1: Setup → Runtime    Week 2: Identity → Ledger            │
│  Week 3: Gateway + UI       Week 4: Ops + Contributing + Capstone│
└──────────────────────────────────────────────────────────────────┘
```

## How to Use This Curriculum

### Step 1: Choose Your Track
Read `syllabus.md` and select Foundations or Accelerated based on your
background.

### Step 2: Follow Modules in Order
Each module builds on previous ones. Don't skip ahead unless you're confident
in the prerequisites.

### Step 3: Complete Workshops
After each module, do the corresponding workshop. These hands-on exercises
solidify understanding and build practical skills.

### Step 4: Check Understanding
Use `assessments.md` to verify you've grasped key concepts before moving on.

### Step 5: Reference Patterns
Consult `patterns.md` when reading unfamiliar code or writing new features.

### Step 6: Complete the Capstone
The capstone project integrates concepts from multiple modules into a
meaningful contribution.

## File Structure

```
docs/onboarding/
├── README.md           # This file - start here
├── syllabus.md         # Course outline and pacing
├── patterns.md         # Common code patterns reference
├── assessments.md      # Quick knowledge checks
├── capstone.md         # Final integrative project
├── reading-map.md      # Module-to-code cross-references
├── modules/            # Lesson content
│   ├── module-00-setup.md
│   ├── module-01-rust-fundamentals.md
│   ├── ...
│   └── module-11-federation.md
├── workshops/          # Hands-on exercises
│   ├── workshop-00-setup.md
│   ├── workshop-01-rust-fundamentals.md
│   ├── ...
│   └── workshop-11-federation.md
└── tracks/             # Track-specific guidance
```

## Quick Start Commands

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

## Contributor Notes

If you update this curriculum:
- Keep lessons aligned with the current codebase
- Ensure all code snippets compile and run
- Test workshops on a fresh environment
- Update `reading-map.md` when file paths change
- Follow the template in `module-template.md` for new lessons
- Document changes in `update-process.md`
