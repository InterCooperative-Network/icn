---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-09
---

# Cooperative Codebase call · Thursday June 11, 2026 · meeting brief

> **Posture: learner-first, peer-technologist.** They are forming a tech cooperative. Formation is not ICN's layer. The goal of this call is one identified seam and one rehearsal candidate, or a clean "not yet" — both are wins. Do not pitch.
>
> Facts and non-claims in this brief are controlled by the [skeptic Q&A](ICN_SKEPTIC_QA.md) and the [evidence map](ICN_INTRODUCTION_EVIDENCE_MAP.md) (landing via [#2002](https://github.com/InterCooperative-Network/icn/pull/2002)). If anything said on the call outruns those documents, it was an overclaim — correct it on the call.

---

## Logistics (verified from the scheduling confirmation, Jun 2)

- **When:** Thursday, June 11, 2026, **3:30 PM Chicago / 4:30 PM Eastern**, 60 minutes.
- **Who:** Aaron Junot (per Matt's notes; the scheduling emails identify only "Cooperative Codebase"), plus Matt. Other participant on the invite: the ICN gmail account.
- **History:** booked via their Zeeg scheduler; rescheduled twice (Jun 1 → Jun 4 → Jun 11), the last at their request ("I have a conflict at the original time"). They have been flexible and apologetic about it — no signal of low interest, just calendar friction.
- **Location: ⚠️ NOT in the final confirmation.** The earlier Jun 1 booking used a MayFirst Jitsi room (link in the May 31 email in matt.faherty@gmail.com). **Pre-call action: confirm the link with Aaron.** Do not assume the old room carries over.
- **Their prep-question field ("anything that will help prepare?") was left blank.** No agenda was set on either side. That makes the first five minutes ours to shape — gently.

## What we actually know about Cooperative Codebase (and what we don't)

**Known (verified):**
- Forming tech cooperative, Chicago (Matt's notes from the original contact).
- No meaningful public footprint as of Jun 9: no website surfaced in search, no directory listings, no GitHub presence found under that name. Consistent with pre-launch.
- Infrastructure choices visible from the scheduling alone: Zeeg (privacy-marketed scheduler) and a MayFirst meeting room. MayFirst is a movement hosting cooperative — **whoever set this up already knows the movement-tech landscape.** Do not explain MayFirst, Co-op Cloud, or platform co-ops to this person from scratch. Assume literacy; verify depth by listening.

**Unknown (this is the listening agenda, not a gap to bluff through):**
- What they do or plan to do (client services? a product? developer collective?).
- How many members, what stage of formation, what legal structure.
- Why they booked the call — what they want from ICN or from Matt.
- Whether they know ICN from the website, the org profile, the Summit orbit, or somewhere else.

## The frame: this call is different from the McKenzie call

McKenzie was a co-op *developer* with a formation platform — the seam was Launch hands off into durable governance. Aaron is (apparently) a *founder*, and a technologist. Three things change:

1. **He is living the formation problem right now.** The honest line from [ICN_FOR_COOPERATIVE_MOVEMENT.md](ICN_FOR_COOPERATIVE_MOVEMENT.md) applies to him directly: a co-op forming today should use formation resources and Loomio, not ICN. Say that early. It buys all the credibility the rest of the call runs on.
2. **He can read the code.** Hard questions may be technical (why Rust, why not CRDTs/ActivityPub/X, what's actually implemented). The [skeptic Q&A](ICN_SKEPTIC_QA.md) tiers apply verbatim: live local proof vs fixture-backed vs design-only. State the tier; never blur.
3. **A tech co-op is a different kind of counterpart.** Four possible seams, in rough order of likelihood — hold all four loosely and let the call pick:
   - **Peer:** comparing notes on movement-tech; mutual orientation; nothing more. Fine outcome.
   - **Future user:** their co-op's own records (standing, decisions, member equity) on ICN someday, post-pilot.
   - **Builder:** a tech co-op that could build *apps* on the substrate, or take paid client work in the ecosystem ICN wants to exist. The kernel/app separation means domain meaning is exactly the layer a co-op like theirs could own.
   - **Contributor:** the bus-factor answer. Broadening the maintainer base is an explicit reason these introduction materials exist. Do not lead with this. If it comes up, the honest framing: "the bus factor is one, and I'm not pretending otherwise."

## First five minutes

1. "Tell me about Cooperative Codebase — I looked and you're nearly invisible publicly, which I assume is on purpose at this stage." (Invites the whole story; signals the homework.)
2. Acknowledge their layer: "You're in formation right now. I'll say up front: ICN is not useful to you for that, and I'll tell you exactly what it's not useful for."
3. Name the honest frame: "ICN is pre-pilot infrastructure for what comes after formation — the records and coordination that have to survive. I'm here to find out if there's a seam between what you're building and that layer, or to hear 'not yet.'"

Then stop talking and listen. Capture his vocabulary verbatim; use his words.

## Questions for Aaron (pick what fits, don't run the list)

- Where did you find ICN, and what made the call worth booking?
- What does Cooperative Codebase want to be — services, product, collective? Who are the members?
- What's your formation path looked like so far — what's been hard? (His answer is field data for the whole introduction effort.)
- What records does your co-op already know it will need to keep — and where do they live today?
- As a technologist: what would make you *distrust* a project like ICN? (Better than asking what would make him trust it.)
- What should software never touch in a cooperative? (The question that found the seam in every prior conversation.)

## ICN in one breath (only after listening)

The spine: **Standing → Authority → Decision → Obligation → Effect → Receipt → Evidence → Review.** Apps own meaning; the kernel enforces constraints without understanding them. Each co-op runs its own node. Receipts are evidence records, not currency.

The current truth, stated plainly (full version in the [skeptic Q&A](ICN_SKEPTIC_QA.md), artifact-by-artifact limits in the [evidence map](ICN_INTRODUCTION_EVIDENCE_MAP.md)):
- **Live local proof:** the 13/13 receipt-chain audit runs against a real local daemon/gateway, reproducible with one command from a repo checkout.
- **Fixture-backed:** the rehearsal shell runs without live infrastructure — presentation-grade by design.
- **Design-only:** production federation, member-facing apps, private-data handling.
- **Nobody uses ICN in production. There is no pilot. The bus factor is one.**

## Demo policy for this call

Default: **no live demo.** A first conversation is for listening. If a demo is genuinely wanted, offer it as the *follow-up* artifact — and only run what was dry-run beforehand (TASKS: dry-run before any live audience). If something must be shown live, the fixture-backed rehearsal shell mode (#1999) is the facilitation-safe choice; the one-command live rehearsal (#1997) is the "run it yourself" homework to offer a technologist instead. "Clone the repo and run one command, then tell me what's overclaimed" is a better gift to a peer engineer than any screen-share.

## The ask (the only one)

> "Tell me about one place in your formation work — or a co-op you've watched — where the records broke. Names stripped. Just the shape of what didn't survive."

If a seam emerges: smallest safe next step is a two-hour sanitized tabletop (one workflow, beat by beat, what must survive, who verifies later, what software should never touch). Not a pilot. No live data. No commitment. "This isn't useful to us" is an acceptable and useful answer.

## Boundaries (muscle memory)

- Not production-ready. Not a pilot ask. Not live federation. Not private-data ready. No co-op adoption claims.
- Vocabulary: settlement, obligation, allocation, position, receipt, provenance, evidence. Never: payment, currency, balance, wallet, token, crypto, blockchain for ICN-native primitives.
- Don't pitch. Don't list features. Don't fill silence. Don't promise anything dated. Don't ask for their data.
- "I don't know" and "that's not what ICN does" are complete sentences.

## Pre-call checklist (Wednesday)

- [ ] Confirm the meeting link with Aaron (final confirmation has none; old MayFirst room may not carry over).
- [ ] Merge [#2002](https://github.com/InterCooperative-Network/icn/pull/2002) so the skeptic Q&A and evidence map are linkable on `main`.
- [ ] Verify intercooperative.network key routes return 200 and claims carry maturity bands (ADR-0032/0033).
- [ ] Dry-run the fixture-backed rehearsal shell once, even though the default is no demo.
- [ ] Re-read the [skeptic Q&A](ICN_SKEPTIC_QA.md) once, out loud, the morning of.

## Capture grid (fill within 24h after the call)

| Field | Answer |
|---|---|
| What Cooperative Codebase actually is | |
| Why they reached out | |
| Seam identified (or "not yet") | |
| Their vocabulary for the records problem | |
| What software should never touch (their answer) | |
| Rehearsal candidate (sanitized scenario) | |
| Follow-up cadence agreed | |
| What I overclaimed and had to walk back | |

Then update `memory` (people entry for Aaron is thin by design) and TASKS.md.

---

*Related: [ICN_FOR_COOPERATIVE_MOVEMENT.md](ICN_FOR_COOPERATIVE_MOVEMENT.md) · [ICN_FOR_EVERYONE.md](ICN_FOR_EVERYONE.md) · [ICN_SKEPTIC_QA.md](ICN_SKEPTIC_QA.md) · [ICN_INTRODUCTION_EVIDENCE_MAP.md](ICN_INTRODUCTION_EVIDENCE_MAP.md) · prior-call pattern: [COOPERATIVE_FORMATION_PLATFORM_MEETING_PREP_2026-05-21.md](COOPERATIVE_FORMATION_PLATFORM_MEETING_PREP_2026-05-21.md)*
