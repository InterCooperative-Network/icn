# ICN Member Shell — v0 reference client

A thin, dependency-free reference client that proves the
[member-shell-v0 rendering contract](../../docs/spec/member-shell-v0.md)
is implementable against the real endpoint shapes. Static HTML + CSS +
vanilla JS. **No framework, no build step, no npm, no service worker.**

This is **not** a revival of `web/pilot-ui` and not the production member
shell. It exists so a member-facing surface — standing, action cards,
receipts — can be demonstrated honestly, with the maturity tier named on
screen at all times.

## What it renders

Per `docs/spec/member-shell-v0.md`, ADR-0020 (`/me/standing`), ADR-0027
(ActionCard), and the structs in `icn/apps/governance/src/http/models.rs`:

- **Who am I** — display label first; DID under a "Show technical
  identifier" disclosure.
- **My standing** — memberships per domain and roles with plain-language
  authority descriptions; raw scope strings and ids under "Show technical
  detail".
- **Action cards** — title, plain summary, mapped (never raw) action/source
  kinds, scope (`entity` / `structure` / `individual`) in plain words,
  authority basis, an authorization check against the member's own
  `authority_scopes` ("You are authorized for this." / "This requires
  authority you do not currently hold: …"), deadline as relative time with
  the absolute timestamp under a disclosure ("No time pressure." when
  absent), risk level as glyph + label (never color alone),
  `accessibility_hint` as a preamble before any controls, and a
  what-happens-if-I-act line driven by `receipt_expected`.
- **Receipts** — plain summary first (who acted, what obligation it
  satisfied, when), with the record class, ids, actor DID, and the raw
  `record_hash` (hex) behind a "Show evidence detail" disclosure.
- **Records status** — only the closed plain-language vocabulary from the
  spec (`Synced`, `Sync delayed`, `Some records are being verified`,
  `Action paused until records sync`, `Receipt available`,
  `Review required`, `Sync delayed / degraded`). No invented strings.

The shell-vs-cockpit boundary is observed: no divergence detail, no peer
DIDs, no digest forms ever reach this surface.

## Modes and maturity tiers

A banner naming the active tier is permanently visible.

| Mode | URL | Banner | Maturity |
|---|---|---|---|
| **Demo** | `?mode=demo` | "Fixture-backed demo — no live node, nothing signed." | Fixture rendering rehearsal only. Consumes the committed pilot-ui fixture pack (`web/pilot-ui/fixtures/icn-organizer-demo/standing.json` + `action-cards.json`) by relative fetch — those files are CI-drift-guarded and are **not** duplicated here. Deadlines are read relative to the fixture's own `generated_at` snapshot (and the UI says so), because frozen data must not pretend to be current. |
| **Live** | default (or `?mode=live`) | "Live-local node — dev rehearsal, not production." | Talks to a locally running gateway (default `http://localhost:8080`). Dev rehearsal against real endpoint shapes; no production, pilot, or live-federation claim. |

### Demo-mode receipt fixture

The pilot-ui fixture pack carries no receipt packet, so this directory adds
one member-shell-local file, `fixtures/demo-completion-receipt.json`, shaped
verbatim like the wire form of `ActionItemCompletionReceipt`
(`icn/crates/icn-governance/src/proof.rs`: `item_id`, `domain_id`,
`actor_did`, `transition`, `completed_at`, `record_hash` as a 32-byte
array). It is fictional (`did:icn:example-*-not-live` convention) and its
`record_hash` is **illustrative bytes, not a real blake3 binding** — the
demo banner and the receipt's own caption both say nothing was signed.

## Live mode: endpoints used

Treat `icn/apps/governance/src/http/handlers.rs` +
`configure.rs` as truth (the static `web/api-docs/openapi.yaml` is stale —
see `docs/dev/openapi-member-surface-gaps.md`):

| Call | Endpoint | Required scope |
|---|---|---|
| Standing | `GET /v1/gov/me/standing` | `governance:read` |
| Action cards | `GET /v1/gov/me/action-cards` | `governance:read` |
| Mark task complete | `PUT /v1/gov/domains/{domain_id}/action-items/{item_id}/status` body `{"status":"completed"}` | `governance:action-item:complete` (completion-only, least-privilege — accepted only for the `completed` transition, and only for an item **assigned to the caller**) — or the broader `governance:meeting:write` / `governance:write` (creator **or** assignee). Caller must also be a member of the domain — all enforced server-side. |
| Completion receipt | `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` | `governance:read` |

**The one mutation shipped (dogfood loop):** marking an `action_item` /
`complete` card as completed, behind a pre-confirm summary (what changes,
authority basis, scope, receipt expected, permanence warning, distinct
Confirm/Cancel), then fetching and rendering the completion receipt. The
card moves through the closed lifecycle strings: `Open` →
`Sent — waiting for receipt` → `Confirmed` (+ `Receipt available`).
Rejections render the gateway's reason; nothing fails silently. All other
surfaces are read-only.

**Credential handling:** the paste-credential field is `type="password"`,
held in a closure variable for the life of the page only — never written to
localStorage/sessionStorage/cookies/URL, never hardcoded, sent only as an
`Authorization: Bearer` header to the gateway address you typed.

## How to run

Serve the **`web/` directory** as the root (the demo fixture fetch crosses
into `web/pilot-ui/`, so serving `web/member-shell/` alone breaks demo
mode):

```bash
cd web
python3 -m http.server 8000
```

Then open:

- Demo: <http://localhost:8000/member-shell/?mode=demo>
- Live: <http://localhost:8000/member-shell/> (gateway at
  `http://localhost:8080`)

Live-mode note: the browser enforces CORS, and the shell's requests carry
an `Authorization` header, which forces a CORS preflight. Start the node
with the shell's page origin allowed:

```bash
ICN_CORS_ORIGINS="http://localhost:8000" icnd --config ... --gateway-enable ...
```

(`ICN_DEV_MODE` also switches to the dev CORS config; see `configure_cors`
in `icn/crates/icn-gateway/src/security.rs`.) Without it the preflight is
rejected and the shell reports "Technical detail: Failed to fetch" — that
symptom means CORS, not a broken node. With the origin allowed, real
gateway answers come through and render in plain language (verified
end-to-end 2026-06-12: a rejected credential renders "The node answered
401: Authentication failed…" with the degraded-sync chip).

## Internationalization (i18n) seam

This client ships the **infrastructure** for language, not a set of
translations (icn#2042). Adding a language is a catalog entry, not a code
change.

### How it works

`i18n.js` (loaded before `shell.js`) exposes `window.ICNI18n`. Every
member-facing string in `shell.js` and `index.html` is externalized into a
catalog keyed by short semantic dotted keys (`sync.synced`, `lifecycle.sent`,
`card.timePressure.none`, `live.invalidUrl`, `receipt.whatThisProves`, …).
`shell.js` renders by calling `ICNI18n.t(key, params)`; `index.html` carries
`data-i18n="key"` on every static text node and
`data-i18n-attr="placeholder:key;aria-label:key"` for attributes, which
`shell.js` applies on boot. Rendering stays `textContent` /
element-construction only — catalog strings can never inject markup.

The closed member-facing vocabulary from `docs/spec/member-shell-v0.md`
(`Synced`, `Sent — waiting for receipt`, `Confirmed`, …) lives in the `en`
catalog **byte-identical** to the spec. Glyphs (`✓ ⚠ ● ○ ◐ ▲`), record-class
names (`ActionItemCompletionReceipt`), raw enum values, DIDs, ids, and hashes
are **not** translated — they stay in code / under "details".

### Adding a language (no code change)

1. Add one entry to `MESSAGES` in `i18n.js`:
   `MESSAGES.fr = { "sync.synced": "Synchronisé", … }`. A **partial** catalog
   is fine.
2. Add one row to `LOCALES`: `fr: { name: "Français", dir: "ltr" }`.

The language appears in the selector automatically. No `shell.js` change.

### Per-key fallback + translation-pending behavior

`t(key, params)` resolves: **active locale → `en` → the key itself** (never
throws, never blanks). A key missing from a non-English catalog falls back to
the English string per the spec's "translation pending" rule — the member sees
real text, never an empty slot. The shipped `ar` locale has **no**
translations on purpose: it demonstrates `dir="rtl"` mirroring plus the
per-key English fallback for an as-yet-untranslated language.

### Switching language

- **URL:** append `?lang=<locale>` (e.g. `?lang=ar`).
- **Selector:** a labelled `<select>` in the header (inside `<nav
  aria-label="Language">`); changing it sets `?lang=` and reloads.
- **Auto:** with no `?lang=`, the active locale is resolved from
  `navigator.language` by prefix match, else `en`.

`applyDocumentLocale()` sets `<html lang>` and `<html dir>` at runtime;
`<html lang="en">` stays the static default in the markup.

### RTL

Layout uses CSS logical properties (`margin-inline`, `padding-inline`,
`inset-inline`, `border-inline-start`, `text-align:start`) so `dir="rtl"`
mirrors the page correctly. Try `?lang=ar`.

### Pseudo-locale coverage test (`?lang=qps-ploc`)

`?lang=qps-ploc` renders every externalized string through a transform that
wraps it in `⟦…⟧` and accents vowels (`⟦Sýncéd⟧`). It derives from the `en`
catalog, so coverage is exact: **any plain-English text left on screen under
`?lang=qps-ploc` is a missed extraction.** Fixture/server data (display names,
card titles, domain names) and raw technical identifiers (ids, enum strings,
DIDs, hashes, class names) are intentionally *not* wrapped — they are data, not
UI chrome. The document `lang` becomes `en-x-ploc` (a valid BCP-47 private-use
tag) so the pseudo-locale keeps `html-lang-valid` passing.

Verified end-to-end with axe-core (`wcag2a/2aa/21aa/22aa`) at three locales:
`en` (0 violations, ltr), `qps-ploc` (0 violations, every chrome string
flipped), `ar` (0 violations, `dir=rtl`, English fallback).

### Non-goals

No real translations ship in this v0 client (the infrastructure is the
deliverable). No server-side locale negotiation. No per-locale number/date
libraries beyond the locale-aware `toLocaleString()` already used for dates.
See `docs/spec/member-shell-i18n-v0.md` for the seam's contract.

## ADR-0028 accessibility checklist (honest)

Implemented in this v0 client:

- [x] Semantic HTML (header/nav/main/sections/headings/lists/dl); ARIA used
      only to extend (`role="status"`, `aria-live`, `aria-current`,
      `aria-describedby`)
- [x] WCAG 2.2 AA contrast — palette chosen against AA and documented in
      `shell.css` (computed ratios noted per color; not yet verified by an
      external audit tool)
- [x] `:focus-visible` ring on every interactive element (links, buttons,
      inputs, disclosure summaries)
- [x] `prefers-reduced-motion` respected (no motion used; guard rule kills
      any future animation/transition)
- [x] No information by color alone — every status carries glyph + text
      label; grayscale-safe
- [x] Scalable type — rem units throughout, `html { font-size: 100% }`
      respects the user's setting
- [x] Plain language first, jargon explained inline (DID defined where
      shown; raw enums/ids/hashes only under "details" disclosures)
- [x] Stable layout — single column, fixed section order, no layout shift
      on status change
- [x] Visible what's-next affordances (mode links, connect button, per-card
      action or explanation of why none, "Check again", help section)
- [x] ≥ 44px (2.75rem) hit targets on buttons and inputs
- [x] Consequences explained pre-action; permanence stated; distinct
      Confirm/Cancel (cognitive-load floor)

NOT yet implemented (declared gaps, per ADR-0028's "partial floor
compliance with the gap declared"):

- [ ] Glossary endpoint integration (icn#1610 — endpoint does not exist yet)
- [x] Multi-language / translation tagging (icn#2042; icn#1740) —
      **infrastructure present** (externalized string catalog + locale switch
      via `?lang=` and a labelled selector + RTL via `dir` + per-key English
      fallback / "translation pending" path + `?lang=qps-ploc` pseudo-locale
      coverage test). **Translations pending:** no real non-English catalog
      ships yet; the `ar` locale demonstrates RTL + fallback only. See the
      "Internationalization (i18n) seam" section above.
- [ ] Offline tolerance — no caching, no draft-intent queue, no service
      worker; this client requires the network it names
- [ ] Screen-reader and switch-control testing — not performed; semantics
      are in place but untested with real AT
- [ ] 200% zoom and low-end-device verification — not performed by a human
- [ ] Privacy-preserving accommodation path (no accommodation profile
      exists in this client)
- [ ] Deadline-justice metadata rendering (cards do not carry it yet)
- [ ] Captions/transcripts — no media in this client (floor not exercised)

## What this does NOT claim

- Not the production member shell; no platform decision (iOS/Android/PWA/
  native) is made or implied.
- Not the full member-shell-v0 information architecture — the spec's ten
  surfaces include Decisions/Governance, Records/Artifacts, Privacy/Access,
  and a scope switcher that this client does not implement.
- No offline support, no local cache, no draft-intent queue.
- No private-record (ScopedVault) surface.
- Demo mode signs nothing and proves no runtime behavior; live mode is a
  local dev rehearsal, not a pilot, not production, not a live federation.
