---
Status: normative
Authority: spec (defines the design-level i18n seam contract for the ICN member-shell v0 reference client; consumes the closed member-facing vocabulary and design principle 9 of `docs/spec/member-shell-v0.md` and §3.1 of `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`; infrastructure-only — ships no translations)
Canonical: no
Last Reviewed: 2026-06-14
---

# Member Shell i18n seam v0

> **Status: spec, work-in-progress.** Defines the dependency-free
> internationalization (i18n) **seam** for the member-shell v0 reference
> client (`web/member-shell/`). This is the *infrastructure* for language —
> string externalization, locale resolution, fallback, `dir`/RTL — not a set
> of translations. It satisfies design principle 9 ("Multilingual /
> inclusive-access path") of [`member-shell-v0.md`](./member-shell-v0.md) and
> §3.1 (Language access) of the accessibility gate at the reference-client
> layer. Issue: icn#2042. It does **not** redefine the rendering contract, the
> closed status vocabulary, any endpoint, or any wire format.

<!-- truth-class: normative -->

## Purpose

The member-shell v0 reference client began English-only with strings inline in
`shell.js` and `index.html`. This seam makes the client **modular with
language**: adding a language is a catalog entry plus a locale-metadata row,
never a code change. The seam is the contract a translator (or an
institution-package localizer) targets; the closed member-facing vocabulary
from `member-shell-v0.md` stays canonical and byte-identical in the English
catalog.

## Non-goals

- **No real translations ship.** The English catalog is the source of truth;
  the only other shipped locales are the `qps-ploc` pseudo-locale (a coverage
  test) and `ar` (an empty catalog demonstrating RTL + fallback). The
  infrastructure is the deliverable.
- **No server-side locale negotiation.** Locale is resolved entirely client-
  side (`?lang=`, `navigator.language`, default). The gateway is not asked for
  a language; no `Accept-Language` contract is defined here.
- **No per-locale number/date libraries** beyond the locale-aware
  `Date.prototype.toLocaleString()` already used for timestamps. No ICU
  message-format, no pluralization library; plural unit selection is handled
  with explicit catalog keys (`time.day` / `time.days`, …).
- **No translation of data.** Only UI chrome is externalized. Fixture/gateway
  data (display names, card titles, domain names) and raw technical
  identifiers (DIDs, ids, enum values, record-class names, hashes) are never
  translated.
- **No change to the rendering contract, endpoints, modes, security model, or
  closed vocabulary** of `member-shell-v0.md`. Strings only move location;
  behavior is preserved verbatim.

## Catalog shape

A single IIFE (`web/member-shell/i18n.js`, loaded before `shell.js`) exposes
`window.ICNI18n`.

`MESSAGES` maps a locale key to a flat object of `key → string`:

```js
MESSAGES = {
  en: {
    "sync.synced": "Synced",
    "lifecycle.sent": "Sent — waiting for receipt",
    "card.timePressure.none": "No time pressure.",
    "card.rel.closesIn": "closes in about {n} {unit}",
    "live.nodeAnswered": "The node answered {status}: {reason}"
    // … one complete English catalog …
  }
  // additional languages added here as one entry each — no code change
};
```

- **`en` is complete and authoritative.** It contains every member-facing
  string from `shell.js` and `index.html`.
- Other catalogs may be **partial**. Missing keys fall back per the order
  below.
- Parametrized strings use `{name}` placeholders, interpolated from a `params`
  object at call time.

## Key naming

Keys are short, semantic, dotted, and grouped by surface or concern:

| Prefix | Covers |
|---|---|
| `sync.*` | the closed sync-state vocabulary |
| `lifecycle.*` | the closed action-lifecycle vocabulary |
| `action.*` / `source.*` / `scope.*` / `risk.*` | ADR-0027 enum plain-language maps |
| `card.*` | action-card rendering chrome |
| `complete.*` | the one mutation (mark-complete) flow |
| `receipt.*` / `receipts.*` | receipt rendering chrome |
| `standing.*` / `membership.*` / `identity.*` | standing + identity surfaces |
| `connect.*` / `live.*` / `launcher.*` / `demo.*` | mode + connection chrome |
| `time.*` / `hash.*` | formatting helpers |
| `nav.*` / `header.*` / `footer.*` / `help.*` / `banner.*` / `skip.*` | document chrome |

Keys are stable identifiers; renaming a key is a breaking change for any
downstream catalog.

## `t()` fallback order

`ICNI18n.t(key, params)` resolves in this order and **never throws and never
returns blank**:

1. **Active locale** catalog has the key → use it.
2. Else **`en`** catalog has the key → use it (this is the "translation
   pending" path — the member always sees real text).
3. Else **return the key itself** (a visible, debuggable last resort).

Then `{param}` placeholders are interpolated from `params`; a missing param is
left as its literal `{name}` so the gap is visible rather than silently blank.

For the pseudo-locale, step 1/2 resolve against `en` and the result is then
transformed (see below).

## Locale resolution precedence

`ICNI18n.resolveLocale()`:

1. `?lang=<locale>` query parameter, if it exactly matches an available locale
   key.
2. Else `navigator.language`, matched against available locales by exact tag
   then by language prefix (e.g. `ar-EG` → `ar`).
3. Else `en`.

## `lang` / `dir`

`ICNI18n.applyDocumentLocale(locale)` sets, at runtime:

- `document.documentElement.lang` — a **valid BCP-47 tag**. Most locale keys
  are already valid; a locale may carry an `htmlLang` override in its metadata
  for keys that are not valid tags (the pseudo-locale key `qps-ploc` emits
  `en-x-ploc`, a valid private-use tag, so `html-lang-valid` keeps passing).
- `document.documentElement.dir` — `ltr` or `rtl` from the locale metadata.

The static markup keeps `<html lang="en">` as the default; the runtime
overrides it once the active locale is resolved.

## RTL

Right-to-left is expressed purely via `dir` on the document plus CSS **logical
properties** (`margin-inline`, `padding-inline`, `inset-inline`,
`border-inline-start`, `text-align:start`/`end`). No physical `left`/`right` is
used for member-facing layout, so `dir="rtl"` mirrors correctly with no
per-locale CSS. The shipped `ar` locale exercises this.

## Pseudo-locale (coverage)

`qps-ploc` is a coverage instrument, not a language. Its catalog is empty; at
render time `t()` resolves the English string and transforms it — wrapping it
in `⟦…⟧` and accenting vowels — while preserving `{param}` placeholders so
interpolation still works. Because every externalized string is visibly
flipped, **any plain-English UI chrome remaining on screen under
`?lang=qps-ploc` is a missed extraction.** Data (fixture/gateway values) and
raw technical identifiers are intentionally left un-transformed; they are not
chrome.

## Translation-pending rule

A locale present in the metadata map but missing some or all keys (the shipped
`ar` is the extreme case — entirely empty) renders the English string for each
missing key via the `t()` fallback. This is the "translation pending"
disposition from `member-shell-v0.md`'s failure-and-safety table
("Translation missing for current language → fallback to the institution's
default language; never silent"). At the reference-client layer the fallback
text is the honest signal; a future surface may add an explicit per-string
"translation pending" tag without changing this seam.

## Relationship to sibling work

| Concern | Where it lives |
|---|---|
| Member shell rendering contract + closed vocabulary | [`docs/spec/member-shell-v0.md`](./member-shell-v0.md) (principle 9; status vocabulary) |
| Accessibility gate, §3.1 Language access | `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` |
| Multilingual / inclusive-access plan | icn#1740 |
| Language / glossary endpoint | icn#1610 |
| This i18n seam (reference client) | icn#2042 |

## Non-claims

- This seam does not ship a production member shell, a platform decision, or
  any non-English translation.
- This seam does not define a server-side language API or an `Accept-Language`
  contract.
- This seam does not redefine the closed member-facing vocabulary, the
  rendering contract, any endpoint, or any wire/receipt format.
- This seam does not translate fixture/gateway data or raw technical
  identifiers; only UI chrome is externalized.
