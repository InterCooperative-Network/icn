---
Status: design-direction
Authority: design (forward-direction; normative only where it restates ADR-0032)
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-09-14
Last Updated: 2026-09-14
Purpose: The design contract between the canonical public website and the future icn.zone operational/access surface — what the second surface inherits from the first, what it must never inherit, and what an implementation pass would have to decide.
---

# Public surface design contract

> **One sentence.** `intercooperative.network` is where you understand ICN and
> `icn.zone` is where you enter it; they must share a design language without
> sharing a job, and owning an `icn.zone` hostname must never come to mean
> owning an institution.

**This document builds nothing.** There is no `icn.zone` implementation in this
repository and no scoped task to create one. This is the contract a future
implementation pass would inherit, written now because the public design system
is being shaped and the cheapest time to make it portable is before a second
surface exists to argue with.

---

## 1 · The three surfaces

| Surface | Job | Carries truth claims? |
|---|---|---|
| `intercooperative.network` | Understand ICN, evaluate it, trust or challenge its claims | **Yes — it is the truth boundary** |
| `icn.zone` | Enter, join, find, use. Access, routing, discovery | **No** |
| `learn.icn.zone` | Teach ICN | **No — it teaches, it does not define** |

Authority for this split:

- [ADR-0032](../adr/ADR-0032-website-truth-boundary.md) — **accepted, implemented.**
  Makes `intercooperative.network` the truth boundary. This is the binding part.
- [RFC-0015](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) —
  **draft, not accepted.** Names the six-domain split. It subordinates itself
  explicitly: "Where this RFC and ADR-0032 disagree, ADR-0032 wins." Everything
  in it beyond the truth boundary is exploratory framing.
- [DOMAIN_ROUTING_AND_DNS_BINDINGS.md](../architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md) —
  design-direction. Owns the hostname/authority distinction.
- [ICN_ZONE_ROUTING.md](../deployment/ICN_ZONE_ROUTING.md) — a manual runbook,
  `Status: Plan — not yet implemented`, and **not registered in
  `docs/registry.toml`**. Treat it as the weakest of these four.

### The rule that does the most work

> An `*.icn.zone` hostname is a **utility route**, never the institution's
> authority. Authority lives in the `InstitutionalDomain` object. DNS never
> equals institutional authority.

The failure this prevents is named directly in the architecture doc: *"Treating
'ICN domain' and 'DNS domain' as the same thing rebuilds the platform-landlord
pattern: institutions become tenants of whoever owns the DNS namespace."*

**Design consequence, and this is the part a UI gets wrong:** no surface may
render an `icn.zone` hostname as an institution's identity. The institution is
identified by its institutional domain object and its own brand; the hostname is
a way packets found it. A header that reads `brightworks.icn.zone` as the
institution's name has already conceded the landlord model. Custom
institution-owned DNS must remain equally first-class in every layout —
never a downgrade path, never a paid tier, never visually secondary.

---

## 2 · What `icn.zone` inherits

The shared layer is everything that makes ICN legible, and nothing that makes it
persuasive. A member who has read the website and then follows a join link
should recognise the system, not the marketing.

### 2.1 Design tokens — inherit wholesale

`website/src/styles/global.css` is the token source: color, type scale
(`--text-*`), spacing, radii, `--measure-prose` / `--measure-wide`,
`--target-min` (the 44px interactive floor), and the dark-default /
`[data-theme="light"]` contract.

No new hardcoded hex. If an operational surface needs a color the public site
does not have — a "queued offline" state, say — it is added to the token file,
not invented locally.

### 2.2 Icon semantics — inherit the meanings, not just the glyphs

`website/src/data/icons.ts` maps each canonical concept to a glyph: `identity`,
`standing`, `authority`, `governance`, `policy`, `accounting`, `execution`,
`provenance`, `federation`, `commons`. These are **semantic bindings**, not
decoration. The `authority` glyph means authority everywhere it appears or it
means nothing anywhere.

A surface that reuses the provenance glyph for "history" generally, or the
standing glyph for "profile", has broken the vocabulary the website spent a
visitor's whole first session teaching.

### 2.3 Scope representation

Scopes are co-equal and never a ladder (`.claude/rules/design.md`). Cooperative,
community, federation, and commons are different kinds, not different tiers. Any
`icn.zone` surface that lists scopes renders them as peers.

The member is the anchor and institutions are changing contexts around them —
so a scope switcher is a **context switch, not an account switch**, and must not
be shaped like a SaaS org-picker.

### 2.4 Identity and standing indicators

Standing attaches to **a role in a scope**, never to a person globally. The
website already states this (`data/walkthrough.ts` step 01: *"Standing attaches
to a role in a scope, not to a person. The same human can hold different
standing in a different cooperative, and neither cooperative learns about the
other."*).

An access surface therefore may not render a global reputation, level, score, or
rank — that is [MUST_NOT_SHIP](MUST_NOT_SHIP.md) §6 vocabulary and a category
error besides. Standing is always shown *scoped*, and always with the basis that
established it reachable.

### 2.5 Truth-state and evidence patterns

The two-axis pattern — maturity band + evidence class, always travelling
together — is inherited **as a mechanism**, not as content. `icn.zone` makes no
maturity claims of its own; where it needs to say a capability is unfinished it
says so plainly and links to `/whats-real-now` on the canonical site.

`DemoLabel` (fixture-backed surfaces) transfers directly and matters more on an
access surface than on a reading one, because a visitor who followed a join link
has a stronger prior that what they are seeing is live.

**Provenance vocabulary stays out of prose.** The label
`repo-grounded public explainer` is UI metadata for reviewers. Preserve the
truth-state semantics; do not make a newcomer decode the QA vocabulary.

### 2.6 Accessibility — the floor is identical, the stakes are higher

WCAG 2.2 AA, keyboard, reduced motion, 200% zoom, 44×44 targets, color never the
sole carrier of meaning. Full rules:
[ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) and
[../design-language/accessibility.md](../design-language/accessibility.md).

Two obligations bind harder on `icn.zone` than on the website:

- **JS adds, never gates.** A join link that fails without JavaScript is a
  member who cannot join. The website's walkthrough already holds this line by
  refusing a stepper; an access surface has less excuse, not more.
- **Localization is not deferrable.** [MUST_NOT_SHIP](MUST_NOT_SHIP.md) §5
  rejects English-only fixed-width UI. Reading pages degrade gracefully in a
  second language; an action card that clips its mandate text does not.

### 2.7 The action-card contract

Any `icn.zone` surface that produces a receipt inherits
[ADR-0027](../adr/ADR-0027-action-card-contract.md) in full: mandate,
reversibility, and named receipt declared **before** the confirm step, plus sync
state and — for governance actions — threshold and current tally
([CONTENT_STYLE_GUIDE](CONTENT_STYLE_GUIDE.md) §"Dangerous-action copy" v0.2
fields 5 and 6).

This is the single highest-risk inheritance. The website has no confirm buttons,
so it has never had to honor this rule at runtime. `icn.zone` is the first
surface where getting it wrong asserts authority the kernel cannot prove.

---

## 3 · What `icn.zone` must NOT inherit

| Pattern | Why not |
|---|---|
| The narrative page sequence | The reading ladder (`01 What is ICN → … → 06 fork`) is a *pedagogical* structure. Someone opening a join link is past it. |
| Maturity-band content | Bands are claims. Claims live on the truth boundary. Rendering a band on `icn.zone` forks the truth surface — the exact failure ADR-0032 exists to prevent. |
| The homepage argument | Fragmentation, the named stack, "own your workplace / rent your nervous system". `icn.zone` is not where the case gets made. |
| Explanatory figures as chrome | `ClosureLoop`, `FragmentationFigure`, `PublicLoop` teach. An access surface that decorates itself with them is performing depth. |
| A root landing page | `ICN_ZONE_ROUTING.md`: *"Do not deploy a second static site at `icn.zone` root. The redirect is the entire root behavior."* |
| A link to `learn.icn.zone` from the public site | Explicitly prohibited — `DOMAIN_ROUTING_AND_DNS_BINDINGS.md` and `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` both forbid it. Holds until the learning surface exists and passes review. |
| "Dashboard" in any form | [MUST_NOT_SHIP](MUST_NOT_SHIP.md) §7. Name surfaces by what they do: standing view, action queue, activity, position. |

---

## 4 · Reserved routes

From `ICN_ZONE_ROUTING.md` §"Reserved prefixes" — **planned, zero
implementation**, and reserved so they do not fall through to the root redirect:

| Route | Intent | Status |
|---|---|---|
| `/j/<code>` | Cooperative join codes | Future (Cloudflare Worker) |
| `/i/<code>` | Member invite links | Future (Worker) |
| `/n/<name>` | Human-readable institution shortcut | Future (Worker); also modelled as a `DnsBinding` of purpose `short_route` |

Everything else 301s to `intercooperative.network`, path-preserving.

**Design obligations that attach to these routes before any of them ships:**

1. A short route is a **pointer**, and the surface it lands on must show what was
   resolved — which institution, which scope, and on whose authority. A code that
   silently becomes a session is a phishing primitive.
2. `/n/<name>` is the landlord risk concentrated into one route. Claiming
   `/n/brightworks` must never confer authority over the Brightworks
   institutional domain, and the resolved page must make the institution's own
   identity — not the `icn.zone` path — the thing on screen.
3. Join and invite codes are **capability-bearing URLs**. They inherit the
   dangerous-action copy rules: what this will do, under what mandate, whether it
   is reversible, and what record it creates — before acceptance, not after.

---

## 5 · What is not decided

An implementation pass has to answer these; none are settled here.

- **Session and identity boundary.** Does `icn.zone` hold a session at all, or
  hand off to an institution-operated surface? Custom-DNS institutions must not
  get a worse experience, which argues for handing off — but a join link has to
  land somewhere before the institution is known.
- **Who serves it.** `ICN_ZONE_ROUTING.md` assumes Cloudflare Worker + KV and
  states this needs no Rust changes. That is an operational convenience, and it
  places a member-facing authorization surface outside the ICN trust boundary.
  Unresolved, and it should be resolved before `/j/` ships rather than after.
- **The `DnsBinding` object does not exist.** No `DnsBinding`, no
  `PublicationReceipt`, nothing in `icn/`, `web/`, `website/`, or `deploy/`.
  Institutional routing has no runtime representation yet.
- **Whether the design system should physically split.** Tokens currently live
  inside `website/`. A second consumer argues for extraction; extracting too
  early creates a package with one consumer and a release process. Recommendation:
  do not extract until a second surface actually renders.

---

## 6 · Implementation brief for the next pass

Smallest honest first slice, in order. Each step is independently shippable and
none of them requires the step after it.

1. **Phase 1 root redirect only.** `icn.zone` → `intercooperative.network`,
   path-preserving, with `/j/`, `/i/`, `/n/` reserved and *not* falling through.
   No UI. This is the runbook that already exists, and it makes the namespace
   real without making it a surface.
2. **Resolve the trust-boundary question** from §5 before any route resolves a
   code. A decision record, not a deployment.
3. **Extract tokens** only once a second surface exists to consume them.
4. **`/j/<code>` first, not `/n/<name>`.** Join codes are bounded, expiring, and
   revocable. Institution name routes are permanent and carry the landlord risk.
   Build the reversible one first.
5. **The first resolved surface must state what it resolved** — institution,
   scope, authority, and what accepting would do — before any accept control.
   This is ADR-0027 applied at the entry point.

**Preconditions.** RFC-0015 is still `draft`; its readiness audit records the
disposition "supersede_or_amend for the canonical-surface section". The
canonical-surface question should be settled — amended RFC or new ADR — before
`icn.zone` acquires a UI, because that is the moment the two-surface split stops
being theoretical.

---

## Related

- [ADR-0032 — Website Truth Boundary](../adr/ADR-0032-website-truth-boundary.md)
- [ADR-0027 — Action Card Contract](../adr/ADR-0027-action-card-contract.md)
- [PUBLIC_SITE_IA.md](PUBLIC_SITE_IA.md) — page jobs on the canonical site
- [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md) — the twelve hard rejections
- [CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md) — regulatory-safe vocabulary
- [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md)
- [../architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md](../architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md)
- [../spec/institutional-domain.md](../spec/institutional-domain.md)
- [../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md)
