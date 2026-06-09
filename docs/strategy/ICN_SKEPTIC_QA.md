---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-09
---

# ICN Skeptic Q&A — the hard questions, answered directly

> For cooperative developers, federation organizers, movement reviewers, funders, and anyone who has been pitched cooperative tech before and is rightly tired of it.
>
> Format borrowed from internal rehearsal practice: each question gets the **bad answer** (the overclaim a pitch would make) and the **honest answer** (what the evidence supports). "No, not yet" appears often, on purpose.
>
> Claims here are bounded by the [evidence map](ICN_INTRODUCTION_EVIDENCE_MAP.md). When in doubt, the evidence map wins.

---

## 1. "Is anyone using this in production?"

**Bad answer**: "We have deployments running since December 2025!"

**Honest answer**:

> "No. No cooperative is using ICN in production. Nothing about ICN is production-ready. The substrate has run on a small self-hosted development cluster ([documented in-repo](../operations/deployment/HOMELAB_DEPLOYMENT.md); anchored in the [evidence map](ICN_INTRODUCTION_EVIDENCE_MAP.md)), which proves it deploys and runs — it does not prove anyone depends on it, because no one does. If you ever hear someone claim a co-op is running on ICN, ask for the evidence; as of this document's review date, there isn't any, because it hasn't happened."

---

## 2. "So what actually works right now?"

**Bad answer**: "Almost everything! Let me show you our crates..."

**Honest answer**:

> "The substrate runs. The eight kernel primitives — identity, ledger, gossip, governance, and the rest — have working implementations and tests. The strongest end-to-end proof today is the **receipt chain**: a decision flows through authority, obligation, effect, and dispatch into receipts, and an audit pass verifies all thirteen links of that chain against a real local daemon and gateway. That demonstration is reproducible with one command by anyone who checks out the repo. What's *not* live: the human-facing apps. There's no member-facing app, no steward dashboard, no production federation. The constraint engine is real; the surfaces a co-op member would actually touch are still rehearsal-stage."

---

## 3. "What's fixture-backed versus actually running? What's only design on paper?"

**Bad answer**: "It all works, we just use fixtures for convenience."

**Honest answer**:

> "Three tiers, and the difference matters.
>
> **Live local proof**: the receipt-chain audit path runs against a real local `icnd` daemon and gateway and verifies 13/13 links. That's real software executing, on one operator's machine, with demonstration data.
>
> **Fixture-backed**: the rehearsal shell can also run from recorded fixtures, with no live daemon at all. That mode exists deliberately, so a facilitated walkthrough doesn't depend on live infrastructure — but a fixture run proves presentation, not execution.
>
> **Design-only**: production federation flows, member-facing applications, real-institution onboarding, private-data handling. These exist as specs, ADRs, and contracts — words and schemas, not running systems.
>
> ICN maintains a [proof-level taxonomy](../reference/project-index/proof-level-taxonomy-capability-matrix.md) precisely so these tiers don't get blurred in conversation. The rule is simple: state the level the evidence supports, and no higher."

---

## 4. "Why should I trust the receipts?"

**Bad answer**: "Cryptographic signatures!"

**Honest answer**:

> "You shouldn't trust the receipt alone. A receipt only proves *that an event happened with these signatures*. It doesn't prove the event was legitimate. Legitimacy comes from the cooperative's process — who had standing, who had authority, what the governance doc said. The receipt is just the part that survives, so members or auditors can verify *years later* that the process actually played out the way the minutes say it did. ICN's abuse-case hardening doctrine is explicit about this: receipts prove events, not legitimacy."

---

## 5. "Is this blockchain? Crypto?"

**Bad answer**: "We're decentralized but not on a blockchain!"

**Honest answer**:

> "No, and the differences are structural, not cosmetic. One: no consensus protocol, no mining, no global state, no token. ICN is per-cooperative or per-federation; each instance is its own substrate. Two: the vocabulary is governance-first by design — settlement, obligation, allocation, position, receipt, provenance, evidence. Receipts are evidence records, not currency. Whether that posture is sufficient under any specific regulatory regime is an attorney question, but the substrate isn't claiming financial-product semantics, and it isn't built on financial-product machinery."

---

## 6. "What about regulation? You're recording economic events."

**Bad answer**: "Our lawyers say it's fine." (There are no such lawyers.)

**Honest answer**:

> "The design discipline is to stay on the substrate side of a bright line: ICN records *settlement intent and evidence* — the decision to allocate, the authority that approved it, the obligation it created, the receipt that survives. It does not transmit money, hold deposits, or execute transactions. That's why the vocabulary avoids 'payment,' 'balance,' and 'wallet' for ICN-native primitives. But hear this clearly: that is a design posture, not a legal opinion. No attorney has signed off on ICN's regulatory position, and any cooperative considering ICN for economic records should ask its own counsel. 'Talk to your lawyer' is the honest answer, not a dodge."

---

## 7. "What prevents capture? Every movement tool eventually gets captured."

**Honest answer**:

> "Several design choices, and one honest gap. The choices: open source (forkable by anyone, so no one can hold the code hostage); self-hosted nodes (no platform operator to be acquired); kernel/app separation (the substrate enforces rules without owning their meaning, so no central party can quietly redefine what your cooperative's rules mean); and federation primitives that let coops verify each other without surrendering to a common platform. The gap: ICN's own long-term governance is unresolved. The stated intent is for the substrate to be governed by the cooperatives that use it, but that structure doesn't exist yet because the user base it would represent doesn't exist yet. Today the practical anti-capture guarantee is forkability, which is real but blunt."

---

## 8. "What prevents this from becoming surveillance infrastructure? You're building a permanent record."

**Honest answer**:

> "The danger is real and the design takes it seriously. Default posture: nothing is public unless explicitly published. Member-private records stay on member-controlled hardware. Cross-organization verification works with hashes and signed claims, not raw data exposure — a federation can verify that your coop's patronage decision happened without reading your member list. And some things should never be records at all: interpersonal conflict, care work, private deliberation. The rehearsal practice bakes that in — facilitators are trained to ask 'what should never be a record?' as part of every walkthrough. What doesn't exist yet: production hardening of any of this. The threat model is written and public; the operational guarantees are not yet earned."

---

## 9. "What about private data? Could my co-op put member records on this today?"

**Bad answer**: "Sure, it's encrypted!"

**Honest answer**:

> "No. Please don't. ICN has no private-data handling readiness: no hardened storage posture, no audited access controls, no data-protection compliance review, no recovery guarantees. Every ICN demonstration and rehearsal to date uses fictional or sanitized data, and that's a hard rule, not a habit. The community-bridge design work around movement events is explicit about the same boundary: consent-first, no raw attendee mirroring, no identity collapsing, no private accessibility or care data. When ICN is someday ready to hold real member data, that readiness will need to be demonstrated, audited, and stated explicitly — not assumed."

---

## 10. "What happens if Matt disappears? What's the bus factor?"

**Bad answer**: "We have a thriving community!"

**Honest answer**:

> "It's bad. The bus factor is one. There is one primary maintainer, and that's an honest structural weakness of the project today. Mitigations, for what they're worth: everything is in the open — code, threat model, security policy, issue taxonomy, roadmap, all on GitHub, forkable by anyone. The substrate doesn't phone home to anything, so running instances wouldn't break if the maintainer vanished. But 'forkable' is not 'maintained,' and no one should mistake one for the other. Part of why these introduction materials exist is to find out whether the project is useful enough to be worth broadening the maintainer base."

---

## 11. "Why should co-ops care? Loomio plus a spreadsheet works fine."

**Honest answer**:

> "Today, for a small co-op with stable members? It mostly does. ICN matters when: (a) the records need to outlast the platform — Loomio is a vendor, the sheet is in someone's Drive; (b) the records need to be verifiable by an outside party — a buyer of a member's interest, an auditor, a regulator — without trusting any vendor; (c) you want to federate with other co-ops and need a shared substrate they can also verify. None of those problems hit a five-person worker co-op in year one. They hit hard in year five, at founder turnover, at audit, at federation. If your co-op never hits them, you genuinely don't need ICN."

---

## 12. "Have you actually talked to co-ops about this, or is this built in a basement?"

**Honest answer**:

> "Less than we should have, and that's being corrected deliberately. The New York cooperative organizing ecosystem is the reference context — summit organizing work provides a real cooperative community to stress-test against, and an organizer learning path exists to translate ICN's institutional spine into organizer reality. Beyond that, outreach was deliberately held back until the substrate could carry an honest demonstration. It can now carry a small one. That's why these documents exist: the project is at the point where one sanitized rehearsal with people who do the actual work teaches more than another year of solo development."

---

## 13. "What's the smallest safe next step if a co-op is curious?"

**Bad answer**: "Sign up for our pilot program!" (There is no pilot program.)

**Honest answer**:

> "A two-hour sanitized tabletop walkthrough. Pick one workflow your cooperative actually runs — a patronage cycle, a board election, a new-member admission. Walk it beat by beat with fictional or name-stripped data: what record is generated, who needs to verify it later, what should never be a record. If ICN's substrate can carry the workflow cleanly, that's a real seam worth exploring with the right additional expertise — co-op lawyer, CPA, federation organizer. If it can't, that's equally useful and costs nothing. No live data. No commitment. No integration. 'This isn't useful to us' is an acceptable outcome, and hearing it early is a gift."

---

## 14. "What's the failure mode for ICN?"

**Honest answer**:

> "Two big ones. First, scope creep into accounting or payments processing — that's how this becomes a fintech project and dies under regulatory weight. The discipline is to stay on the substrate side. Second, building a substrate so abstract that no co-op can use it. The cure for that is rehearsal against real cooperative problems instead of building in the dark — which is exactly the phase the project is in now."

---

## Tone discipline (for anyone presenting ICN)

Rescued from internal rehearsal practice because it generalizes:

- Don't apologize for not being further along. Don't oversell either.
- "I don't know" is allowed and often correct.
- "That's a good question, let me think" is allowed.
- "That's not what ICN does" is allowed when accurate.
- If you catch yourself listing features, stop mid-sentence and ask the other person a question instead.
- Don't claim production-readiness. Don't say "blockchain," "crypto," "currency," "token," or "wallet" to describe ICN primitives.
- Don't promise a pilot. Don't ask for anyone's data.
- The goal of any introduction conversation is one identified seam and one rehearsal candidate, or a clear "not yet" — both are wins.

---

## Non-claims

What this document does **not** claim:

- **Not production-ready.** No part of ICN is hardened for production.
- **Not a pilot.** No formal pilot has been authorized with any institution.
- **Not live federation.** No multi-institution federation has been demonstrated between real organizations.
- **Not private-data ready.** All demonstrations use fictional or sanitized data.
- **Not legal, privacy, or compliance advice.** The vocabulary discipline is a design posture; regulatory questions belong to attorneys.
- **Not full implementation.** Some capabilities described in ICN's specs are design-only; the [evidence map](ICN_INTRODUCTION_EVIDENCE_MAP.md) draws the line.
- **Not adoption.** No cooperative or institution has adopted ICN.

---

*Related: [ICN_FOR_COOPERATIVE_MOVEMENT.md](ICN_FOR_COOPERATIVE_MOVEMENT.md) · [ICN_FOR_EVERYONE.md](ICN_FOR_EVERYONE.md) · [ICN_INTRODUCTION_EVIDENCE_MAP.md](ICN_INTRODUCTION_EVIDENCE_MAP.md)*
