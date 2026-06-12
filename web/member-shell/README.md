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
| Mark task complete | `PUT /v1/gov/domains/{domain_id}/action-items/{item_id}/status` body `{"status":"completed"}` | `governance:meeting:write` or `governance:write` (plus: caller must be the item's creator or assignee, and a member of the domain — enforced server-side) |
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

Live-mode note: the browser enforces CORS. The gateway must be started with
a CORS mode that allows the shell's origin (see `configure_cors` in
`icn/crates/icn-gateway/src/security.rs`), or the shell must be served from
the same origin as the gateway. Otherwise the shell reports the failure in
plain language ("Your standing is currently unavailable…") with the
technical detail beneath.

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
- [ ] Multi-language / translation tagging (icn#1740; all strings are
      English; no "translation pending" path)
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
