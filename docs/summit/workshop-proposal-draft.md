# Workshop Proposal: Democratic Infrastructure for Cooperatives

**Submitted to:** New York Cooperative Summit — October 2026 Call for Presenters
**Draft date:** 2026-03-23 · **Revised:** 2026-05-30 (claims aligned to verified runtime; proven live loop folded in)
**Track:** Technology & Infrastructure
**Format:** Workshop (90 minutes)

---

## Title

**Infrastructure That Cooperatives Own: Turning Cooperative Work into Verifiable Records**

## Tagline

What if your cooperative's decisions and obligations were recorded by code you controlled — and anyone could verify them without trusting a platform vendor?

---

## Abstract

Most cooperative technology runs on platforms owned by someone else. You can use Slack until Slack changes its pricing. You can organize on Google Drive until Google decides your account violated a policy. The coordination layer — the substrate your democratic processes actually run on — is rented infrastructure, not cooperative infrastructure.

The InterCooperative Network (ICN) is an attempt to fix that. It is free, open-source coordination software, built in Rust, that runs on hardware a cooperative owns — where the rules of the organization are expressed in a cooperative contract language and enforced by code anyone can audit.

This workshop is a live demonstration, and it is honest about its own maturity. The spine of the session is **one thing we run live, from zero, on a node a cooperative controls**: ICN turning a real piece of organizing work into a **legible obligation** and a **verifiable completion receipt** — a tamper-evident record anyone can check. That loop is not a mockup and not a slide; it runs on a laptop in the room.

Around that proven core, we walk the fuller picture — democratic decisions, patronage settlement, cross-cooperative clearing, compliance reporting, shared compute — and we are precise, out loud, about what runs today versus what is still being built. No "trust us." When the system proves something, it shows you the proof. When it doesn't yet, we say so.

The session is designed for cooperative organizers, not engineers. The math stays off the screen. The questions it answers: Can your members verify that a decision happened and was carried out, without trusting the software vendor? Can you prove what your organization did without hiring an auditor? Can you run shared infrastructure with another cooperative without merging your organizations?

One proven loop you can verify in the room. A clear-eyed map of what comes next. Zero central servers.

---

## Session Format and Agenda

**Total time:** 90 minutes

### Part 1: Why Cooperative Technology Is a Contradiction Right Now (20 minutes)

Open with the framing problem: cooperatives organize around democratic ownership but run their organizations on platforms owned by venture capital. The governance layer is democratic; the infrastructure layer is not.

What this costs cooperatives in practice:
- Platform lock-in prevents data portability between coops
- Pricing changes are imposed, not negotiated
- Surveillance capitalism extracts value from cooperative activity
- No interoperability between organizations without a shared platform

The thesis: coordination infrastructure should be a commons. Not a product. Not a service. Infrastructure.

### Part 2: What ICN Does (15 minutes)

Non-technical overview of the five-layer stack:

1. **Identity** — Every member gets a cryptographic identity they control. Not a username in a database — a key pair they own.
2. **Governance** — Decisions and obligations produce verifiable receipts. The audit trail is mathematically provable, not organizationally promised.
3. **Economics** — Mutual credit, patronage settlement, and treasury management without a bank as the trust intermediary.
4. **Compute** — Shared compute resources allocated by cooperative rules, not market pricing.
5. **Federation** — Inter-cooperative trust and clearing, without a central clearinghouse.

Key principle: your cooperative's rules — your bylaws, your procedures — are expressed in a contract language the system enforces mechanically. The rules and the enforcement are not separated.

### Part 3: Live Demo (40 minutes)

#### 3a. The proven loop — run live, from zero (15 min)

This is the part that runs in the room, every time, on a local node (a laptop is enough — the homelab cluster is the same software at scale). It is the smallest honest claim ICN can make, and it is real today:

> **ICN turns a piece of cooperative work into a legible obligation and a verifiable receipt.**

Live, step by step, in plain language:

1. **The cooperative's own node is running** — local, on hardware the group controls. No central server.
2. **An organizer has a cryptographic identity** (a DID) — like a passport they alone hold.
3. **We stand up the organization** — a federation and its governance domain — from a cooperative's own institution package.
4. **A piece of organizing work becomes a legible obligation** — e.g. *"Confirm the Summit venue booking."*
5. **The organizer sees it as a plain-language action card** — what they owe, with a note that a receipt is expected.
6. **They do the work and mark it done.**
7. **The node issues a verifiable completion receipt** — it binds *which* obligation (item + domain), *who* completed it, the *transition* (`pending → completed`), and *when*, into a `record_hash`: a tamper-evident fingerprint of that completion event. It proves *this member discharged this obligation at this time* — the editable title/description text is not part of the hash — checkable by anyone, no vendor required.
8. **The obligation is discharged** — the card clears, and the proof remains for anyone to check.

No platform vendor sits in the middle. The receipt is the proof.

#### 3b. The fuller picture — honestly labeled (25 min)

We then walk the scenarios that show where this goes, narrating clearly what is running, what is rehearsed against fixtures, and what is next-build. (See the truth box below.)

- **Democratic decisions → receipts.** A proposal, deliberation, and a recorded outcome. *Status: the decision-and-obligation machinery runs today; the full member-voting path is in active development (it depends on the cooperative's standing/membership layer being provisioned).* We show the architecture and the fixtures, and we are explicit that live multi-member voting is the next slice — not something we pretend works on stage.
- **Patronage settlement anchored to a decision.** The economic outcome cryptographically linked to the decision that authorized it — governance and economics on the same ledger.
- **Cross-cooperative clearing.** Mutual credit positions across organizations without a bank. The network is the bank.
- **Compliance reporting.** A signed report over a cooperative's own history, anchored to its decisions — auditable without access to internal systems.
- **Commons compute admission.** A task admitted by trust score and capability scope, and a restricted token rejected at the same gate. Authorization is not advisory.

For each, we say plainly which parts are proven, which are rehearsed against example data, and which are roadmap. That honesty *is* the demo: a system that overstates its proofs is the opposite of what cooperatives need.

### Part 4: Discussion — What Would You Build? (15 minutes)

Structured Q&A around real cooperative use cases:

- What decisions and commitments does your cooperative make that should have verifiable receipts?
- What do you share with other cooperatives that you currently can't verify?
- What shared infrastructure could your federation run if you owned the coordination layer?

Close on status: ICN is free, self-hosted, open to contributions, and runs on commodity hardware. The cooperatives in the scenarios are fictional; the infrastructure running the proven loop is not.

---

## What's proven / rehearsed / next (truth box)

Shown to attendees, on a slide, in these words:

| Capability | Status (as of the workshop) |
|---|---|
| Work → legible obligation → **verifiable completion receipt** (the proven loop, 3a) | **Runs live, from zero, on a local node** |
| Cooperative identity (DID), institution bootstrap (federation + governance domain) | **Runs live** |
| Decision/obligation records + plain-language action cards | **Runs live** |
| Full member **voting** (propose → vote → decision receipt) | **In development** — gated on the membership/standing layer; shown as architecture + fixtures, not claimed live |
| Patronage settlement, cross-coop clearing, compliance reporting, commons-compute admission | **Rehearsed against example data / roadmap** — narrated as such, not claimed as production |
| Production deployment, live federation between real cooperatives | **Not yet** — this is research-grade infrastructure, said plainly |

---

## Learning Outcomes

Attendees will leave with:

1. A concrete mental model of cooperative-*owned* coordination infrastructure and how it differs from cooperative-*controlled* tooling on proprietary platforms.
2. A first-hand look at the one loop that runs today — work becoming a verifiable, member-checkable record — and a clear map of the coordination patterns being built around it (decisions, settlement, clearing, reporting, compute).
3. An understanding of why cooperative participation history (a trust graph) can be a better basis for resource allocation than market pricing.
4. A short list of questions to ask about their own technology stack: who owns it, what can you verify, and what happens if the platform changes its terms.
5. Contact with ICN's contribution pathways for technically-inclined members.

---

## Who This Is For

**Primary audience:** Cooperative organizers, federation builders, and cooperative developers thinking about long-term infrastructure strategy. No technical background required.

**Secondary audience:** Cooperative technologists who want to understand the ICN architecture and evaluate it.

**Not for:** People looking for a ready-to-deploy product with support SLAs. ICN is research-grade infrastructure, and this session is honest about that.

---

## Presenter

**Matthew Faherty** — Software architect and cooperative organizer. Core developer of the InterCooperative Network. Organizer of the NY Cooperative Summit (technology track). Based in the Finger Lakes region of New York.

ICN is maintained at [github.com/InterCooperative-Network/icn](https://github.com/InterCooperative-Network/icn).

---

## Technical Requirements

- HDMI connection to a projector or large display.
- The proven loop (3a) runs on a **local node** and needs no internet. The fuller-picture scenarios (3b) can run against a homelab cluster if a reliable connection is available; local Docker Compose is the fallback.
- Seating that lets attendees see the presenter's screen.
- Optional: power strips for attendees following along on laptops.

---

## Notes for Organizers

This session intentionally avoids the word "blockchain." ICN is not a blockchain — no token, no coin, no cryptocurrency-style distributed ledger. It is a peer-to-peer coordination substrate on established cryptographic primitives (Ed25519 signatures, QUIC transport, Blake3-class hashes). The terms that matter: cooperative infrastructure, legible obligations, verifiable receipts, democratic governance enforcement.

The scenarios use fictional cooperatives but real software. The proven loop in 3a is reproducible from the project's own institution package and shipped binaries; a recorded transcript is available as backup if the room's setup fails.

---

## Appendix A — Slide outline (to build)

1. **Title** — Infrastructure That Cooperatives Own.
2. **The contradiction** — democratic org, landlord infrastructure (one image: the rented stack).
3. **What you'll see today** — one proven loop + an honest map. Set the truth-telling expectation.
4. **The five-layer stack** — identity / governance / economics / compute / federation (one line each).
5. **The proven loop (1 slide per beat, or one build slide)** — node → identity → institution → obligation → action card → done → **receipt (`record_hash`)** → discharged.
6. **Truth box** — proven / rehearsed / next (the table above). This is the trust-builder.
7. **The fuller picture** — decisions, settlement, clearing, reporting, compute (one slide, labeled by status).
8. **What would you build?** — the three discussion prompts.
9. **Get involved** — open source, self-hosted, contribution paths, contact.

## Appendix B — One-page attendee handout (to build)

- **What ICN is** (2–3 sentences) and what it is *not* (not a blockchain, not a product).
- **The one thing proven today**, in plain language: work → obligation → verifiable receipt; what a `record_hash` means for a member.
- **The truth box** (proven / rehearsed / next) — reproduced verbatim.
- **Five questions** to ask about your own cooperative's tech stack.
- **Where to look:** the repo, the "What's Real Now" page, and how to reach the presenter.
- **Reproduce it yourself:** a 4-line pointer to running the proven loop on a laptop.

---

*Revised 2026-05-30 — submit-ready pending a presenter voice pass and the Summit call's deadline/channel. Claims aligned to what the system verifiably does today (see truth box); live multi-member voting deliberately presented as in-development, not staged.*
