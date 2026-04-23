# Workshop Proposal: Democratic Infrastructure for Cooperatives

**Submitted to:** New York Cooperative Summit — October 2026 Call for Presenters
**Draft date:** 2026-03-23
**Track:** Technology & Infrastructure
**Format:** Workshop (90 minutes)

---

## Title

**Infrastructure That Cooperatives Own: A Live Demo of Democratic Coordination**

## Tagline

What if your cooperative's governance rules were enforced by code you controlled — not a platform that could change its terms on you tomorrow?

---

## Abstract

Most cooperative technology runs on platforms owned by someone else. You can use Slack until Slack decides to change its pricing. You can organize on Google Drive until Google decides your account violates a policy. The coordination layer — the substrate on which your democratic processes run — is rented infrastructure, not cooperative infrastructure.

The InterCooperative Network (ICN) is an attempt to fix that. It is free and open-source coordination software, built in Rust, deployed on hardware cooperatives own, where the rules of governance are expressed in a cooperative contract language and enforced by code that anyone can audit.

This workshop is a live demonstration. We will run five cooperative scenarios, end to end, on a real Kubernetes cluster — not a slide deck, not a mockup. Attendees will watch:

- A cooperative governance vote run and produce a verifiable receipt
- Patronage dividends settled and anchored to the decision that authorized them
- A clearinghouse record cross-cooperative credit positions across member organizations
- A worker cooperative generate a signed compliance report over its own transaction history
- A compute task submitted to a commons resource pool, admitted by trust score, rejected by a restricted token

Five cooperatives. Five cryptographic proofs. Zero central servers.

The session is designed for cooperative organizers, not engineers. The math stays off the screen. The questions it answers are: Can your members verify the vote count without trusting the software vendor? Can you prove where your money went without hiring an auditor? Can you share infrastructure with another cooperative without merging your organizations?

---

## Session Format and Agenda

**Total time:** 90 minutes

### Part 1: Why Cooperative Technology Is a Contradiction Right Now (20 minutes)

Open with the framing problem: cooperatives organize around democratic ownership, but they run their organizations on platforms owned by venture capital. The governance layer is democratic; the infrastructure layer is not.

Brief survey of what this costs cooperatives in practice:
- Platform lock-in prevents data portability between coops
- Pricing changes are imposed, not negotiated
- Surveillance capitalism extracts value from cooperative activity
- No interoperability between organizations without a shared platform

Introduce the thesis: coordination infrastructure should be a commons. Not a product. Not a service. Infrastructure.

### Part 2: What ICN Does (15 minutes)

Non-technical overview of the five-layer stack:

1. **Identity** — Every member gets a cryptographic identity they control. Not a username in a database. A key pair they own.
2. **Governance** — Proposals, votes, and tallies produce verifiable receipts. The audit trail is mathematically provable, not organizationally promised.
3. **Economics** — Mutual credit, patronage settlement, and treasury management without a bank as the trust intermediary.
4. **Compute** — Shared compute resources allocated by cooperative rules, not market pricing.
5. **Federation** — Inter-cooperative trust and clearing, without a central clearinghouse.

Key architectural principle: the rules of your cooperative — your bylaws, your governance procedures — are expressed in a contract language that the system enforces mechanically. The rules and the enforcement are not separated.

### Part 3: Live Demo — Five Cooperative Flows (40 minutes)

Live terminal demo on a real Kubernetes cluster. Five scenarios, five cooperatives:

**Flow 1 — Governance (8 min)**
Brightworks Collective runs a membership vote. A member proposes expanding digital services. Three members vote. Outcome: accepted, with a cryptographic receipt containing the decision hash. The receipt can be verified by any member at any time without trusting the platform.

**Flow 2 — Patronage Settlement (10 min)**
Brightworks distributes patronage dividends. The settlement is anchored to the governance receipt from Flow 1 via a provenance chain — the economic outcome is cryptographically linked to the democratic decision that authorized it. Governance and economics are not separate systems; they are the same ledger.

**Flow 3 — Clearinghouse (8 min)**
Rochester Cooperative Clearinghouse records inter-cooperative credit positions. Mutual credit clearing across organizations without a bank. The network is the bank.

**Flow 4 — Compliance Reporting (8 min)**
Harbor Freight Workers Cooperative generates a signed compliance report over its transaction history. The report is anchored to governance decisions. An auditor can verify the chain without access to internal systems.

**Flow 5 — Commons Compute (6 min)**
Finger Lakes CDN submits a compute task to a commons pool. Two gates: the member's capability token must carry compute:write scope, and their trust score must meet the admission threshold. Both pass. The task enters the queue with a provenance hash. A restricted token is then used to attempt the same submission — and is rejected. Authorization is not advisory.

**What we do not claim:** Task execution by registered executor nodes is next-sprint work. The admission gate — the trust check, the scope enforcement, the provenance hash — is what we demonstrate. We are precise about what is proven.

### Part 4: Discussion — What Would You Build? (15 minutes)

Structured Q&A structured around cooperative use cases:

- What decisions does your cooperative make that should have verifiable receipts?
- What data do you share with other cooperatives that you currently can't verify?
- What shared infrastructure could your federation run if you owned the coordination layer?

Close with the open-source status: ICN is free, self-hosted, and accepts contributions. The code runs on commodity hardware. The cooperatives that ran this demo are fictional; the infrastructure running them is not.

---

## Learning Outcomes

Attendees will leave with:

1. A concrete mental model of what cooperative-owned coordination infrastructure looks like and how it differs from cooperative-controlled tooling on proprietary platforms
2. Familiarity with five specific coordination patterns — governance, settlement, clearing, reporting, and compute — and how cryptographic receipts make them auditable
3. An understanding of the trust graph model and why cooperative participation history is a better basis for resource allocation than market pricing
4. A list of practical questions to ask about their organization's current technology stack: who owns it, what can you verify, and what happens if the platform changes its terms
5. Contact with ICN's contribution pathways for technically-inclined members

---

## Who This Is For

**Primary audience:** Cooperative organizers, federation builders, and cooperative developers who are thinking about long-term infrastructure strategy. No technical background required.

**Secondary audience:** Cooperative technologists who want to understand the ICN architecture and evaluate it for their organization.

**Not for:** People looking for a ready-to-deploy product with support SLAs. ICN is research-grade infrastructure. This session is honest about that.

---

## Presenter

**Matthew Faherty** — Software architect and cooperative organizer. Core developer of the InterCooperative Network. Organizer of the NY Cooperative Summit (2nd year running, technology track). Based in the Finger Lakes region of New York.

ICN is maintained at [github.com/InterCooperative-Network/icn](https://github.com/InterCooperative-Network/icn).

---

## Technical Requirements

- HDMI connection to projector or large display
- Reliable internet connection (demo runs on a remote K3s cluster; fallback is local Docker Compose network)
- Tables or chairs that allow attendees to see the presenter's screen
- Optional but useful: power strips if attendees want to follow along on laptops

---

## Notes for Organizers

This session intentionally avoids the word "blockchain." ICN is not a blockchain. It has no token, no coin, and no distributed ledger in the cryptocurrency sense. It is a peer-to-peer coordination substrate built on established cryptographic primitives (Ed25519 signatures, QUIC transport, Blake3 hashes). The terminology that matters: cooperative infrastructure, verifiable receipts, and democratic governance enforcement.

The demo scenarios use fictional cooperatives but real software. The Brightworks Collective, Harbor Freight Workers Cooperative, Rochester Cooperative Clearinghouse, New England Mesh Network, and Finger Lakes CDN are illustrative; the Kubernetes cluster they run on is physical homelab hardware.

---

*Draft — 2026-03-23. Not submitted.*
