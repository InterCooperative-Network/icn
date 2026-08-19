---
Status: normative
Canonical: yes
Owner: Matt Faherty
Last Reviewed: 2026-08-17
Purpose: Defines how canonical repository state is projected onto the public website, including the exact vocabulary mapping between docs/status.toml and the ADR-0032 public maturity bands.
---

# Public state projection

> **One sentence.** The public website renders a *projection* of canonical
> repository state, never a second copy of it, and this document defines every
> place the internal vocabulary meets the public one.

Implements the durable requirement of
[#1369](https://github.com/InterCooperative-Network/icn/issues/1369). Read
alongside [claim-boundaries.md](claim-boundaries.md) (the two-axis rule) and
[ADR-0032](../../adr/ADR-0032-website-truth-boundary.md) (the public band
vocabulary and the seven claim rules).

---

## 1 · Why a projection rather than a website tracker

Before this, `website/src/data/roadmap.json` held a hand-written account of
project state. It was a second project tracker, and it did what second trackers
do: at the time it was removed it was roughly a month behind `docs/status.toml`,
the file it was supposed to reflect, and nothing in CI or in review would have
caught that.

The rule now is one-directional:

```
docs/status.toml  →  website/scripts/gen-project-state.mjs  →  the page
```

Nobody edits the public claim. They edit `docs/status.toml`, which is where the
claim was supposed to live all along, and the website follows on the next build.

---

## 2 · What is projected

| Source field | Projected as | Notes |
|---|---|---|
| `subsystems.<id>.name` | Subsystem name | verbatim |
| `subsystems.<id>.status` | Public maturity band | mapped — see §3 |
| `subsystems.<id>.evidence_type` | Public evidence class | mapped — see §4 |
| `subsystems.<id>.last_verified` | "Verified" date | verbatim |
| `subsystems.<id>.gaps[]` | "Recorded gaps" column | verbatim, unabridged |
| `subsystems.<id>.crates[]` | Crate list under the name | verbatim |

### What is deliberately not projected

The `[summary]` table is **not** projected at all. Three of its fields are
unpublishable and the table has no schema guaranteeing a fourth will not be
added:

- `total_tests_note` — `status.toml` marks its own test count stale, and
  `docs/PHASE_PROGRESS.md` carries an incompatible baseline for the same date.
- `loc_approx` — contradicted by `PHASE_PROGRESS.md`; a third number was
  previously computed at build time by the website itself.
- `deployment` — contains a `running since <date>` phrase, which
  [show-readiness-map.md](show-readiness-map.md) § red lines forbids on a public
  surface.

Excluding the whole table is a deliberate fail-safe choice over maintaining a
per-field denylist that a new field could quietly outflank.

---

## 3 · Status → maturity band

`docs/status.toml` declares a closed `status` vocabulary; ADR-0032 declares a
closed public band vocabulary. No mapping between them existed anywhere in the
repository before this document. This is it.

| `status.toml` value | Public band | Reasoning |
|---|---|---|
| `exceeds-spec` | `strong` | Verified working beyond the specified behaviour. |
| `confirmed` | `strong` | Verified working. The band the project defends in public. |
| `partially-confirmed` | `maturing` | Real implementation; verification incomplete. Not `advancing`, because the limit is what has been *checked*, not what has been *built*. |
| `simplified` | `maturing` | A deliberately reduced implementation. Real, but not the full concept — which is precisely what "real but maturing" says. |
| `working-immature` | `advancing` | Works; not yet reliable enough to lean on. The active frontier. |
| `foundation-only` | `notyet` | Scaffolding exists, the capability does not. |

**Unmapped values fail the build.** `gen-project-state.mjs` exits non-zero on
any `status` it does not recognise rather than defaulting to a band. A new
status value is a decision about a public claim, and it should be made here,
deliberately, not absorbed silently by a fallback.

> **Vocabulary defect, recorded here so it is not rediscovered:** ADR-0032
> writes the fifth band as `not-yet`, while `MaturityBadge.astro` declares it
> `notyet`. The code spelling is what the site uses. Reconciling the two is a
> separate, narrow change to ADR-0032 and the component together.

---

## 4 · Evidence type → evidence class

The second axis. [claim-boundaries.md](claim-boundaries.md) requires
implementation maturity and evidence strength to be carried together and never
collapsed; this is the public rendering of the second one.

| `status.toml` `evidence_type` | Public class | Public wording |
|---|---|---|
| `test` | `test-backed` | Exercised by the automated test suite in this repository. |
| `ci` | `ci-backed` | Enforced by a check that runs on every change. |
| `demo` | `fixture-backed` | Demonstrated against deterministic fixture data, not live institutional use. |
| `code-review` | `reviewed` | Established by reading the implementation, not by an automated check. |
| `human-asserted` | `asserted` | Stated by a maintainer. No automated check currently proves this one. |

The wording matters more than the label. "Test-backed" must not be readable as
"has been run by an institution", so the sentence says what the evidence
actually covers.

**Why this axis exists at all:** without it, a subsystem marked `strong` reads
as finished. With it, `Governance — strong — fixture-backed` says the true and
more useful thing: well-built, and demonstrated only against prepared data.

---

## 5 · Freshness

The page shows two dates and they are not interchangeable:

- **Subsystem data verified** — the newest `last_verified` across subsystems, from
  `status.toml`. Generated. It moves only when someone re-verifies a subsystem
  against source.
- **Narrative last reviewed** — a constant in `whats-real-now.astro`, covering the
  hand-written prose. It moves only when someone re-reads that prose against the
  generated table.

Neither is the build timestamp, deliberately. A rebuild must never look like a
re-verification — that is the exact failure mode that makes a "last updated"
stamp worthless.

---

## 6 · Fail-closed behaviour

`gen-project-state.mjs` exits non-zero, taking the build with it, when:

- `docs/status.toml` is missing or unparseable;
- any `status` or `evidence_type` value is unmapped;
- any subsystem lacks `last_verified`;
- fewer than 8 subsystems parse.

A public page whose entire purpose is trustworthiness is worse than useless
when it silently renders partial data. A red build is the cheap failure.

The same discipline applies to the sibling generators:
`gen-docs-classification.mjs` refuses to publish an unfiltered or near-empty
docs surface, and `gen-concepts.mjs` refuses to emit a glossary missing any of
the nine closure-loop concepts.

---

## 7 · Subsystems with no row

`status.toml` has no row for membership, receipts/provenance, the member-facing
shell, or the rehearsal appliance. The website does not invent one. Those are
covered in the hand-written narrative on `/whats-real-now`, and the page states
plainly that they are absent from the generated table.

That absence is itself worth publishing: the machine-readable record does not
yet cover everything the project talks about, and hiding that would be its own
small dishonesty.

---

## Related

- [claim-boundaries.md](claim-boundaries.md) — the orthogonal-axes rule
- [show-readiness-map.md](show-readiness-map.md) — red lines for public claims
- [proof-level-taxonomy-capability-matrix.md](proof-level-taxonomy-capability-matrix.md) — the L0–L8 evidence ladder
- [ADR-0032](../../adr/ADR-0032-website-truth-boundary.md) — public band vocabulary and claim rules
- [../../design/PUBLIC_SITE_IA.md](../../design/PUBLIC_SITE_IA.md) — page responsibilities and plain-language conventions
