---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-18
---

# Mackenzie ICN Meeting Prep — 2026-05-21

> A reading-frame for one conversation with someone who knows the work.

This packet adapts the [Thursday meeting brief](ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md) for the 2026-05-21 conversation with Mackenzie Jones (NYCN/Summit organizer, two-plus years inside the work). If the two disagree, the Thursday brief controls ICN repo-state facts for this meeting; this packet controls the conversational shape. Project-level truth still lives in the source-of-truth docs.

Treat Summit-organizing reality as shared ground. No 101 on committees, sponsor outreach, accessibility planning, registration, or handoff pain. Mackenzie knows.

## 1. The cooperative-movement problem

Cooperatives have democratic values and almost no owned digital infrastructure to run those values on. Governance lives in Loomio, Google Forms, and Slack threads. Accounting lives in QuickBooks or a spreadsheet. Membership lives in a Mailchimp list. Inter-organizational coordination lives in email. Each piece is owned by a landlord. When the landlord changes terms, prices, or politics, institutional memory walks with them — and there is no way to prove to a funder, accountant, partner co-op, or future organizer that a decision actually happened, by whom, under what authority, without asking them to trust the organization's word for it.

The values are there. The **owned digital rails** for governance, coordination, evidence, membership, obligations, and federation are not.

## 2. What ICN is trying to become

Not "an app." Not a database. Not crypto. Not SaaS. Not a federation server.

ICN is **social software running on social infrastructure**: a substrate for standing, authority, decisions, obligations, receipts, evidence, member participation, stewardship, and inter-cooperative coordination — designed so a cooperative or federation can run its own institution without trusting a vendor with its memory.

The substrate enforces constraints. The cooperative carries meaning. The kernel does not understand "vote," "credit limit," or "trust score" — it sees signed records, capabilities, and constraint sets, and produces receipts a third party can verify later.

A real substrate exists: daemon, identity, ledger, governance primitives, gateway, and proof-bearing receipt paths. Most of the human-facing surface — the app a member would actually use, the dashboard an organizer would actually open — is still design and fixture rehearsal, not production. NYCN is an active partnership track, not a formally committed pilot.

## 3. Why this matters for NYCN / Summit

Summit already contains the real workflow: committees, chairs, sponsor outreach, speakers, venues, accessibility planning, registration, logistics, follow-up, document sprawl, organizer memory, post-Summit handoff. The ICN spine maps onto that world. Each layer below is a thing Summit already does — the column on the right is what ICN is trying to let Summit eventually own without trusting a vendor with it.

- **Standing** — who sits on which committee with which authority class. Today: a holder-label in a roster doc. Tomorrow: queryable standing the next chair can verify.
- **Authority** — which clause or mandate lets a chair commit a venue or sign a sponsor letter. Today: implicit, sometimes "I'll ask Mackenzie." Tomorrow: the authority basis is on the action before it's signed.
- **Decisions** — a committee deciding "we book Venue X / accept Sponsor Y / publish Speaker Z." Today: a Google Doc minute. Tomorrow: a governance decision with cited mandate.
- **Obligations** — sponsor commitments, speaker honoraria, venue contracts, accessibility accommodations promised, day-of logistics. Today: forwarded threads + organizer memory. Tomorrow: tracked from creation through fulfillment.
- **Effects** — the bookings, the agreements, the published program, the accommodations actually arranged. Today: a calendar + an email. Tomorrow: an effect dispatched against the cited mandate.
- **Receipts** — the durable, plain-language record that an effect happened, who confirmed it, when. Today: screenshots. Tomorrow: receipts the next organizer can read without backchanneling six people.
- **Evidence** — what survives chair rotation and post-Summit handoff without leaking private fields. Today: organizer memory + zip files. Tomorrow: repo-safe evidence per NYCN's existing `PILOT-REHEARSAL-EVIDENCE.md` shape.
- **Review / federation** — outside verification: a funder, a partner co-op, a future organizer, an accountant. Today: trust the organizer's word. Tomorrow: receipts and provenance the outside party can verify without trusting us.

These are not future promises Mackenzie needs to take on faith. They are the layers ICN is trying to build, and the question for Thursday is whether the layers look real from inside Summit organizing.

## 4. The roles ICN intends to fill

- **Member interface** — plain-language participation, action cards, standing, obligations, current status. Mobile-first, offline-tolerant, accessibility-first. Spec at [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md). No live app.
- **Steward / operator interface** — coordination, divergence detection, follow-up, evidence trails, degraded states surfaced honestly. Spec at [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md). No live cockpit.
- **Institution package layer** — NYCN-specific charter, committee structure, Summit-specific fixtures and templates live in NYCN's own repo, not in ICN core. The NYCN repo already does this; the boundary is intentional.
- **Federation layer** — cooperatives coordinating across organizations with verifiable records. Not a blockchain. Not a global ledger. Receipts that can travel between cooperatives without a shared landlord.
- **Developer / cooperative-builder layer** — reusable substrate that other cooperative institutions can adopt. The point is not that NYCN is special; the point is that NYCN is the first instance and other cooperatives could be next.

## 5. The conversation — shape, not script

A back-and-forth structure, not a monologue. Each beat is *show → ask → listen*, in that order. Each beat should be one or two minutes of Matt talking, then five-plus minutes of Mackenzie talking. If Matt is talking more than half the meeting, the structure is failing.

**Ask less, listen more — and do not defend in the first round.** When a critique lands, do not argue. Capture the objection, translate it into a design question, and ask what would make the thing safer or more useful. The meeting's value is calibration, not persuasion. If something sounds wrong to Mackenzie, that signal is more valuable than any in-the-room explanation Matt could give.

### Beat 1 — Open with the problem, not the product

Two sentences:
> Cooperatives rent their digital rails. When the landlord changes terms, the cooperative's memory walks.

Then ask:
> *Where in Summit work have you seen institutional memory actually get lost — chair rotation, platform migration, fiscal sponsor change, end-of-year handoff?*

Listen. The answer becomes the anchor for the next two beats.

### Beat 2 — Mirror it back

Take whatever Mackenzie names and reflect it in ICN-spine terms:
> *What you're describing is a standing problem* (or an authority problem, or an evidence problem, or a handoff problem).

This is the moment of legibility, not the moment of pitch. Do not introduce vocabulary she hasn't already implicitly used.

### Beat 3 — Introduce the spine in one breath

> *Standing → authority → decisions → obligations → effects → receipts → evidence → review / federation.*

Eight words. Then ask:
> *Does that order match the order of how Summit work actually moves through your committees?*

The answer reshapes the rest of the conversation. If she pushes back on the order, follow her order.

### Beat 4 — Walk through one Summit workflow

See §6 below. Walk one candidate end-to-end through the spine. Stop after each beat and ask:
> *Does this look like how it actually works for you, or am I missing something?*

This is the heart of the meeting.

### Beat 5 — State what is not real yet, plainly

- No live member app.
- No live steward cockpit.
- Not a formally committed pilot. NYCN is the intended first cooperative partner; the next concrete human gate is exactly this kind of conversation, not a deployment.
- The Debian appliance boots in a dev image; it is not signed, not immutable, not partner-ready.
- Receipts alone do not prove legitimacy.

Then ask:
> *What of this would you trust? What would you not? What would you look at first?*

### Beat 6 — Surface the anti-features

Where should ICN stay out? What stays human and process-first and should not be automated? Ask:
> *What is the right anti-software boundary?*

Anti-features are as load-bearing as features in this kind of project. The most useful thing Mackenzie can do is name one.

### Beat 7 — Close on the smallest honest rehearsal

Not a launch. Not a pilot announcement. A tabletop walkthrough against fictional or sanitized data of one Summit workflow. Ask together:
> *What is the smallest thing that would be useful to rehearse? Who else should be in that room?*

## 6. The walkthrough — one Summit workflow, through the spine

Two candidates. Pick whichever fits where Mackenzie's earlier answers land. Walk it through the spine in seven short beats; after each beat ask:
> *How does this happen for you today? What would change if there were a receipt at this step?*

### Candidate A — Sponsor commitment
*Good for: external proof, obligations, future fundraising handoff.*

- **Standing** — the outreach committee, with the sponsorship clause delegated by the steering body.
- **Authority** — charter section authorizing the committee to solicit and accept sponsorship within the pre-approved sponsorship policy; outside that policy, steering approval is required.
- **Decision** — the committee decides to approach Sponsor Y at tier Z.
- **Obligation** — Sponsor Y commits support at tier Z in exchange for visibility deliverables A, B, C.
- **Effect** — agreement signed, logo published, table booked, accessibility provisions arranged.
- **Receipt** — each deliverable confirmed; the sponsor's counter-deliverable acknowledged.
- **Evidence** — post-Summit, the record survives in a form a future fundraising chair can read without backchanneling six people.

### Candidate B — Accessibility accommodation request
*Good for: privacy boundaries, care obligations, learning across years.*

- **Standing** — the accessibility committee, with charter authority to commit resources for accommodations within the committee's pre-approved scope.
- **Authority** — clause in the Summit operating doc that allows the committee to bind the organization to ASL, captioning, sensory-aware spaces, dietary accommodations, and similar.
- **Decision** — the committee approves accommodation X for Speaker / Attendee Y.
- **Obligation** — vendor booked, arrangements confirmed, day-of confirmation required.
- **Effect** — the accommodation is delivered.
- **Receipt** — the requesting party confirms the accommodation met the need.
- **Evidence** — post-Summit, the record exists in a form that lets next year's accessibility chair learn what worked and what didn't, without seeing private medical or personal details.

Both candidates are intentionally abstract enough to map to anyone's experience. Neither uses real NYCN/Summit private records.

## 7. The 90-second spoken explanation

Say this out loud. Not to read — to speak in Matt's voice. Direct, serious, not melodramatic, no manifesto, no investor pitch. Target under 100 seconds.

> Cooperatives have spent decades building democratic values and almost no time building the digital infrastructure those values need. We govern in Loomio, account in QuickBooks, remember in Slack threads, and federate in email. Every piece of the institutional spine — who's a member, who has authority, what was decided, what was promised, what was delivered — sits in someone else's product. When that product changes hands, when a chair rotates, when a fiscal sponsor leaves, the cooperative's memory walks. And we have no way to prove to a funder or a partner co-op that a decision actually happened, by whom, under what authority, without asking them to trust our word.
>
> ICN is an attempt to build the infrastructure layer cooperatives actually need to own: a substrate for standing, authority, decisions, obligations, receipts, evidence, and federation. Not a vote app. Not a payment app. Not a blockchain. A constraint-enforcement engine the cooperative runs, where the rules come from a charter the members ratify, and the records are signed in a way an outside party can verify later without trusting us.
>
> A real substrate exists: daemon, identity, ledger, governance primitives, gateway, and proof-bearing receipt paths. But most of the human-facing surface — the app a member would actually use, the dashboard an organizer would actually open — is still design and fixture rehearsal, not production. NYCN is the partnership track that's helping shape what that surface should look like. I'm not here to ship anything. I'm here to ask whether the spine looks real from inside Summit work, and what the smallest honest rehearsal would be.

## 8. Questions to ask Mackenzie

Ask one at a time. Leave silence after each one. Do not stack questions.

- Where in Summit work have you seen institutional memory actually get lost?
- Which Summit workflow would be worth rehearsing under this spine — and which would be a waste of it?
- Which decisions need proof later, and who needs that proof — funders, members, accountants, partner co-ops, future versions of you?
- What must stay human and process-first? Where is the right anti-software boundary?
- What would make this trustworthy to organizers — and what would make it untrustworthy?
- What is the smallest sanitized / tabletop rehearsal that's worth a meeting?
- Who else should be in this conversation — accountants, lawyers, mutual-aid operators, federation organizers — that we haven't invited yet?

## 9. Claims not to make

Matt reads this list before the call. This is muscle memory, not background.

- Not production-ready. Not externally audited. Not legally or regulatorily certified.
- Not a live federation. Not an end-to-end federation lifecycle.
- Not a formally committed cooperative pilot. NYCN is an active partnership track.
- Not a live member or mobile app. Not a live steward cockpit dashboard.
- Not a signed, immutable, A-B-updated, or production-ready appliance.
- Receipts alone do not prove legitimacy. Authority shortcuts must label themselves as shortcuts.
- The kernel/app firewall is partially CI-enforced (Wave 1 complete); the denylist is currently advisory through Waves 2–6.
- No use of *payment, currency, balance, wallet, token, crypto, blockchain, timebank* for ICN-native primitives. Vocabulary stays **settlement, obligation, allocation, unit, position, receipt, provenance, evidence**. This is a regulatory boundary, not a style preference.
- No real NYCN private data has been or will be put in git for this work. The Summit-workflow walkthroughs above are abstract enough to map to anyone's experience.

## 10. The freeze rule (Tuesday onward)

After Tuesday's rehearsal, the **human-facing story is frozen**. No new claims, no updated framing, no Thursday-facing changes to this packet or the Thursday brief unless something on `main` materially changes a stated fact. Normal ICN development continues on branches. Repo-detail rabbit holes are out of scope for Thursday unless Mackenzie explicitly asks.

## 11. Tuesday rehearsal checklist

Matt runs this Tuesday afternoon:

- Read the packet end-to-end once, top to bottom.
- Say the §7 ninety-second version aloud, twice. Time it. If it runs over 100 seconds, cut.
- Walk through candidate A aloud — one beat, one ask, listen-pause, next beat. Then do candidate B.
- Read the §8 questions aloud once. If any feels stiff or scripted, rewrite.
- Read the §9 claims-not-to-make list aloud.
- Pull up the three docs Matt will reference if asked: [`docs/OVERVIEW.md`](../OVERVIEW.md), [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md), and [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md).
- Confirm `git status` is clean on `main`. Confirm `gh pr list -R InterCooperative-Network/icn --state open` has not surfaced a surprise.
- Stop. Do nothing else with the repo before Thursday.

## 12. Post-meeting capture

Filled in after Thursday — kept here so it isn't drafted under time pressure.

- What Mackenzie identified as **real**:
- What she identified as **wrong / overbuilt / unclear**:
- The **smallest rehearsal** chosen:
- Who else she said should be in the next conversation:
- What needs to change in the spine doctrine, the specs, or the NYCN docs:

## Post-freeze delta log

| Date | Repo | Change | Does it change Thursday claims? | Action needed |
|------|------|--------|---------------------------------|---------------|
|      |      |        |                                 |               |

## Related reading (meeting-safe order)

If anyone wants to read the underlying material before Thursday:

1. [`docs/strategy/ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md`](ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md) — controls facts.
2. [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) — Phase 2 is ⏳.
3. [`docs/OVERVIEW.md`](../OVERVIEW.md) — the eight primitives, the receipt chain, the meaning firewall, the regulatory vocabulary.
4. [`docs/spec/network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md) — the proof rail; eight phases, wire-stable records, not yet emitted in runtime.
5. [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) and [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) — the human-facing contracts. Specs, not implementations.
6. [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) — doctrine. Receipts prove events, not legitimacy. Authority shortcuts must label themselves. Unresolved standing is not standing in production.
7. NYCN [`README.md`](https://github.com/InterCooperative-Network/nycn/blob/main/README.md) and [`docs/ORGANIZER-USER-READINESS.md`](https://github.com/InterCooperative-Network/nycn/blob/main/docs/ORGANIZER-USER-READINESS.md) — Mackenzie's side of the partnership track.
