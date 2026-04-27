---
id: "0015"
title: "Public Surface and Learning Repo Architecture"
status: "draft"
created: "2026-04-27"
updated: "2026-04-27"
authors: ["Matt Faherty"]
reviewers: []
related_adr_candidates: ["0028", "0032", "0033", "0034"]
related_issues: []
supersedes: []
superseded_by: []
---

# RFC 0015: Public Surface and Learning Repo Architecture

## Status

`draft` — being written, not yet ready for review.

This RFC clarifies how ICN's public-facing identity is split across multiple domains and across multiple repos, and defines a future learning/onboarding repo (deploying to `learn.icn.zone`) without creating it in this RFC.

**Accepted RFC does not mean implemented.** This RFC is doctrinal coordination; the follow-up work is repo creation and content authoring, both of which happen outside this RFC.

## Summary

ICN currently confuses three things into one surface: the canonical public truth about ICN (intercooperative.network), utility routing for cluster endpoints and short links (icn.zone and its subdomains), and the future role-based learning/onboarding material that should not pollute the canonical site (a future `learn.icn.zone`). This RFC names six domains, assigns each a single role, and defines a future learning repo that teaches ICN without claiming the authority to define ICN. It explicitly does **not** create that repo, build that site, or change DNS — those are follow-up tasks.

## Problem statement

The project currently has more domains than well-defined surfaces. Several distinct concerns are conflated, which produces drift over time:

- **Canonical truth** is mixed with **marketing**, **onboarding pedagogy**, and **operational endpoints**. Without naming each surface separately, contributors land doc and content changes in whichever repo or branch is closest to hand, and the public site slowly accumulates onboarding content that should live elsewhere.
- **Onboarding and curriculum** ("how to join", "how to contribute as a developer / designer / organizer / operator / cooperative partner") have no home. Putting them on intercooperative.network risks (a) overclaiming program maturity per ADR-0032 and ADR-0033, and (b) burying the canonical truth surface under teaching material.
- **Adjacent surfaces** (a future Cooperative Systems Institute, a future Summit/event/convergence surface, Matt's personal hub) need to exist as named possibilities with assigned roles so they do not later silently take over the ICN identity.
- **`icn.zone` is already documented** ([docs/deployment/ICN_ZONE_ROUTING.md](../deployment/ICN_ZONE_ROUTING.md)) as utility routing with a redirect at root. That decision is correct and should be preserved here.

The cost of leaving this implicit: the public truth surface keeps drifting, contributors don't know where to put new content, and the project ships either an overclaiming front page or an underexplained one.

## Goals

- Name every public-facing domain ICN currently controls or anticipates, with one role each.
- Make explicit the rule that a learning/onboarding surface lives in a separate repo and a separate domain.
- Preserve the canonical truth boundary at intercooperative.network (ADR-0032, ADR-0033).
- Preserve the `icn.zone` utility routing model documented in [docs/deployment/ICN_ZONE_ROUTING.md](../deployment/ICN_ZONE_ROUTING.md).
- Define the future `learn.icn.zone` learning repo's purpose, guardrails, and content areas — without committing to its content here.
- Keep the regulatory-safe vocabulary discipline (RFC-0001, [docs/design/CONTENT_STYLE_GUIDE.md](../design/CONTENT_STYLE_GUIDE.md)) intact across surfaces.
- Keep the public website free of NYCN, "New York Cooperative Network", "Summit", "reference federation", or "first institution package" references.

## Non-goals

This RFC explicitly does **not**:

- Create a third repo (`InterCooperative-Network/icn-learn` or any other).
- Build the `learn.icn.zone` site.
- Draft the curriculum.
- Change DNS records.
- Change Cloudflare configuration.
- Move existing developer or contributor docs out of the main ICN repo.
- Make the public ICN website mention NYCN, Summit, reference federation, or any institution package.
- Create content for `cooperative-systems.org` or `thesync.net`.
- Decide what `faherty.network` contains — only its role.
- Mass-rename archived wallet or mobile docs (those are a separate scoped change).

## Background / current state

- **Canonical site**: `intercooperative.network`. Live, GitHub-Pages-deployed, sourced from `website/src/` in this repo. Bound by [ADR-0032 — Website Truth Boundary](../adr/ADR-0032-website-truth-boundary.md) and [ADR-0033 — Public Maturity Claims and Evidence Links](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md).
- **Utility routing**: `icn.zone` plus `api.icn.zone`, `pilot.icn.zone`, `metrics.icn.zone`. Documented in [docs/deployment/ICN_ZONE_ROUTING.md](../deployment/ICN_ZONE_ROUTING.md). Root redirect to `intercooperative.network` is the entire root behavior; subdomains serve K3s endpoints.
- **No onboarding home**: ICN has rich docs in this repo (`docs/onboarding/`, `docs/getting-started`, `docs/development/`), but no single learner-facing site. Material that wants to be progressive ("welcome → orientation → deep dive") has no home.
- **Adjacent ideas have surfaced**: a future Cooperative Systems Institute (broader theory/research/field-building) and a future Summit/event surface (`thesync.net`) have been mentioned in planning conversations but never assigned a doctrinal role.
- **NYCN / Summit content is already segregated**: the NYCN repo (private) holds NYCN- and Summit-specific docs and assets per the boundary established in PR #9 and the [ICN_DESIGN_SYSTEM.md](../design/ICN_DESIGN_SYSTEM.md) "no NYCN/Summit on the public ICN website" rule.
- **The project has an RFC process**: see [RFC-0000](RFC-0000-rfc-process.md). This RFC follows that lifecycle.

## Design options

### Option A — Single canonical site, ad-hoc onboarding

Keep onboarding and curriculum inside this repo. Add a `learn/` section under `intercooperative.network`. Treat any future `learn.icn.zone` as a CNAME to a path on the canonical site. Adjacent surfaces (`cooperative-systems.org`, `thesync.net`) are deferred until they have content.

### Option B — Separate learning repo + named domain roles (recommended)

Define one role per domain:

| Domain | Role |
|--------|------|
| intercooperative.network | Canonical ICN public truth surface |
| icn.zone | Utility routing — short links, QR, redirects, cluster subdomains |
| learn.icn.zone | Future role-based learning/onboarding site, separate repo |
| cooperative-systems.org | Future Cooperative Systems Institute; broader field-building |
| thesync.net | Future Summit/event/demo/convergence surface |
| faherty.network | Matt's personal/professional hub; points at the work, doesn't define it |

Create a future repo `InterCooperative-Network/icn-learn` for `learn.icn.zone` content. That repo teaches ICN; it does not define ICN. Architecture, ADRs, RFCs, state, and code remain in this repo. The learning repo references canonical truth via deep links rather than rewriting it.

### Option C — Single super-repo / monorepo

Pull all content into this repo: canonical site, learning, future Cooperative Systems Institute, future Summit/event surface. Use path-based routing or build pipelines to deploy to multiple domains from one source.

## Tradeoffs

| Option | Easier | Harder | Invariants preserved | Invariants stressed | New failure modes | Cost |
|--------|--------|--------|----------------------|---------------------|--------------------|------|
| A | Adding onboarding content fast | Keeping ADR-0032/0033 truth boundary; preventing curriculum from overclaiming program maturity; finding canonical truth amid teaching | Single source of build artifacts | Canonical truth boundary; member-first information design | Curriculum gradually rewrites canonical claims; public site becomes an explainer instead of a truth surface | Low up-front; high later (cleanup) |
| **B** (recommended) | Keeping canonical truth surface clean; letting curriculum evolve at its own cadence; keeping NYCN/Summit boundaries intact; future repos start with explicit guardrails | Maintaining cross-repo links; coordinating two release cadences | Truth boundary; meaning firewall; institution-package boundary; vocabulary discipline | Cross-repo navigation requires deliberate UX | Stale cross-repo links; duplicated explanations if the learning repo is not disciplined | Modest — new repo + deploy target |
| C | Single deploy pipeline | Every constraint above (truth boundary, institution boundary, surface separation) routed through path-based hacks | None — actively erodes most of them | All of them | Single repo becomes the everything-bucket; new contributors can't tell what the canonical truth is | High in coupling; rework cost when surfaces diverge |

Option B is recommended. The canonical site stays small, evidence-bound, and stable. The learning repo can iterate fast without touching the truth surface. Adjacent surfaces (`cooperative-systems.org`, `thesync.net`) have named roles waiting for them when content exists.

## Core/package boundary

This RFC is doctrinal: it defines surface separation, not a runtime contract. It nonetheless interacts with the kernel/app and institution-package boundaries:

- The canonical public site sources content from this repo. Public claims are bound by ADR-0032 and ADR-0033; they may not silently encode institution-package vocabulary.
- The future learning repo consumes ICN substrate documentation. It does not define ICN primitives, ADRs, or RFCs. Its README must declare this guardrail explicitly.
- The institution-package boundary stays at the NYCN/ICN seam. The learning repo can teach how to use ICN with an institution package conceptually; it must not encode any specific institution package as canonical.
- The vocabulary discipline ([docs/design/CONTENT_STYLE_GUIDE.md](../design/CONTENT_STYLE_GUIDE.md)) applies to every surface: obligation/allocation/settlement/unit/position/settlement asset/external settlement instruction/bridge receipt for ICN-native primitives; payment/wallet/balance/currency only when describing external systems on their own terms.

## Accessibility implications

The learning repo is the highest-leverage accessibility surface in the ecosystem because it is where new contributors meet ICN.

- The future `learn.icn.zone` site must inherit [docs/design/ACCESSIBILITY_BASELINE.md](../design/ACCESSIBILITY_BASELINE.md) (WCAG 2.2 AA floor; plain language; mobile-first; low-bandwidth; reduced motion; multilingual support paths).
- Curriculum content must use plain language, with glossary linkage rather than expert-only phrasing.
- Role-based paths (developer / designer / organizer / cooperative partner / node operator / tester / researcher / communications / educator / maintainer / trusted collaborator) must each have a phone-readable, low-bandwidth-readable entry point.
- The canonical public site keeps its current accessibility commitments; this RFC neither raises nor lowers that floor.

## Conflict / dispute path

This RFC defines a doctrinal split, not a runtime mechanism. If a downstream change attempts to put learning material on the canonical site or canonical truth on the learning site, the resolution path is:

- Reviewer flags the surface violation in PR review.
- Author moves content to the correct surface.
- If the question is whether content belongs on the canonical site at all, ADR-0032 governs.
- If the question is whether content overclaims program maturity, ADR-0033 governs.
- This RFC is not the venue for relational conflicts (those route through ADR-0029 once accepted).

## Security / privacy implications

- Splitting surfaces reduces leak risk: curriculum and onboarding flows that expect input from learners (forms, contact, accounts) live on a separate domain from the canonical truth surface, so a future contact form on `learn.icn.zone` does not put PII on the canonical site.
- The canonical site remains static, member-data-free, and low-attack-surface.
- DNS posture is unchanged by this RFC; that is a Non-goal.
- Cross-domain analytics, if ever introduced, must be opt-in and must respect [docs/design/CONTENT_STYLE_GUIDE.md](../design/CONTENT_STYLE_GUIDE.md) and accessibility baseline.

## Compute / automation boundary

Compute is not directly involved. Build pipelines deploying multiple sites must remain deterministic and reproducible. No runtime authority flows through these surfaces; they are static and informational.

## Website / public truth implications

- `intercooperative.network` remains the only surface bound by ADR-0032 and ADR-0033.
- `learn.icn.zone` is **not** a truth surface. It teaches; it does not claim. Maturity-band statements about ICN primitives must defer to the canonical site or to ADRs cited by deep link.
- `icn.zone` has no truth content; it is utility routing.
- `cooperative-systems.org` and `thesync.net` are not yet content-bearing; their truth posture is to be defined when they have content.
- `faherty.network` is personal; it does not carry ICN truth claims.

The canonical site continues to enforce: no NYCN, no "New York Cooperative Network", no "Summit", no "reference federation", no "first institution package".

## Migration / compatibility

- No existing content moves in this RFC. The current `intercooperative.network` source under `website/src/` is unchanged.
- The existing `icn.zone` redirect at root (per [docs/deployment/ICN_ZONE_ROUTING.md](../deployment/ICN_ZONE_ROUTING.md)) is preserved.
- Existing onboarding docs in `docs/onboarding/`, `docs/getting-started`, and `docs/development/` remain in place. They may eventually be referenced from the future learning repo via deep link, but no migration is forced.
- No archived wallet or mobile docs are renamed by this RFC.

## Open questions

- Should the learning repo and the canonical site share design tokens once those exist, or should each keep an independent visual layer?
- Should `learn.icn.zone` present an "I am a [developer / designer / organizer / cooperative partner / node operator / tester / researcher]" picker on first visit, or should role paths be discoverable through navigation?
- What is the canonical-truth deep-link convention from the learning repo to ICN ADRs and RFCs? Permalink anchors? Versioned references?
- Does Cooperative Systems Institute (`cooperative-systems.org`) deserve its own RFC when it has content, or is it a reuse of this surface model?
- Does `thesync.net` need its own RFC, or is the surface model from this RFC sufficient when content is ready?

## Decision criteria

Pick **Option B** if:

- The project intends to keep ADR-0032's truth-boundary discipline.
- The project intends to grow role-based onboarding without overclaiming program maturity.
- The project values cross-repo navigation cost less than monorepo coupling cost.

Pick **Option A** only if:

- The project is willing to absorb learning content under the canonical site and accepts the truth-boundary stress that creates.

Pick **Option C** only if:

- The project commits to building a single deployment pipeline that can route per-domain content correctly *and* enforce per-surface truth posture inside one repo.

## Outcome

To be filled in when this RFC moves to `accepted` or `rejected`. If accepted, the follow-up work is enumerated under "Follow-up implementation issues" below; this RFC does not produce ADRs by itself, but a small ADR may follow if cross-surface deployment policy needs to be recorded.

## Follow-up ADRs

If accepted, this RFC may produce:

- An ADR recording the canonical-site / learning-site truth-boundary policy (the rule that learning material may not encode canonical truth claims and must defer via deep link).
- A small ADR describing the cross-domain deployment guardrail (per-domain build, no shared analytics without opt-in, vocabulary discipline carried across surfaces).

ADRs are not pre-allocated in this RFC; they will be enumerated when this RFC moves toward `accepted`.

## Follow-up implementation issues

If accepted, the project should open issues to:

1. Create `InterCooperative-Network/icn-learn` repo with explicit README guardrail: "This repo teaches ICN. It does not define ICN. Canonical architecture, implementation status, ADRs, RFCs, contribution rules, and runtime documentation live in the main ICN repo."
2. Scaffold the static site for `learn.icn.zone` (build target chosen by the new repo's contributors; not constrained here).
3. Add the README guardrail in #1 with explicit links back to the main ICN repo's `README.md`, `docs/INDEX.md`, `docs/STATE.md`, and `docs/rfcs/RFC-0000-rfc-process.md`.
4. Add a first curriculum structure in icn-learn organized by role:
   - universal ICN orientation
   - developer path
   - docs/writer path
   - designer / member-experience path
   - accessibility path
   - organizer path
   - cooperative partner path
   - node operator path
   - tester / QA path
   - researcher path
   - communications / outreach path
   - educator / facilitator path
   - maintainer / steward path
   - trusted collaborator path
   - generic institution-package onboarding hooks (no specific package named)
   - glossary and language drills
   - exercises, workshops, templates
   - "where truth lives" reference page
5. Add canonical-doc references back to this repo using a stable deep-link convention.
6. Add a domain-map reference page in icn-learn naming the six domains and their roles.
7. Add vocabulary and public-claims discipline to icn-learn's contribution guide (carry [CONTENT_STYLE_GUIDE.md](../design/CONTENT_STYLE_GUIDE.md) discipline across).
8. Configure the deployment target for `learn.icn.zone` (specific provider chosen at repo-creation time).
9. Configure DNS for `learn.icn.zone` only after the repo and site exist and pass an internal review.
10. Later: update the canonical site's "Get involved" content (currently in [website/src/pages/get-involved.astro](../../website/src/pages/get-involved.astro)) to link into `learn.icn.zone` once that site is live, replacing or augmenting in-repo onboarding pointers as appropriate.

These are pointers, not commitments. The follow-up work happens after this RFC reaches `accepted` (or its substance is rolled into a smaller, faster decision).

## Validation / proof plan

This RFC is doctrinal. "Proof" of acceptance is structural:

- A future PR creates `InterCooperative-Network/icn-learn` with the README guardrail.
- A future PR adds the canonical-truth deep-link convention.
- A future deployment connects the new repo's build artifact to `learn.icn.zone`.
- The canonical `intercooperative.network` build remains green and continues to pass:
  - `python3 docs/scripts/doc_control_check.py --strict`
  - `cd website && npm run build && npm run lint`
  - `grep -RIE 'NYCN|New York Cooperative|reference federation|Summit|first institution package' website/src/` returns clean
- ADR-0032 / ADR-0033 enforcement remains the truth gate for the canonical site; no exception is created here.

---

## Notes

- This RFC is a small, deliberate piece. It does not draft curriculum, create repos, or change DNS. Its job is to make those follow-ups possible without re-opening the surface debate later.
- For *amendments* during `draft`, edit in place and bump `updated`.
- This RFC's relationship to ADR-0032 and ADR-0033 is downstream: it extends the truth-boundary doctrine across multiple domains rather than concentrating it on one site.
