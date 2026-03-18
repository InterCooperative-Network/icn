# ICN in Practice: Six Scenarios

**March 17, 2026**

These aren't hypotheticals. Each scenario maps to real API endpoints, real data structures, and real code paths in the current ICN codebase. The technical annotations show exactly what fires under the hood.

---

## 1. Harbor Homes Housing Cooperative — Provable Governance for a Grant Application

**The situation.** Harbor Homes is a 40-unit housing cooperative in Rochester, NY. They've applied for a $200K rehabilitation grant from NYS Homes and Community Renewal. The grant application requires evidence of democratic governance — proof that members actually vote on capital improvements, that budgets are approved by quorum, that spending follows approved allocations.

Harbor Homes has meeting minutes. Typed by the secretary. Filed in a Google Drive folder. The grant reviewer has no way to verify that these minutes reflect what actually happened. Neither does anyone else.

**What happens with ICN.**

The property manager installs `icnd` on the co-op's server — a Raspberry Pi in the utility closet. Each of the 40 member-households gets a DID, stored in the ICN mobile app on their phones.

A board member drafts a proposal: "Approve $180K roof replacement from reserve fund, contractor: Allied Roofing, timeline: June-August 2026."

```
POST /v1/gov/proposals
{
  "domain_id": "harbor-homes-board",
  "proposal_type": "budget",
  "title": "Roof replacement — Allied Roofing",
  "body": "...",
  "amount": 180000,
  "budget_category": "capital_reserve"
}
```

The proposal enters Draft state. The board opens it for member voting. Members get a notification on their phones. Over the next 14 days, 34 of 40 member-households vote.

```
POST /v1/gov/proposals/{id}/vote
{ "choice": "for", "comment": "Long overdue." }
```

The proposal closes. 31 for, 2 against, 1 abstain. Quorum met. The system produces a Decision Receipt — a hash of the proposal, every signed vote attestation, the tally, and the outcome, linked in a tamper-evident chain.

```
GET /v1/gov/proposals/{id}/proof
→ {
    decision_hash: "bafy2...",
    attestations: [...34 signed voter attestations...],
    tally: { for: 31, against: 2, abstain: 1, total: 34 },
    quorum_met: true,
    outcome: "accepted",
    chain: ["proposal/bafy2...", "decision/bafy2...", "allocation/bafy2..."]
  }
```

Harbor Homes submits this receipt with their grant application. The reviewer doesn't have to trust Harbor Homes. They don't have to trust ICN. They verify the cryptographic proof independently. The math confirms: 34 member-households participated, 31 approved, the decision is linked to the specific budget allocation.

**What this proves.** Democratic governance isn't a claim on a form. It's a verifiable fact.

**Under the hood:** Identity (DIDs for members), Authorization (governance:vote capability), State (proposal + vote log), Coordination (quorum check + tally), Time (14-day voting window). The kernel doesn't know this is a housing cooperative or a roof. It sees entities, capabilities, and thresholds.

---

## 2. Finger Lakes Federation — Two Cooperatives That Don't Trust Each Other

**The situation.** Ithaca Solar Co-op and Geneva Food Co-op want to do a deal. Ithaca Solar will install panels on Geneva Food's warehouse. Geneva Food will pay in mutual credit — redeemable for produce, distributed over 18 months. Neither trusts the other's accounting. They've never worked together. They share no infrastructure.

**What happens with ICN.**

Both cooperatives already run `icnd`. Each has their own governance domains, their own membership rolls, their own internal economics. They've never connected.

Ithaca Solar's daemon discovers Geneva Food's daemon via the federation registry:

```
GET /v1/federation/coops
→ [...list of registered cooperatives with trust scores and attestation counts...]
```

Before proposing anything, Ithaca Solar checks Geneva Food's governance proofs:

```
GET /v1/gov/proposals/{budget-proposal-id}/proof
→ { ...cryptographic proof that Geneva Food's board voted 7-0 to authorize
     the solar installation and the mutual credit obligation... }
```

They don't have to call Geneva Food and ask "did your board approve this?" The receipt proves it. They can verify the signatures of each Geneva Food board member independently.

Ithaca Solar's members vote to accept the deal. Their own Decision Receipt is produced. Now both sides have cryptographic proof that the other's governance approved the arrangement.

The mutual credit obligation is recorded:

```
POST /v1/federation/clearing
{
  "creditor_coop": "did:icn:ithaca-solar",
  "debtor_coop": "did:icn:geneva-food",
  "amount": 15000,
  "unit": "usd-equivalent",
  "terms": "18 monthly settlements of 833.33",
  "governance_receipt": "bafy2..."
}
```

Every monthly settlement produces its own receipt. After 18 months, both cooperatives have a complete, independently verifiable record of the entire relationship — from the initial governance approvals through every settlement.

**What this proves.** Two organizations that don't trust each other can transact with full transparency and zero platform dependency. Trust isn't assumed. It's computed from verifiable participation and proven through cryptographic receipts.

**Under the hood:** Communication (daemon-to-daemon discovery), Naming (federation registry lookup), Authorization (federation:read, federation:write capabilities), State (clearing agreement log), Coordination (bilateral settlement tracking). The kernel doesn't know this is a solar installation or a food co-op. It sees two entities, an obligation lifecycle, and signed state transitions.

---

## 3. Rochester Cooperative Collective — A Community That Isn't a Cooperative

**The situation.** RCC meets every 2nd and 4th Thursday, 6-8pm. They're not a cooperative. They're not incorporated. They're a group of 25 people who care about the cooperative economy in the Rochester area. They discuss projects, share resources, connect people, and occasionally make collective decisions about events and endorsements.

They have no legal entity. No bylaws. No financial accounts. But they make decisions that matter — which events to sponsor, which projects to endorse, which speakers to invite to the summit. Right now those decisions happen in a room and get posted to a Google Group. Nobody outside the room can verify what was decided.

**What happens with ICN.**

RCC registers as a Community entity, not a Cooperative. The kernel doesn't care about the distinction — it sees an entity ID, 25 member DIDs, and a capability graph. The app layer is what knows this is a community, not a co-op.

Their governance is loose. Consent-based, no formal quorum. The governance app translates this:

```
APP: "Consent-based decisions. Any member can block.
      Discussion period: 7 days. Silence = consent."

KERNEL: ConstraintSet {
    threshold: 1.0,           // unanimity (no blocks)
    min_duration: 604800,     // 7 days
    capability_required: "governance:vote"
}
```

When RCC decides to endorse a cooperative's application for a regional grant, the endorsement gets a Decision Receipt. The cooperative applying for the grant can include RCC's cryptographic endorsement — verifiable proof that a community organization supported their application.

When RCC co-sponsors a summit workshop, the sponsorship decision has a receipt. The summit organizing committee can verify it.

Members participate from their phones. No server required — RCC doesn't have one. One member runs `icnd` on their home machine. Others connect through the mobile client. The community's records live on the network, replicated across member devices, owned by nobody and verified by everyone.

**What this proves.** You don't need to be a legal entity to make provable collective decisions. Communities are first-class participants. The civic engine and the economic engine run on the same infrastructure.

**Under the hood:** Same eight primitives. Different PolicyOracle configuration. The kernel is completely agnostic about whether this is a community, a cooperative, or a book club.

---

## 4. NY Cooperative Network Federation — Coordinating 50 Organizations Without a Platform

**The situation.** The NY Cooperative Network is a loose federation of ~50 cooperatives and community organizations across New York State. They coordinate on advocacy, shared purchasing, educational events, and the annual summit. Currently this coordination happens through email chains, Google Docs, and quarterly Zoom calls. There is no shared infrastructure. There is no authoritative record of what the federation has decided. When a member organization asks "did the federation vote to endorse this legislation?", the answer is "let me check my email."

**What happens with ICN.**

The federation runs its own `icnd` instance. Each member organization — cooperatives and communities alike — connects their daemon to the federation's.

```
POST /v1/federation/init
{
  "name": "NY Cooperative Network",
  "governance_model": "delegated_consent",
  "membership_criteria": "vouching_required"
}
```

New members join through a vouching process. Two existing member organizations must vouch for a new applicant. The vouching is recorded, scored, and verifiable:

```
POST /v1/federation/coops/{id}/vouch
{
  "voucher_coop": "did:icn:ithaca-solar",
  "attestation": "We have worked with Geneva Food Co-op for 3 years...",
  "trust_domains": ["governance", "financial"]
}
```

The federation's trust graph emerges organically. Organizations that honor commitments, participate in governance, and vouch responsibly build reputation. Organizations that don't, don't.

When the federation votes on policy positions, every member organization's delegate casts a vote. The Decision Receipt contains attestations from 50 organizations. Anyone — a legislator, a journalist, a prospective member — can verify that the federation actually voted and what the outcome was.

Shared purchasing decisions (bulk orders from cooperative suppliers) get the same treatment. Budget allocations for the annual summit get the same treatment. Every decision the federation makes is provable. Every obligation between member organizations is tracked with receipts.

The federation doesn't own anybody's data. Each member organization runs sovereign infrastructure. The federation is a coordination layer, not a platform. If a member organization leaves, they take their records with them. If the federation dissolves, the member organizations continue operating independently.

**What this proves.** A federation can coordinate 50+ organizations across a state without any central platform. Federation is a relationship, not a dependency. Exit is free. Sovereignty is preserved at every level.

**Under the hood:** Communication (50+ daemons talking over QUIC/TLS), Naming (hierarchical resolution — `nycn/ithaca-solar`, `nycn/geneva-food`), Authorization (federation:admin capabilities delegated to elected representatives), Coordination (consensus across organizational boundaries), State (federation-level decision log).

---

## 5. Developer Node — Infrastructure Without Membership

**The situation.** A sympathetic technologist in Buffalo runs a homelab. Proxmox cluster, decent bandwidth, spare compute. They believe in the cooperative movement but they're not a member of any cooperative. They want to contribute infrastructure to the network.

**What happens with ICN.**

They install `icnd` on a VM. They generate a DID. They don't join any cooperative or community. They operate an independent node.

Their node provides services to the network: relay capacity, gossip propagation, storage replication. The network discovers their node through mDNS (local) and gossip (wide-area). Organizations that interact with the node build trust scores through verifiable participation.

The developer node doesn't vote in any governance. It doesn't hold any organization's data authoritatively. It relays, stores replicas, and provides compute — all gated by the trust graph. As the node builds trust through reliable service, it gets higher rate limits and more relay capacity.

If the developer later joins a cooperative or starts a community organization, their existing DID and trust history carry over. The network already knows them. Their reputation is portable.

**What this proves.** The network isn't just for organizations. Individual infrastructure operators can contribute without surrendering autonomy to any single organization. The network grows because people with spare capacity can participate without commitment beyond running a daemon.

**Under the hood:** Identity (standalone DID, no organizational membership required), Communication (relay services), State (storage replication), the trust graph naturally incentivizes reliable service through rate limit escalation.

---

## 6. Mobile Member — Voting From the Parking Lot

**The situation.** Maria is a worker-owner at a cleaning cooperative in Syracuse. She works 6am-2pm. The co-op's monthly governance meeting is at 6pm. She has kids. She can't make it most months. Under the current system — Zoom meetings with hand-raise votes — she effectively has no voice in the decisions that govern her workplace.

**What happens with ICN.**

Maria has the ICN mobile app on her Pixel. Her DID is stored locally. She's a member of Syracuse Shine Co-op with `governance:vote` capabilities.

A proposal comes in: "Approve 5% wage increase effective April 1, funded by Q1 surplus." Maria gets a push notification. She taps it in the school pickup line.

She reads the proposal, reads the discussion thread (three other members have commented), and votes yes. Her vote is signed with her private key, transmitted to her co-op's daemon, and incorporated into the tally. She gets a receipt on her phone — cryptographic proof that her vote was counted.

The entire interaction took 90 seconds. She didn't need to attend a meeting. She didn't need to be online at any specific time. She didn't need to trust that someone would record her proxy vote correctly. She voted directly, from her phone, and she has the receipt to prove it.

The co-op's governance app enforces the discussion period (72 hours minimum before close). It enforces quorum (60% of members must participate). It enforces the wage increase ceiling defined in their charter. None of these rules require Maria to know they exist — the infrastructure enforces them mechanically.

When the proposal closes, the co-op has a Decision Receipt signed by 12 of 15 worker-owners. The accountant executes the wage adjustment with an Execution Receipt linked to the Decision Receipt. The full chain is auditable: proposal → discussion → votes → decision → execution. Every link is signed and verifiable.

**What this proves.** Democratic participation doesn't require synchronous meetings. A worker-owner with a phone and 90 seconds can participate as fully as someone sitting in the meeting room. The mobile client isn't a second-class experience — it produces the same cryptographic receipts, the same signed attestations, the same verifiable proofs.

**Under the hood:** The React Native SDK handles key management, proposal display, vote signing, and receipt verification locally on the device. The SDK talks to the co-op's daemon over QUIC/TLS. The daemon handles it identically to a vote from any other client surface. The kernel has no concept of "mobile" vs. "desktop" — it sees a signed message from a DID with the required capability.

---

## The Pattern

Six scenarios. Six different contexts. One infrastructure.

The kernel doesn't know about roofs, solar panels, community meetings, federations, homelabs, or school pickup lines. It knows about entities, capabilities, state transitions, and cryptographic proofs. The applications assign meaning. The kernel enforces constraints. The receipts prove it happened.

That's ICN.
