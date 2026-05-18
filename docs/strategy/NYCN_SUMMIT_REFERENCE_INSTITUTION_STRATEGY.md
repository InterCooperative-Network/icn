---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-18
---

# NYCN / Summit as ICN's Reference Institution Strategy

> Recording the project-shaping insight that NYCN / New York Cooperative Summit is the first **institutional laboratory** for ICN — not merely an event demo, not a generic use case, and not a pilot. This document controls meeting-safe and design-time framing for that thesis. It does not commit ICN, NYCN, or any cooperative to a deployment, pilot, or production claim.

## 1. Core thesis

NYCN/Summit already has the shape of a **recurring democratic institution**. Across years it manages standing, authority, decisions, obligations, effects, receipts, and evidence — through committees, charters, fundraising, content/logistics, accessibility, marketing, multilingual materials, attendee feedback, lessons learned, follow-up, and handoff. The work exists. The infrastructure to remember the work does not.

ICN's job is not to replace organizers. It is to help the institution **remember what it decided, honor what it promised, learn from what happened, and carry the movement forward without making exhausted people become the database.**

NYCN/Summit is therefore the first realistic institutional laboratory for ICN, in the same way Mondragón became a reference institutional pattern for cooperative theory — a concrete instance worth shaping primitives against, not a one-off demo target.

## 2. What the Summit record reveals

Looking at NYCN's documentation, runbooks, and rehearsal materials, the Summit demonstrates:

- A **recurring yearly cycle**: recruitment, charter, committee formation, fundraising, content/logistics, accessibility, marketing, run-of-show, post-event, carryover into the next cycle.
- A **content / logistics seam** running from session idea → speaker outreach → confirmation → bio/headshot/description → support and interpretation needs → room and material needs → schedule slot → public program → run-of-show → day-of confirmation → post-event material collection.
- A **feedback loop** that takes raw private feedback, distills it into anonymized themes, runs it through organizer review, and (sometimes) converts it into obligations the next cycle's planning honors.
- **Accessibility and care obligations** that carry real consequences for people, with privacy boundaries that should never be casually crossed.
- **Sponsor, funder, and accountability trails** with bilateral obligations and proof requirements for outside parties.
- A persistent **organizer load and handoff problem**: institutional memory walks when chairs rotate.
- **Movement follow-through**: attendees willing to help who never get re-contacted because the list of "people who said yes" did not survive the week after Summit.

The Summit is not a niche workflow. It is a compressed annual rehearsal of nearly every primitive ICN claims to support.

## 3. What belongs in ICN core (generic, domain-agnostic)

ICN core must remain meaning-firewalled. It does not know what a Summit, speaker, session, or accessibility request "means." It only sees:

- **Entity** — who exists in the institution.
- **Standing** — who is currently in which body with which authority class.
- **Role** — the named position a standing instance carries.
- **Authority** — the clause / mandate that authorizes an action.
- **Mandate** — the authorized scope itself, with thresholds and expiry.
- **Obligation** — a commitment that creates expected future action.
- **Action / Effect** — the act dispatched against an authority/mandate.
- **Receipt** — signed, verifiable evidence that an action happened.
- **Evidence class** — receipt typology for downstream audit.
- **Privacy class** — visibility / retention / disclosure constraints.
- **Review status** — challenge, appeal, escalation state.
- **Escalation / handoff state** — transition of standing or obligation between holders.

ICN core does not gain a `Summit`, `Speaker`, `Session`, or `Accessibility` type. Those are domain meanings carried by institution packages.

## 4. What belongs in the NYCN / Summit package (domain-specific)

The Summit-specific meanings live in NYCN's repo (or in a `summit-package` once factored), built on top of ICN primitives:

- **SummitYear** — the recurring institutional cycle instance.
- **Committee** — content, logistics, finance, marketing, accessibility, technology, outreach.
- **Session** — proposed and accepted content units.
- **Speaker** — identified content contributor, with consent boundaries.
- **PublicProgram** — the authoritative public-facing schedule artifact.
- **FeedbackTheme** — anonymized aggregation of attendee feedback.
- **PlanningObligation** — a next-cycle commitment derived from feedback or retrospective.
- **AccessibilityCommitment** — bound resource/service promised, with privacy-class enforcement.
- **FollowUpInvitation** — consented, opt-in re-contact after the event.
- **EvidencePacket** — repo-safe, sanitized record of what happened in a Summit cycle.
- **HandoffPacket** — the durable transfer from this year's chair to next year's chair.

These are NYCN's meanings, not ICN's. ICN's job is to make sure the receipts, evidence, privacy classes, and authority chains underneath them are honest.

## 5. Primary rehearsal candidates

Three workflows are the most useful first rehearsals. Each is small enough to be walked at a tabletop, large enough to stress-test the spine, and abstract enough to never require real attendee data.

### 5.1 Content / session → public program (PRIMARY)

The content/logistics seam most relevant to a Summit content/program organizer. From session idea through speaker outreach, support, interpretation, schedule, public program, run-of-show, day-of confirmation, post-event material collection. This is the highest-leverage rehearsal because it exercises every spine layer in a single recurring workflow.

### 5.2 Feedback → next-year obligation (PRIMARY-2)

The hardest privacy / care boundary. Raw private feedback → anonymized themes → committee review → decision to act, defer, or reject → assigned obligation → planning change → evidence the decision was honored → next-cycle review. Forces the project to demonstrate institutional learning under explicit privacy class.

### 5.3 Organizer-interest follow-up (SECONDARY)

The movement-follow-through gap. Attendee opts into a possible role → consented follow-up invitation → committee invitation → accepted role / standing → assigned planning obligation. Exercises consent, invitation, role-assignment, and standing transitions without touching feedback content.

Sponsor commitment and accessibility accommodation remain useful **example workflows**, but they sit alongside, not above, the content seam.

## 6. Anti-software boundaries

Some things should not be formalized. Anti-features are as load-bearing as features. The Summit/NYCN reflex on what to keep informal includes:

- **Private organizer judgment calls** — discretion about how to handle a difficult speaker, a fragile sponsor relationship, or a sensitive attendee situation should not be reduced to a software state machine.
- **Care and accommodation specifics** — needs sit with the people receiving and providing them; only sanitized themes, retention-limited, ever travel into evidence.
- **Drafts that should expire** — agendas, schedules, and notes prior to publication should not become permanent records.
- **Personal contact information** — phone numbers, home addresses, accessibility specifics, dietary specifics, partner/spouse details — never on-chain, never bridged, never default-public.
- **Informal trust relationships** — long-standing relationships between organizers, sponsors, speakers; formalizing every interaction would chill the work.
- **Anything requiring consent** — re-contact, contact-sharing, public attribution, organization-affiliation disclosure — opt-in only, revocable, with retention limits.

ICN must support a clear **decay / expiry policy** and a clear **"never write this down"** category. The spine does not own everything an institution does.

## 7. Implications for the ICN roadmap

If NYCN/Summit is the reference institution, several priorities reorder:

- **Obligation memory** (creating, honoring, retiring obligations across institutional cycles) likely matters earlier than broad member-facing voting UX.
- **Privacy-preserving evidence** (sanitized themes, retention/expiry classes, consent-gated disclosure) is a prerequisite for any honest pilot, ahead of dashboards.
- **Tabletop rehearsal** against fictional/sanitized fixtures is the right gating step, not a live pilot.
- **Member and steward surfaces** should be shaped by real organizer load (committees, runsheets, post-event review), not abstract feature lists. The content/logistics seam is the right design driver.
- **Handoff** (cross-cycle transfer of standing, mandate, and obligation memory) deserves first-class spec treatment, not bolt-on doc structure.
- **The institution-package boundary** (what stays in NYCN vs. what ICN core owns) needs to be the central organizing principle of any Summit-related work — without it, NYCN-specific semantics will leak into the kernel.

## 8. Non-claims

- Not production-ready. Not externally audited. Not legally or regulatorily certified.
- Not a live cooperative pilot. NYCN is the **intended first cooperative partner**, not a formally committed pilot.
- Not ingesting real private NYCN data. All Summit-workflow modeling uses fictional / sanitized data only.
- Not replacing accountants, organizers, facilitators, lawyers, accessibility coordinators, or human judgment. ICN is infrastructure underneath that work, not a substitute for it.
- Not a CRM, social network, accounting system, attendee-management SaaS, or marketing platform.
- Receipts alone do not prove institutional legitimacy. Authority shortcuts must label themselves as shortcuts.
- Regulatory vocabulary stays **settlement, obligation, allocation, unit, position, receipt, provenance, evidence**. No use of *payment, currency, balance, wallet, token, crypto, blockchain, timebank* for ICN-native primitives.

## 9. Next concrete slices

These are the small, safe slices that follow from this thesis. None is a pilot. All are sanitized / tabletop / design-only unless explicitly upgraded later.

1. **ICN repo (this PR)** — record the thesis durably in `docs/strategy/`.
2. **NYCN/Summit organizer meeting packet** ([`NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md`](NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md), landed on [icn#1882](https://github.com/InterCooperative-Network/icn/pull/1882)) — already aligned with this thesis: content/program rehearsal is primary; feedback-to-obligation questions are explicit.
3. **NYCN repo** — sanitized rehearsal docs and the recurring-institution model:
   - `docs/strategy/SUMMIT_RECURRING_INSTITUTION_MODEL.md`
   - `docs/rehearsals/CONTENT_TO_PROGRAM_REHEARSAL.md`
   - `docs/rehearsals/FEEDBACK_TO_OBLIGATION_REHEARSAL.md`
4. **icn-learn repo** — a small organizer-facing learning-path stub at `docs/paths/nycn-summit-organizer-learning-path.md` translating the spine into learner terms.
5. **icn-community-bridge repo** — a small design note at `docs/nycn-summit-followup-bridge.md` on consented post-event follow-up boundaries and what must never be bridged.
6. **Network-ops repo** — no changes. Rehearsal work must not imply live infrastructure readiness.

Any further slice (real fixture wiring, runtime integration, formal pilot framing) is gated on the Thursday conversation with the NYCN/Summit organizer and explicit subsequent decisions.

## 10. Related reading

- [`docs/strategy/ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md`](ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md) — meeting-safe ICN facts for the 2026-05-21 conversation.
- [`docs/strategy/NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md`](NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md) — human-facing prep packet built on this thesis.
- [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) — Phase 2 is ⏳; NYCN partnership track sits inside Phase 2.
- [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) — doctrine for honest authority, shortcuts, and legitimacy.
- [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) and [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) — surfaces that should be shaped by the institutional load described here.
- NYCN [`README.md`](https://github.com/InterCooperative-Network/nycn/blob/main/README.md) and [`docs/architecture/NYCN_OPERATING_SURFACES.md`](https://github.com/InterCooperative-Network/nycn/blob/main/docs/architecture/NYCN_OPERATING_SURFACES.md) — the operating-surfaces model this thesis builds on.
