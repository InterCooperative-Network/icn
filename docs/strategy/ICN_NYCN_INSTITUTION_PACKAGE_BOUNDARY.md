---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-18
---

# ICN, NYCN, and the Institution Package Boundary

A short conceptual clarification. If a reader of the meeting stack is asking *"is ICN the platform, NYCN the demo, or are they the same thing?"* — this doc answers that.

The answer: **ICN is the generic infrastructure. NYCN is the first real reference institution. Institution packages are the reusable layer between the two.**

## 1. The clean separation

- **ICN is the substrate / constraint engine.** It owns the generic machinery and nothing more.
- ICN core owns: identity, standing, roles, authority, mandates, obligations, actions/effects, receipts, evidence, privacy classes, review/challenge, federation hooks.
- ICN core does **not** know what a Summit, speaker, sponsor, accessibility request, worker-coop formation path, board seat, or patronage worksheet *means*. Those are domain concepts.
- Domain meanings belong in **institution packages**, not in the kernel.

> ICN should not become a pile of hard-coded cooperative-sector nouns. It should provide the generic machinery that lets institutions express their own nouns safely.

## 2. NYCN / Summit as reference institution

NYCN/Summit is not "the ICN demo" in the sense of a toy showcase. It is the first **reference institution**: a real cooperative ecosystem whose recurring work lets ICN test whether its generic primitives can support actual democratic institutional life. The long-term goal is not one NYCN-shaped system, but a library of institution packages that other cooperatives, federations, events, associations, and support organizations can fork, adapt, and govern for themselves.

That role gives NYCN/Summit two responsibilities:

- **Stress-test ICN primitives** by running real institutional seams against them — committees, sponsors, sessions, accessibility, feedback, follow-up, handoff, recurring cycles.
- **Not leak Summit-specific concepts into ICN core.** If NYCN ever needs an ICN primitive that does not exist, the work goes into the kernel as a *generic* primitive; the Summit-specific shape stays in the NYCN package.

The reference-institution thesis is recorded separately in [`NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md`](https://github.com/InterCooperative-Network/icn/blob/main/docs/strategy/NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md) (landed via companion PR [icn#1883](https://github.com/InterCooperative-Network/icn/pull/1883)).

## 3. Institution packages

An **institution package** is the reusable meaning layer between ICN core and a local deployment. It carries domain knowledge — the *meanings* the kernel deliberately does not know.

A package may contain:

- role maps
- governance templates
- workflow templates
- obligation patterns
- evidence classes
- privacy defaults
- fixture data
- tabletop rehearsal scripts
- schema examples
- onboarding docs
- member / steward surface language
- bridge patterns for advisors, service providers, accountants, lawyers, fiscal sponsors

Plausible packages over time:

- worker cooperative formation package
- cooperative board governance package
- Summit / recurring event package (NYCN is the first instance)
- federation membership package
- community land trust package
- mutual aid package
- participatory budgeting package
- fiscal sponsorship package
- patronage / allocation governance package
- consented follow-up / community bridge package
- committee governance package

The goal is not a marketplace and not a vendor catalog. The goal is a versioned, forkable, governable library of *worked examples* that real cooperative builders can adapt without having to re-derive the institutional spine from scratch.

## 4. Four-layer table

| Layer | Owns | Does not own |
|---|---|---|
| **ICN core** | Generic primitives, constraint enforcement, signed receipts, evidence, privacy classes, identity, federation hooks | Summit-specific, Launch-specific, or co-op-sector-specific meanings |
| **Institution package** | Reusable domain workflows, role maps, templates, fixtures, privacy defaults, evidence patterns | Kernel logic, universal protocol rules, local private records |
| **NYCN / Summit package** | Summit cycles, committees, sessions, speakers, sponsors, accessibility, feedback, handoff patterns | Generic ICN primitives or universal protocol behavior |
| **Local institution deployment** | Actual members, charter choices, private records, local policies, real-world authority | Changing ICN core semantics or pretending one local model is universal |

If a question is unclear about which layer it belongs to, the safest move is to push it *up* (away from the kernel) until the universality test fails — then pull it back down one layer.

## 5. Why this matters for the Thursday Launch conversation

The Thursday meeting is about whether the formation/development platform's workflow has a clean seam with ICN's durable governance/memory layer. The layering above is what makes that question precise:

- Launch itself is not an institution package. It is a co-op formation/development platform.
- Launch's workflow may reveal a future **worker cooperative formation package** — or a **formation-to-governance handoff package** — that other formation tools (and cooperatives themselves) could fork.
- A package would not replace Launch. It would model the handoff *from* formation steps *into* durable governance: standing, authority, obligations, documents, advisor/provider evidence, and post-formation institutional memory.
- That seam — where Launch's workflow output becomes a package-shaped artifact ICN can carry forward — is what we are probing Thursday.

## 6. Non-claims

- Not production-ready.
- Not a live institution package marketplace, registry, or store.
- Not a formal NYCN pilot.
- Not a formal Launch pilot.
- Not claiming NYCN is the universal model — it is the first worked example.
- Not replacing Launch, TA providers, lawyers, CPAs, bookkeepers, organizers, or human judgment.
- Not putting real partner or private data in git.
- Not introducing Summit-specific (or Launch-specific) types into ICN core. The kernel stays generic.
- Vocabulary stays **settlement, obligation, allocation, unit, position, receipt, provenance, evidence**.

## 7. Related reading

- [`COOPERATIVE_FORMATION_PLATFORM_MEETING_PREP_2026-05-21.md`](COOPERATIVE_FORMATION_PLATFORM_MEETING_PREP_2026-05-21.md) — Thursday primary prep packet (formation-to-governance discovery).
- [`ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md`](ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md) — controls meeting-safe project-state facts.
- [`NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md`](NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md) — secondary / backup NYCN/Summit organizer variant.
- [`NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md`](https://github.com/InterCooperative-Network/icn/blob/main/docs/strategy/NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md) — reference-institution thesis (landed via companion PR [icn#1883](https://github.com/InterCooperative-Network/icn/pull/1883)).
- [NYCN repo `README.md`](https://github.com/InterCooperative-Network/nycn/blob/main/README.md) — what NYCN already does today.
- NYCN [`#67`](https://github.com/InterCooperative-Network/nycn/pull/67) — Summit recurring-institution model + sanitized rehearsals.
