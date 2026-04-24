# ICN Institutional Ecosystem Arc

> Status: living planning doc, written 2026-04-24, revisable
> Companion to: `ICN-Institutional-Operability-Roadmap.md`,
> `ICN-Roadmap-Live.md`, and the NYCN-* series in `docs/strategy/`.

This is the long arc from "stable substrate" to "usable institutional
ecosystem platform." It is execution-oriented and repo-grounded. It is not
a manifesto.

---

## Framing correction

NYCN is **not** "one customer running ICN." NYCN is the **first ecosystem
seed**. The Summit is its first high-pressure coordination cycle.

The product proof is not "an ICN gateway runs in a VLAN." The product proof
is: a small cooperative ecosystem can govern, coordinate, remember, and
verify its own work better with ICN than with the usual junk drawer of
email, spreadsheets, Google Docs, Facebook groups, private chats, payment
apps, disconnected SaaS, and exhausted organizer memory.

NYCN translates into:

- NYCN itself — a coordinating institution / federation seed (entity)
- Member co-ops — federated entities or external partner relationships
- The Summit — a program/event inside the institution
- Committees, working groups — scoped sub-entities (structures, not
  sovereign entities)
- Organizers — role-bearing members of NYCN and/or its committees
- Speakers, sponsors, venues, partners — relationship/obligation records
- Attendees — participant identities (DIDs, may or may not be members)
- Budgets, sponsorships, sessions, logistics — governed allocations and
  obligations, not ad-hoc spreadsheets
- Decisions, actions, receipts — institutional memory

This is the dogfood. It is also the template for the next ecosystem.

---

## Core thesis

ICN succeeds when a real cooperative ecosystem runs itself through ICN
because it is **easier and clearer** than the alternative — not because
people are ideologically committed to it.

The real test is not "did the Summit happen." The real test is "did NYCN's
internal coordination get *cleaner* and *more legible* because ICN was the
substrate." If the answer is no, ICN failed to be infrastructure.

---

## The long arc

1. **Stable substrate** — daemon, gateway, identity, gossip, registry,
   monitoring, CI, deployment. Approximately complete (April 2026).
2. **Living institutional ecosystem loop** — a single end-to-end loop
   from member identity → standing → action card → decision → effect →
   receipt, with persistence and audit. *Current focus.*
3. **NYCN/Summit dogfood as first ecosystem seed** — institution exists,
   committees exist, members have standing, decisions cause real effects,
   organizers see coordination state, sponsors/speakers/partners are
   represented as relationships not as bespoke Summit tables.
4. **Repeatable ecosystem package** — what NYCN has becomes a charter
   template, a bootstrap manifest, a starter set of committees, action
   cards, and program scaffolding that another ecosystem can apply
   without bespoke per-ecosystem code.
5. **Federation between real institutions** — when two institutions exist
   on ICN and need to coordinate (clearing, joint program, mutual
   recognition), the federation primitives carry that load. Not before.
6. **Commons / economic coordination layer** — settlement, mutual credit,
   resource grants, shared infrastructure pools. After (5), not before.
7. **Public legitimacy and adoption** — onboarding, mobile, public
   credibility, regulatory posture. Continuous from (3) onward, peaks
   after (5).

Phases overlap in practice. The ordering rule is: **never build phase N+1
on a phase N that is faking it.**

---

## Where we are right now

### Done (substrate)
- K3s pilot cluster, monitoring, durable storage (#1596), CI/runner
  hardening (#1597), pod anti-affinity (#1598), bootstrap idempotency
  (#1601 via #1617), web steward dashboard URL hygiene (#1599 via #1620).
- Daemon + gateway running, identity primitives, governance domain
  vocabulary, gossip, ledger primitives, kernel/app separation, meaning
  firewall enforced in CI.
- NYCN bootstrap apply: integration-tested and live-validated (#1593).

### Not yet done (the gap that matters)
- Domains do not persist across gateway restart in standalone mode
  (#1600).
- Charter is a YAML draft, not a registered/activated charter document
  (#1602).
- Bootstrap role assignment uses placeholder DIDs, not real person DIDs
  (#1603).
- A member cannot ask "what is my standing" (#1604).
- A member cannot see "what needs my attention" (#1608).

Until those land, the loop is not real. The substrate is healthy; the
**institutional fabric** is not.

---

## Next issue sequence (and why this order)

| # | Issue | Why it must come now |
|---|-------|----------------------|
| 1 | #1600 — persist governance domains across gateway restarts | Without this, every gateway restart wipes the institution. NYCN bootstrap must be re-applied from scratch. The "institution exists" claim requires durable state. |
| 2 | #1602 — live charter registration / activation endpoint | The charter is the constitutional document. Until the gateway can ingest and activate one, NYCN has no rule-of-law. Committees, roles, and decisions all reference it. |
| 3 | #1603 — bootstrap role assignment with real person DIDs | Without real DIDs, "members" are fiction. Action cards, voting, standing, and audit trails all collapse to noise. This unblocks (#1604) and (#1608). |
| 4 | #1604 — `GET /me/standing` | The first API a real human (or mobile client) calls. "Who am I in this institution? What roles? What scopes?" Without this, the member has no entry point. |
| 5 | #1608 — `GET /me/action-cards` | The first thing the member *acts on*. Vote, review, approve, verify, complete, acknowledge. This is where governance touches the ground. |

After this sequence, the loop closes:

> identity → standing → action card → decision → effect → receipt

That is the minimum living institutional ecosystem loop. Everything else
in the long arc builds on it.

---

## NYCN/Summit as first ecosystem package

NYCN is not a special case. It is the first instance of a reusable
institutional package. Every concept must generalize:

| NYCN concrete thing | ICN primitive | Generalizes to |
|---|---|---|
| NYCN itself | Cooperative / federation entity with charter | Any coordinating body, association, network |
| Member co-op of NYCN | Federated/external entity, with relationship record | Any participating org in any ecosystem |
| Outreach committee | Structure (sub-entity) inside NYCN with scoped authority | Working groups, task forces, standing committees |
| Summit program | Activity / program record under NYCN | Conference, campaign, mutual aid drive, organizing cycle |
| Summit session | Activity child / event record under the program | Workshops, plenaries, breakouts |
| Sponsor | Relationship record (partner) with obligations and receipts | Any external entity contributing resources |
| Speaker | Relationship record (participant role) with optional obligations | Any guest/contributor with a defined role |
| Venue | Resource/relationship record | Physical or virtual venue, infra provider |
| Sponsorship payment received | Settlement record (regulatory-safe terminology) attached to relationship | Any inbound resource flow |
| Organizer task | Action item / action card assigned to a role-bearer | Any institutional obligation |
| Decision to accept Summit budget | Proposal → vote → accepted → effect (allocation) → receipt | Any governed decision |

The non-negotiable: **do not hardcode "summit_session," "sponsor_tier," or
"speaker_bio"** as bespoke types. Build relationship/obligation/activity
primitives that the Summit consumes. Otherwise the second ecosystem cannot
adopt the platform without a fork.

---

## UX principles

### Member / mobile UX (the "tired person on a phone" test)

When a normal person opens ICN on a phone, in five seconds they should
understand:

- **who they are** — DID, display name, the institution they are looking at
- **what they are allowed to do** — roles and scopes, in plain language
- **what needs their attention** — action cards, in priority order
- **what relationship or role they are acting through** — "as a member of
  X committee," "as a delegate for Y"
- **what receipt proves what happened** — every confirmed action returns
  a receipt they can show

If a tired organizer cannot use it after a long day without reading
documentation, the UX has failed.

### Organizer / ecosystem UX

An organizer logging in should see, without hunting:

- committees and working groups they are part of (or oversee)
- programs and events in flight
- pending decisions (their own and the institution's)
- open obligations (whose, when, status)
- budget / resource records (with provenance)
- partner / sponsor / participating-org relationships (with their
  obligations and receipts)
- upcoming deadlines
- audit trail / institutional memory

This is "the ecosystem nervous system" view. It is the value proposition
to the people who actually keep institutions alive.

### The Action Card principle

Human-facing institutional power collapses into concrete cards:

- vote
- review
- approve
- join
- verify
- complete
- delegate
- escalate
- respond
- assign
- acknowledge

If a feature cannot be expressed as one of these, it is probably not
ready for a human yet.

---

## Regulatory-safe language rule

Use:

- settlement, unit, position, obligation, allocation, grant, receipt,
  treasury, resource flow

Avoid:

- payment, currency, balance, money, transfer, wallet-as-bank-account,
  invoice-as-billing

This is not cosmetic. It defines the legal posture of every flow ICN
mediates. CI enforces some of this via the regulatory compliance linter;
human review enforces the rest. New endpoints, new docs, and new UI must
respect it.

---

## Do not build the cathedral first

Things that should **not** be the next thing built, no matter how
intellectually appealing:

- Advanced federation protocols (treaties, multi-party clearing,
  cross-coop trust propagation) — premature without two real institutions
- Commons compute scheduler with trust-gated task placement — premature
  without active membership and obligations
- Speculative economic features (demurrage knobs, market-shaped flows,
  fancy credit policies) — premature without active settlement
- Marketplace / discovery surfaces — premature without trusted entities
  and active programs
- Anything that does not appear on the path from member identity to
  member action card

When the loop is real and at least one ecosystem has run a coordination
cycle on it, the rest unblocks naturally. Until then, scope discipline is
the most valuable engineering output.

---

## Success tests

A success test is a yes/no question that can be answered by a real
person doing a real thing. These are the tests for the next phase:

1. Can a coordinating body (NYCN or analogous) create and activate an
   institutional home — charter registered, parameters live — without
   hand-editing repo state?
2. Can a program or event (the Summit) live inside it as a first-class
   record, not as a bespoke schema?
3. Can committees and working groups hold scoped authority, with
   members, roles, and decisions of their own?
4. Can a real member call `/me/standing` and see who they are in the
   institution and what they may do?
5. Can a real member call `/me/action-cards` and see what currently
   needs them?
6. Can a member act on a card (vote, approve, complete, acknowledge)
   and watch the institutional state change as a result?
7. Can an accepted proposal cause a real, downstream operational
   effect (allocation, role assignment, obligation creation), not just a
   log line?
8. Can a task or obligation generated from governance be tracked to
   completion, with receipts?
9. Can a receipt / provenance chain be reconstructed for any decision,
   showing who, when, why?
10. Can sponsors, partners, speakers, and participating co-ops be
    represented as relationships and obligations — without hardcoding
    Summit-specific types?
11. Can a tired organizer use the system without understanding the
    architecture?
12. Can a second ecosystem repeat the setup without Matt manually
    translating everything? (This is the only real proof that NYCN is a
    seed and not a snowflake.)

When all twelve are yes, ICN has crossed from "infrastructure project"
to "institutional ecosystem platform."

---

## Architectural posture (recap, for agents)

- NYCN is the first ecosystem seed.
- The Summit is the first stress test.
- Committees and working groups are scoped sub-entities, not sovereign
  entities.
- Participating co-ops are ecosystem / federation relationships.
- Sponsors, speakers, venues, partners are relationship and obligation
  records.
- Budgets and resource flows are governed allocations and settlements,
  with receipts. They are not payment-app flows.
- Organizer tasks become action cards.
- Institutional memory is the product, not an afterthought.

The kernel/app separation and the meaning firewall are non-negotiable.
Domain semantics live in apps; the kernel enforces constraints without
understanding them. Persistence work in the next sequence (#1600 first)
must respect those boundaries.

---

## Metaphor (kept short on purpose)

ICN gives a cooperative ecosystem a nervous system:

- NYCN is the body
- The Summit is the stress test
- Committees and working groups are organs
- Members are cells with agency
- Receipts are memory
- Governance is coordination
- Federation is how bodies connect without one eating the other

If a section of this doc starts drifting into metaphor instead of
specifying repo-actionable work, cut it.

---

## What this doc is not

- It is not a replacement for `ICN-Institutional-Operability-Roadmap.md`,
  which goes deeper on the operability layer.
- It is not a roadmap of issues. The roadmap of issues is in
  `ICN-Roadmap-Live.md` and on GitHub. This doc orders **why**.
- It is not a NYCN spec. The NYCN-specific docs in `docs/strategy/NYCN-*`
  carry that detail. This doc reframes them as the first ecosystem seed.
- It is not a marketing artifact. The pitch lives in `ICN-Pitch.md`.
