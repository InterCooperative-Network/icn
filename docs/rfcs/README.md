# docs/rfcs/

Request-for-Comment design documents for ICN.

> **RFCs explore. ADRs decide. Issues build. Tests prove. The website claims only what the proof supports.**

## What an RFC is

An RFC explores a design space. It is the document where tradeoffs get named, options get compared, and consequences get traced — *before* a decision is made. RFCs are the "we are not yet sure how to do this; here is a structured way to think about it" layer of the project.

An RFC is **not** a decision. It is **not** an implementation commitment. It is **not** authority for runtime change.

## Why RFCs are separate from ADRs

| Layer | What it is | Where it lives |
|---|---|---|
| RFC candidate | Unresolved design space worth exploring | [ops/coordination/rfc_candidates.yaml](../../ops/coordination/rfc_candidates.yaml) |
| RFC | Design exploration and tradeoff analysis | `docs/rfcs/RFC-NNNN-*.md` |
| ADR candidate | Likely future decision the project intends to record | [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) |
| ADR | Architectural decision receipt | [docs/adr/ADR-NNNN-*.md](../adr/) |
| Issue | Implementation commitment | GitHub issues |
| Test | Evidence that implementation matches the decision | `crates/*/tests/` |
| Website | Public truth boundary; only claims what evidence supports | [website/src/](../../website/src/) |

ADRs in `docs/adr/` are *decision receipts* — they record what was decided and why, with explicit lifecycle governed by [ADR-0018](../adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md). RFCs are upstream of decisions: they exist so the project can hold a design conversation in writing without prematurely committing to an outcome.

Drafting an ADR before the design space is explored produces shallow ADRs. Drafting an RFC and then an ADR produces decisions whose tradeoffs are documented and whose alternatives are auditable.

## RFC lifecycle

| Status | Meaning |
|---|---|
| `draft` | Author is still composing; not ready for broad review. |
| `active` | Open for review, comments, alternative proposals. |
| `accepted` | Project has converged on a direction. The follow-up ADR(s) record the decision. **Accepted RFC does NOT mean implemented.** |
| `rejected` | Project decided against the proposal. Body preserved for future reference. |
| `superseded` | A later RFC replaces this one. Both bodies preserved; cross-linked. |
| `withdrawn` | Author or project removed the RFC before reaching a decision. |

See [RFC-0000](RFC-0000-rfc-process.md) for the process detail (when to write an RFC, how to move it forward, how to supersede or withdraw).

## File naming

`RFC-NNNN-short-kebab-title.md` — four-digit zero-padded number, hyphen, kebab-case title. The number is allocated when the RFC moves from `candidate` (in `rfc_candidates.yaml`) to `draft`. Numbers are sequential and never reused.

## Frontmatter contract

Every RFC carries YAML frontmatter conforming to [`template.md`](template.md). The required fields are:

- `id` — string, four-digit zero-padded
- `title` — short title
- `status` — one of `draft | active | accepted | rejected | superseded | withdrawn`
- `created` — ISO date the RFC was first written
- `updated` — ISO date of the last meaningful change
- `authors` — list of names or DIDs
- `reviewers` — list of names or DIDs (may be empty during `draft`)
- `related_adr_candidates` — list of ADR ids the RFC may inform
- `related_issues` — list of GitHub issue references
- `supersedes` — list of RFC ids this RFC replaces (empty unless replacing one)
- `superseded_by` — list of RFC ids that replace this one (empty unless replaced)

Implementation status is **not** an RFC field. Implementation status lives on ADRs and only changes with code/test evidence.

## What RFCs are NOT

- Not decisions. ADRs decide.
- Not implementation commitments. Issues commit.
- Not evidence. Tests prove.
- Not public claims. The website claims.

A future contributor reading "RFC-0042 says X" should understand that as "the project explored X in RFC-0042" — not as "X is the law of the project," and not as "X is implemented."

## See also

- [RFC-0000](RFC-0000-rfc-process.md) — the RFC process itself.
- [docs/adr/ADR-0018](../adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md) — ADR lifecycle vocabulary.
- [docs/adr/ADR-0034](../adr/ADR-0034-adr-candidate-registry-as-architectural-memory.md) — ADR candidate registry as architectural memory.
- [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../strategy/ICN_CONSTITUTIONAL_ROADMAP.md) — long-arc architectural roadmap.
- [ops/coordination/rfc_candidates.yaml](../../ops/coordination/rfc_candidates.yaml) — unresolved design spaces awaiting RFCs.
- [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) — likely future decisions awaiting ADRs.
