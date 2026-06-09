---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-09
---

# ICN Introduction Evidence Map

> Every public claim in ICN's introduction materials should trace to a verifiable artifact, and every artifact has limits. This map lists the artifacts behind the claims in [ICN_FOR_COOPERATIVE_MOVEMENT.md](ICN_FOR_COOPERATIVE_MOVEMENT.md), [ICN_FOR_EVERYONE.md](ICN_FOR_EVERYONE.md), [ICN_HANDBILL.md](ICN_HANDBILL.md), and [ICN_SKEPTIC_QA.md](ICN_SKEPTIC_QA.md) — and states explicitly what each one does **not** prove.
>
> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). Merge states below were verified against GitHub on 2026-06-09.

---

## How to read this

Each entry answers three questions:

1. **What is it?** The artifact, with a link.
2. **What does it prove?** The claim the artifact actually supports.
3. **What does it NOT prove?** The larger claim someone might be tempted to make, which this artifact does not support.

The honest summary of the whole table: **ICN's strongest current evidence is local live-daemon proof and fixture-backed rehearsal.** Nothing below is production evidence, pilot evidence, or adoption evidence.

---

## Core proof artifacts (icn repository)

### [icn#1985](https://github.com/InterCooperative-Network/icn/pull/1985) — live receipt-chain audit path reaches 13/13 (merged 2026-06-06)

- **Proves:** The full receipt chain — decision through authority, obligation, effect, dispatch, and receipt — can be audited end-to-end against a **real local `icnd` daemon and gateway**, with all thirteen verification links passing. This is live-execution evidence, not fixture playback.
- **Does NOT prove:** Multi-node operation, multi-institution federation, performance under load, production hardening, or that anyone outside the maintainer has run it.

### [icn#1997](https://github.com/InterCooperative-Network/icn/pull/1997) — one-command local 13/13 receipt-chain rehearsal (merged 2026-06-09)

- **Proves:** The 13/13 receipt-chain demonstration is **reproducible by a local operator with a single command**. The proof in #1985 is not a one-off; it is packaged and repeatable from a repo checkout.
- **Does NOT prove:** That the rehearsal has been independently reproduced by a third party, or that it works outside the documented local environment.

### [icn#1998](https://github.com/InterCooperative-Network/icn/pull/1998) — pending-publish summary row contract (merged 2026-06-09)

- **Proves:** A **stable contract (schema) exists** for summarizing pending-publish state, so rehearsal surfaces and audit tooling share one validated shape instead of ad-hoc output.
- **Does NOT prove:** Live production use of the contract. A contract is schema-level evidence (proof that the shape is defined and validated), not runtime evidence.

### [icn#1999](https://github.com/InterCooperative-Network/icn/pull/1999) — fixture-backed rehearsal shell demo mode (merged 2026-06-09)

- **Proves:** A facilitated walkthrough of the receipt-chain story **can run from recorded fixtures with no live infrastructure**, making demonstrations portable and removing live-system risk from facilitation settings.
- **Does NOT prove:** Anything about live execution. Fixture mode is presentation-grade evidence by design; the live-execution evidence is #1985/#1997. Conflating the two modes is exactly the overclaim this evidence map exists to prevent.

### [icn#2000](https://github.com/InterCooperative-Network/icn/pull/2000) — proof-level taxonomy and rehearsal capability matrix (in review as of 2026-06-09)

- **Proves (once merged):** ICN has a **shared claim-boundary vocabulary** (proof levels L0–L8, from design-only through production hardening) and a per-capability matrix, so contributors, facilitators, and reviewers cannot accidentally over- or under-claim. This is anti-overclaim infrastructure — it raises the honesty of claims, not the readiness of the system.
- **Does NOT prove:** Any capability's readiness. A taxonomy describes evidence; it doesn't create it.

Supporting merged work behind the receipt-chain path, for completeness: [icn#1990](https://github.com/InterCooperative-Network/icn/pull/1990) (durable effect-dispatch evidence persistence), [icn#1993](https://github.com/InterCooperative-Network/icn/pull/1993) (bounded dispatch-evidence backfill), [icn#1996](https://github.com/InterCooperative-Network/icn/pull/1996) (decision-hash ledger index for scalable receipt-chain lookup). These harden the evidence path; they carry the same limits as #1985.

---

## Cross-repository artifacts

### [nycn#78](https://github.com/InterCooperative-Network/nycn/pull/78) — live economic receipt-chain demo v4 (merged 2026-06-08)

- **Proves:** The ICN live receipt-chain path can be **driven from an external repository** (the NYCN organizing repo), reaching the same 13/13 audit-verified result over a real local `icnd`/gateway. The proof story is not confined to ICN's own test harness.
- **Does NOT prove:** Independent third-party validation (the demo is operated by the same maintainer), live NYCN institutional use of ICN, or any production relationship between NYCN and ICN.

### [icn-learn#3](https://github.com/InterCooperative-Network/icn-learn/pull/3) — NYCN/Summit organizer learning path (merged 2026-05-18)

- **Proves:** A **learning scaffold exists**: eight modules translating ICN's institutional spine (standing, authority, decision, obligation, effect, receipt, evidence, review) into cooperative-organizer reality.
- **Does NOT prove:** That organizers have completed it, that it has been validated in a training setting, or anything about deployment. It is teaching material, not deployment or production documentation.

### [icn-community-bridge#1](https://github.com/InterCooperative-Network/icn-community-bridge/pull/1) — post-event follow-up bridge boundaries (merged 2026-05-18)

- **Proves:** The **privacy boundary for movement-event follow-up is designed and written down**: consent-first, no raw attendee mirroring, no identity collapsing, no private accessibility or care data. The project's data-minimal posture toward real communities is documented before any implementation exists.
- **Does NOT prove:** Implementation. This is a design note. No bridge software exists, and the note explicitly makes no implementation claim.

---

## Claim-to-evidence quick reference

| Claim made in the introduction materials | Supporting artifact(s) | Evidence tier |
|---|---|---|
| "The substrate runs; the receipt chain verifies end-to-end (13/13) against a real local daemon and gateway" | icn#1985, icn#1997 | Live local proof |
| "The demonstration is reproducible with one command" | icn#1997 | Live local proof (packaged) |
| "A facilitated walkthrough can run without live infrastructure" | icn#1999 | Fixture-backed |
| "Rehearsal surfaces share a validated summary contract" | icn#1998 | Schema/contract |
| "ICN maintains a shared proof-level vocabulary against overclaim" | icn#2000 (in review as of 2026-06-09) | Documentation/process |
| "The proof path can be driven externally from the NYCN repo" | nycn#78 | Live local proof, externally driven |
| "An organizer learning path exists" | icn-learn#3 | Teaching scaffold |
| "Event-data privacy boundaries are designed consent-first" | icn-community-bridge#1 | Design note |

Claims **absent** from this table are absent on purpose: there is no production evidence, no pilot evidence, no multi-institution federation evidence, no private-data handling evidence, and no adoption evidence to cite.

---

## Non-claims

What this evidence map does **not** claim:

- **Not production-readiness.** No artifact above is production evidence.
- **Not a pilot.** No artifact above authorizes or documents a pilot with any institution.
- **Not live federation.** No artifact above demonstrates federation between real institutions.
- **Not private-data readiness.** Every artifact above uses fictional or sanitized data.
- **Not legal, privacy, or compliance advice.**
- **Not completeness of implementation.** Specs and contracts cited here may describe capabilities that are design-only; the proof tier column states the boundary.
- **Not adoption.** No cooperative or institution has adopted ICN.

---

*Maintenance note: when an artifact's state changes (e.g. icn#2000 merges, or a new proof tier is reached), update this map in the same change that lands the evidence — stale evidence claims are overclaims with a delay.*
