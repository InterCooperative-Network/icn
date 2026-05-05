---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-04
---

# No-CLI organizer and member rehearsal workflow (generic ICN)

This document specifies the **guided, browser- or mobile-first** rehearsal story that institution packages should present to **organizers**, **members**, and **committee participants** — without treating a terminal or local script ladder as the default user journey.

It complements operator runbooks and ladder checkers: those remain **steward/operator** and **verifier** tooling. It does **not** replace package-local ingest pipelines; it defines the **human-facing** steps and boundaries those pipelines must respect.

**Non-claims:** No production readiness, no Phase 2 completion, no formal pilot for any partner, no live Google Drive/Groups/Sheets sync, no K3s/DNS/Forgejo mutation, no commitment that every step is implemented in product UI today. No private partner data belongs in the ICN repository.

**Privacy:** ICN docs and examples stay **generic** and **repo-safe**. Partner-specific nouns, fixtures, and overlay policy live only in partner repositories.

**Vocabulary:** Prefer **obligation**, **allocation**, **settlement**, **unit**, **position**, **receipt**, **provenance**, and **evidence** in substrate-facing copy. Do not describe ICN-native primitives as payment, currency, balance, or wallet surfaces.

**Tracking:** [ICN#1724](https://github.com/InterCooperative-Network/icn/issues/1724), [NYCN#52](https://github.com/InterCooperative-Network/nycn/issues/52), gate context [ICN#1703](https://github.com/InterCooperative-Network/icn/issues/1703), [NYCN#41](https://github.com/InterCooperative-Network/nycn/issues/41).

## 1. Three paths (must stay distinct)

| Path | Primary actor | Default surface | Role of CLI / scripts |
|------|----------------|-----------------|------------------------|
| **Organizer** | Coordinating body, facilitator | Guided shell (web/mobile); slides or scripted demo acceptable for May presentations | Invisible in the **default** story; may appear only as “what stewards run later” |
| **Steward / operator** | Trusted technical runner | Runbooks, localhost gateway, fenced execute modes | **Expected** — validation, smoke, ingest stages, proof checks |
| **Future member** | Participant acting on assigned work | Standing → action cards → native completion flows → receipt visibility | None for day-to-day participation; optional developer tools |

## 2. Mapped ICN primitives (substrate)

| Workflow step (plain language) | ICN primitive (generic) | Notes |
|--------------------------------|-------------------------|--------|
| Who am I; where may I act? | **Standing** — `GET /v1/gov/me/standing` | Join point for accessible shells; see [MEMBER_STANDING.md](../architecture/MEMBER_STANDING.md) |
| What needs my attention now? | **Action cards** — `GET /v1/gov/me/action-cards` | Derived rows only; cards do not define a separate mutation API ([ADR-0027](../adr/ADR-0027-action-card-contract.md)); contract [action-card.schema.json](../contracts/institution-package/action-card.schema.json) |
| Governed work units from decisions | **Action items** | Created and updated through governance HTTP surfaces for the domain |
| Completing an action item | **Action item** status change + **ActionItemCompletionReceipt** | Proof loop; read-side retrieval where documented ([nycn-organizer-user-readiness.md](nycn-organizer-user-readiness.md), runtime maps) |
| Votes and meetings | **Proposals**, **meetings**, linked **receipts** | Emitted action-card pairs include `proposal`/`vote`, `meeting`/`attend`, `action_item`/`complete` |
| Durable attestations | **Receipts** (governance, attendance, completion, per path) | Append-only records; HTTP retrieval is **not** uniform across every receipt kind today — shells must not over-promise |
| Audit narrative | **Provenance** / **evidence** | Human procedure + **repo-safe** artifacts; not private spreadsheets in the ICN tree |

Institution packages map local labels to these primitives in **package** docs and mappings — not by extending ICN core vocabulary.

## 3. Guided workflow (organizer-visible)

The organizer path is **review-first** and **mutation-last**. Numbers are the logical order for UX and training; implementation may batch steps where safe.

1. **Start rehearsal** — From a browser-friendly shell (or facilitator-led demo), open a “rehearsal” mode that is clearly labeled **not live production** and **not** a pilot commitment.
2. **Select material** — Choose **committed fixture / example** content prepared for the rehearsal, or upload **reviewed** organizer material per package policy. No silent import from live cloud drives on behalf of organizers.
3. **Preview parsed proposals** — Show **plain-language** summaries of proposed **action items** (and other governed objects if applicable) **before** any publish or assign step.
4. **Approve, reject, or edit** — Organizers record decisions in human-readable form; the system reflects edits in the preview. This mirrors the package’s human-review artifact boundaries (e.g. review → decisions files in partner tooling).
5. **Assign by human name / role / label** — Assignment UX uses **labels** organizers understand first. **DID binding** and directory overlay steps are a **separate**, guided **activation / private-overlay** flow (package-local policy; not replicated into ICN core docs).
6. **Preview before mutation** — Show exactly what will be created or changed (counts, titles, domains, assignee labels, mode flags). No mutation without an explicit second screen.
7. **Confirm with warnings** — Require explicit acknowledgement of **privacy** (what leaves the device, what enters git, what stays in overlay) and **mutation** (that the rehearsal may write to a **non-production** gateway when operators enable it).
8. **After confirmation** — Surface **action cards** (for holders who can authenticate), **completion status**, **receipts** where retrievable, **proof** or validation status in plain language, and a short **provenance / evidence** summary suitable for organizers.
9. **Export evidence packet** — Produce a **repo-safe** rehearsal packet (basenames, categories, redacted summaries) suitable for version control or public issue comments — **no** private fields by default.

## 4. Hard boundaries (all paths)

- **Preview / review boundary before mutation** — Organizers never “skip ahead” into publish without a visible diff or summary equivalent.
- **CLI as backend layer** — Scripts and modules remain for stewards, CI, and mechanical verification; they are not the organizer default story.
- **Privacy-safe evidence by default** — Evidence exports list commands, modes, artifact basenames, and outcome categories — not private contact or accommodation data.
- **No private partner data in ICN** — Partner repos hold package fixtures and overlay rules.
- **No production pilot claim** from running tooling or demos.
- **No live Google sync** from rehearsal tooling.
- **No K3s/DNS/Forgejo mutation** from package rehearsal defaults.

## 5. Accessibility (release-blocking for the user-facing path)

When a product shell implements this workflow, it must be **mobile-first**, **plain-language**, **keyboard** and **screen-reader** usable, with **large touch targets**, and **no terminal assumption**. Until that shell exists, facilitators should use **reduced-motion**, high-contrast materials and avoid requiring attendees to read raw JSON or stack traces in meeting rooms.

The PR-time review gate that operationalizes this requirement is [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md). Any PR that introduces or changes a shell, action-card surface, preview/review surface, evidence packet rendering surface, or member-facing governance flow must run §5's checklist before being treated as organizer-ready, member-facing, or pilot-ready.

## 6. Implementation status and smallest presentation storyboard

**Today:** ICN exposes the **HTTP** surfaces above; institution packages own ingest and review file formats. The ICN `web/pilot-ui` tree is oriented to **pilot community** flows (governance, timebank-style demos) and **steward enrollment** (`steward-dashboard`); it does **not** yet implement the full ingest → review → assign → preview → confirm ladder as a single guided organizer shell.

**Smallest presentation-grade approach for May (recommended):**

1. **Facilitator storyboard** — Walk through §3 using printed or slide **wireframes** (one screen per step). Use generic labels (“governing body,” “action item,” “holder”) — partner deck copy lives in the partner repo ([NYCN companion doc](https://github.com/InterCooperative-Network/nycn/blob/main/docs/NO-CLI-ORGANIZER-REHEARSAL-WORKFLOW.md)).
2. **Optional live substrate peek (steward-only)** — On a **non-production** gateway, demonstrate **standing** and **action cards** JSON in a tool the room consents to (e.g. REST client), **after** stating privacy boundaries — not as the organizer default.
3. **Defer product build** — A thin PWA that implements §3 end-to-end is **multi-sprint** work; track as follow-ups below rather than shipping a half-baked UI under presentation pressure.

## 7. Follow-up work (issues / contracts)

Track as discrete work items (GitHub issues welcome; not all may exist yet):

- **UI prototype** — Single guided shell (organizer mode) with the §3 screen list.
- **Fixture-backed demo** — Deterministic fixture load with no network by default.
- **Generic preview/review API contract** — Stable read models for “pending publish” summaries (package-agnostic shapes).
- **Evidence export contract** — Machine-readable **repo-safe** evidence summary schema shared with partners. Substrate contract: [`rehearsal-evidence-export.schema.json`](../contracts/rehearsal-evidence-export.schema.json) with companion notes at [`rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md). `$id` is the non-DNS URN `urn:icn:contract:rehearsal-evidence-export:v1` ([ICN#1729](https://github.com/InterCooperative-Network/icn/issues/1729)).
- **Private-overlay / DID binding activation flow** — Documented UX for label → directory → DID without leaking overlay into public repos.
- **Accessibility review** — WCAG-oriented pass on any new shell before calling it organizer-ready.
- **Package validation** — Mechanical checks that package outputs align with [action-card.schema.json](../contracts/institution-package/action-card.schema.json) where applicable ([ICN#1713](https://github.com/InterCooperative-Network/icn/issues/1713), partner validation issues).

## 8. See also

- [nycn-organizer-user-readiness.md](nycn-organizer-user-readiness.md) — ICN runtime surfaces for organizers (NYCN-oriented bridge)
- [NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md](../strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md) — Human gate: presentation → formalization → rehearsal
- [Institution package ActionCard notes](../contracts/institution-package/README.md)
- [STATE.md](../STATE.md), [PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
