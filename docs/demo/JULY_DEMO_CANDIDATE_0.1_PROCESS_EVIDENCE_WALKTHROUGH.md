# July Demo Candidate 0.1 — organizer-steward process-evidence surface walkthrough

**Truth label.** DEV/DEMO, fixture-mode, single-actor, fictional data. This document
records an accessibility and rendered-browser review of the **organizer-steward
evidence surface** (icn#2289) — a fixture-only, read-only view on the **member-shell v0
reference client** (`web/member-shell/`), reached at `?mode=demo&set=process-evidence`.
It is **proof of path, not deployment readiness**. It is not production, not a pilot,
not organizer-ready, not member-ready, not live federation, not NYCN activation, not
Phase 2 completion, not real member/partner data, and makes no signed/partner claim.

This surface reads the **four already-landed ADR-0026 Layer 2 process-transition
receipt classes** (`icn/crates/icn-governance/src/proof.rs`) — no new receipt class is
introduced, no Rust runtime is changed, and no OpenAPI/SDK is touched. It implements the
design contract landed in **#2290**
([`docs/design/organizer-steward-evidence-surface-runtime-dogfood.md`](../design/organizer-steward-evidence-surface-runtime-dogfood.md)).

## 1. What was run, and what was not

| Path | Status |
|---|---|
| Rendered-browser walkthrough of the process-evidence view (`?mode=demo&set=process-evidence`) | **RUN** (headless Chromium, this pass) |
| Regression check of the pre-existing demos (`?mode=demo` and `?mode=demo&set=community`) | **RUN** (this pass — both still pass; see §4) |
| Automated WCAG scan (axe-core; tags `wcag2a, wcag2aa, wcag21a, wcag21aa, wcag22aa`) | **RUN** (this pass) |
| Keyboard-navigation + visible-focus trace | **RUN** (this pass) |
| 200% zoom (CSS), narrow/mobile reflow, reduced-motion render | **RUN** (this pass) |
| i18n pseudo-locale coverage of the new evidence copy (`&lang=qps-ploc`) | **RUN** (this pass — new copy is fully keyed; see §4) |
| Repo-safe evidence-summary export schema validation (`urn:icn:contract:rehearsal-evidence-export:v1`) | **RUN** (this pass — validates; see §4) |
| **Live** gateway / real receipts | **NOT run** — this slice is fixture-only by design; the `ProcessGateResultReceipt` is fixture-simulated (a wire-shaped record), not a live gate run. |
| **Screen-reader** (VoiceOver/NVDA/Orca) smoke | **NOT run** — no AT in this headless environment. Remains **owed** (tracked in **#2041**). |
| **Human** low-vision / switch-control / low-end-device verification | **NOT run** — automated proxies only; human pass remains **owed** (**#2041**). |

> Participation access is architecture, not polish. This pass clears the automated and
> rendered-structural floor for the new evidence view **and** confirms the two existing
> demos still pass; the human AT pass (#2041) is explicitly recorded as still owed.

## 2. Method (reproducible, zero new dependencies)

- Served the repo's `web/` directory statically (`python3 -m http.server 8099`), opened
  `…/member-shell/?mode=demo&set=process-evidence`.
- Drove headless Chromium via Playwright + `@axe-core/playwright`, the **declared
  devDependencies of `web/pilot-ui`** (`@axe-core/playwright` per #1735) — no new
  dependency stack is introduced.
- **Committed audit script:** [`web/member-shell/a11y-walkthrough.cjs`](../../web/member-shell/a11y-walkthrough.cjs).
  For #2289 it gained one additive, backward-compatible hook: an optional `MSHELL_SET`
  env var appends `&set=<value>` to the audited URL. **Unset (the default) audits
  `?mode=demo` byte-for-byte as before**, so the prior accessibility evidence is
  undisturbed. Reproduce:
  ```
  ( cd web/pilot-ui && npm ci && npx playwright install chromium )
  ( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )     # serve the web/ root on :8099
  # the new process-evidence view:
  MSHELL_SET=process-evidence NODE_PATH=web/pilot-ui/node_modules \
    node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out
  # regression: the two pre-existing demos still pass (unset = default demo; community):
  NODE_PATH=web/pilot-ui/node_modules node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out-demo
  MSHELL_SET=community NODE_PATH=web/pilot-ui/node_modules \
    node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out-community
  ```
- **Browser-revision provenance (honest):** Playwright resolves its own pinned Chromium
  for the *installed* Playwright version, so the exact binary is **environment-dependent,
  not a single pinned revision**. This run used the lockfile-pinned **Playwright 1.60.0 →
  chromium-1223**. These are static-surface WCAG checks (semantic HTML, ARIA, contrast,
  focus); the 0-violation result is expected to be stable across recent Chromium
  revisions, but the recorded numbers below are specifically for 1.60.0/chromium-1223.
- **Schema validation:** `python3 docs/scripts/validate-rehearsal-evidence.py
  web/member-shell/fixtures/process-evidence-export.json` (existing validator, no new
  tooling; Python 3.11+ + `jsonschema`).
- Fixtures consumed: `web/pilot-ui/fixtures/icn-organizer-demo/{standing,action-cards}.json`
  (reused, unchanged) and the two new member-shell-local fixtures
  `web/member-shell/fixtures/process-evidence-{receipts,export}.json` (fictional
  `did:icn:example-*-not-live`, illustrative `record_hash`/`body_hash`, nothing signed).

## 3. Rendered walkthrough — observed

The process-evidence view renders the `receipt → surface → evidence/export` tail of the
vertical spine **as read views**: the four process-transition receipts in sequence, a
fixture-safe privacy/redaction boundary, and a repo-safe evidence-summary export. No
mutation is performed; no gateway is contacted.

| Observation | Result |
|---|---|
| Honesty banner text (permanently visible) | `Fixture-backed demo — no live node, nothing signed.` |
| Standing pane visible; memberships / roles rendered | yes (reused demo standing) |
| Action cards pane visible | yes (reused demo cards) |
| Receipts pane: four process-transition receipts rendered in order | yes — **4 receipts** (`ProcessSessionOpenedReceipt` → `DeliberationEntryRecordedReceipt` → `DecisionRecordedReceipt` → `ProcessGateResultReceipt`) |
| Plain-language summary first; record fields under "Show evidence detail" | yes (per receipt) |
| Privacy/redaction boundary legible | yes — the deliberation entry shows "What the steward body sees" (fictional summary) **and** "What members and the export see" (redaction reason + `record_hash`/`body_hash` proof pointers, no input text) |
| Proof pointers labeled honestly | yes — `record_hash` = proof pointer; `body_hash` = "proof of content; the input itself is never stored"; recorder DIDs labeled "who recorded this fact, not who decided" |
| Evidence-summary export panel visible; read-only | yes — renders `urn:icn:contract:rehearsal-evidence-export:v1`; **no download/generate/copy control present** |
| Landmarks present | `header, nav, main, footer` |
| Skip link present | yes (`a.skip-link → #main`) |
| Credential in URL (demo mode) | **none** |
| Mobile (360px) horizontal scroll | none (clean reflow) |

Screenshots (desktop, 200% zoom, mobile-360, reduced-motion) are archived in the artifact
class (see §8). No credential, token, key, private IP/hostname, real name, or email
appears in any screenshot or fixture (demo mode pastes nothing; all fixture data is
fictional).

## 4. Automated check results

Process-evidence view (`?mode=demo&set=process-evidence`), and the two regression runs:

| Check | Process-evidence | Default `?mode=demo` (regression) | `&set=community` (regression) |
|---|---|---|---|
| **axe-core** (`wcag2a, wcag2aa, wcag21a, wcag21aa, wcag22aa`) | **0 violations**, 27 passes | 0 violations, 27 passes | 0 violations, 27 passes |
| Keyboard: focusables reached by Tab | 16 | 16 | 15 |
| Keyboard: **every** focused element had a visible outline | **true** | true | true |
| Receipts rendered | **4** | 1 | 1 |
| 200% CSS zoom render | no layout break | no layout break | no layout break |
| Mobile (360px) horizontal scroll | none | none | none |
| `prefers-reduced-motion: reduce` render | standing pane renders | renders | renders |
| Harness verdict | `REPORT_WRITTEN` (exit 0) | `REPORT_WRITTEN` (exit 0) | `REPORT_WRITTEN` (exit 0) |

Additional checks this pass:

| Check | Result |
|---|---|
| i18n pseudo-locale coverage of new evidence copy (`&lang=qps-ploc`) | **Pass** — new headings/labels render bracketed (e.g. `⟦Évídéncé súmmáry⟧`); no un-extracted English in the evidence view |
| Evidence-summary export validates against `urn:icn:contract:rehearsal-evidence-export:v1` | **Pass** (`OK: … validates`, exit 0) |
| Receipt objects contain no `body`/`content`/`text` field (privacy discipline) | **Pass** — `body_hash` only |
| No download/generate/copy/mutation control in the export section | **Pass** — 0 such controls |

Automated scans are a floor, not a substitute for human AT testing. 0 axe violations
means no machine-detectable WCAG failures on the rendered fixture view; it does not
certify screen-reader comprehension or human low-vision usability.

## 5. Organizer / member accessibility gate — 12-category outcome

Per `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` §5. Outcomes: **Pass** /
**Pass w/ follow-ups** / **Blocked** / **N/A**. Applied to the process-evidence view.

- [x] **3.1 Language access** — Pass. All new evidence copy is externalized through the
  i18n seam (#2043) and verified under the pseudo-locale (`&lang=qps-ploc` brackets every
  new string); canonical terms (session, deliberation entry, decision, gate result,
  receipt, provenance, `record_hash`, `body_hash`) are paired with inline plain-language
  explanations. **Still owed (separate lane, not blocking this English DEV/DEMO pass):**
  real translations + human language-quality QA (the seam ships `en` + a pseudo-locale +
  an RTL fallback demo only; broader website multilingual is #1740).
- [x] **3.2 Screen-reader / non-visual access** — Pass with documented follow-ups (**#2041**).
  Semantic HTML, landmarks, headings, list views, `<dl>` key/value; the redaction boundary
  is conveyed with headings + text, not visuals. **Real screen-reader testing not
  performed** — owed (**#2041**).
- [x] **3.3 Low-vision access** — Pass with documented follow-ups (**#2041**). 200% CSS zoom
  rendered without layout break (screenshot); rem units throughout; contrast ratios
  documented per token in `shell.css` (reused, unchanged). **Human low-vision + external
  contrast-audit-tool verification not performed** — owed (**#2041**).
- [x] **3.4 Color-independent meaning** — Pass. Receipt status carries glyph + text; the
  redaction boundary is carried by headings and text (not color); axe found no
  color-contrast violations.
- [x] **3.5 Keyboard / switch / non-pointer access** — Pass with documented follow-ups (**#2041**).
  16 focusables reached by Tab, all with a visible focus outline; every evidence-detail
  and export-detail disclosure is a native `<details>`/`<summary>` (keyboard-reachable);
  no hover-only controls; targets ≥44px per `shell.css`. **Switch-control software not
  exercised** — owed (**#2041**).
- [ ] **3.6 Captions, transcripts, non-audio access** — N/A with reason: this view ships no
  audio/video/narration media.
- [x] **3.7 Cognitive load and step complexity** — Pass. Each receipt shows a plain-language
  summary first; record-level fields sit behind progressive disclosure. This surface is
  **read-only** — there is no confirm step and no mutation, so no irreversible action is
  offered.
- [x] **3.8 Low-bandwidth / low-device access** — Pass. Dependency-free static HTML/CSS/vanilla
  JS; the two new fixtures are small JSON files fetched over the same static path; no
  framework, build, animation, or autoplay added.
- [ ] **3.9 Assistive-technology compatibility** — Pass with documented follow-ups (**#2041**).
  Real HTML elements over `div[role]`; native `<details>`. **AT exercised: none** (headless
  environment). Owed (**#2041**).
- [x] **3.10 Privacy-preserving accommodation path** — Pass (with reason). No
  disability/medical/accommodation data is collected, rendered, or committed. The
  privacy/redaction boundary demonstrated here operates on **fictional deliberation
  content only**, and is honest by construction: the receipt stores a `body_hash` (a
  content fingerprint), never the input text, so the member/export view shows the proof
  pointer and redaction reason and nothing else.
- [x] **3.11 Receipts, provenance, and evidence access** — Pass. **(Load-bearing for this
  surface.)** Each receipt shows a plain-language summary before any raw structured data;
  the fixture maturity tier ("Fixture-backed demo — nothing signed") is visible on screen,
  so a viewer can tell fixture-only apart **without parsing JSON**; proof surfaces are
  explained in plain language (`record_hash` = proof pointer; `body_hash` = proof of
  content, body never stored; recorder DIDs = who recorded, not who decided); the
  evidence-summary export is a read-only view of a repo-safe committed artifact.
- [x] **3.12 Governance and action access** — Pass. The receipts state authority basis
  honestly in plain language (recorder-not-decider; the decision receipt "asserts nothing
  about its standing"; the gate receipt states `accessibility_review` = `pass`, labeled
  fixture-simulated). This slice is **read-only with no confirm step**, so the gate's
  "mandate + receipt named before confirm" requirement is N/A here — there is no action to
  confirm; the surface only reads receipts that already exist.

**Surface readiness conclusion:** the process-evidence view **clears the automated +
rendered-structural floor** and the two pre-existing demos still pass; it is **NOT
organizer-ready / member-ready / pilot-ready** until the owed human AT pass (3.2 / 3.9,
and human 3.3 / 3.5) is completed. No category is hard-**Blocked**; the human-AT items
(**#2041**) remain owed-follow-ups that gate the "organizer-ready" label.

## 6. Findings

```
Finding: Real screen-reader / AT testing not performed.
Evidence: §1, gate 3.2 / 3.9; headless environment with no VoiceOver/NVDA/Orca.
Impact: Cannot honestly call this surface "organizer-ready" or "pilot-ready" without it.
Recommendation: One human (or AT-equipped) pass with ≥1 screen reader + ≥1 non-mouse
  input; tracked in #2041 — attach results there.
Blocking? no (for DEV/DEMO review) / yes (for organizer-ready / pilot-ready labeling)
```
```
Finding: Automated WCAG scan is clean (0 axe violations, 27 passes) on the process-
  evidence render, and the two pre-existing demos still pass unchanged.
Evidence: §4.
Impact: Positive — machine-detectable WCAG floor is met and no regression was introduced.
Recommendation: Keep axe in the loop; re-run (all three views) if member-shell markup changes.
Blocking? no — verified okay.
```
```
Finding: The ProcessGateResultReceipt is fixture-simulated, not a live gate run.
Evidence: §1, §3; the receipt is a wire-shaped fixture record (gate_kind=accessibility_review,
  result=pass) so the surface can carry a receipt-backed statement that the gate ran without
  standing up a gateway.
Impact: A reviewer could mistake the fixture gate receipt for a live-proven one.
Recommendation: Keep the "Fixture-backed demo — nothing signed" banner prominent (it is) and
  the illustrative-hash label on record_hash (it is); this doc states the fixture-simulated
  provenance explicitly.
Blocking? no — verified okay (banner + hash label already present).
```
```
Finding: The privacy/redaction boundary is demonstrated on fictional content only.
Evidence: §3, gate 3.10/3.11; receipts hold a body_hash only, no stored text exists to leak.
Impact: Positive — honest by construction; the "redacted" view shows exactly what the receipt
  actually holds (a hash + metadata), and the steward summary is clearly-fictional fixture context.
Recommendation: Keep the two-audience framing (steward-visible vs member/export-redacted).
Blocking? no — verified okay.
```

Classification: 1 important-follow-up (screen-reader/AT #2041), 3 verified-okay. **No
blockers** for the DEV/DEMO review; the human-AT pass (#2041) gates the organizer-/pilot-
ready label.

## 7. Interface review — does the UI keep the distinctions honest?

| Distinction | Made clear? |
|---|---|
| DEV/DEMO posture | yes — permanent honesty banner + footer reference-client disclaimer |
| Fixture vs live-local mode | yes — per-mode banner ("Fixture-backed demo — no live node, nothing signed.") |
| Illustrative vs canonical hash | yes — `record_hash` labeled "illustrative fixture value — not a real blake3 binding" in demo mode |
| Steward-visible vs member/export-redacted | yes — the deliberation entry shows both, side by side, with the redaction reason + proof pointers |
| Recorded fact vs decision authority | yes — recorder-not-decider labeling; the decision receipt "asserts nothing about its standing" |
| Fixture-simulated vs live gate | yes — the gate receipt is labeled fixture-simulated in copy and in this doc |
| Read-only view vs generated/downloaded export | yes — the export panel is a read-only render of a committed fixture; no download/generate/copy control exists |
| Proof of path vs production readiness | yes — banner + footer + this doc's non-claims |

## 8. Artifacts

The audit **script is committed**: [`web/member-shell/a11y-walkthrough.cjs`](../../web/member-shell/a11y-walkthrough.cjs)
— so the checks are runnable from committed materials (see §2). The committed **fixtures**
(`web/member-shell/fixtures/process-evidence-{receipts,export}.json`) and the committed
**export** (which validates against the rehearsal-evidence-export contract) are the
repo-safe evidence. The **screenshots** and the **axe/keyboard JSON reports** for the three
runs live in the operator's local artifact store under the artifact class
`july-demo-process-evidence-<date>` — repo-safe (no credentials/keys/IPs) but kept out of
the repo per the evidence-log convention (they are binary / machine output). This doc is
the committed, shareable summary; the committed script + fixtures let a reviewer regenerate
them.

## 9. Owed before "organizer-ready / pilot-ready"

> The reproducible human-tester packet for the owed items below is in
> [`JULY_DEMO_CANDIDATE_0.1_HUMAN_A11Y_VALIDATION.md`](JULY_DEMO_CANDIDATE_0.1_HUMAN_A11Y_VALIDATION.md),
> whose **§4G** now carries the process-evidence-surface-specific human/AT protocol (the
> four-receipt story, evidence-detail disclosures, proof-pointer language, steward/member
> redaction boundary, and read-only export panel). It is a template, not a completed pass — this
> surface does **not** complete it, and #2041 stays open until a human/AT run fills it in.

- Screen-reader smoke (≥1 of VoiceOver/NVDA/Orca) + ≥1 non-mouse input (3.2 / 3.9). [#2041]
- Human 200% browser-zoom + small-device pass (3.3 / 3.5 / 3.8). [#2041]
- External contrast-audit-tool confirmation of the documented ratios (3.3). [#2041]
- (Separate lanes, not this pass) real translations + human language QA (#1740); the
  broader human-operability spine (#1748 / #2141) remains open.

---

_Refs #2289. Refs #2290. Refs #1748. Refs #2141. Refs #2041._
