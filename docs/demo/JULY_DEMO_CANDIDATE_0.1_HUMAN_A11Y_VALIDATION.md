# July Demo Candidate 0.1 — human accessibility validation packet

**Status: NOT a completed human pass.** This is a reproducible packet/template for a human
tester to run the assistive-technology validation owed by [#2041]. No human / screen-reader /
switch-device testing has been performed by filling out this doc — the fields below are empty
on purpose. Filling them in (with repo-safe evidence) is the work [#2041] tracks.

Target surface: the member-shell v0 reference client (`web/member-shell/`), the human surface
of July Demo Candidate 0.1. DEV/DEMO member surface only — **not** a production or pilot
accessibility certification.

Commit this packet was prepared against: `origin/main` `2b148e72`.

## 1. What is already proven (automated) vs what this packet owes (human)

The **automated / rendered-structural floor is cleared** and recorded in
[`JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md`](JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md)
(PR #2039). Reproduced for this packet at `2b148e72` (fixture mode, headless Chromium via the
committed `web/member-shell/a11y-walkthrough.cjs`):

| Automated check | Result at `2b148e72` |
|---|---|
| axe-core (`wcag2a, wcag2aa, wcag21a, wcag21aa, wcag22aa`) | **0 violations**, 27 passes |
| Keyboard: focusable elements reached by Tab, all with a visible focus outline | 16 reached, all outlined |
| Landmarks / skip link / single `h1` | `header, nav, main, footer`; skip link present; one `h1` |
| Credential in `?mode=demo` URL | none (`url_has_no_credential = true`) |
| Honesty banner perceivable in DOM | `Fixture-backed demo — no live node, nothing signed.` |
| Mobile 360px horizontal scroll / reduced-motion render | none / standing still visible |
| i18n seam (`en`, `qps-ploc`, `ar`): locale resolve + `lang`/`dir` applied + fallback | smoke PASS (see §6E) |
| WCAG contrast **math** on `shell.css` tokens (arithmetic only) | all 7 tokens ≥ AA, 7.0–17.0:1 (see §6C) |

**Automation verifies structure, not perception.** It cannot confirm that a real screen-reader
*conveys* the flow understandably, that a switch user can *complete* the task, that focus is
*findable* by a human, or that the *rendered* surface looks right at 200% zoom / in RTL. Those
are the four owed `ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` categories this packet is for:

- **3.2** Screen-reader / non-visual access
- **3.3** Low-vision access (human 200% zoom + external contrast-tool confirmation)
- **3.5** Keyboard / switch / non-pointer access (keyboard + visible focus already automated; switch owed)
- **3.9** Assistive-technology compatibility (≥1 screen reader + ≥1 non-mouse input; record which AT)

## 2. Environment to record (fill in before testing)

| Field | Value |
|---|---|
| Date | |
| OS + version | |
| Browser + version | |
| Screen reader + version (≥1 of VoiceOver / NVDA / Orca) | |
| Non-mouse input exercised (keyboard / switch software / voice) | |
| Viewport size / zoom level(s) | |
| Commit SHA tested | |
| External contrast tool used (for 3.3) | |

## 3. Setup + launch (exact, fixture mode, no live node)

```bash
# from the repo root ( cd "$(git rev-parse --show-toplevel)" )
( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )   # serve the web/ root
# then open in your browser:
#   http://127.0.0.1:8099/member-shell/?mode=demo            (fixture mode, no JWT)
#   http://127.0.0.1:8099/member-shell/?mode=demo&lang=qps-ploc
#   http://127.0.0.1:8099/member-shell/?mode=demo&lang=ar
```

Fixture mode renders the participation spine as read views (standing → action cards →
completion receipt → plain-language evidence) with no live mutation and nothing signed. The
live `seed → discharge → verify` loop is a separate sealed run (operator evidence outside the
repo) — do not re-run or re-claim it here. To reproduce the automated floor, see the Method
section of the walkthrough doc.

## 4. Owed-category checklists (mark each item, record observation)

Use: **Pass** / **Pass with documented follow-up #\<issue\>** / **Blocked** / **N/A (reason)**.

### A. Keyboard-only navigation (3.5)
*(Automation already confirmed: 16 focusable reached, all with a visible outline, no axe focus
violation. A human still confirms order/comprehension/no-trap.)*

- [ ] Every interactive control (mode nav, language selector, Start-local-demo if shown, each
  "Show technical detail / Show evidence detail" disclosure, every link) is reachable by Tab. → ___
- [ ] Tab order matches reading order / is understandable. → ___
- [ ] Focus is visibly findable on every stop (not a thin gray underline). → ___
- [ ] No keyboard trap: the "Show technical/evidence detail" sections are native `<details>` disclosures — they toggle with Enter/Space and you can Tab past them, which is **not** a trap. Native disclosures are **not** expected to close on `Esc`; reserve any `Esc`-to-close check for a real modal/popover (none ship in this surface today). → ___
- [ ] Action-card / receipt / evidence controls are reachable and operable by keyboard. → ___

### B. Screen-reader smoke (3.2 / 3.9) — AT used: ____________
- [ ] Document title and page structure (landmarks, headings) are announced understandably. → ___
- [ ] The standing / action-cards / receipt / evidence panes are each announced as distinct, named regions. → ___
- [ ] The permanent honesty banner ("Fixture-backed demo — no live node, nothing signed.") is perceivable, not visually-only. → ___
- [ ] The language selector is announced and operable. → ___
- [ ] Fixture/demo status is not hidden from AT. → ___
- [ ] Dynamic changes (e.g. opening a disclosure, "Start local demo") are announced or at least not silently confusing. → ___
- [ ] Raw identifiers (DIDs, `record_hash`, class names) are not read as if they were prose/translatable. → ___

### C. Low-vision / 200% zoom + contrast (3.3)
- [ ] At 200% browser zoom: no essential content clipped, controls remain usable, no loss of function. → ___
- [ ] At a narrow viewport (≈320–360px): clean reflow, no horizontal scroll for primary content. → ___
- [ ] Focus outline remains visible at zoom. → ___
- [ ] **External contrast tool** confirms the rendered text meets the documented `shell.css` ratios below. *(The ratios are arithmetically ≥ AA — see §6C — but a tool must confirm the rendered surface uses these tokens at these sizes.)* → ___

| Token | Foreground on background | Documented (`shell.css`) | Recomputed (WCAG) |
|---|---|---|---|
| `--ink` | `#1c1c21` on `#ffffff` | ~16.1:1 | 17.0:1 |
| `--ink-muted` | `#4d4d57` on `#ffffff` | ~7.9:1 | 8.4:1 |
| `--accent` | `#0f4f4a` on `#ffffff` | ~8.7:1 | 9.4:1 |
| `--banner-demo-ink` | `#594400` on `#fff3cd` | ~7.3:1 | 8.4:1 |
| `--banner-live-ink` | `#07395c` on `#dceefb` | ~9.4:1 | 10.1:1 |
| `--ok-ink` | `#1d5c33` on `#e6f4ea` | ~6.9:1 | 7.0:1 |
| `--warn-ink` | `#79302a` on `#fdeceb` | ~7.2:1 | 8.1:1 |

### D. Switch / non-pointer access (3.5 / 3.9) — input used: ____________
- [ ] With switch-control software (or another non-keyboard, non-mouse input), all interactive controls are reachable and operable. → ___
- [ ] No hover-only controls; touch hit targets are ≥ ~44×44 CSS px where touch is expected. → ___

### E. Language-selector usability (i18n seam) — honesty-bounded
*(The seam is infrastructure, not translation. `en` is the only real catalog; `qps-ploc` is a
pseudo-locale coverage test; `ar` ships an intentionally empty catalog that demonstrates RTL
while falling back to English — its document `lang` stays `en` so AT is not told the English
fallback is Arabic.)*

- [ ] The language selector is discoverable and operable (mouse, keyboard, and AT). → ___
- [ ] `?lang=qps-ploc`: member-facing strings are visibly pseudo-localized (`⟦…⟧`, accented). Any plain-English string left under the pseudo-locale is a missed extraction — note it. → ___
- [ ] `?lang=ar`: layout mirrors to RTL (`dir=rtl`) without breaking the demo surface; text remains English fallback (expected — `ar` is a layout demo, not a translation); `lang` stays `en`. → ___
- [ ] Switching locales does not strand focus or hide the honesty banner. → ___

### F. Reduced motion (supportive)
- [ ] With `prefers-reduced-motion` set, the surface remains usable and content stays visible. → ___

## 5. Evidence-recording rules (repo-safe)

- Capture screenshots / a short screen-reader transcript per category. Keep binaries in your
  **local artifact store, not committed** (mirror the walkthrough doc's artifact convention);
  reference them by path/hash in this doc.
- **Never** capture or paste a JWT, token, key, passphrase, private IP/hostname, or any
  credential. Fixture mode has no credential in the URL; live mode does — do not use live mode
  for evidence here.
- Record the exact AT name+version and input method per category (3.9 requires naming the AT).

## 6. Automated evidence reproduced for this packet (at `2b148e72`)

- **6A axe / keyboard / structure:** see §1 table — reproduced via the walkthrough doc's exact
  command (after `( cd web/pilot-ui && npm ci && npx playwright install chromium )` per its Method section):
  `NODE_PATH=web/pilot-ui/node_modules node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out`.
  axe 0 violations / 27 passes; 16 focusable reached, all outlined; landmarks + skip link + single `h1`.
- **6C contrast math:** independent WCAG relative-luminance recomputation of the seven `shell.css`
  tokens yields all ratios ≥ AA (range 7.0–17.0:1). Recomputed values are **≥** the documented
  values (the `shell.css` comments are conservative; deltas ≤ ~1.1, all still ≥ 4.5:1) — a minor
  doc-accuracy nit, not a contrast failure. **This verifies the token math only; it does not
  verify the rendered surface or human perception — 3.3 still owes a human + external-tool pass.**
- **6E i18n smoke:** loading `web/member-shell/i18n.js` for `?lang=en|qps-ploc|ar` resolves the
  locale, applies `lang`/`dir` (`ar`→`dir=rtl`, `lang=en` fallback override; `qps-ploc`→`lang=en-x-ploc`),
  and returns English for `en`, pseudo-text for `qps-ploc`, English fallback for `ar`. PASS.

## 7. After the human pass is complete

1. Fill in §2 and every checklist item in §4 with a verdict + observation + evidence reference.
2. In [`JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md`](JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md)
   §5, flip gate rows 3.2 / 3.3 / 3.5 / 3.9 from "Pass with documented follow-ups #2041" to
   **Pass** (or Blocked, with the real reason), and update §9 "owed" and the surface-readiness
   conclusion to match what was actually observed.
3. Update [#2041] with the result and close it **only** if the gate categories genuinely pass.

## 8. What must NOT be claimed until this packet is actually completed by a human

- Automated axe/browser evidence exists; **human AT validation is separate and, as of this commit, still owed.**
- Language-modularity **infrastructure** exists; **real translations are not complete**; the
  pseudo-locale is **not** a translation; the `ar` RTL path is a **layout demo with English
  fallback**, not Arabic-language completion.
- This is still **local / dev / demo / single-actor**. Not production. Not pilot-ready. Not
  organizer-ready until the gate categories pass. Not live federation. Not NYCN-approved/adopted.
  Not partner-ready. Not private-data ready. Not a two-member flow. Not production signed receipts.

[#2041]: https://github.com/InterCooperative-Network/icn/issues/2041
