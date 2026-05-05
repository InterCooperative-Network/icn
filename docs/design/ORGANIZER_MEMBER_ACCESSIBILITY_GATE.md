---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-05-05
---

# Organizer / member accessibility gate

> **Participation access is architecture, not polish.** A surface that does not pass this gate is not organizer-ready, member-facing, or pilot-ready, regardless of how complete its functionality looks.

This document is the **PR-time review checklist** for any organizer-facing or member-facing surface introduced or changed in ICN. It is the operational layer beneath the architecture due-diligence reflex in [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) (participation-access half) and beside the surface-floor doc in [`./ACCESSIBILITY_BASELINE.md`](ACCESSIBILITY_BASELINE.md). It does not replace either; it is the checklist the **reviewer** runs when the PR lands.

## 1. Principle

A person who can participate in a cooperative or federation should not be excluded because the system assumes English fluency, strong vision, precise motor control, high literacy, fast devices, stable broadband, low cognitive load, or terminal fluency. Organizer and member surfaces are **democratic participation surfaces**: their accessibility is not a polish phase, it is part of institutional legitimacy.

CLI and terminal paths may exist for stewards, operators, and CI. They must not be required for organizer or member participation.

This gate runs **before a surface is treated as demo-ready, pilot-ready, or member-facing**. A surface that fails this gate may continue to exist as a steward-only or experimental surface, but must not be linked from organizer- or member-facing materials, demoed to organizers as the participation path, or rolled into pilot-track docs.

## 2. Scope

The gate applies to any new or changed surface that an organizer or member encounters as part of governance, coordination, or participation. Concretely:

- organizer rehearsal shells and any guided organizer workflow (browser, mobile, or PWA)
- member-facing standing surfaces (`GET /v1/gov/me/standing` consumers and their UI)
- action-card surfaces (any list, detail, or interaction view derived from `/v1/gov/me/action-cards`)
- preview / review surfaces for pending publish, governance proposals, or assignment flows
- evidence packet rendering, export, and review surfaces
- receipt / provenance review surfaces (governance decision, action-item completion, meeting attendance, provenance summaries)
- member-facing governance flows (vote, attend, complete, sign, challenge)
- any partner-package surface that mirrors any of the above

The gate does **not** apply to:

- steward / operator runbooks or CLI ladders (those have their own conventions; the gate notes if a CLI assumption has leaked into a member surface)
- internal CI tooling
- private overlays
- documentation that is not member-facing copy

## 3. Review categories

Each category gives one short paragraph for what the reviewer is looking for. The reviewer marks each category **Pass**, **Pass with documented follow-ups**, **Blocked**, or **N/A with reason** (see §4 for definitions and §5 for the copy-paste checklist).

### 3.1 Language access

Member-facing copy is in plain language. Stable canonical terms (e.g. *standing*, *action card*, *obligation*, *settlement*, *receipt*, *provenance*) may be used, but every member-facing use must be paired with a plain-language explanation on first encounter, and translation / localization readiness must be at least **identified** for each piece of member-facing copy even if not implemented yet. Internal jargon (`ConstraintSet`, `PolicyOracle`, `AllowAllOracle`, etc.) does not appear in member-facing copy.

### 3.2 Screen-reader and non-visual access

The surface uses semantic HTML with meaningful headings and landmarks. Every visual card, badge, chart, diagram, status marker, and provenance trail has a non-visual equivalent — text summary, structured data, ARIA labels, or a list view — that a screen reader can announce. Dense records have a structured summary above them so a non-visual user gets the gist before drilling in.

### 3.3 Low-vision access

Text scales to at least 200% zoom without breaking layout. Body text contrast meets WCAG AA (4.5:1); large text and UI components meet 3:1. No critical label is below 14px or relies on subtle styling alone. Both the dark and light theme pass contrast independently.

### 3.4 Color-independent meaning

Run the surface through a grayscale preview. Any state, status, or distinction that disappears in grayscale is a violation. Color may reinforce meaning; it must not be the only carrier. Status indicators pair color with text, icon, position, or pattern.

### 3.5 Keyboard, switch, and non-pointer access

Every interactive element is reachable by keyboard. Visible focus state on every focusable element (the focus state is unambiguous, not a thin gray underline). Tab order matches reading order. No keyboard trap; Escape always exits a modal or popover. No hover-only controls. Where touch is expected, hit targets meet WCAG AA size (~44×44 CSS pixels minimum). Custom widgets are tested against switch-control software, not just spec-compliance.

### 3.6 Captions, transcripts, and non-audio access

Any video, audio, or demo narration ships with captions or a transcript path. Audio cues are never the only signal of state change. Voice prompts and audio decision recordings are paired with text equivalents.

### 3.7 Cognitive load and step complexity

The surface shows what matters first and uses progressive disclosure for depth. Consequences are explained before the action that triggers them. Hidden prerequisites are surfaced. Long irreversible flows are avoided; where they exist, the review / challenge / reversibility window is legible at the moment of decision. Action cards show authority basis, status, required decision, consequence, and (where applicable) reversibility window in plain language before the confirm step.

### 3.8 Low-bandwidth and low-device access

Mobile-first means more than screen size — it means older devices, slower CPUs, less memory, and intermittent connectivity. Critical participation does not require heavy animations, autoplaying media, large unscaled images, or high-end hardware. Where possible, review / receipt / provenance artifacts are exportable and offline-readable so a member with no connection can still inspect what was decided.

### 3.9 Assistive-technology compatibility

The surface uses real HTML elements over generic `<div>` with role attributes. Standard input controls are preferred over custom widgets. Where a custom widget is unavoidable, it is tested with at least one screen reader (VoiceOver, NVDA, or Orca) and at least one non-mouse input (keyboard, switch, or voice). The reviewer notes which assistive technologies were exercised.

### 3.10 Privacy-preserving accommodation path

Access needs must not be committed to public git. Disability, medical, language, accommodation, and personal-support details do not appear in any public artifact, issue body, PR comment, evidence packet, or partner-facing record. Where holder-label assignment, accommodation notes, or partner support details exist, they live in private overlays or partner-private records. The accommodation path itself is privacy-preserving: a member can request an accommodation without surrendering more private information than the institution actually needs.

### 3.11 Receipts, provenance, and evidence access

Receipts and provenance are understandable in summary form before a member is asked to drill into raw structured data. Evidence packet status visibly distinguishes fictional, fixture-only, dry-run, local non-production, and live boundaries — a member viewing a packet must be able to tell which one it is without parsing JSON. Proof surfaces are explained in plain language; technical verification literacy is not required to understand basic standing and consequence.

### 3.12 Governance and action access

Action cards expose authority basis, status, reversibility / challenge window, required decision, and consequence in plain language. Where applicable, "not now," "review later," "challenge," and "ask for help" paths are legible from the action card itself, not buried under a settings menu. The mandate name and the receipt that will be created by an action are stated in plain language **before** the confirm step.

## 4. Gate outcomes

For each of the twelve categories above, the reviewer marks one of:

- **Pass** — the category is satisfied as shipped.
- **Pass with documented follow-ups** — the category is acceptable for this PR, with one or more named follow-up issues that capture the remaining work. Each follow-up must reference a concrete issue or be opened alongside this PR.
- **Blocked** — the category is not satisfied at a level that would let this surface be called organizer-ready, member-facing, or pilot-ready. The PR may still merge as a steward-only / experimental surface, but the surface must not be linked from organizer- or member-facing materials, demoed as the participation path, or rolled into pilot-track docs until the block is resolved.
- **N/A with reason** — the category does not apply to this PR's surface. The reviewer states the reason in one sentence (e.g. "no audio content," "no visual layout — backend-only PR").

A surface is **organizer-ready / member-facing / pilot-ready** only when every category is **Pass** or **Pass with documented follow-ups** or **N/A with reason**. A single **Blocked** category gates the surface out of those tracks until resolved.

## 5. PR-review checklist (copy into the PR body)

Reviewers and authors should copy the following block into the PR body when the PR introduces or changes a surface in §2 scope. Mark each line. Replace `<reason>` and `<issue>` with concrete text.

```
## Organizer / member accessibility gate (docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md)

- [ ] 3.1 Language access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.2 Screen-reader and non-visual access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.3 Low-vision access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.4 Color-independent meaning — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.5 Keyboard, switch, and non-pointer access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.6 Captions, transcripts, and non-audio access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.7 Cognitive load and step complexity — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.8 Low-bandwidth and low-device access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.9 Assistive-technology compatibility — AT exercised: <screen reader and input method tested> — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.10 Privacy-preserving accommodation path — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.11 Receipts, provenance, and evidence access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>
- [ ] 3.12 Governance and action access — Pass | Pass with documented follow-ups #<issue> | Blocked | N/A with reason: <reason>

**Surface readiness conclusion:** organizer-ready | member-facing | pilot-ready | steward-only-experimental
```

## 6. Privacy-preserving accommodation path (process boundary)

Beyond §3.10's reviewer check, the project commits to an institutional norm: **accommodation requests must not become a back-channel that builds a registry of disability, language, or device profiles.** The architecture should assume accommodations exist and provide for them by default — readable text, keyboard support, captions, plain language, mobile-first — so that disclosure is rarely required.

Where holder-label assignment, accommodation notes, or partner support details are unavoidable, they live in **private overlays** or **partner-private records** documented in partner-package policy (e.g. NYCN holder-label / DID activation policy). They do not appear in any public ICN artifact, public NYCN artifact, evidence packet committed to a public repository, or any field that a future audit would surface.

If a PR introduces or changes a surface that touches accommodation data, the reviewer must mark §3.10 **Blocked** unless the surface explicitly routes accommodation data through a private-overlay or partner-private path that is documented in the same PR or in a referenced policy doc.

## 7. Non-claims

This gate explicitly does **not**:

- certify full legal ADA, Section 508, EN 301 549, or WCAG conformance for any surface; legal accessibility audit is a separate process and a separate set of obligations
- make a surface production-ready; production readiness depends on substrate maturity, security review, deployment posture, and partner agreement, none of which are in scope here
- authorize a formal partner pilot; pilot formalization remains the partnership-track decision documented elsewhere
- permit any private access-needs data in public git; §3.10 and §6 are explicit about this boundary
- replace the per-surface floor in [`./ACCESSIBILITY_BASELINE.md`](ACCESSIBILITY_BASELINE.md) or the component-level rules in [`../design-language/accessibility.md`](../design-language/accessibility.md); this gate is the PR-time review companion, not a substitute
- impose a CI lint or mechanical scan; this is a human review reflex. Mechanical accessibility lints (axe-core, Lighthouse, contrast checkers) are useful complements documented in [`../design-language/accessibility.md`](../design-language/accessibility.md) §7, not blocking gates
- declare any current surface organizer-ready or member-facing on its own; the gate runs against PRs as they land

## 8. Where this fits

| Layer | Doc | Role |
|-------|-----|------|
| Upstream architecture-review reflex | [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) | "Should this surface even exist this way? What participation barriers does it assume?" |
| Binding decision | [`../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md`](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md) | "What's the institutional floor?" |
| Per-surface floor | [`./ACCESSIBILITY_BASELINE.md`](ACCESSIBILITY_BASELINE.md) | "What hard requirements does each surface category meet?" |
| Component-level rules | [`../design-language/accessibility.md`](../design-language/accessibility.md) | "What WCAG-grounded patterns govern color, contrast, motion, focus, semantics?" |
| **PR-time review gate** (this doc) | `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` | **"What checklist does the reviewer run when an organizer / member surface PR lands?"** |
| Pilot-track narrative | [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) | "What is the no-CLI organizer / member story this gate protects?" |

## 9. See also

- [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) — upstream reflex; participation-access half is the principle this gate operationalizes
- [`./ACCESSIBILITY_BASELINE.md`](ACCESSIBILITY_BASELINE.md) — surface-floor doc; this gate runs against the floor at PR time
- [`../design-language/accessibility.md`](../design-language/accessibility.md) — component-level WCAG rules; tools and review checklist in §6 / §7 of that doc are useful complements to this gate
- [`../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md`](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md) — binding decision
- [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) §5 — release-blocking accessibility framing for the no-CLI organizer / member rehearsal workflow
- [`../contracts/rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md) — repo-safe evidence export contract; §3.11 of this gate sits over its rendering surfaces
- [icn#1731](https://github.com/InterCooperative-Network/icn/issues/1731) — issue this doc closes
- [icn#1740](https://github.com/InterCooperative-Network/icn/issues/1740) — open: website multilingual / inclusive-access planning (separate; this gate covers organizer / member shells, not the public website)
- [icn#1728](https://github.com/InterCooperative-Network/icn/issues/1728) — open: preview/review read-model contract (separate; this gate will run against any UI built on top)
- [icn#1726](https://github.com/InterCooperative-Network/icn/issues/1726) / [icn#1727](https://github.com/InterCooperative-Network/icn/issues/1727) — open: organizer rehearsal shell + fixture demo mode (separate; this gate runs against those PRs when they land)
