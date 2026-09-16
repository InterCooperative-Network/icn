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
- [`docs/spec/icn-civic-shell-v0.md`](../spec/icn-civic-shell-v0.md) §"Domain and
  route doctrine" — `Status: normative`. The strongest statement of what
  `icn.zone` is *for*: "the short operational/access/discovery domain… Its job is
  fast routing into action surfaces, not republishing ICN's public narrative."
  It also fixes the status of every route name: `/status`, `/forge`, `/dev`,
  `/docs`, **`/join`**, `/dashboard` are "examples only until a separate PR
  proves any route live." And: "No DNS record is created or changed by this spec."
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

### 2.4a The authority direction is one-way

[`icn-civic-shell-v0.md`](../spec/icn-civic-shell-v0.md) §"Authentication" is
normative and this is the rule an access surface is most likely to violate,
because every off-the-shelf pattern points the wrong way:

```text
permitted:  DID / ICN standing / mandate  →  short-lived service/session claim
forbidden:  IdP group                     →  ICN authority
```

"Groups are projection state, not authority." An identity provider may carry
browser-session state; none of them grants ICN authority by itself. A join
surface that reads "user is in the `brightworks-members` group, therefore they
are a member of Brightworks" has inverted the arrow and rebuilt the platform
model inside the login box.

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

### 2.7 The pre-confirm contract

Any `icn.zone` surface that mutates institutional state inherits the pre-confirm
flow. **Two documents, and it is worth being precise about which owns what**,
because the obvious citation is the wrong one:

- **[`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) §"Signing /
  confirmation flow" owns the flow** — a normative ten-step contract opening
  "No member action that mutates institutional state happens without passing
  through a pre-confirm summary": review summary, authority basis, scope,
  consequence, reversibility/challenge window, **named** receipt class, privacy
  warning, offline/sync warning, distinct confirm/cancel affordances, and a
  post-action receipt status. Irreversible actions require an explicit "this
  cannot be undone" line and a second, distinct confirmation.
- **[ADR-0027](../adr/ADR-0027-action-card-contract.md) owns the card's data
  model**, not the flow. It defines the eight card elements and the closed
  `card_kind` taxonomy. Steps 2 and 3 above render from its `authority_basis`
  and `scope` fields.

[MUST_NOT_SHIP](MUST_NOT_SHIP.md) §3 states the rule but attributes it to
ADR-0027 alone; ADR-0027's text does not contain a pre-confirm contract. Cite
`member-shell-v0.md` for the flow and ADR-0027 for the fields. Governance
actions additionally render threshold and current tally above the confirm step
([CONTENT_STYLE_GUIDE](CONTENT_STYLE_GUIDE.md) §"Dangerous-action copy" v0.2
fields 5 and 6).

`MembershipApproval` and `MembershipDeparture` are already reserved
`card_kind`s, so an admission flow does not require amending the taxonomy — but
note that **`member-shell-v0.md` specifies no entry, onboarding, or admission
surface at all.** It begins after a member already has standing. A join flow is
new specification work, not an extension of an existing one.

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

## 6 · The edge trust boundary

[`ICN_ZONE_ROUTING.md`](../deployment/ICN_ZONE_ROUTING.md) §"Phase 2" specifies a
Cloudflare Worker that "looks up code in Cloudflare KV **or** queries the ICN
gateway API."

That `or` is the whole architectural decision, and it is currently a disjunction
inside a runbook step. The two branches are not variants of one design:

- **Worker queries the ICN gateway.** Cloudflare is a proxy. ICN stays
  authoritative. The edge sees a code in transit — unavoidable for any TLS
  terminator — and stores nothing.
- **Cloudflare KV holds the mapping.** Cloudflare becomes a store of record for
  admission-adjacent data, inside the ICN trust boundary but outside ICN
  governance, outside the repository, outside review, and outside
  `just website-verify`.

The runbook lists "Worker deployment does not require changes to the ICN Rust
codebase" as a benefit. For a dumb redirect it is one. For anything that resolves
a code it is a warning: it means the resolution logic is unversioned relative to
the kernel it is fronting.

### The layering

The question is not "can Cloudflare route this?" It is which layers may sit
outside the ICN trust boundary. Flattening these into one decision is the error.

| Layer | Outside ICN? | Why |
|---|---|---|
| HTTP redirect, path preservation, TLS | **Yes** | No ICN semantics. This is what a CDN is for. |
| Rate limiting, abuse control on code endpoints | **Yes** | Wants to be at the edge; carries no authority. |
| Resolving an opaque code → *which public page to show* | **Only as a cache** of an ICN-authoritative answer, never as the store of record | A store of record at the edge means a KV misconfiguration can point a join code at an attacker-controlled destination. |
| Deciding whether an invitation is **valid** | **No** | Validity is governed state. |
| Membership / invite **authorization** | **No** | Mandate-gated (ADR-0027). |
| **Admission** — actually making someone a member | **No** | A governed decision that must produce a receipt. |
| Institutional **authority** | **No** | Lives in `InstitutionalDomain`. DNS never confers it. |
| Identity information about the invitee | **No** | Must not persist at the edge in linkable form. |
| Receipt issuance or validation | **No** | Kernel path, ADR-0026. |

### The rule that keeps the line clean

> **A short route resolves to a page, never to an outcome.**

`/j/<code>` identifies *which invitation to display*. It must not itself admit
anyone, grant standing, or create a record. Admission happens afterwards, inside
ICN, under a mandate, producing a receipt — with the action-card contract
(ADR-0027) satisfied at that step: mandate, reversibility, and named receipt
declared before the member confirms.

This rule is what makes the edge question tractable. Under it, even the KV
variant holds only *code → which institution's public join page*, not *code →
membership grant*. A URL that conveys an outcome is a bearer credential, and a
bearer credential resolved by a third party is an authorization decision
delegated outside the trust boundary.

It leaks less, not nothing: an enumerable code→institution map still reveals
which institutions exist and lets an attacker probe for live codes. So codes must
be high-entropy, revocable, expiring, and rate-limited regardless of where they
are resolved.

---

## 7 · Implementation brief for the next pass

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

**Preconditions — RFC-0015 is not one of them.**

It is tempting to read RFC-0015's `draft` status plus its readiness-audit
disposition ("supersede_or_amend for the canonical-surface section") as a gate on
this work. It is not, and treating it as one would block Slice 1 on a question
that is already answered.

The audit
([`ops/coordination/RFC_ADR_READINESS_AUDIT.md:41`](../../ops/coordination/RFC_ADR_READINESS_AUDIT.md),
2026-04-28) prescribed a specific remedy: *"add a status note in the RFC that
ADR-0032 is the accepted policy for the canonical-truth-surface decision."*
**That remedy was applied the same day** — the RFC's frontmatter records
`updated: "2026-04-28" # ADR-0032 alignment note added`, and its §Status now
carries a "Relationship to ADR-0032 (accepted)" section ending "Where this RFC
and ADR-0032 disagree, ADR-0032 wins."

What remains `draft` in RFC-0015 is the **learning-repo direction**, and every
one of its five open questions is about `learn.icn.zone`, `cooperative-systems.org`,
or `thesync.net`. None is about `icn.zone`. Its three design options differ only
on where *learning* content lives; all three assign `icn.zone` the same role
("utility routing — short links, QR, redirects, cluster subdomains"), so no
option in the RFC is in tension with building it.

The real preconditions are §5's, and only two of them gate Slice 1:

1. **The edge trust boundary** (§5, "Who serves it") — unresolved, and it is the
   one that matters. See §6.
2. **What a code resolves to** — a design decision, not a doctrine one, but it
   must be made before code is written rather than discovered afterwards. See §6.

`DnsBinding` not existing in code gates `/n/<name>`, not `/j/<code>`.

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
