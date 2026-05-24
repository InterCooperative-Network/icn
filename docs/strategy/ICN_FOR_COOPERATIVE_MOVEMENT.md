---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-21
---

# ICN: Infrastructure for the Cooperative Movement

> A plain-English introduction for cooperative developers, federation organizers, TA providers, member-owners, and anyone who has lived through what it costs to run a democratic organization on infrastructure built for extraction.
>
> Pre-pilot. Open source. Not a vendor. Not a startup. Not for sale.

---

## The one-sentence version

ICN is software that lets cooperatives govern, remember, coordinate, and federate **on infrastructure the movement owns**, so the cooperative and solidarity economy can materially exist as a coordinated ecosystem, not just ideologically exist as a shared set of values.

---

## The problem this sits in

The cooperative movement is roughly 180 years old. It has produced credit unions, worker cooperatives, housing cooperatives, agricultural cooperatives, consumer cooperatives, mutual aid networks, community land trusts, the solidarity economy. It has principles (Rochdale, ICA), institutions (USFWC, NCBA CLUSA, ICA), and a long memory of what works.

What it does **not** have is digital infrastructure it owns.

- Governance lives in Loomio (a B-Corp, not a coop).
- Documents live in Google Workspace.
- Accounting lives in QuickBooks.
- Fundraising lives in Open Collective.
- Communications live in Slack.
- Membership records live in Mailchimp.
- Inter-coop coordination lives in email.
- Records die when founders rotate, advisors move on, or a vendor changes its terms.

Every tool in the cooperative tech stack is rented from a company whose values are not cooperative. When the vendor raises prices, coops absorb the cost. When the vendor changes its terms, coops comply or migrate. When the vendor shuts down, coops lose their institutional records.

For a movement built on democratic ownership, this is a structural contradiction. **The solidarity economy runs on infrastructure designed for extraction.**

ICN is one attempt to change that, starting from the substrate up.

---

## What ICN actually is

A peer-to-peer software substrate. Each cooperative runs its own node, on its own hardware. Nodes communicate over encrypted peer-to-peer connections. There is no central server. There is no platform operator. There is no vendor.

The substrate provides eight primitives that any democratic organization needs:

**Standing → Authority → Decision → Obligation → Effect → Receipt → Evidence → Review.**

Apps sit on top of the substrate and add the domain meaning — worker-coop bylaws, housing-coop equity structures, federation treaties, ag-coop patronage calculations. The substrate enforces the rules without understanding them. The meaning lives in the apps, where it can be governed by the people it affects.

This separation is intentional. It means:
- The substrate is reusable across cooperative sectors.
- Domain meaning stays with the people who understand it.
- No one (including ICN) can change the rules of your cooperative from outside.

---

## The eight-word spine, with examples from movement work

| Word | What it means | Example from cooperative practice |
|---|---|---|
| **Standing** | Who has the right to act in this context. | A patronage refund vote that only full members can cast. A board action only directors can take. |
| **Authority** | Who can authorize what, under what governance rule. | The bylaws say the board can engage a CPA up to $25k without a member vote. ICN holds that rule as a constraint the kernel enforces. |
| **Decision** | A choice the institution made, under authority, with a threshold met. | The vote to admit a new member. The board resolution to enter a federation. The general assembly's approval of the annual capital plan. |
| **Obligation** | A commitment created by a decision. | The patronage allocation owed to a member. The federation's commitment to a shared purchasing pool. The advisor engagement letter. |
| **Effect** | The actual change in the world that the decision caused. | Member admitted. Allocation lands in the capital account. Bylaw adopted. Federation joined. |
| **Receipt** | A verifiable record that the decision and its effect happened. | A signed record linking the proposal, the vote, the threshold met, and the allocation that resulted. |
| **Evidence** | Receipts plus their context, available to verify later. | The full chain a new treasurer in 2030 can read to understand 2026's patronage cycle. |
| **Review** | Anyone (member, board, federation, regulator) can verify the chain without trusting a vendor. | A federation auditing a member coop's patronage history. A new board reviewing what the founders actually decided. A CPA cross-checking the books against governance evidence. |

The spine isn't novel as a concept — every cooperative does this work today, just in spreadsheets, meeting minutes, email threads, and founder memory. ICN is a substrate that lets the work survive what the spreadsheets and email don't survive.

---

## The seam ICN is trying to be

**Formation → governance → coordination → federation.**

Most cooperative-tech work today focuses on either *formation* (Launch.coop, USFWC's Worker Co-op Academy, state cooperative-development programs) or *operations* (Loomio for governance, QuickBooks for accounting, Slack for comms). What's missing is the durable layer underneath both — the records that have to survive founder rotation, advisor turnover, fiscal-year close, member exit, dissolution, and eventually federation with other coops.

ICN is the layer underneath. Not formation. Not operations. The continuity layer.

The federation case is where it gets interesting. Today, when two coops want to coordinate — joint purchasing, shared services, a federated credit pool, mutual recognition of membership, a regional alliance — they negotiate bilateral contracts and trust each other's word. There's no shared verifiable record of who owes what, under what authority, against what evidence. ICN's federation primitives let coops verify each other without surrendering to a common platform.

For a movement that has always *talked* about federation and rarely *done* it at scale, that's the actual unlock.

---

## How ICN compares to tools you already know

Honest comparisons, not sales:

### Loomio
**What Loomio does well:** structured online deliberation, threaded proposal-and-vote workflows, a familiar interface for cooperative decision-making.
**Where it leaves a gap:** Loomio records that a vote happened on Loomio. It does not produce records that survive Loomio's pricing changes, terms changes, or shutdown. If your cooperative needs to prove to a future board, a funder, or a regulator that a specific decision happened on a specific date under specific authority, Loomio's records are vendor-dependent.
**Where ICN sits:** underneath Loomio (or alongside it). ICN's receipts are signed, content-addressed, verifiable without the vendor. Use Loomio for the deliberation, ICN for the durable record.

### Co-op Cloud (and similar self-hosted stacks)
**What Co-op Cloud does well:** packaged self-hosting of standard SaaS-style apps (Nextcloud, Wordpress, CryptPad, etc.) on infrastructure owned by the cooperative.
**Where it leaves a gap:** Co-op Cloud is an excellent answer to "how do we run our own infrastructure?" It does not answer "how do our institutional records survive across years, member rotations, and platform migrations?" — that's a layer above hosting.
**Where ICN sits:** complementary. Co-op Cloud + ICN is the right pairing. Run your tools on Co-op Cloud. Hold the governance evidence in ICN. Different problems, both real.

### Open Collective
**What Open Collective does well:** transparent fundraising and budget transparency, fiscal sponsorship, a friendly public-facing surface for member contributions.
**Where it leaves a gap:** Open Collective is a hosted platform. Your records live on their servers. Their values and your values may align today but you do not control the infrastructure.
**Where ICN sits:** orthogonal. Open Collective is good for *public* transparency. ICN is for *durable governance evidence* that doesn't depend on a hosted platform.

### QuickBooks
**What QuickBooks does well:** general-purpose accounting that your CPA already knows how to use.
**Where it leaves a gap:** QuickBooks is the system of record for the books. It does not represent the governance side of economic events — the decision to allocate, the authority that approved it, the obligation it created.
**Where ICN sits:** beside it. ICN holds the governance evidence; QuickBooks holds the books; both verify each other. The CPA still does the CPA's work.

### Blockchain / Web3 / DAOs
**What they do:** speculation, tokens, financialization of governance, "smart contracts" that are neither smart nor contracts.
**Where ICN sits:** nowhere near them. ICN uses cryptographic verification for record integrity (signing decisions so you can verify them later), not for tokens, currency, or speculation. If anyone reads "peer-to-peer" or "cryptographic" and thinks Web3, they're reading the wrong project.

---

## What ICN is NOT

A short list, kept honest:

- **Not a vendor.** No SaaS subscription, no per-seat pricing, no platform you're locked into. Each cooperative runs its own node.
- **Not a startup.** No investors, no exit strategy, no growth-at-all-costs imperative. Funded by grants and volunteer time.
- **Not for sale.** ICN is infrastructure the movement owns. Long-term, the substrate is governed by cooperatives that use it, not by a company.
- **Not replacing existing cooperative tech.** Loomio, Co-op Cloud, Open Collective, QuickBooks, CryptPad — all good in their domains. ICN sits underneath the durable-records question, not on top of any of them.
- **Not replacing TA providers, lawyers, CPAs, bookkeepers, or co-op developers.** The human work of cooperative formation, governance, conflict resolution, legal opinion, and accounting is irreducibly human. ICN holds the *evidence* of work humans did.
- **Not accounting software.** Not banking. Not a wallet. Not a token. Not a payment rail. Not financial-intermediary software in any form.
- **Not production-ready.** Pre-pilot. The substrate runs (daemon, gateway, identity, ledger primitives, governance primitives, federation primitives, deployed on a small cluster since December 2025). The human-facing surfaces — member-facing apps, steward dashboards, production federation flows — are next, not done.
- **Not a coup of the existing cooperative-tech world.** A long, slow, careful project to build one missing layer. It will only matter if it's actually useful to the people doing the work.

---

## Where it fits in your work

Some honest guesses about where ICN might be useful, by role:

### If you're a co-op founder
Mostly: nothing today. Use Launch.coop or your local TA provider for formation. Use Loomio for early governance. When your co-op is a few years old and you start losing records to founder rotation, ICN may be a layer to consider for the records that have to survive.

### If you're a co-op developer or TA provider
Eventually: a layer where your advisor work product (engagement scope, evidence of completion, handoff documentation) becomes verifiable across years and successor advisors. Not today; the human-facing surfaces aren't there yet.

### If you're on a federation organizing committee
The clearest fit. Federation primitives are why ICN exists. If you've ever tried to set up a regional alliance or a shared purchasing pool and gotten stuck in bilateral-contract land, this is the problem ICN's federation layer is aimed at. Still pre-pilot, but if your work survives a few more years, ICN may be ready when you are.

### If you're a member of an existing coop
Probably nothing visible. The point is for governance to keep working when founders rotate and your board changes, without anyone having to reconstruct the institution from email.

### If you're a CPA, bookkeeper, or financial professional
Adjacent. ICN doesn't replicate your books. It holds the governance side that runs alongside the books — the decision to allocate, the authority that approved it, the obligation it created. Both verify each other.

### If you're at USFWC, NCBA CLUSA, ICA, DAWI, or similar
Long-arc. The movement-infrastructure question is real and ICN is one bet at one layer. There are other bets (Co-op Cloud, MayFirst, RPS, .coop registry, MyCoop). All worth funding. ICN is not in competition with them; it's a different layer.

---

## A useful test

If anything here feels real, the next step is a sanitized tabletop walkthrough, not a sales conversation.

1. **Pick one workflow your cooperative actually runs** — a patronage cycle, a board election, a new-member admission, a federation negotiation, a dissolution.
2. **Walk it beat by beat.** What record is generated? Who needs to verify it later? What should never be a record (interpersonal conflict, care work, private deliberation)?
3. **Capture what works and what breaks.** If ICN's substrate can carry the work cleanly, that's a real seam. If it can't, that's also useful.

No live data required. No commitment required. A two-hour tabletop with someone who knows the workflow.

If that surfaces a seam worth pursuing, the next step is bringing in the right additional expertise (co-op lawyer, CPA, federation organizer) — not building product.

---

## FAQ — questions movement people actually ask

### "Why hasn't this been done before?"
Pieces of it have been. NCBA's mutual-aid networks, the Working World's loan substrate, ICA Group's co-op-developer infrastructure, Loomio's governance layer, Co-op Cloud's hosting, MayFirst's hosting cooperative, RPS's coordination work. ICN is one attempt at the substrate underneath all of them. None of those efforts is wrong; ICN is a different layer.

### "How is this different from a credit union shared-services platform?"
Credit unions have shared services (Co-op Solutions, PSCU, Catalyst). Those are necessary infrastructure for the credit union sector. ICN is at a lower layer (records, governance, verification) and a wider scope (any democratic organization, not just credit unions). They're complementary.

### "Why not just push for Loomio / Co-op Cloud / X to be better?"
We should. ICN sits underneath those tools, not in competition with them. The records-survival problem is real even if every cooperative-tech vendor was movement-aligned tomorrow. Vendors change hands. Funding dries up. Founders leave. The substrate question is downstream of any specific vendor.

### "Who's funding it?"
Currently: self-funded and volunteer time. Grant applications in flight (Mozilla, NLnet, Sovereign Tech Fund). Long-term: funding should come from movement institutions, not from VC. If you know movement-aligned funding sources, talk to us.

### "What's the governance of ICN itself?"
Open source today. Long-term intent: the substrate is governed by the cooperatives that use it, through whatever federation structure makes sense for the movement. We don't have the answer yet. We're trying to build the substrate well enough that the governance question is worth answering.

### "I've been burned by tech projects that promised this and disappeared."
Fair. ICN is pre-pilot. We're not promising anything. The substrate runs. The human-facing surfaces don't yet. If we disappear, the open-source substrate is on GitHub and can be forked. We are not trying to be the next Diaspora.

### "Will my cooperative have to learn cryptography?"
No. The cryptography is invisible plumbing for record integrity. A board member uses ICN the way they use email — they don't think about TLS. The whole point of the substrate is that the cooperative gets durable records without having to become a crypto project.

### "What about privacy?"
Default privacy posture: nothing is public unless explicitly published. Member-private records stay on member-controlled hardware. Cross-coop verification works with hashes and signed claims, not raw data exposure. The privacy model has explicit primitives and an explicit threat model.

### "What about Mondragon? They've made this work for 70 years without ICN."
Mondragon has made it work with extraordinary institutional discipline, a banking subsidiary, and a culture built over decades. Most cooperatives don't have that institutional infrastructure. ICN is one attempt to provide a thinner version that smaller and younger cooperatives can use without becoming Mondragon.

### "What's the long-term goal?"
A cooperative and solidarity economy that materially exists as a coordinated ecosystem — with governance, obligations, federation, and evidence — on infrastructure the movement owns and runs, instead of infrastructure rented from companies whose business models contradict ours. The 2025 NY Cooperative Summit called this "the infrastructure that lets co-ops scale." ICN is one bet at that infrastructure.

---

## Glossary, kept short

| Term | What it means in cooperative-practice language |
|---|---|
| **Substrate** | The infrastructure underneath the apps — identity, records, governance primitives, federation primitives. Domain-agnostic. |
| **App** | A piece of software that knows what *kind* of cooperative this is (worker, housing, ag, consumer, credit union, federation). Apps add meaning; the substrate enforces rules. |
| **Receipt** | A signed, durable record that something happened, verifiable independently of any platform. |
| **Settlement** | The final, durable record of an economic event. Replaces "payment" in ICN's vocabulary (a regulatory choice). |
| **Obligation** | A forward-looking commitment created by a decision. Includes patronage allocations, advisor agreements, federation commitments. |
| **Allocation** | An assignment of credits, patronage refunds, or other resources. Replaces "distribution" in some contexts. |
| **Position** | A view of where a member or organization stands relative to credits, obligations, equity. Replaces "balance" / "account." |
| **Provenance** | The full chain of authority and evidence behind a record. |
| **Federation** | A coordination layer between cooperatives that lets them verify each other and settle obligations across boundaries, without surrendering to a common platform. |
| **DID** | Decentralized Identifier. A cryptographic identity primitive. Members and organizations have DIDs. |
| **CCL** | Cooperative Contract Language. The domain-specific language ICN uses to express governance rules, allocation formulas, and lifecycle constraints. Inspectable and amendable. |
| **Pre-pilot** | The current status of ICN. Substrate runs. Human-facing surfaces don't yet. No live cooperative is running on ICN as production infrastructure. |

---

## How to get involved

Today:
- Read this document. Push back. Tell us where it's wrong.
- If you're at a movement institution and the federation layer is interesting, set up a conversation. We're not trying to sell you anything.
- If you run a workshop or session at a movement gathering and want to include "what would coop-owned infrastructure look like?" as a topic, we'll show up.
- If you have a sanitized tabletop scenario you'd be willing to walk through, that's the single most valuable thing.

Not today (yet):
- Don't migrate your cooperative onto ICN. Pre-pilot means pre-pilot. The substrate runs; the surfaces a member would use don't.
- Don't recommend ICN to a coop forming today. Use Launch.coop and your local TA provider. Come back when ICN is past pilot.
- Don't try to integrate ICN with existing systems in production. The integration surfaces are not ready.

Where to find more:
- Source code, threat model, technical docs: [github.com/InterCooperative-Network/icn](https://github.com/InterCooperative-Network/icn)
- Architecture overview: `docs/ARCHITECTURE.md`
- Threat model: `docs/security/threat-model.md`
- Economic vision: `docs/design/economics/ECONOMIC_VISION.md`
- This document: `docs/strategy/ICN_FOR_COOPERATIVE_MOVEMENT.md`

---

## Closing

The cooperative movement already exists. Has for a long time. What it doesn't have yet is shared infrastructure that survives turnover, vendor changes, and the years between annual gatherings without requiring everyone to reconstruct the institution from email and exhausted memory.

ICN is one attempt at that infrastructure. Open source. Self-hosted. Movement-owned in long-term intent. Pre-pilot today.

The point is for a cooperative and solidarity-economy ecosystem to materially exist, not just ideologically exist. We're not promising we'll get there. We're saying it's worth trying.
