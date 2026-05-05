---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-05-04
---

# Architecture due diligence

> **Convenience surfaces may be centralized. Authority surfaces should not be.**
>
> **Participation access is architecture, not polish.**

This document codifies a small, durable checklist that ICN authors and reviewers run when introducing or changing any architectural surface — schemas, contracts, identifiers, distribution paths, governance touchpoints, dependency choices, public artifacts, member- or organizer-facing flows. It exists so that subtle authority-capture mistakes and quiet participation exclusions get caught at design time, not after the fact.

The checklist is not a ban list. ICN does not forbid centralized dependencies, and ICN does not require every surface to be perfectly inclusive on the first commit. ICN requires that **dependencies be named, bounded, and assigned to the correct layer** — convenience or authority — and that **participation barriers be named and addressed in the design**, not deferred to "later" or to "polish."

## 1. Why this exists

### 1.1 The authority half

Centralized infrastructure is comfortable. DNS resolves to your domain. GitHub renders your README. Google Drive autocompletes addresses for the people you actually meant. Hosted issue trackers preserve threads across years. None of these are mistakes when they are used as **convenience**: discovery, mirroring, collaboration, import, presentation. The defaults are designed to be sticky for that reason.

The danger is **accidental authority transfer through defaults**. The same convenience surfaces, used without thinking, can become the place where institutional decisions get pinned, identified, or remembered — at which point a registrar policy, a hosting provider's pricing change, or an account suspension becomes a governance event for the institution.

The triggering example for this half was the rehearsal evidence export schema (icn#1729 / icn#1734). The first draft used an HTTP `$id` rooted at `https://intercooperative.network/contracts/...` because the existing `action-card.schema.json` set that precedent. That would have made the schema's identity depend on registrar continuity, ICANN policy, and hosting choice. The fix was to use a non-DNS URN (`urn:icn:contract:rehearsal-evidence-export:v1`) and explicitly demote DNS-backed surfaces to discovery / routing / mirror status.

### 1.2 The participation half

A formally correct architectural surface can still exclude people who should be able to participate. A governance system that assumes English fluency, strong vision, precise motor control, high literacy, fast devices, stable broadband, or low cognitive load is **silently deciding who can participate** in governance and coordination — without naming that decision and without asking whether the institution intended it.

ICN is institutional infrastructure for collective self-rule. Accessibility is not polish. It is part of institutional legitimacy. If the substrate is only legible to people with good eyesight, fast bandwidth, native English, high literacy, big screens, and insider vocabulary, then it is not infrastructure for collective self-rule — it is infrastructure for a class of insiders. The detailed surface-level rules live in [`../design-language/accessibility.md`](../design-language/accessibility.md); this document is the upstream architecture-review reflex that catches the same exclusions before they reach a surface.

The mistake — in either half — is easy to almost-make. The next mistake of the same shape will be too. This checklist surfaces the question every time, instead of relying on a reviewer noticing.

## 2. Core rules

### 2.1 Convenience vs authority

> Convenience surfaces may be centralized. Authority surfaces should not be.

A surface is a **convenience surface** if its job is to make something easier for a human or a system without arbitrating truth: a redirect, a short link, a QR code, a hosted UI, a collaboration tool, an import path. A convenience surface is allowed to fail; downstream work continues from the underlying truth, possibly degraded.

A surface is an **authority surface** if its job is to identify, name, govern, or arbitrate something institutional: a contract `$id`, a governance decision, an institutional decision record, a canonical identifier, a verification root. An authority surface that fails takes institutional truth with it.

The same external service can be both, in different roles. GitHub is a convenience surface for collaboration; the underlying git history is closer to an authority surface for code provenance. DNS is a convenience surface for discovery; it is not the authority surface for contract identity.

### 2.2 Participation access

> Participation access is architecture, not polish.

A person who can participate in a cooperative or federation should not be excluded because the system assumes a particular language, body, device, bandwidth level, literacy level, sensory mode, or technical skill level. The substrate's contracts, receipts, action cards, explanations, review flows, and governance surfaces must be **designed for participation from the start**, not retrofitted.

This is not a request for every surface to be perfectly accessible on the first commit. It is a requirement that participation barriers be **named in the design**, addressed where practical, and recorded as work-to-do where not.

Neither rule says "no centralized convenience" or "no defaults that favor any group." Both rules say: name the role and the assumption explicitly, and do not let defaults silently become authority or quiet exclusion.

## 3. The checklist

Run these questions when introducing or changing any architectural surface. They are short on purpose. They are meant to be answered, not skipped. The two halves correspond to the two core rules; both apply to most surfaces.

### 3.A Authority and dependencies

#### 3.A.1 Root of authority

What is the canonical identity / authority for this surface? Does it depend on DNS, a registrar, a hosting provider, a single SaaS account, or a single contractor? Name the dependency in one sentence.

#### 3.A.2 External dependencies

List every centralized service this surface relies on. Do not hide them inside "the platform." If the surface relies on AWS, GitHub, Google, Cloudflare, an SMS gateway, or a registrar, name each one and what role it plays.

#### 3.A.3 Revocation, censorship, and pricing risks

What happens if the dependency revokes access, raises price, changes policy, is deplatformed, or is taken offline by force? "It would be inconvenient" is acceptable for a convenience surface. "It would erase institutional truth" is not acceptable for an authority surface.

#### 3.A.4 Capture surface introduced

Does this change create a new place where a single party — a registrar, a host, a vendor, an admin account, a single contractor — can capture institutional decisions, identity, or memory? If yes, the surface is implicitly authoritative even if the design did not intend it.

#### 3.A.5 Human convenience surfaces

Is the centralized piece used for human convenience — typing a short URL, scanning a QR code, sharing a link, finding a doc? Convenience is acceptable here. **Document the surface as convenience, not as authority**, in the artifact itself.

#### 3.A.6 Machine authority surfaces

Is the centralized piece used as the canonical identifier or arbiter of truth for some machine consumer (a validator, a `$ref`, a webhook signer, a content-addressed lookup)? If yes, design for **non-centralized authority**: URN, content-addressed identifier, signed local artifact, in-repo path with git provenance, signed receipt.

#### 3.A.7 Offline / local / forkable fallback

Can the system continue to function — even degraded — when the centralized dependency is unavailable? If not, does that match the surface's role? A convenience surface is allowed to fail. An authority surface that requires a third party to be online is not actually authority; it is a SaaS dependency wearing an authority label.

#### 3.A.8 Verification path

How does a future maintainer (or a partner repository) verify the artifact's authenticity without trusting the centralized dependency? Pin to URN + repository path + git history + future receipt / provenance, not to a hosted URL alone. A verification path that runs through a single corporate domain is an SLA, not a verification path.

#### 3.A.9 Governance override or migration path

If the centralized dependency must change — registrar moves, host migrates, policy shifts, vendor exits the market — what is the migration path? Is it documented? Is the version pin clean? Can the change happen without the institution losing identity?

### 3.B Participation access

#### 3.B.1 Language access

Does this surface assume one language as the default? If so, is that documented, and is there a plan for translation, locale overrides, or non-default-language equivalents? Canonical schema field names may need stable terms (English is acceptable for substrate-level identifiers); member-facing **explanations** of those terms need localization paths from the start.

#### 3.B.2 Plain-language access

Are the words a member-facing surface uses readable by someone without legal, cooperative-governance, cryptography, or systems-theory background? Formal terms are allowed but must be paired with plain-language explanations on first use. "Patronage allocation reconciliation" needs an everyday-language gloss the first time a member encounters it.

#### 3.B.3 Screen-reader and non-visual access

Is the meaning of every visual element — cards, badges, charts, diagrams, icons, status bands — also conveyed in text or structured data that a screen reader can announce? A diagram-only explanation is an exclusion.

#### 3.B.4 Low-vision access

Does the surface support large text, high contrast, scalable typography, and adequate spacing without breaking? Does it survive a 200% browser zoom? Does it work in a high-contrast OS theme without losing meaning?

#### 3.B.5 Color-independent meaning

Run a grayscale preview. If two things are now indistinguishable, meaning is leaking into color. Every color-coded distinction must also carry a text label, icon, pattern, position, or structural cue.

#### 3.B.6 Keyboard, switch, and non-pointer access

Is every interactive element on a member- or organizer-facing surface usable without precise pointer control? Keyboard navigation, large hit targets, and switch-control compatibility should be baseline assumptions, not adaptations.

#### 3.B.7 Captions, transcripts, and non-audio access

Does any audio or video material include captions, transcripts, and a usable text fallback? Audio-only governance surfaces — voice prompts, audio decision recordings, podcast-only explanations — are exclusions unless paired with text equivalents.

#### 3.B.8 Cognitive load and step complexity

Does the surface require the member to hold many steps in mind at once, or to assemble information that should have been pre-assembled? Action cards and review flows must surface what matters first, then offer progressive disclosure. A formally correct process can still exclude people if it requires too many steps, too much jargon, or hidden prerequisites.

#### 3.B.9 Low-bandwidth and low-device access

Does the surface work on an older device with a slower CPU, less memory, and intermittent connectivity? Mobile-first means more than screen size. Heavy JavaScript bundles, autoplaying media, large unscaled images, and synchronous round trips are exclusions disguised as "modern" defaults.

#### 3.B.10 Assistive-technology compatibility

Does the surface use semantic HTML, ARIA roles where needed, and standard input controls — instead of custom widgets that break screen readers, switch controllers, or voice-control software? Custom widgets must be tested against real assistive technology, not just spec-compliance.

#### 3.B.11 Privacy-preserving accommodation path

When a participant needs an accommodation, can they get it without surrendering more private information than the institution actually needs? Accommodation requests must not become a back-channel that builds a registry of disability, language, or device profiles. The architecture should assume accommodations exist and provide for them by default, so disclosure is rarely required.

### 3.C Closing

#### 3.C Non-goals and non-claims

What is this surface explicitly **not** doing? Naming the non-claims prevents quiet scope creep into authority territory or implied participation guarantees. "This is a presentation surface, not a governance surface." "This URL is a discovery hint, not a contract identifier." "This shell is a fixture-only rehearsal, not a production deployment." Make the non-claims part of the artifact — in the README, the schema description, the doc preamble.

## 4. Worked examples

The checklist is easier to apply with named patterns. The list below is not exhaustive.

### 4.1 DNS

- **Acceptable**: discovery (`intercooperative.network`), routing (`api.icn.zone`, `pilot.icn.zone`), mirrors, short links, QR codes, human-facing references in print and conversation.
- **Not acceptable**: canonical contract authority. Schema `$id` URIs, contract identifiers, and verification roots must not be HTTP URIs whose continued existence depends on registrar, ICANN, or a single hosting provider.
- **Pattern**: use non-DNS URNs (e.g. `urn:icn:contract:rehearsal-evidence-export:v1`) for contract identity. Distribution / mirror URLs may be listed in companion fields like `x-icn-distribution-hints`, but those fields are explicitly **not authority**. See [`../contracts/rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md) (Identity section) and [`../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md`](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) for the worked rationale.

### 4.2 GitHub

- **Acceptable**: collaboration (PRs, code review, branch hosting), issue tracking, discussions, the rendered README, the public web UI for the repo, CI runners.
- **Not acceptable**: GitHub the platform as the **sole** source of institutional truth. The git history itself can support verification (it is content-addressed and reproducible across forks); the GitHub UI cannot.
- **Pattern**: pair the GitHub-hosted artifact with in-repo provenance — receipts, signed artifacts, mirrored history, content-addressed identifiers — so the institutional record survives the platform. Treat issue threads as coordination, not as the canonical decision record. Decisions belong in repo-safe artifacts (decision records, evidence packets) that survive an issue-tracker migration.

### 4.3 Google Drive / Sheets / Groups

- **Acceptable**: import source for organizer material, working drafts, collaborative editing during a planning cycle, organizer convenience.
- **Not acceptable**: canonical institutional memory. A document that exists only in a Google Drive owned by one organizer's account is one account suspension away from being lost.
- **Pattern**: anything that becomes institutional memory must be exported into the repository (or a non-Google-controlled store) with explicit provenance — a basename, a hash, a date, a role-attributed reviewer. The import path is convenience; the durable record is authority. See [`../contracts/rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md) for the repo-safe export contract.

### 4.4 Hosted UI

- **Acceptable**: an organizer shell, a member dashboard, a presentation surface, a mobile app — any surface designed to make participation easier.
- **Not acceptable**: a hosted UI as governance authority. The UI renders standing, action cards, decisions, and receipts; it does not arbitrate them. The hosted UI must also **not** be the only way a person can understand their standing, obligations, allocations, receipts, or governance choices — text, structured data, and offline-readable artifacts must remain available.
- **Pattern**: every hosted UI built on ICN must document its role as access surface, not governance authority, and must pair its visual flows with structured-data and text equivalents. The UI is the convenience layer over a substrate that an organizer or auditor can verify directly, with or without the UI.

### 4.5 Hosted issue trackers

- **Acceptable**: cross-team coordination, async review, issue triage, public discussion, stakeholder visibility.
- **Not acceptable**: the issue tracker as the institution itself. The thread is conversation; the decision is the institutional artifact.
- **Pattern**: when an issue thread converges on a decision, capture the decision in a repo-safe artifact (ADR, decision record, receipt) that survives the tracker. The tracker is the workshop; the institution is what walks out of the workshop.

### 4.6 Language

- **Acceptable**: choosing a working language for substrate-level identifiers and the first version of organizer-facing copy. English is acceptable as a working language; many ICN substrate identifiers will remain in English for stability.
- **Not acceptable**: treating English as the **only legitimate access path** to participation. A member who speaks another language is not less of a member.
- **Pattern**: canonical records may need stable terms in one language; member-facing **explanations** of those terms need localization paths and plain-language equivalents from the start. Translation is not a v2 feature; it is part of the architecture of any member-facing surface.

### 4.7 Vision

- **Acceptable**: visual cards, diagrams, colors, badges, and charts as **convenience** layers that make a surface easier for sighted members to scan.
- **Not acceptable**: visual-only meaning. A status bar that conveys "your obligation is settled" only through a green checkmark is invisible to a screen reader, indistinguishable in grayscale, and unhelpful at low vision.
- **Pattern**: the same meaning must be available to screen readers, text summaries, structured records, non-color cues (text labels, icons, position), and high-contrast modes. Every visual is paired with the structured equivalent that downstream consumers — assistive tech, partner systems, auditors — can read directly.

### 4.8 Cognitive load

- **Acceptable**: progressive disclosure, optional advanced views, "show details" toggles for members who want depth.
- **Not acceptable**: requiring every member to hold the full system model in their head before they can act. A formally correct process can still exclude people if it requires too many steps, too much jargon, or hidden prerequisites.
- **Pattern**: action cards and review flows surface what matters first, then allow progressive disclosure. Plain-language explanations sit at the top; formal terminology is paired with its plain-language gloss on first use. Required next steps are visible, not hidden behind navigation.

### 4.9 Motor access

- **Acceptable**: pointer-friendly UIs as a convenience layer for members who use them.
- **Not acceptable**: any member-facing governance or review surface that requires precise pointer control, fine motor work, or rapid timing to participate.
- **Pattern**: keyboard, switch, and large-target interaction are baseline assumptions for any organizer- or member-facing flow. Custom widgets are tested against real assistive technology, not just spec-compliance. Hit targets follow the WCAG AA minimum, not the desktop-only minimum.

### 4.10 Bandwidth and device

- **Acceptable**: progressive enhancement that loads richer experiences when the device and connection support them.
- **Not acceptable**: assuming every member has a recent flagship phone, a fast home connection, and uncapped data. Mobile-first means more than screen size; it means designing for the older device, the intermittent connection, and the bandwidth cap.
- **Pattern**: text-first member-facing surfaces; structured data over heavy client-side rendering for canonical reads; lazy-loaded media; caching; offline-readable artifacts where the member's role allows. Heavy JavaScript bundles, autoplaying media, large unscaled images, and required synchronous round trips are exclusions disguised as "modern" defaults.

## 5. How to use this checklist

- **Author**: run §3 mentally before drafting an RFC, ADR, schema, or process doc that introduces or changes an architectural surface. If any of §3.A.1–§3.A.9 or §3.B.1–§3.B.11 produce an answer that surprises you, write the answer into the artifact itself, not just into the PR description.
- **Reviewer**: when reviewing an architectural change, the checklist is the lens. Ask the questions explicitly. "What is the root of authority?" "Who can revoke this?" "Who is excluded by this default?" "What survives if the convenience surface disappears?" If the artifact does not answer them, request that it does.
- **Maintainer**: when an architectural surface changes hands or migrates, re-run the checklist. The right answer can drift as the project grows; what was acceptable centralization at thirty users may not be acceptable at thirty thousand, and an accessibility default that worked for a closed alpha may exclude people in an open rehearsal.

The checklist is intentionally light. It is the **v1 reflex layer**. It is expected to evolve as ICN learns more cases. Worked examples in §4 will grow when the project encounters new patterns worth naming.

## 6. Non-goals

- **Not a retroactive audit of every existing surface in this document.** The non-DNS `$id` audit for existing contract schemas lives in [icn#1737](https://github.com/InterCooperative-Network/icn/issues/1737); the surface-specific accessibility review gate for organizer / member shells lives in [icn#1731](https://github.com/InterCooperative-Network/icn/issues/1731). Other surface audits should be filed as their own issues.
- **Not a replacement for the surface-level accessibility rules.** The detailed WCAG-grounded rules — color, contrast, typography, motion, focus order, semantic HTML — live in [`../design-language/accessibility.md`](../design-language/accessibility.md). This document is the upstream architecture-review reflex; the design-language doc is the surface-implementation contract.
- **Not a CI gate.** A mechanical lint that scans for "DNS as authority" patterns or for missing `aria-label` is a separate, larger discussion. This document is the human reflex layer; a lint is a possible future complement.
- **Not a final or exhaustive list.** The checklist is short on purpose. Expand it through worked examples as the project encounters new cases — not by stuffing more questions into §3.
- **Not a ban list.** Centralized convenience is acceptable. Imperfect first cuts of accessibility are acceptable when the gaps are named and tracked. The requirement is to **name, bound, and assign to the correct layer**, not to eliminate.
- **Not architectural authority on its own.** This is a process / principle doc. ADRs decide. RFCs explore. Issues build. Tests prove. This document only sharpens the question that authors and reviewers hold while doing those things.

## 7. Non-claims preserved

This document does not imply or claim:

- production deployment, Phase 2 completion, or formal pilot status for any partner
- live federation, live Google Drive / Groups / Sheets sync, or any K3s / DNS / Forgejo mutation
- private-data handling
- a licensing decision

NYCN remains the **intended first cooperative partner**, not a committed formal pilot.

## 8. See also

- [`../contracts/rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md) — Identity section: worked rationale for the non-DNS `$id` decision that triggered this document
- [`../design-language/accessibility.md`](../design-language/accessibility.md) — surface-level accessibility rules (WCAG-grounded color, contrast, typography, motion, semantic HTML); this checklist is upstream of those rules
- [`../design-language/brief-v0.md`](../design-language/brief-v0.md) — design-language brief: civic, calm, legible, accessible by design
- [`../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md`](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) — surface-architecture context: DNS-backed surfaces are discovery / routing / human-facing, not contract authority
- [`../rfcs/README.md`](../rfcs/README.md) — RFC process; this checklist is upstream of RFC drafting
- [`../adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md`](../adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md) — ADR lifecycle; this checklist is upstream of ADR drafting
- [`../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md`](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md) — accessibility baseline for member interfaces; the surface-level decision the participation half of this checklist sits upstream of
- [icn#1738](https://github.com/InterCooperative-Network/icn/issues/1738) — issue this doc closes
- [`../contracts/schema-id-audit.md`](../contracts/schema-id-audit.md) — first deliberate application of this checklist: audit of every contract-schema `$id` in the repo, with per-schema keep / migrate / investigate recommendations and the migration safety rules
- [icn#1737](https://github.com/InterCooperative-Network/icn/issues/1737) — sibling issue: audit existing schema `$id` values (deliverable lives at the audit doc above)
- [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — surface-specific PR-time review gate for organizer / member shells; the operational layer the participation-access half of this checklist sits upstream of
- [icn#1731](https://github.com/InterCooperative-Network/icn/issues/1731) — sibling issue: surface-specific accessibility review gate for organizer / member shells (deliverable lives at the gate doc above)
- [icn#1729](https://github.com/InterCooperative-Network/icn/issues/1729) — closed: rehearsal evidence export schema (the trigger)
