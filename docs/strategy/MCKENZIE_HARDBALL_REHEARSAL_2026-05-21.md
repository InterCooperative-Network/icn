---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-20
---

# Thursday · McKenzie call · hardball questions

> Questions that have edges. Practice the response out loud before the call. The goal is not to "win" — it's to stay honest under pressure without collapsing the conversation.

She is friendly, technically literate, and has been burned by tech-for-coops vendors before. She ran the "Platform coops / tech tools for coops" session at the 2025 Summit. Assume she has seen every pitch shape.

---

## 1. "So what's actually working right now?"

**Bad answer**: "Almost everything! Let me show you our 30 crates..."

**Honest answer**:

> "The substrate runs. Daemon's on a K3s cluster since December. Eight kernel primitives — identity, ledger, gossip, governance, etc. — all have working implementations and tests. What's *not* live: the human-facing apps. There's no member-facing wallet UI, no steward dashboard, no production federation. The constraint engine is real; the surfaces a co-op member would actually touch are still rehearsal-stage."

---

## 2. "Why isn't this just another blockchain pitch?"

**Bad answer**: "Because we're decentralized but not on a blockchain!"

**Honest answer**:

> "Two reasons. One: no consensus protocol, no mining, no global state, no token. ICN is per-cooperative or per-federation; each instance is its own substrate. Two: the vocabulary is governance-first by design — settlement, obligation, allocation, position, receipt, provenance, evidence. Whether that posture is sufficient under any specific regulatory regime is an attorney question, but the substrate isn't claiming financial-product semantics."

---

## 3. "Why should I trust the receipts?"

**Bad answer**: "Cryptographic signatures!"

**Honest answer**:

> "You shouldn't trust the receipt alone. A receipt only proves *that an event happened with these signatures*. It doesn't prove the event was legitimate. Legitimacy comes from the cooperative's process — who had standing, who had authority, what the governance doc said. The receipt is just the part that survives so members or auditors can verify *years later* that the process actually played out the way the minutes say it did. We have an ABUSE_CASE_HARDENING doc that's explicit about this — receipts prove events, not legitimacy."

---

## 4. "How is this different from Loomio + a Google Sheet?"

**Honest answer**:

> "Today, for a small co-op with stable members? It's not, much. Loomio + a sheet works. ICN matters when: (a) the records need to outlast the platform — Loomio is a vendor, the sheet is in someone's Drive; (b) the records need to be verifiable by an outside party — a buyer of a member's interest, an auditor, the IRS — without trusting us; (c) you want to federate with other co-ops and need a shared substrate they can also verify. None of those problems hit a five-person worker co-op in year one. They hit hard in year five."

---

## 5. "What happens if you disappear? Bus factor?"

**Honest answer**:

> "It's bad. I'm the primary maintainer. There's a SECURITY policy, an issue taxonomy, a roadmap, all of it's in the open at github.com/InterCooperative-Network. Fiscal sponsor is Alchemical Nursery. But the bus factor on Matt-specifically is one. That's an honest constraint. Part of why I'm having this conversation is to find out if there are co-ops that would benefit enough from a sanitized rehearsal to make broadening the maintainer base worth the recruiting cost."

---

## 6. "Have you actually talked to any co-ops about this?"

**Honest answer**:

> "Not the way I should. NYCN is the reference institution — the Summit work we've done together gives me a real cooperative ecosystem to stress-test against. Beyond that I've held back deliberately. I didn't want to show this work until the substrate could carry a rehearsal. We're at the point now where I think one rehearsal — with a fictional or sanitized scenario — would tell us more than another year of solo development. That's why I asked for this conversation."

---

## 7. "Why now? Why this conversation?"

**Honest answer**:

> "Two reasons. One: you opened the patronage / ICA piece in your email — that's the area I think the substrate can actually demonstrate, and I wanted to know what you've been hitting in your own projects. Two: Launch and ICN sit on different layers. Launch makes formation doable. ICN cares about what survives after formation. If there's a clean seam between those two — formation hands off into durable governance — that's worth finding out together, and it doesn't require either of us to change what we're building."

---

## 8. "What do you want from me?"

**Honest answer**:

> "Honestly? Forty-five minutes of your real experience with one co-op whose patronage or capital-account record broke. Sanitized — no names, no real numbers. Just the shape of what didn't work. If after that you think there's a rehearsal worth doing together, we can talk about that. If you think the seam doesn't exist or isn't worth my time, I want to hear that too. I'm not asking for a pilot, I'm not asking for data, and I'm not asking Launch to integrate anything."

---

## 9. "Why not just publish a paper?"

**Honest answer**:

> "I have. Several. The whitepaper, the kernel/app separation architecture doc, the abuse-case hardening doc — all public. Papers don't tell me whether a cooperative developer would actually use the seam. That's a conversation, not a download."

---

## 10. "What's the failure mode for ICN?"

**Honest answer**:

> "Two big ones. First, scope creep into accounting or payments — that's how this becomes a fintech project and dies under regulatory weight. The discipline is to stay on the substrate side. Second, building a substrate so abstract that no co-op can use it. The cure for that is what I'm trying to do today — rehearse against a real cooperative's actual problem instead of building in the dark."

---

## Tone discipline (read before the call)

- Don't apologize for not being further along. Don't oversell either.
- "I don't know" is allowed and often correct.
- "That's a good question, let me think" is allowed.
- "That's not what ICN does" is allowed when accurate.
- Don't fill silence. If she pauses, let her.
- If you catch yourself listing features, stop mid-sentence and ask her a question instead.
- The goal of the call is to leave with **one identified seam** and **one rehearsal candidate**, or with a clear "not yet" — both are wins.
