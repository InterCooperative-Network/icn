# July Demo Candidate 0.1 — accessibility + rendered-browser walkthrough evidence

**Truth label.** DEV/DEMO, fixture-mode, single-actor, fictional data. This document
records an accessibility and rendered-browser review of the **member-shell v0 reference
client** (`web/member-shell/`) — the human-facing surface of July Demo Candidate 0.1.
It is **proof of path, not deployment readiness**. It is not production, not a pilot, not
federation, not NYCN activation, not real member data, and makes no signed/partner claim.

This is an **evidence/documentation pass**. No runtime behavior was changed.

## 1. What was run, and what was not

| Path | Status |
|---|---|
| Rendered-browser walkthrough of member-shell **fixture mode** (`?mode=demo`) | **RUN** (headless Chromium, this pass) |
| Automated WCAG scan (axe-core; tags `wcag2a, wcag2aa, wcag21a, wcag21aa, wcag22aa` — WCAG 2.0/2.1 A & AA plus 2.2 AA) | **RUN** (this pass) |
| Keyboard-navigation + visible-focus trace | **RUN** (this pass) |
| 200% zoom (CSS), narrow/mobile reflow, reduced-motion render | **RUN** (this pass) |
| **Live** member-shell against a running gateway (discharge mutation) | **NOT run here** — no live appliance/gateway in this environment. The live `seed → discharge → verify → reset/reseed` + 13/13 receipt-chain loop was run and **sealed separately on 2026-06-13** (operator evidence: `~/artifacts/icn/demo-image-20260612/EVIDENCE-MAP.md`). This pass does not re-run or re-claim it. |
| **Screen-reader** (VoiceOver/NVDA/Orca) smoke | **NOT run** — no AT in this headless environment. Remains **owed** (matches the client's own declared gap). |
| **Human** low-vision / switch-control / low-end-device verification | **NOT run** — automated proxies only; human pass remains **owed**. |

> Participation access is architecture, not polish. A demo surface with blocked keyboard,
> zoom, or screen-reader access is not organizer-ready. This pass clears the automated and
> rendered-structural floor; the human AT pass is explicitly recorded as still owed.

## 2. Method (reproducible, zero new dependencies)

- Served the repo's `web/` directory statically (`python3 -m http.server`), opened
  `…/member-shell/?mode=demo`.
- Drove headless Chromium via Playwright + `@axe-core/playwright`, which are **declared
  devDependencies of `web/pilot-ui`** (`@axe-core/playwright` per #1735) — so no new dependency
  stack is introduced for this pass.
- **Committed audit script:** [`web/member-shell/a11y-walkthrough.cjs`](../../web/member-shell/a11y-walkthrough.cjs)
  (repo-safe; a dev/audit tool, not loaded by the shell). Reproduce (the serve port and the
  script's base-URL argument must match — the script defaults to `:8099`):
  ```
  ( cd web/pilot-ui && npm ci && npx playwright install chromium )
  ( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )     # serve the web/ root on :8099
  NODE_PATH=web/pilot-ui/node_modules node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out
  ```
- **Browser-revision provenance (honest):** Playwright resolves its own pinned Chromium for the
  *installed* Playwright version, so the exact binary is **environment-dependent, not a single
  pinned revision**. This run used the install already present in `web/pilot-ui/node_modules`
  (Playwright **1.58.1** → **chromium-1208**). The lockfile pins Playwright **1.60.0**, so a
  reviewer who runs `npm ci` first installs 1.60.0 and a newer Chromium and validates on *that*
  browser — not on 1208. These are static-surface WCAG checks (semantic HTML, ARIA, contrast,
  focus); the 0-violation result is expected to be stable across recent Chromium revisions, but
  the recorded numbers below are specifically for 1.58.1/chromium-1208. A fresh-checkout reviewer
  should confirm on their installed version.
- Fixtures consumed: `web/pilot-ui/fixtures/icn-organizer-demo/{standing,action-cards}.json`
  (CI-drift-guarded, #2021) and `web/member-shell/fixtures/demo-completion-receipt.json`
  (fictional `did:icn:example-*`, illustrative `record_hash`, nothing signed).

## 3. Rendered walkthrough — observed

The fixture-mode page renders the participation spine **as read views**: standing →
action cards → a completion receipt → plain-language evidence. (The fixture path does **not**
perform a live discharge mutation; the discharge→receipt step is exercised by the sealed live
run, not here. In fixture mode the receipt shown is the pre-seeded receipt fixture.)

| Observation | Result |
|---|---|
| Honesty banner text (permanently visible) | `Fixture-backed demo — no live node, nothing signed.` |
| Standing pane visible; memberships / roles rendered | yes — 2 memberships, 2 roles |
| Action cards pane visible; cards rendered | yes — 3 cards |
| Receipts pane visible; receipt rendered | yes — 1 receipt |
| Plain-language evidence + "Show evidence detail" disclosure present | yes (2 evidence disclosures) |
| Landmarks present | `header, nav, main, footer` |
| Skip link present | yes (`a.skip-link → #main`) |
| Credential in URL (demo mode) | **none** (`url_has_no_credential = true`) |
| Mobile (360px) horizontal scroll | none (clean reflow) |

Screenshots (desktop, 200% zoom, mobile-360, reduced-motion) are archived in the artifact
class (see §8). No credential, token, JWT, key, private IP/hostname, or passphrase appears
in any screenshot (demo mode pastes nothing; the only credential field is live-mode and was
never exercised).

## 4. Automated check results

| Check | Result |
|---|---|
| **axe-core** (tags `wcag2a, wcag2aa, wcag21a, wcag21aa, wcag22aa`) | **0 violations**, 25 passes |
| Keyboard: focusable elements reached by Tab | 15 |
| Keyboard: **every** focused element had a visible outline (`outline-style≠none`, `width≠0`) | **true** |
| 200% CSS zoom render | no layout break observed in screenshot |
| `prefers-reduced-motion: reduce` render | standing pane renders identically (client uses no motion) |

Automated scans are a floor, not a substitute for human AT testing. 0 axe violations means no
machine-detectable WCAG failures on the rendered fixture view; it does not certify
screen-reader comprehension or human low-vision usability.

## 5. Organizer / member accessibility gate — 12-category outcome

Per `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` §5. Outcomes: **Pass** /
**Pass w/ follow-ups** / **Blocked** / **N/A**.

- [x] **3.1 Language access** — Pass. Plain-language English copy; canonical terms (standing,
  action card, receipt, provenance, DID) each paired with an inline explanation; no kernel jargon
  in member copy. The **language-modularity infrastructure (i18n seam) is now implemented** —
  landed in **#2043** (closes #2042): a locale-keyed message catalog the UI reads via `t()` with a
  per-key English fallback, `?lang=` / `navigator.language` resolution, `<html lang>` + `dir` with
  RTL, a translation-pending fallback, and a pseudo-locale coverage test. Adding a language is a
  catalog entry, not a code change; axe is 0 at `en` / pseudo / `ar`; raw identifiers stay literal;
  the `ar` RTL demo announces its English fallback as English (`lang=en`) honestly. **Still owed
  (separate lanes, not blocking this DEV/DEMO English evidence pass):** real translations and a
  human language-quality pass for each shipped locale — the seam ships only `en` + a pseudo-locale
  + an RTL fallback demo, no real translations yet; the broader website multilingual path is #1740.
  Language access is access — the seam is the infrastructure floor; translations are the next lane.
- [x] **3.2 Screen-reader / non-visual access** — Pass with documented follow-ups (**#2041**).
  Semantic HTML, landmarks, headings, `role="status"`/`aria-live` regions, list views,
  `aria-describedby`. **Real screen-reader testing not performed** — remains owed (tracked in
  **#2041**; the client also declares this gap) before calling this surface organizer-ready.
- [x] **3.3 Low-vision access** — Pass with documented follow-ups (**#2041**). 200% CSS zoom
  rendered without layout break (screenshot); rem units throughout; contrast ratios documented
  per token in `shell.css`. **Human low-vision + external contrast-audit-tool verification not
  performed** — owed (tracked in **#2041**).
- [x] **3.4 Color-independent meaning** — Pass. Status carries glyph + text label (risk level
  is "glyph + label, never color alone" per spec/README); axe found no color-contrast violations.
- [x] **3.5 Keyboard / switch / non-pointer access** — Pass with documented follow-ups (**#2041**).
  15 focusables reached by Tab, all with a visible focus outline; skip link present; no
  hover-only controls in markup; buttons/inputs sized ≥44px (2.75rem) per `shell.css`. **Switch-
  control software not exercised** — owed (tracked in **#2041**).
- [ ] **3.6 Captions, transcripts, non-audio access** — N/A with reason: the reference client
  ships no audio/video/narration media.
- [x] **3.7 Cognitive load and step complexity** — Pass. Single column, fixed section order,
  progressive disclosure (`<details>` for DID, technical detail, evidence). The one mutation
  (mark-complete, live mode only) is gated behind a pre-confirm summary stating what changes,
  authority basis, scope, receipt expected, and permanence, with distinct Confirm/Cancel
  (per README/spec; not exercised in fixture mode).
- [x] **3.8 Low-bandwidth / low-device access** — Pass. Dependency-free static HTML/CSS/vanilla
  JS; no framework, build, animation, or autoplay. Follow-up (not blocking): no offline
  tolerance / cache / draft queue (declared gap).
- [ ] **3.9 Assistive-technology compatibility** — Pass with documented follow-ups (**#2041**).
  Real HTML elements over `div[role]`; standard inputs. **AT exercised: none** (headless
  environment, no screen reader / switch). This single line keeps the surface from being called
  "organizer-ready" until an AT pass is done — owed (tracked in **#2041**).
- [ ] **3.10 Privacy-preserving accommodation path** — N/A with reason: no accommodation profile
  exists in this reference client; no disability/medical/accommodation data is collected, rendered,
  or committed. (A real deployment must add this; tracked as a declared gap, not a regression.)
- [x] **3.11 Receipts, provenance, evidence access** — Pass. Receipt shows a plain-language
  summary first (who acted, what obligation, when); record class, ids, actor DID, and raw
  `record_hash` sit behind "Show evidence detail". The fixture maturity tier ("Fixture-backed
  demo — nothing signed") is visible on screen — a member can tell it is fixture-only without
  parsing JSON.
- [x] **3.12 Governance and action access** — Pass. Action cards expose authority basis,
  status, scope, an authorization check against the member's own scopes, consequence, and a
  what-happens-if-I-act line in plain language; the mandate/receipt-expected is stated before
  the confirm step (live mode). "Ask for help" path is present (steward contact, unwired in the
  reference client by design).

**Surface readiness conclusion:** **member-facing reference-client floor cleared for automated +
rendered-structural review; the language-modularity infrastructure (3.1) is now implemented
(#2043, merged); NOT yet organizer-ready / pilot-ready** until (a) the owed human AT pass
(3.2 / 3.9, and human 3.3 / 3.5) is completed, and (b) **real translations + a human language-quality
pass** ship for the target locales (the i18n seam exists but ships only `en` + a pseudo-locale +
an RTL fallback demo). No category is hard-**Blocked**; the human-AT items (#2041) and the
translations/human-language lane remain owed-follow-ups that gate the "organizer-ready" label.

## 6. Findings

```
Finding: Real screen-reader / AT testing not performed.
Evidence: §1, gate 3.2 / 3.9; environment is headless with no VoiceOver/NVDA/Orca; the client's
  own README declares this gap.
Impact: Cannot honestly call the member surface "organizer-ready" or "pilot-ready" without it.
Recommendation: One human (or AT-equipped) pass with at least one screen reader + one non-mouse
  input; tracked in #2041 — attach results there.
Blocking? no (for DEV/DEMO review) / yes (for organizer-ready / pilot-ready labeling)
```
```
Finding: Human low-vision / 200% browser-zoom / low-end-device verification not performed.
Evidence: §4 used CSS zoom + narrow viewport as automated proxies; no human confirmation.
Impact: Automated zoom proxy can miss real reflow/comprehension problems.
Recommendation: Human 200% browser-zoom + small-device pass; fold into #2041.
Blocking? no (DEV/DEMO) / important follow-up (organizer-ready)
```
```
Finding: Automated WCAG scan is clean (0 axe violations, 25 passes) on the fixture render.
Evidence: §4.
Impact: Positive — machine-detectable WCAG floor is met on the rendered fixture view.
Recommendation: Keep axe in the loop; re-run if member-shell markup changes.
Blocking? no — verified okay.
```
```
Finding: Fixture mode renders read views only; it does not perform a live discharge mutation.
Evidence: §3; fixture mode shows a pre-seeded receipt fixture.
Impact: A reviewer could mistake the fixture receipt for a live-proven one.
Recommendation: Keep the "nothing signed / fixture-backed" banner prominent (it is); cite the
  sealed live run for the discharge→receipt→13/13 proof rather than implying the fixture proves it.
Blocking? no — verified okay (banner already present).
```
```
Finding: Language-modularity infrastructure — RESOLVED by #2043 (merged). Language access is
  accessibility, and the member shell now has an i18n seam.
Evidence: §5 gate 3.1; #2043 added a locale-keyed message catalog read via `t()` with per-key
  English fallback, `?lang=`/`navigator.language` resolution, `<html lang>`/`dir` + RTL, a
  translation-pending fallback, and a pseudo-locale coverage test; axe 0 at en/pseudo/ar.
Impact: The surface can now be made modular with language — adding a language is a catalog entry,
  not a code change. **Still owed (separate lane):** real translations + a human language-quality
  pass per shipped locale (the seam ships only `en` + a pseudo-locale + an RTL fallback demo);
  the broader website multilingual path is #1740.
Recommendation: Open/queue a translations lane (real per-locale catalogs + human language QA) when
  a target locale is chosen; the seam (#2043) is the infrastructure floor.
Blocking? no (DEV/DEMO English demo; seam implemented) / important follow-up: real translations +
  human-language QA before organizer-/pilot-ready for non-English cooperatives
```

Classification: 1 important-follow-up (screen-reader/AT #2041), 1 follow-up (human zoom/device
#2041), 1 resolved (language-modularity infrastructure — i18n seam landed in #2043; real
translations + human-language QA remain a separate lane), 2 verified-okay. **No blockers** for the
DEV/DEMO English review; the human-AT pass (#2041) and the translations/human-language lane gate
the organizer-/pilot-ready label.

## 7. Interface review — does the UI keep the distinctions honest?

| Distinction | Made clear? |
|---|---|
| DEV/DEMO posture | yes — permanent honesty banner + footer reference-client disclaimer |
| Fixture vs live-local mode | yes — Mode nav + per-mode banner text ("Fixture-backed demo…" vs "Live-local node — dev rehearsal…") |
| Launcher vs manual fallback | yes — launcher section ("Start local demo", no paste) and "Connect manually instead" both present |
| Reset vs reseed | n/a in member-shell (operator-side; documented in HANDS_ON §8 / DEMO_QUICKSTART / OPERATOR_CHECKLIST) — UI does not conflate them |
| Receipt vs proof/audit | yes — plain summary first; raw `record_hash`/class/ids behind "Show evidence detail"; fixture caption says nothing is signed |
| Proof of path vs production readiness | yes — banner + footer ("not the production member shell… claims no live-federation or production posture") |

## 8. Artifacts

The audit **script is committed**: [`web/member-shell/a11y-walkthrough.cjs`](../../web/member-shell/a11y-walkthrough.cjs)
— so the checks are runnable from committed materials (see §2). The **screenshots** and the
**axe/keyboard JSON report** live in the operator's local artifact store under the artifact class
`july-demo-accessibility-<date>` (sibling to the sealed-image evidence) — repo-safe (no
credentials/keys/IPs) but kept out of the repo per the evidence-log convention (they are binary /
machine output). This doc is the committed, shareable summary; the committed script lets a reviewer
regenerate them.

## 9. Owed before "organizer-ready / pilot-ready"

> A reproducible human-tester packet for the owed items below — exact setup/launch commands,
> per-category fill-in checklists, expected observations, and evidence/pass-fail templates — is in
> [`JULY_DEMO_CANDIDATE_0.1_HUMAN_A11Y_VALIDATION.md`](JULY_DEMO_CANDIDATE_0.1_HUMAN_A11Y_VALIDATION.md). It is a template, not a completed pass.

- Screen-reader smoke (≥1 of VoiceOver/NVDA/Orca) + ≥1 non-mouse input (3.2 / 3.9). [#2041]
- Human 200% browser-zoom + small-device pass (3.3 / 3.5 / 3.8). [#2041]
- External contrast-audit-tool confirmation of the documented ratios (3.3).
- **Language-modularity infrastructure (3.1) — DONE** (i18n seam landed in **#2043**, closes #2042):
  locale-keyed catalog, `t()` + per-key English fallback, `?lang=`/`navigator.language`, `lang`/`dir`
  + RTL, translation-pending fallback, pseudo-locale coverage. **Still owed:** real translations +
  a human language-quality pass per shipped locale (the seam ships only `en` + a pseudo-locale + an
  RTL fallback demo); the broader website multilingual path is #1740.
- (Declared, separate lanes, not this pass) glossary endpoint #1610, multi-language #1740,
  offline tolerance, privacy-preserving accommodation path, deadline-justice metadata.
