# ICN: Digital Infrastructure for the Democratic Economy

**March 2026 | Matt Faherty | intercooperative.network**

---

Every cooperative, every community land trust, every mutual aid network faces the same problem. You make decisions together. You can't prove it.

A vote happens. Someone records it. You trust that what they wrote is what happened. For seven people in a room, that works. For a federation of fifty organizations coordinating across a state, it doesn't. For a grant funder evaluating whether your governance is real, meeting minutes are a document someone typed. For two organizations that want to work together but have never met, "trust us" is not a foundation.

And every tool you use to coordinate — Google Workspace, Loomio, Slack, QuickBooks — is rented from a company that doesn't share your values. The cooperative movement, built on the principle of democratic ownership, runs on infrastructure it doesn't own. That's a philosophical contradiction. It's also a practical vulnerability. The landlord can change terms, raise prices, or disappear. Your institutional records go with them.

ICN solves three problems that nothing else solves.

---

**You can prove your decisions.** When a vote happens through ICN, it produces a cryptographic receipt. The receipt contains a hash of the decision, signed attestations from every voter, and a verifiable chain linking the decision to the proposal that triggered it and the execution that followed it. Anyone with the receipt can verify it independently. No platform required. No authority required. The math proves it happened.

A grant funder verifying your governance. A federation partner verifying that your board approved a deal. A regulator auditing your compliance. None of them have to trust your word. They check the proof.

Nothing else in the cooperative ecosystem provides this. Loomio records votes. ICN proves them.

---

**You own your infrastructure.** ICN runs on hardware you control. A server. A Raspberry Pi. A laptop. The daemon runs on your machine. Your identity lives in your wallet. Your records live in your storage. No platform holds your data. No company operates the network. If you can run a process on a computer, you can run ICN.

Your institutional identity survives any platform's shutdown. Your decision history is immutable and locally stored. Your governance rules are enforced by your own infrastructure, not by a vendor's feature set.

---

**You can coordinate without surrendering.** ICN lets organizations discover each other, verify claims, and settle obligations across boundaries — without a common platform. Each organization runs its own daemon. Daemons talk to each other over encrypted peer-to-peer connections. Trust is established through verifiable participation, not through mutual membership in a platform.

A regional federation coordinates procurement for twenty cooperatives on infrastructure no single member controls. Two organizations that have never met verify each other's governance before entering an agreement. A cooperative and a community land trust federate across different governance models. Exit is free. Sovereignty is preserved.

---

**Who is this for?**

Any organization that governs itself democratically and wants to prove it.

Cooperatives are the first adopters — their legal structure requires democratic governance, and they have the clearest need for proof. But the infrastructure is equally relevant to community land trusts, mutual aid networks, neighborhood associations, worker centers, and civic organizations. If you make collective decisions, ICN is your infrastructure.

Individual members participate from their phones. A worker-owner votes on a wage proposal from the school pickup line. A community member endorses a project from the bus. Full cryptographic receipts. Same proofs as someone sitting at a desktop.

Developers and infrastructure operators run nodes that contribute to the network without joining any organization. The network grows because anyone with spare capacity can participate.

---

**How it works, briefly.**

ICN is a constraint engine. Eight mechanical primitives — identity, authorization, state, compute, communication, time, coordination, naming — form the kernel. The kernel enforces constraints: rate limits, capability checks, fuel budgets, quorum thresholds. It does this mechanically. It doesn't interpret what any of it means.

Applications sit on top and assign meaning. A governance app knows what a "vote" is. A mutual credit app knows what an "obligation" is. A membership app knows what a "cooperative" is. The kernel sees numbers, capability strings, and signed state transitions. This separation — the Meaning Firewall — is what makes ICN infrastructure rather than a product.

The kernel is like TCP/IP. TCP/IP doesn't know whether your packets are email or video. ICN doesn't know whether your state transitions are votes or purchases. The applications carry the meaning. The infrastructure carries the constraints.

---

**Why not just use existing tools?**

If proof doesn't matter, use Loomio. If ownership doesn't matter, use Google Workspace. If federation doesn't matter, keep emailing.

If proof matters, ownership matters, and federation matters — there is nothing else. No existing tool lets a democratic organization produce cryptographic proof of its decisions, own the infrastructure those decisions run on, and coordinate with other organizations without trusting a platform.

Blockchains require global consensus. ICN is local-first — your data stays on your hardware, verification is peer-to-peer, no mining, no chain. Web apps are landlords. ICN runs on your machine. ActivityPub federates social posts. ICN federates institutional decisions where proof of democratic authorization is the whole point.

---

**Where it stands.**

ICN is a 38-crate Rust workspace with 70+ real API endpoints, four working demo flows, TypeScript and React Native SDKs, and a K3s deployment running four federated cooperative nodes. Phase 0 and Phase 1 are complete. The project is roughly 75% to first external deployment.

The immediate path: a recorded demo and workshop proposal for the 2026 NY Cooperative Summit in October, Sovereign Tech Fund grant application, and the first pilot partnership with an upstate NY cooperative or community organization.

---

**The vision.**

The cooperative movement has spent 180 years building alternatives to capitalist enterprise. Worker cooperatives. Housing cooperatives. Consumer cooperatives. Credit unions. Community land trusts. The Rochdale Principles. The ICA. Mondragon. The solidarity economy.

All of it runs on infrastructure the movement doesn't own.

ICN changes that. Not by replacing every tool at once, but by providing a foundation that cooperatives, communities, and federations can build on together. Start with provable governance. Add federation when you're ready. Let the network grow because each participant benefits from being connected.

Prove your decisions. Own your records. Coordinate with your neighbors. No platform. No landlord. No trust required.

The math handles it.

---

**Contact**

Matt Faherty | matt.faherty@gmail.com
intercooperative.network | github.com/InterCooperative-Network/icn
