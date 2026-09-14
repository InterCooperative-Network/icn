---
Status: normative
Canonical: yes
Owner: Matt Faherty
Last Reviewed: 2026-09-14
Purpose: Defines the public website's information architecture — one primary job per page, the narrative order, the reduced top-level navigation, and the plain-language-first convention for introductory surfaces.
---

# Public site information architecture

> **One sentence.** Every public page has exactly one primary job, the homepage
> explains before it routes, and introductory surfaces lead with plain language
> and name the ICN term second.

Implements [#2608](https://github.com/InterCooperative-Network/icn/issues/2608)
and the cognitive-accessibility half of
[#1740](https://github.com/InterCooperative-Network/icn/issues/1740). Read
alongside [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md),
[CONTENT_STYLE_GUIDE.md](CONTENT_STYLE_GUIDE.md), and
[../design-language/concept-map.md](../design-language/concept-map.md).

---

## 1 · The governing rule

**Explain first; route second.**

The previous homepage asked a first-time visitor to classify themselves as
developer / non-technical contributor / institution / funder within the first
screen and a half — before anything had said what ICN was. A visitor who cannot
yet describe the project cannot choose a lane in it, so the choice either gets
made wrong or the visitor leaves.

Role routing now sits at the bottom of the homepage, after the explanation, the
worked example, and the maturity account.

---

## 2 · Homepage order

Fixed, and the order is the argument:

| # | Section | Job |
|---|---|---|
| 1 | Hero | One claim, one supporting explanation, two ways forward: *Follow one decision* and *Start reading*. |
| 2 | The problem | Fragmentation, **shown** as a two-column comparison rather than asserted in prose. |
| 3 | A concrete story | One fictional cooperative, one decision, told as a human sequence — not as protocol nouns. |
| 4 | What ICN changes | The shift, stated narrowly enough to be true. |
| 5 | The simple model | Six plain-language stages, each mapped to the ICN stations it covers. |
| 6 | See it work | The walkthrough. The strongest single link on the page. |
| 7 | What's real now | Evidence, positioned as a trust mechanism rather than a feature list. |
| 8 | Deeper architecture | For readers who want to descend. |
| 9 | Participation | Role routing — last, on purpose. |

**Visual rhythm.** Sections alternate weight: hero → wide figure → narrow
reading column → figure → full-width band → grid. The failure mode being
avoided is a page assembled entirely from equally-weighted card grids, where
nothing leads and the eye has no path through it.

---

## 3 · One job per page

A page that has two jobs will do the more flattering one. Each public page has
exactly one.

| Page | Primary job | Explicitly not its job |
|---|---|---|
| `/` | Understand ICN quickly | Routing by role before the explanation |
| `/what-is-icn` | The conceptual model — what the system models directly | Arguing the politics; that is `/why-icn` |
| `/why-icn` | The institutional and political problem | Explaining mechanisms; that is `/how-it-works` |
| `/how-it-works` | Architecture and mechanisms, station by station | Making maturity claims outside their band |
| `/see-it-work` | One decision followed end to end, in fixture data | Being a product surface, or implying live use |
| `/whats-real-now` | Evidence and maturity, dated and per-subsystem | Forward-looking promises |
| `/for-cooperatives` | Adoption and evaluation for an institution | General explanation — link back rather than repeat |
| `/for-developers` | Contribution surface and technical orientation | Restating the conceptual model |
| `/get-involved` | Participation routes that are actually maintained | Explaining what ICN is |
| `/docs` | Current reference, with archival separation visible | Presenting history as current |
| `/cooperative-economy` | Economic framing and the bridge institutions | Claiming economic capability the system lacks |

### Duplication removed in this pass

- **`/roadmap` → `/whats-real-now`.** Two pages describing project state, one from
  a hand-maintained JSON file that had drifted a month behind canonical state.
  Splitting "where we are" from "where we are going" invited the second page to
  make promises the first would not.
- **`/community` → `/get-involved`.** Overlapping participation routing, plus
  repository counts (branches, merged PRs, doc files) whose meaning and
  freshness could not be defended. See §6.

Both are permanent redirects in `astro.config.mjs`, not deletions — external
links keep working.

### Duplication resolved in the 2026-09-14 pass

The institutional-problem argument had been appearing in some form on `/`,
`/why-icn`, `/for-cooperatives`, `/cooperative-economy`, and `/what-is-icn`.
`/why-icn` owns it. The other three now hand off to it and keep only their own
angle:

- **`/for-cooperatives`** — three sections ("Bridges, and what the bridge does
  not cover", "The institutional problem you actually face", "Why generic
  software is not enough") became one short handoff. One sentence had been
  *verbatim identical* to `why-icn.astro` ("Integrating them only spreads the
  gap more evenly"), which is how the duplication was found. What replaced them
  is the question the page never answered: the adoption path (§9).
- **`/cooperative-economy`** — the third full run at the argument became a link
  plus the economic claim that is specific to this page: the gap is an unfilled
  institutional role, not merely bad software.
- **`/what-is-icn`** — the named-stack inventory became one sentence and a link.
  The page also stopped rendering the full nine-station `ClosureLoop` beneath
  `PublicLoop`; carrying the mechanism in full is `/how-it-works`' single job,
  and two versions of one diagram on a screen makes the reader choose which to
  learn.

Two further consolidations in the same pass:

- **Hand-written maturity prose.** `/for-cooperatives` restated the
  per-subsystem account in four hand-maintained paragraphs. `/whats-real-now`
  generates that account from `docs/status.toml` and states that where the two
  disagree it wins. A second hand-maintained copy is precisely how `/roadmap`
  drifted a month behind canonical state. The summary is now derived.
- **Two "How to engage" headings** sat adjacent on `/for-cooperatives`, one in
  abstract prose and one as steps. One section now.

### Duplication still outstanding

The "ICN does not replace human judgement" argument appears on `/`,
`/how-it-works` (a 33-line section), and `/get-involved`. `/how-it-works` should
keep the long version. `/get-involved` routes every audience three times — as a
route card, as a long-form path section, and again in a closing fallback; the
long-form sections predate the card model that replaced them and were never
collapsed into it.

---

## 4 · Top-level navigation

Reduced from eight items to five plus Docs:

```
What is ICN · How it works · See it work · What's real now · Get involved | Docs
```

`Why ICN`, `For cooperatives`, and `For developers` left the top level. All
three still exist, still have their own job, and are reached from the reading
ladder, the homepage, and `/get-involved`. What they no longer do is ask a
first-time visitor to choose between them in the site chrome.

### Reading ladder

The narrative sequence rendered at the foot of each narrative page, defined in
`website/src/data/readingOrder.ts`:

```
01 What is ICN → 02 Why ICN → 03 How it works → 04 See it work
→ 05 What's real now → 06 (fork) For cooperatives | For developers
```

`See it work` is inserted at 04 deliberately: after the reader has a conceptual
model, and before the maturity account. Someone who has watched a decision
become a receipt can read maturity claims with something concrete in mind;
someone who has not is being asked to evaluate honesty about a system they
cannot yet picture.

---

## 5 · Plain language first

The convention, applied to every introductory surface:

```
plain-language concept  →  ICN term  →  deeper meaning on demand
```

So: *who counts as a member here* (**standing**), *where you are acting*
(**scope**), *proof of why something happened* (**provenance**).

### Where the words come from

Both halves come from
[../design-language/concept-map.md](../design-language/concept-map.md), which
already maps every canonical concept to a public label and a one-line gloss.
`website/scripts/gen-concepts.mjs` parses that file at build time into
`src/data/concepts.generated.json`, and `<Term>` renders from the projection.

**The website never authors a public label for an ICN concept.** Copying a
label into an `.astro` file would let the public wording drift from the design
language with nothing to detect it. To change a public label, change the concept
map.

### Rendering rules

| Variant | Renders | Use |
|---|---|---|
| `plain` | plain label only | The most introductory copy, where naming the ICN term would be premature |
| `inline` | plain label + `(icn term)`, term links to the glossary | Running prose |
| `defined` | plain label + ICN term + the one-line gloss, as a block | The first, defining appearance on a page |

**Never a tooltip.** A `title` attribute is unreachable by keyboard, unreliably
announced by screen readers, and invisible on touch — which is most public
traffic. The gloss is either inline or it is a glossary link.

**Technical reference pages are exempt.** #1740 is explicit that precision on
developer surfaces must not be flattened. This convention applies to `/`,
`/what-is-icn`, `/why-icn`, `/see-it-work`, and introductory sections
elsewhere — not to `/for-developers`, `/architecture`, or `/docs`.

### The two loops

`PublicLoop` renders six plain-language stages — People, Rules, Decisions,
Action, Proof, Memory — each labelled with the ICN stations it covers.
`ClosureLoop` renders the canonical nine. The simplified view **must** state its
mapping to the nine, so it reads as a view of the real architecture rather than
a separate metaphor invented for newcomers. A simplified diagram that cannot be
traced back to the implementation is a marketing artifact.

---

## 6 · Public metrics

A number goes on the public site only if its source is mechanical and its
freshness is stated. Everything else comes off.

Removed in this pass: lines-of-code, test count, merged-PR count, active-branch
count, and total doc-file count. Each was either contradicted by another
canonical source, computed differently in three places, or — in the case of the
branch count — actively misleading, since it counted dead branches and presented
the total as a sign of life.

What remains is the crate count, the current commit, and the generated
project-state dates, each carrying a `trust` note in `stats.json` describing
exactly how it was obtained.

---

## 7 · Docs layering

Four public layers, derived from `docs/registry.toml` rather than from directory
names, by `website/scripts/gen-docs-classification.mjs`:

| Layer | Contents |
|---|---|
| Learn | Orientation, guides, worked examples, glossary |
| Current reference | Architecture, specifications, APIs, operations |
| Decisions | ADRs and RFCs, including superseded ones |
| Archive | Historical material, on its own page, `noindex`, excluded from default search |

Documents whose registry role is `internal` or `development_session`, and
partner-specific directories, are **not published to the website at all**. They
remain in the repository and on GitHub — this is a publication decision, not a
retention decision.

---

## 8 · Fixture-backed surfaces

Any page presenting fictional institutional data carries a visible truth label
and cannot hide it. `/see-it-work` is labelled `illustrative direction` — the
record shapes are real, the assembled guided surface is not shipped, and
[ICN_VISUAL_EXPLAINER_BIBLE.md](ICN_VISUAL_EXPLAINER_BIBLE.md) §3 requires a
visual sitting between two labels to carry the less optimistic one.

Fictional entities are drawn from the set already public on this site —
Brightworks Collective, Northeast Worker Federation, Maple Street Mutual Aid.
Real partner names appear only inside that partner's own institution package,
never in generic ICN material.

**No personal names, even fictional ones.** People appear by role — "a shop
steward" — which avoids fake-PII entirely and is also the more accurate way to
talk about standing, since standing attaches to a role in a scope rather than to
a person.

---

## 9 · The adoption path

`/for-cooperatives` carries one figure the rest of the site does not: a six-stage
progressive adoption sequence (`AdoptionPath.astro`), from *look at what you
already run* through *retire the incumbent when it is safe*.

**Why it lives there.** Its job — "how could an institution like ours
realistically approach this?" — is the second half of that page's one job. It is
not a roadmap, so it does not belong on `/whats-real-now`; it is not a mechanism,
so it does not belong on `/how-it-works`.

**Why it carries its own readiness axis.** The sequence spans current capability,
active development, and architectural direction, so each stage states which it
is: *possible today* · *parts exist, path does not* · *architectural direction*.

This axis is deliberately **not** the ADR-0032 maturity bands. Those describe
subsystems verified against source in `docs/status.toml`; adoption stages are not
subsystems and have no row in that file, so borrowing the band vocabulary would
imply a verification that has not happened. Keeping the vocabularies separate is
what stops the figure from becoming a second, un-generated maturity claim.

**The shape has to stay honest.** The two ends of the path rest on implemented
subsystems; the bridging middle — reconciling against incumbent systems, running
a function in parallel with one — is the least built part of ICN. There is no
import tool, no connector library, and no migration product. The figure's closing
note says so, and it is load-bearing: an adoption diagram is the easiest place on
a public site to imply a product that does not exist.

---

## Related

- [../reference/project-index/public-state-projection.md](../reference/project-index/public-state-projection.md) — how canonical state reaches the public page
- [ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md) — the accessibility floor
- [../design-language/accessibility.md](../design-language/accessibility.md) — component-level accessibility rules
- [../design-language/concept-map.md](../design-language/concept-map.md) — canonical source for every public concept label
- [MUST_NOT_SHIP.md](MUST_NOT_SHIP.md) — the twelve hard rejections
- [../adr/ADR-0032-website-truth-boundary.md](../adr/ADR-0032-website-truth-boundary.md) — public claim discipline
