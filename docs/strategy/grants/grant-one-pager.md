# ICN: Digital Public Infrastructure for the Cooperative Economy

**InterCooperative Network** | Matt Faherty | intercooperative.network | March 2026

---

## The Problem

Democratic organizations cannot prove their decisions happened. A cooperative votes on a budget. Someone writes it down. A grant funder, a federation partner, or a regulator has to take their word for it. Meeting minutes are a document someone typed. For seven people in a room, this works. For a federation of fifty organizations coordinating across a state, it doesn't.

Meanwhile, the cooperative movement runs on infrastructure it doesn't own. Google Workspace, Loomio, Slack, QuickBooks. Every tool is rented from a company that can change terms, raise prices, or disappear. Democratic organizations built on the principle of ownership run on platforms they'll never control.

## The Solution

ICN is a peer-to-peer coordination substrate that lets cooperatives and community organizations:

1. **Prove decisions.** Every governance action produces a cryptographic receipt. A vote, a budget allocation, a federation agreement. Anyone with the receipt can verify it independently. No platform required.

2. **Own infrastructure.** ICN runs on hardware you control. Your identity, your records, your governance rules. No vendor holds your data.

3. **Coordinate without surrendering.** Organizations discover each other, verify claims, and settle obligations across boundaries without a common platform. Exit is free.

## What Exists Today

| Metric | Value |
|--------|-------|
| Rust codebase | 451K lines, 34 crates |
| Test suite | 5,933 passing tests |
| API surface | 70+ REST endpoints |
| Demo flows | 4 (governance, patronage, federation, reporting) |
| SDKs | TypeScript + React Native |
| Deployment | 4-node K3s cluster running federated demo |
| Completion | ~75% to first external deployment |

**Proven in CI:** Complete 6-link receipt chain (proposal -> vote -> decision -> allocation -> execution -> audit) verified end-to-end in automated tests.

**Proven on K3s:** Governance flow (Flow 1) and federation flow (Flow 3) run clean against live cluster. Patronage and reporting flows updated and pending redeploy verification.

## Architecture

ICN implements a **constraint enforcement architecture** with a Meaning Firewall:

- **Apps** (governance, mutual credit, membership) translate domain semantics into generic constraints
- **Kernel** enforces constraints mechanically without understanding what they mean
- This separation makes ICN infrastructure, not a product. Like TCP/IP doesn't know whether your packets are email or video, ICN doesn't know whether your state transitions are votes or payments

This architecture is not theoretical. It is enforced by CI gates that reject code violating the boundary.

## Who It's For

Worker cooperatives, housing cooperatives, community land trusts, mutual aid networks, cooperative federations, and any organization that governs itself democratically and needs to prove it.

**First pilot target:** Upstate New York cooperative ecosystem, supported by the NY Cooperative Summit network (3rd annual summit planned October 2026).

## What Funding Enables

| Phase | Timeline | Deliverable |
|-------|----------|-------------|
| Demo polish + grant prep | Now - Mar 31 | Recorded demo, grant submissions |
| Pilot deployment | Apr - Jun 2026 | First external cooperative running ICN |
| Mobile member UX | May - Jul 2026 | Phone-based voting and receipt verification |
| Summit workshop | Oct 2026 | Live demonstration at NY Cooperative Summit |
| Federation pilot | Q4 2026 | 3-5 cooperatives federated on ICN |

## The Team

**Matt Faherty** — Sole architect and developer. Background in cooperative organizing (NY Cooperative Summit co-organizer, 2 years running), software architecture, and sports science pedagogy. 12 years of coaching experience translating complex systems into accessible practice. CompTIA A+ certified. Building ICN full-time with AI-augmented development.

**Advisory network:** Joe Marraffino (Cooperative Fund of New England), Frank Cetera (Institute for the Cooperative Digital Economy), Rochester Cooperative Collective, ACCES-VR vocational rehabilitation program.

## Contact

Matt Faherty | matt.faherty@gmail.com | 585-506-8579
GitHub: github.com/InterCooperative-Network/icn
Web: intercooperative.network
