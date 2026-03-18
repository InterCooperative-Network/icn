# What ICN Is

**March 17, 2026**

---

## One Sentence

ICN is a P2P daemon that lets democratic organizations prove their decisions happened, own their own infrastructure, and coordinate with each other — without trusting a platform.

---

## The Problem

Democratic organizations run on trust. A vote happens in a meeting. Someone writes it down. Everyone agrees that's what happened. This works when there are seven people in a room.

It stops working at scale. When a federation of 50 cooperatives makes a budget decision affecting 2,000 members, "someone wrote it down" is not sufficient. When a community land trust needs to prove to a municipality that its governance is legitimate, meeting minutes are a document someone typed. When Cooperative A and Community B want to settle a dispute about a shared resource, neither trusts the other's records. When a member checks their phone to vote on a proposal, they're trusting a platform they don't control to count it correctly.

Today, democratic organizations solve this by renting infrastructure from platforms that don't share their values. Google Workspace for documents. Loomio for voting. QuickBooks for accounting. Slack for coordination. Each one is a landlord. Each one can change terms, raise prices, or shut off access. For movements built on the principle of democratic ownership, every rented platform is a philosophical contradiction that becomes a practical problem the moment the landlord decides to act like one.

There is no way — today — for a cooperative, a community organization, or a federation of either to:

1. **Prove** that a democratic decision happened to someone who wasn't there, without that person having to trust the organization's word for it
2. **Own** the infrastructure those decisions run on, rather than renting it from a platform
3. **Coordinate** with other organizations across boundaries without both surrendering to the same landlord

ICN makes all three possible.

---

## What ICN Provides

### Provable Decisions

When a vote happens through ICN, it produces a **cryptographic receipt**. The receipt contains: a hash of the decision, signed attestations from every voter, and a verifiable chain linking the decision to the proposal that triggered it and the execution that followed it.

Anyone with the receipt can verify it independently. No platform required. No authority required. No trust required. The math proves it happened.

This is not a feature of a governance app. It is a property of the infrastructure. Every state transition in ICN is deterministic, content-addressed, and signed. The system cannot produce a valid receipt for a decision that didn't happen. It cannot alter a receipt after the fact without breaking the hash chain.

**What this enables that can't be done otherwise:**
- A grant funder can verify that an organization's governance is democratic — not by reading meeting minutes, but by checking cryptographic proofs
- A federation member can verify that a budget allocation was properly voted on before their cooperative's money was spent
- A municipality can audit a community land trust's compliance without trusting the organization's self-reporting
- Two organizations that don't trust each other can verify each other's claims about internal decisions
- A member on their phone can cast a vote and receive a receipt proving it was counted — no platform required

### Sovereign Infrastructure

ICN runs on hardware you control. The daemon (`icnd`) runs on your machine. Your identity lives in your wallet. Your organizational records live in your storage. Your governance rules live in your charter.

No platform holds your data. No company operates the network. No one can shut it down, change the terms, or revoke access. If you can run a process on a computer, you can run ICN.

**What this enables that can't be done otherwise:**
- An organization's institutional identity survives any platform's shutdown or policy change
- Decision history is immutable and locally stored — not dependent on a third party's data retention policy
- Governance rules are self-enforced by your own infrastructure, not by a vendor's feature set
- The democratic economy — cooperatives, community organizations, and their federations — can build shared digital infrastructure that it actually owns, governed by the same principles it advocates for everything else
- A developer or infrastructure operator can run a node that contributes to the network without being tied to any single organization

### Federation Without Surrender

ICN lets organizations discover each other, verify each other's claims, and settle obligations across boundaries — without requiring a common platform. Each organization runs its own daemon. Daemons talk to each other over encrypted peer-to-peer connections. Trust is established through verifiable participation, not through mutual membership in a platform.

Federation is opt-in. Exit is free. Sovereignty is preserved.

**What this enables that can't be done otherwise:**
- A regional federation can coordinate procurement, share resources, and settle obligations across member organizations on infrastructure no single member controls
- Two organizations that have never met can verify each other's governance and trust scores before entering an agreement
- A national network of cooperatives and communities can exist as a coordination layer, not as a platform that members depend on
- The democratic economy can develop network effects (the more organizations participate, the more valuable participation becomes) without those network effects being captured by a platform
- A community organization and a cooperative can federate and settle obligations across their different governance models — because the kernel doesn't care what governance model you use

---

## How It Works (The Kernel)

ICN achieves these three properties through a constraint engine with eight mechanical primitives:

| Primitive | What it does |
|---|---|
| **Identity** | Create, sign, verify, resolve decentralized identifiers |
| **Authorization** | Issue and check unforgeable capability tokens |
| **State** | Append-only logs, blob storage, KV — content-addressed, tamper-evident |
| **Compute** | Execute deterministic sandboxed programs with fuel metering |
| **Communication** | Pub/sub, request/response, streaming over QUIC/TLS |
| **Time** | Logical clocks, scheduling, leases |
| **Coordination** | Consensus groups, CRDTs, deterministic conflict resolution |
| **Naming** | Hierarchical name resolution, service discovery |

The kernel enforces constraints on these primitives mechanically. It does not interpret what they're used for. This separation — the **Meaning Firewall** — is what makes ICN infrastructure rather than a product.

---

## The Meaning Firewall

This is the load-bearing architectural decision.

The kernel does not know what any of those eight primitives are being used for. It doesn't know that a state log entry is a "vote." It doesn't know that a compute execution is "distributing patronage." It doesn't know that a communication message is a "governance proposal." It doesn't know that a naming resolution is looking up a "cooperative."

The kernel enforces constraints: rate limits, capability checks, fuel budgets, credit limits, quorum thresholds. It enforces them mechanically. It does not interpret them semantically.

**Applications** sit above the Meaning Firewall. They assign meaning to the primitives:

```
APP: "This cooperative uses consent-based governance
      with a 72-hour discussion period and 75% approval."

        ↓ translates into ↓

KERNEL: ConstraintSet {
    rate_limit: 100/sec,
    min_duration: 259200,   // 72 hours in seconds
    threshold: 0.75,
    capability_required: "governance:vote"
}
```

The app knows what "consent-based governance" means. The kernel sees numbers and capability strings. The boundary between them is the `PolicyOracle` interface — apps implement it, the kernel calls it, and the kernel never looks inside the meaning.

**Why this matters:**

If the kernel interprets meaning, ICN is a product. A governance platform. A payment processor. A voting system. Each of those is a regulated category with specific legal obligations.

If the kernel is mechanical infrastructure — like TCP/IP, like the Linux kernel, like electricity — then it's a public utility. The applications built on it carry their own regulatory obligations. The infrastructure itself is neutral.

This is not clever framing. It's architecture. The dependency graph enforces it: kernel crates cannot import domain crates. CI checks it. `kernel_surface.toml` audits 29 crates: 9 clean, 11 currently infected (semantic business logic being extracted), 9 needs-review.

The cleanup of those 11 infected crates is the single most important technical task in the project right now.

---

## The Entity Model

ICN recognizes four types of participants. All of them interact with the same eight primitives. All of them are first-class:

**Person** — A self-sovereign identity. Holds their own keys in a local wallet. Participates from a desktop daemon, a mobile client, or both. The kernel sees a DID and a set of capabilities. What makes this entity a "person" is meaning assigned by an app.

**Cooperative** — A group of people with shared economic activity. Worker cooperatives, consumer cooperatives, housing cooperatives, platform cooperatives. The kernel sees an entity ID, a set of member DIDs, a state log, and a capability graph. What makes this entity a "cooperative" is meaning assigned by an app.

**Community** — A group of people with shared civic interests. Community land trusts, neighborhood associations, mutual aid networks, solidarity groups. Same kernel-level representation as a cooperative. Different app-level meaning. Communities are the civic engine — cooperatives are the economic engine. Neither is subordinate to the other.

**Federation** — A group of cooperatives and communities that coordinate across organizational boundaries. Regional cooperative networks, national associations, cross-sector alliances. Same kernel-level representation. Different app-level meaning. A federation bridges organizations without owning them.

Membership is recursive. A cooperative's members can be cooperatives. A federation's members can be federations. A community's members can include cooperatives. The kernel doesn't care — it sees entities containing entities, each with their own capability graphs.

**Access is flexible.** A person interacts with ICN through whatever runs `icnd` or connects to a daemon: a server, a laptop, a Raspberry Pi, a phone. The React Native SDK provides mobile access. The TypeScript SDK provides web access. The daemon provides full node participation. You choose your surface based on what you need — full sovereignty or lightweight participation.

---

## What Runs on ICN (Apps, Not the Kernel)

Everything that was previously described as "what ICN does" is actually what *applications running on ICN* do. The distinction matters:

| App | Not ICN. Uses: | The kernel doesn't know: |
|-----|---------------|------------------------|
| **Governance** | State, Communication, Coordination, Authorization, Time | what a "vote" is |
| **Mutual credit** | State, Compute, Authorization | what a "credit" is |
| **Membership** | Identity, State, Authorization, Naming | what a "member" is |
| **Federation** | Communication, Naming, Authorization, Coordination | what a "federation" is |
| **Trust scoring** | State, Compute → PolicyOracle | what "trust" means |

These five apps ship with `icnd` today. They are not ICN. They are the first things built on ICN. Other apps are possible. A scheduling app. A resource-sharing app. A dispute resolution app. A cooperative marketplace. Whatever a group of people needs — expressed as constraints on eight primitives.

**Formally descoped from all iterations:**
- Mana regenerative economic model — killed
- Compute marketplace — app if ever needed, not kernel
- Tiered storage economy — app if ever needed, not kernel
- Shared data transfer protocol — Communication primitive is sufficient
- Governance engine as a kernel feature — governance is an app

---

## The Network

Daemons talk to each other over QUIC/TLS with DID-based mutual authentication. Discovery happens via mDNS (local) and gossip (wide-area). Messages are signed envelopes with replay protection. Topics provide pub/sub. The trust graph gates who can talk to whom at what rate.

The network is the Communication primitive made physical. It moves bytes between daemons. It doesn't know what the bytes mean.

---

## The Five Invariants

1. **Adversarial-by-default.** Every peer is untrusted until trust is established through verifiable participation. Security comes from the architecture, not from assuming good faith.

2. **Determinism.** Same inputs produce same outputs. No floating point. Seeded RNG. Canonical serialization. If two nodes process the same log, they arrive at the same state.

3. **Canonical encodings.** One valid byte representation per value. No ambiguity in wire format, proof format, or storage format.

4. **No panics in protocol paths.** Failures are handled. The daemon does not crash on bad input.

5. **Kernel/app boundary is inviolable.** The dependency graph enforces it. CI checks it. The Meaning Firewall is structural, not aspirational.

---

## What ICN Is Not

**Not a governance platform.** Governance is an app that runs on ICN. ICN provides the primitives that make governance possible.

**Not a ledger.** The mutual credit system is an app that runs on ICN. ICN provides the state and compute primitives that make accounting possible.

**Not a federation protocol.** Federation is an app that runs on ICN. ICN provides the communication and naming primitives that make inter-entity coordination possible.

**Not a compute marketplace.** Compute is a kernel primitive. What you compute and how you pay for it are app-level decisions.

**Not a storage system.** State is a kernel primitive. What you store and how you organize it are app-level decisions.

**Not a data transfer protocol.** Communication is a kernel primitive. What messages mean is an app-level decision.

**Not a blockchain.** No global consensus. No chain. Local-first with gossip-based synchronization. Entities agree on their own state and verify claims from others.

**Not a platform.** No central server. No company. You run your own daemon. You hold your own keys.

---

## Why ICN Instead of Something Else

The honest version.

**"Why not just use Loomio + QuickBooks + Slack?"** Because you can't prove anything. You can show someone a screenshot of a Loomio vote. You can show them a QuickBooks report. But you cannot hand them a cryptographic receipt that independently verifies a decision happened, a budget was allocated, and the allocation was executed — all linked in a tamper-evident chain that nobody can alter after the fact. If proof doesn't matter to you, use Loomio. If proof matters, there is nothing else.

**"Why not a blockchain?"** Because democratic organizations don't need global consensus. A three-person cooperative doesn't need the entire planet to validate their vote. A neighborhood association doesn't need a mining pool to approve a zoning response. They need their own members to validate decisions and outsiders to be able to verify the results. ICN is local-first. Your data stays on your hardware. Verification is peer-to-peer. No mining. No gas fees. No chain.

**"Why not just build a web app?"** Because a web app is a landlord. Someone hosts it. Someone maintains it. Someone controls the database. When that someone disappears, changes terms, gets acquired, or raises prices, your institutional records go with them. ICN runs on your hardware. Your records are yours. The daemon talks to other daemons. No central point of failure, capture, or control.

**"Why not ActivityPub / Fediverse?"** ActivityPub solves social federation. ICN solves institutional federation. The difference is the receipt. When a Mastodon post federates, nobody needs to prove it was democratically authorized. When a cooperative's budget allocation federates to a partner cooperative, proof is everything.

**"This sounds like it's only for cooperatives."** It's for any organization that governs itself democratically and wants to prove it. Cooperatives are the first adopters because they have the clearest need — their legal structure requires democratic governance. But community land trusts, mutual aid networks, neighborhood associations, worker centers, and civic organizations all face the same problem: they make collective decisions, and they can't prove those decisions happened to anyone who wasn't in the room. If you govern democratically, ICN is your infrastructure. The entity model treats cooperatives and communities as co-equal. Federations bridge both.

**"What's the minimum viable reason to adopt ICN?"** You want a verifiable record that your organization's decisions were made democratically. That's the entry point. Everything else — federation, mutual credit, sovereignty — follows from groups that already care about provable governance discovering they can also coordinate with each other.

---

## The Adoption Path

ICN doesn't need to replace everything at once. The entry point is narrow:

**Stage 1: Provable governance.** A single organization — cooperative, community group, land trust — runs `icnd` and uses it for proposals and votes. Members participate from whatever device they have. Every decision gets a cryptographic receipt. The organization can now prove its governance to funders, regulators, and potential partners.

**Stage 2: Federation.** Two organizations running `icnd` discover each other and begin verifying each other's governance decisions. A cooperative and a community land trust in the same region. Two worker cooperatives in the same sector. They start settling small obligations. Trust builds through participation.

**Stage 3: Network effects.** As more organizations join, the value of participation increases. Federated discovery means you can find partners across sectors. Cross-boundary settlement means cooperatives and communities can trade services. Verified trust scores mean you can assess reliability before entering an agreement. Developer nodes provide infrastructure services. Mobile clients give every member a voice. The democratic economy develops infrastructure that it owns.

Each stage is independently useful. An organization at Stage 1 gets value even if no one else ever joins. Stage 2 adds value but doesn't require Stage 3. The network grows because each node benefits from being connected, not because a platform markets to them.

---

## The Soul

A democratic organization is people who decided to do something together and govern it collectively. A cooperative. A community land trust. A mutual aid network. A federation of all three. The problem is that "govern it collectively" requires proving it — to yourselves, to each other, to the outside world. And right now, there is no infrastructure for that proof that democratic movements actually own.

ICN is that infrastructure.

Prove your decisions. Own your records. Coordinate with your neighbors. Vote from your phone. Run your own node. No platform. No landlord. No trust required — the math handles it.
