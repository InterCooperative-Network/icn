# ops/ideas/ — Idea Refinery

The pre-RFC intake layer. Where raw ideas, product frames, operating
insights, source reviews, dogfood opportunities, institutional patterns,
seed backlog clusters, framing briefs, design sketches, teaching
candidates, and package-only examples enter the project before being
promoted into RFCs, ADRs, issues, package tasks, learning material, or
public claims.

> **Ideas enter. The refinery decides what they must become next.**

This layer exists because the prior pipeline assumed every new concept
deserved a full RFC or ADR. That produces architecture sprawl,
premature doctrine, and stale design docs that outpace runtime.

## What this is not

- Not the RFC layer. RFCs explore unresolved design spaces with enough
  shape to enumerate options. Most ideas do not have that shape.
- Not the ADR layer. ADRs record decisions. Ideas are upstream of
  decisions.
- Not a backlog. Ideas are not implementation commitments.
- Not a future-map. Naming a future object in a doc is not an idea
  promotion.

## The pipeline

```text
   raw idea
      │  capture
      ▼
   idea card                       ops/ideas/templates/idea-card.md
      │  triage
      ▼
   framed / classified             ops/ideas/ideas.yaml row
      │
      │  one or more of:
      ▼
   framing brief                   ops/ideas/templates/framing-brief.md
   source review                   ops/ideas/templates/source-review.md
   dogfood slice                   ops/ideas/templates/dogfood-slice.md
      │
      │  promotion review
      ▼
   promotion review                ops/ideas/templates/promotion-review.md
      │
      │  promote (one of)
      ▼
   ┌─────────────┬─────────────┬─────────────┬──────────────┬──────────────┬─────────────┐
   │ RFC         │ ADR         │ GitHub      │ NYCN package │ icn-learn    │ website     │
   │ candidate   │ candidate   │ issue       │ task         │ packet       │ claim       │
   └─────────────┴─────────────┴─────────────┴──────────────┴──────────────┴─────────────┘
      │
      │  (or)
      ▼
   parked / rejected / superseded
```

After promotion, the existing pipeline resumes:

> **RFCs explore. ADRs decide. Issues build. Tests prove. The website claims only what the proof supports.**

(See [`ops/coordination/README.md`](../coordination/README.md).)

## Hard rules

1. **No idea enters the build backlog until it has a scope, a boundary,
   and a proof path.** Capture is cheap; promotion has thresholds.
2. **Accepted RFC does not mean implemented.**
3. **Accepted ADR does not mean implemented.**
4. **Future map does not mean backlog commitment.** Naming an object in
   `docs/architecture/*.md` does not promote it to an issue.
5. **Public claim requires evidence.** Website touches gate on shipped
   runtime, ADR `implementation_status`, or test/proof receipts.
6. **NYCN/Summit-specific meaning does not go into ICN core.** It lives
   in the NYCN repo as institution package material.
7. **Private operational data does not go into Git.** Drive sources are
   bootstrap material; promotion requires a privacy review.
8. **Drive/Sheets are bootstrap source material, not canonical
   architecture.** They are imported into typed records via
   `BridgeImportReceipt`, never pinned as truth.

## Statuses

| Status | Meaning |
|---|---|
| `raw` | Captured but not framed. |
| `captured` | Has a one-sentence problem and a source. |
| `framed` | Has a framing brief or equivalent shape. |
| `classified` | `kind`, `belongs_to`, and `layer` are set. |
| `decomposed` | Broken into smaller ideas where appropriate. |
| `needs_source_review` | Requires a Drive/external-system review before promotion. |
| `needs_dogfood` | Requires a NYCN dogfood slice before generic promotion. |
| `promotion_review` | A promotion review is in progress. |
| `promoted_rfc_candidate` | Added as a row in `rfc_candidates.yaml`. |
| `promoted_adr_candidate` | Added as a row in `adr_candidates.yaml`. |
| `promoted_issue` | Filed as a GitHub issue. |
| `promoted_package_task` | Captured as a NYCN repo task. |
| `promoted_learning_task` | Captured as an icn-learn repo task. |
| `promoted_website_claim` | Eligible for a public-site change with evidence. |
| `parked` | Real but not actionable now. |
| `rejected` | Will not be pursued. |
| `superseded` | Replaced by another idea (cite the replacement). |

## Destinations (`belongs_to`)

| Destination | Example |
|---|---|
| `icn` | Generic ICN substrate, kernel, or app. |
| `nycn` | Institution-specific application or operating material. |
| `icn-learn` | Teaching, onboarding, packet, or course material. |
| `website` | Public claim on `intercooperative.network`. |
| `private_overlay` | Private operational data; not in Git. |
| `google_drive` | Lives in Drive as bootstrap/source material. |
| `external` | Outside the ICN ecosystem entirely. |
| `unknown` | Triage incomplete. |

## Idea kinds

| Kind | Use for |
|---|---|
| `product_frame` | High-level product framing or naming (e.g. "ICN as cooperative relationship infrastructure"). |
| `institutional_need` | An institution's operating need (e.g. "consent-based outreach"). |
| `architecture_concept` | A design concept that may eventually need an RFC. |
| `runtime_gap` | A gap between current runtime and a stated direction. |
| `package_pattern` | Pattern that belongs in NYCN, not ICN core. |
| `learning_material` | Teaching/onboarding candidate for icn-learn. |
| `public_claim` | Candidate website claim. |
| `source_review` | Drive / Sheets / external-source mapping work. |
| `dogfood_slice` | A real NYCN slice to validate a generic ICN primitive. |
| `process_rule` | Coordination/governance/ops rule for the project itself. |
| `privacy_boundary` | Boundary or carve-out for private/PII data. |
| `evidence_gap` | A claim that lacks evidence/proof. |
| `repo_hygiene` | Tooling, templates, hooks, or CI hygiene. |
| `future_research` | Long-horizon question; not actionable now. |

## Promotion thresholds

Promotion is not automatic and is not scheduled. A promotion review
exists for each promotion. Thresholds:

### Promote to RFC candidate only if

- The design space is unresolved.
- Multiple viable options exist with real tradeoffs.
- The decision would affect generic ICN architecture (not just NYCN).
- The idea has enough shape to enumerate options, not just a name.

### Promote to ADR candidate only if

- The decision is clear enough to record.
- Scope is generic, or back-fills actual implementation.
- Consequences are understood.
- Implementation status can be tracked separately from the ADR itself.

### Promote to GitHub issue only if

- The build slice is clear.
- Acceptance criteria are clear.
- Affected files / crates / docs are identifiable.
- A validation / proof path is known.

### Promote to NYCN package task only if

- The institution-specific meaning belongs in NYCN.
- The generic ICN substrate already exists or is explicitly marked
  planned in an ICN doc.
- No private data is committed.

### Promote to icn-learn only if

- The canonical source already exists or is explicitly linked.
- The teaching material does not define doctrine.

### Promote to website only if

- The claim is backed by state docs, tests, ADR `implementation_status:
  implemented` or `verified`, or shipped runtime.
- The maturity band is honest (per ADR-0033).

## Framing briefs

Concrete framing briefs (instances of the framing-brief template) live
under [`framing/`](framing/). Each brief is anchored to one or more
idea cards in `ideas.yaml` and is descriptive, not normative.

## Templates

- [`templates/idea-card.md`](templates/idea-card.md) — minimal capture.
- [`templates/framing-brief.md`](templates/framing-brief.md) — refine
  scope and boundary before promotion.
- [`templates/source-review.md`](templates/source-review.md) — map
  Drive / external sources to typed records and privacy classes.
- [`templates/dogfood-slice.md`](templates/dogfood-slice.md) — design
  a NYCN-real slice that exercises a generic ICN primitive.
- [`templates/promotion-review.md`](templates/promotion-review.md) —
  promotion gate before any RFC / ADR / issue / package / learning /
  website target.

## Validator

```sh
python3 ops/ideas/validate_ideas.py
```

Stdlib-only. Checks unique IDs, required fields, valid statuses, valid
destinations, valid kinds, that promoted statuses include a
`promoted_target`, that `public_claim_ready: true` has an
`evidence_required` entry that is satisfied, and that
`implementation_ready: true` has a proof path. Exits non-zero on
failure.

## Cross-repo coordination

Idea destinations span repos. The cross-repo merge order rule is in
[`ops/coordination/PR_STACK_PROTOCOL.md`](../coordination/PR_STACK_PROTOCOL.md):

1. ICN canonical first.
2. NYCN application second.
3. ICN Academy teaching third.
