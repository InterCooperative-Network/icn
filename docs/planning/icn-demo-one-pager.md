# ICN Governance Demo: What It Is, What It Proves

**InterCooperative Network** | Matt Faherty | March 2026

---

## The Short Version

ICN is digital infrastructure for cooperative governance. Not a platform. Not a token. A coordination substrate that lets cooperatives make decisions, track consent, and prove accountability without centralized authority.

The governance demo runs a live Rust gateway and walks through the full lifecycle: three equal founding members bootstrap a cooperative, ratify their own charter, propose and vote on a $12,000 budget item, and generate a cryptographic proof of every decision. No admins. No privileged roles. Coordinator is elected, temporary, and rotating. Any member can propose. Any member can close a vote. Every action is signed and verifiable.

19 steps. 4 phases. All green.

## What the Demo Actually Does

**Phase 1 — Founding Assembly.** Three members generate Ed25519 cryptographic identities (decentralized identifiers, no central registry). They elect a founding coordinator — a temporary, rotating role, not a position of authority. The coordinator registers the cooperative and adds the other two founders as equal members.

**Phase 2 — Charter Ratification.** The cooperative's first democratic act: all three members vote to adopt their own governing rules. 50% quorum, simple majority, equal voting weight, 90-day coordinator terms. This retroactively legitimizes the bootstrap process. Unanimous ratification (3-0).

**Phase 3 — Democratic Decision.** Bob (not the coordinator) submits a budget proposal for community kitchen equipment. All three members vote with cryptographically signed ballots and written comments. Approved 3-0.

**Phase 4 — Verification.** Carol (not the coordinator, not the proposer) closes the vote. The system generates a tamper-proof cryptographic receipt: vote hash, decision hash, full tally. Anyone can verify the result independently. No trust required.

## Technical Stack

The demo runs against `icnd`, a Rust binary (~38 crates in the workspace). REST API with JWT authentication via Ed25519 challenge-response. Governance payloads support text, budget, membership, and config_change types. The gateway serves both the API and a browser-based demo at `/static/demo.html`. Everything is open source.

The demo script itself is pure Python with zero external dependencies beyond `cryptography` for Ed25519 signing. It generates fresh identities on every run — no hardcoded keys, no stored state.

---

## For the NY Cooperative Summit (Oct 2026)

**Context:** Tech/infrastructure track. Workshop or live demo session.

**What this shows the movement:** Cooperatives already know how to govern democratically. What they lack is digital infrastructure that reflects those values. Every tool cooperatives currently use for governance — Google Forms, email threads, Slack polls — was designed for corporations. ICN is governance infrastructure built from cooperative principles: equal voice, rotating authority, transparent decisions, cryptographic accountability.

**Workshop pitch (for May call for presenters):** "From Bylaws to Binary: Building Digital Infrastructure for Cooperative Governance." Live demo of the founding flow + discussion of how this applies to real cooperatives (food co-ops, worker co-ops, housing co-ops). Interactive segment where attendees design governance rules for a hypothetical cooperative using ICN's parameter system (quorum thresholds, voting periods, approval percentages).

**Why it matters for the summit audience:** The cooperative movement talks about democratic governance. This demo shows what it looks like when you engineer it into the infrastructure layer. The rules aren't in a PDF on a shelf. They're in the protocol. Every vote is signed. Every decision has a receipt. The coordinator can't override the quorum because the system won't let them.

---

## For TechCollective / Cooperative Job Applications

**Context:** Worker-owned MSP, actively hiring remote tech. Joe Marraffino intro (Dec 2025). Demonstrating technical range + cooperative alignment.

**What this shows about me:** I architected and built a 38-crate Rust workspace implementing cooperative governance from the protocol level up. The demo script handles Ed25519 key generation, challenge-response authentication, and a 19-step API integration test — all in pure Python. I debug at the binary level (traced a scope mismatch to a stale compiled binary vs. updated source code), navigate complex Docker/port conflicts in a homelab environment, and ship working software with proper git hygiene.

But the technical part isn't the point. The point is that I built this because I believe cooperative infrastructure should exist as a public good. I've been organizing the NY Cooperative Summit for two years. I'm a member of Rochester Cooperative Collective. I didn't learn about cooperatives from a textbook. I learned them from Joe Marraffino, from Cooperation Buffalo, from showing up to planning calls.

**Relevant skills demonstrated:**
- Rust systems programming (38-crate workspace, gateway server, crypto primitives)
- Python scripting and API integration
- Linux infrastructure (Proxmox homelab, K3s, VLANs, WireGuard, Docker)
- REST API design with scope-based authorization
- Cryptographic identity (Ed25519, DIDs, challenge-response auth)
- Git workflow, CI/CD, technical documentation

---

## For Grant Applications

**Context:** Sovereign Tech Fund (primary target, rolling), Outta Excuses ($500-$3K, Mar 31), Verizon Digital Ready ($10K, Mar 31).

**What this proves to funders:** This is not a whitepaper. This is working software. The governance demo runs against a live compiled binary, exercises real API endpoints, performs real cryptographic operations, and produces verifiable output. The 38-crate Rust workspace is on GitHub. The commit history shows sustained, serious development — not a weekend hackathon.

**For Sovereign Tech Fund specifically:** ICN fits their mandate for open source digital infrastructure. The governance layer demonstrates a novel approach: cooperative decision-making with cryptographic accountability, built on decentralized identifiers, with no dependency on blockchain or token economics. This is coordination infrastructure, not fintech.

**Key grant-ready claims (all verifiable from the demo):**
- Working REST API with JWT authentication and scope-based authorization
- Ed25519 decentralized identifiers — no central identity registry
- Democratic governance lifecycle: propose → vote → close → tally → cryptographic proof
- No privileged admin role — coordinator is elected, temporary, and constrained by protocol
- Charter ratification as the founding democratic act (the rules are voted on, not imposed)
- Open source, Rust, zero-dependency demo script
- Commit `ccd3f8a4` on main, verified on two separate machines (Zentith + icn-dev)

**What's next (for grant timelines):**
- Federation: multiple cooperatives coordinating across network boundaries
- Treasury integration: budget proposals that execute on approval
- Member identity portability: carry your DID across cooperatives
- Governance templates: pre-built profiles for common cooperative structures (worker, consumer, housing, multi-stakeholder)

---

*Built in the Finger Lakes. No venture capital. No tokens. No bosses.*
