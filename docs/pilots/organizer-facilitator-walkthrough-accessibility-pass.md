---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-28
---

# Organizer facilitator walkthrough accessibility evidence (partial)

## Status

**Partial browser-assisted evidence only. The human accessibility / assistive-technology pass remains incomplete.**

This review found useful positive evidence and several blocking gaps, but it did not use a human tester, screen reader, switch-control device, voice-control tool, or graphical browser session. It therefore does not satisfy the human review required by the [organizer / member accessibility gate](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) and does not close [#2041](https://github.com/InterCooperative-Network/icn/issues/2041).

Gate conclusion: **steward-only-experimental**. The walkthrough remains fixture-backed, read-only, demo-mode, non-mutating, and L2 evidence only.

## Scope

Reviewed the four-step facilitator path merged in [#2239](https://github.com/InterCooperative-Network/icn/pull/2239):

1. Standing
2. Action Cards
3. Review Preview
4. receipt/evidence explanation

The review covered the public-repository fixture surface only. It did not test live data, private overlays, assignment, review persistence, receipt creation, or evidence export.

## Surface tested

- URL: `web/pilot-ui/?mode=demo`, served locally from the merged `main` tree
- Merge commit: `a1e1bcc0d7ac078cddc99d9943992be96f1781e7`
- Fixture bundle: `web/pilot-ui/fixtures/icn-organizer-demo/`
- Gate: `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`
- Existing automated coverage: `web/pilot-ui/tests/e2e/facilitator-walkthrough.spec.js` and adjacent organizer/demo suites

## Date

2026-06-28 (UTC)

## Environment

| Item | Value |
|---|---|
| Host | `icn-dev`, Linux command-line session |
| Browser | Google Chrome, headless |
| Browser viewport | 1280×720; 640×720 reflow approximation; 375×812 narrow viewport |
| Browser driver | Playwright from the locked `web/pilot-ui` dependency tree |
| Display session | Unavailable (`DISPLAY` unset; session type `tty`) |
| Screen reader | Unavailable; Orca is not installed and platform-specific NVDA, VoiceOver, and Narrator are not available on this host |
| Switch / voice input | Unavailable |

## Tools used

- Playwright-dispatched keyboard events for direct focus-order inspection
- Chrome accessibility-tree snapshots (`ariaSnapshot`) for semantic structure inspection
- Chrome forced-colors emulation
- reduced-motion media emulation
- full-page screenshots at default, narrow, forced-colors, 200%-equivalent reflow, and supplemental 200% root-text scaling
- merged axe/Playwright and CI evidence from #2239

These tools provide browser-assisted and automated evidence. They are not substitutes for a human screen-reader, low-vision, switch-control, or cognitive review.

## Input modes tested

| Input mode | Evidence level | Result |
|---|---|---|
| Keyboard events | Browser-assisted, Playwright-dispatched; not human hand-driven | All four step controls were reachable and activated without pointer input, but continuation order is awkward; see F2. |
| Pointer / touch | Geometry inspection only | Walkthrough buttons were 285×53 CSS px and disabled review choices were at least 78×50 CSS px at the reference desktop viewport. Some global navigation controls were only 32 CSS px high; see F5. |
| Screen reader | Not tested | Blocked by unavailable screen reader and display/audio session. |
| Switch control | Not tested | No switch-control software or device was available. |
| Voice control | Not tested | No voice-control tool was available. |

## What passed

### Browser-assisted observations

- The walkthrough was reachable from a fresh demo-mode page without pointer input. The first step was the tenth Tab stop after the skip link, header controls, and primary navigation.
- Each step moved focus to a meaningful destination heading (`My Standing`, `Action Cards`, or `Review preview`) or to the native receipt/evidence `<summary>`.
- Keyboard focus was visibly rendered on the walkthrough buttons and destination headings in headless Chrome.
- No modal, popover, or keyboard trap appeared on the four-step path.
- All four walkthrough buttons had meaningful accessible names and exceeded 44×44 CSS px at the reference desktop viewport.
- Disabled Review Preview choices remained native disabled buttons, had meaningful labels, and were represented as disabled in the accessibility tree.
- The walkthrough region, ordered list, headings, status text, Review Preview row list, definition lists, and evidence explanation appeared as structured semantic content in Chrome's accessibility tree.
- `FIXTURE ONLY`, `READ-ONLY`, `NON-MUTATING`, and “Expected does not mean issued” boundaries were present in both visible text and the accessibility tree.
- Forced-colors emulation preserved boundary text, the current step's visible label, and `aria-current="step"`; statuses did not depend on color alone.
- A 640 CSS px viewport, used as a headless 200%-equivalent reflow approximation from a 1280 CSS px reference, had no page-level horizontal overflow. The primary navigation remained an internal horizontal scroll region.
- The committed 375 CSS px automated check passed, and direct narrow screenshots showed the path as a single-column layout.
- Reduced-motion preference was active and no essential motion was observed. The path contains no audio, video, or animation-dependent instruction.
- The organizer/facilitator, member-facing fixture, steward/operator, and future boundaries remained visibly and semantically distinct.
- No raw JSON was required to follow the organizer story.

### Existing automated evidence

- #2239 CI passed Web UI, Accessibility Tests, CodeQL, Readiness Overclaim Check, and all required checks.
- The merged facilitator suite covers demo-only visibility, fixture-only reads, focus destinations, non-mutation language, axe WCAG A/AA rules, and narrow viewport overflow.

Automated evidence does not establish a human accessibility or assistive-technology pass.

## Findings / blockers

### F1 — Actual screen-reader and switch-control review is absent

**Gate effect:** 3.2 and 3.9 Blocked.

Chrome's accessibility tree is coherent, but no screen reader announced the surface and no switch-control software exercised the controls. Announcement timing for the live regions, disabled-choice clarity in a real screen reader, virtual-cursor navigation, verbosity, and switch scanning remain unknown.

**Follow-up:** #2041. Run at least one named screen reader/browser pair and one non-mouse AT input, recording the exact platform and observations.

### F2 — Forward Tab does not continue the four-step story

**Gate effect:** 3.5 Blocked; contributes to 3.7.

Activating a step moves focus forward in the DOM to its destination heading. From `My Standing`, forward Tab reaches the `Action Items tab` link and then cycles through the page chrome; it does not reach `View Action Cards` next. The operator had to reverse with Shift+Tab to return to the walkthrough controls (ten reverse stops in the recorded path after additional forward exploration; three from the Action Cards heading to Review Preview; two from the Review Preview heading to the evidence step).

The controls are individually keyboard reachable and there is no trap, but the tab sequence does not naturally follow the stated story.

**Follow-up:** #1726 and #2041. Design and human-test a clear “continue to next step / return to walkthrough” path without disrupting destination-heading announcement.

### F3 — Actual 200% browser-zoom and human low-vision review remain incomplete

**Gate effect:** 3.3 Blocked.

This headless session could not operate Chrome's browser chrome to set and visually review real 200% browser zoom. A 640 CSS px reflow approximation had no page-level overflow. A supplemental 200% root-text scaling stress test did expose horizontal overflow in Review Preview definition-list/layout content (1348 CSS px content in a 1280 CSS px viewport); combining 200% root text with a 375 CSS px viewport also overflowed (409 CSS px content).

The supplemental stress is not equivalent to browser zoom, so it is evidence of a resilience gap, not a claim that the gate's 200% zoom check definitively failed.

**Follow-up:** #2041. Repeat in a graphical browser at actual 200% zoom in light and dark themes, with a human low-vision review.

### F4 — Raw tokens and historical fixture deadlines add cognitive burden

**Gate effect:** 3.1 and 3.7 Blocked.

The accessibility tree exposes substrate/developer strings such as `/v1/gov/me/standing`, `assigned_action_item`, `program_review`, `session_scheduling`, and `veto_for_cause`. Action Cards also announce 2024 deadlines without an adjacent explanation that those past dates are frozen fictional fixture values. These details can distract from the plain-language organizer story and are not localization-ready labels.

**Follow-up:** #1726 and #2041. Provide plain-language labels first, retain technical values only under progressive disclosure where useful, and label frozen fictional dates unambiguously.

### F5 — Touch-target evidence is limited to walkthrough/review controls

**Gate effect:** 3.5 Pass with documented follow-up #2041.

The four walkthrough controls and disabled review choices met the target-size floor in the reference viewport. Global navigation and header controls on the path included 32 CSS px-high controls, so the whole page cannot claim the same result.

**Follow-up:** #2041. Measure and human-test the complete page chrome on touch and coarse-pointer devices.

### F6 — Demo-mode page load still requests an external QR library

**Gate effect:** 3.8 Blocked.

The fixture reads were same-origin static JSON, but loading `?mode=demo` also requested `qrcodejs` from `cdnjs.cloudflare.com`. This is not live participant data or sync, and the resource is SRI-pinned, but it means the page is not fully external-network-independent by default.

**Follow-up:** #1727. Keep the full no-network/demo-mode requirement open and decide whether demo mode should avoid or locally serve that nonessential login asset.

### F7 — Action Cards do not expose the full action-access set

**Gate effect:** 3.12 Blocked.

The fixture cards expose authority basis, required scope, risk, summary, deadline where present, and whether a receipt is expected. They do not consistently expose a plain-language action status, reversibility/challenge window, “review later,” “challenge,” or “ask for help” path. This read-only fixture surface performs no action, but the gate still requires those fields before an Action Card surface can be treated as member-facing.

**Follow-up:** #1726 and #2041. Preserve the read-only boundary while defining the missing member-facing explanation and help/challenge affordances in a separate slice.

## Organizer / member accessibility gate results

| Gate category | Outcome | Evidence / reason |
|---|---|---|
| 3.1 Language access | **Blocked** | Plain framing is strong, but raw endpoint/scope/authority tokens and unlabeled historical fixture dates remain. |
| 3.2 Screen-reader and non-visual access | **Blocked** | Semantic tree inspection passed; actual screen-reader smoke was unavailable. |
| 3.3 Low-vision access | **Blocked** | 200%-equivalent reflow was clean, but actual browser zoom/human review was unavailable and supplemental text scaling overflowed. |
| 3.4 Color-independent meaning | **Pass with documented follow-up #2041** | Forced colors retained text and current-step semantics; human grayscale review remains owed. |
| 3.5 Keyboard, switch, and non-pointer access | **Blocked** | Controls are reachable with visible focus and no trap, but continuation order is awkward and switch control was not exercised. |
| 3.6 Captions, transcripts, and non-audio access | **N/A — no audio or video content** | The path is text-only. |
| 3.7 Cognitive load and step complexity | **Blocked** | The four-step framing and consequences are clear; raw tokens, historical dates, and reverse navigation add avoidable burden. |
| 3.8 Low-bandwidth and low-device access | **Blocked** | Static fixtures are lightweight, but an external CDN request remains and older/slow hardware was not exercised. |
| 3.9 Assistive-technology compatibility | **Blocked** | No screen reader, switch control, or voice control was available. |
| 3.10 Privacy-preserving accommodation path | **Pass for this fixture-only scope** | No access-needs data is collected or committed; fixtures are fictional and repo-safe. No accommodation-request flow is implemented or claimed. |
| 3.11 Receipts, provenance, and evidence access | **Pass for this read-only explanatory scope** | Expected categories, fixture status, non-mutation, and “not issued” boundaries are in plain text and semantic structure. No receipt/export is created. |
| 3.12 Governance and action access | **Blocked** | Cards omit status/reversibility/challenge/help information required for a member-facing Action Card surface. |

**Surface readiness conclusion:** steward-only-experimental. This conclusion preserves the gate's exact outcome vocabulary; it is not a production, organizer, member, or pilot readiness claim.

## What remains untested

- actual screen-reader announcement with NVDA, Orca, VoiceOver, or Narrator
- virtual-cursor/browse-mode navigation and live-region timing
- switch-control scanning and activation
- voice-control naming and activation
- actual graphical-browser 200% zoom in light and dark themes
- human low-vision and cognitive-comprehension review
- human grayscale review
- touch exploration and coarse-pointer use on a physical device
- older/slower hardware and intermittent/offline behavior
- localization and translated-copy expansion

## Issue effects

- Refs #2041 for the still-open human accessibility / assistive-technology pass.
- Refs #1726 for organizer-shell continuation, plain-language, and Action Card gaps.
- Refs #1727 for the still-open full no-network demo-mode posture.
- Refs #1746 because these blockers remain relevant to organizer rehearsal operability.
- Does not close #2041, #1726, #1727, or #1746.

## Nonclaims

- No human accessibility-pass completion claim.
- No screen-reader, switch-control, voice-control, or physical-device pass claim.
- No legal WCAG, ADA, Section 508, or EN 301 549 conformance claim.
- No production readiness claim.
- No organizer readiness claim.
- No member-facing readiness claim.
- No formal NYCN pilot readiness claim.
- No live federation or Phase 2 completion claim.
- No entity-aware auth enforcement, #2082 completion, or #2113 completion claim.
- No Forge/Forgejo canonical or Matrix-as-governance claim.
- No private/provider topology or live Google Drive/Groups/Sheets sync claim.
- No persisted review decision, mutation, assignment persistence, DID binding, receipt creation, or evidence export claim.

## Next steps

1. Run the human screen-reader and non-mouse AT smoke under #2041 with named tools, platform, browser, and tester observations.
2. Test the path at actual 200% browser zoom, dark/light themes, grayscale, and physical narrow/touch conditions.
3. Under #1726, design the smallest sequential keyboard continuation and plain-language labeling slice.
4. Keep #1727 open for full external-network-independent demo-mode behavior.
