---
id: "0018"
title: "ADR Lifecycle and Canonical Decision Index"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["adr", "process", "decisions", "documentation", "tooling", "lifecycle"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented (tooling: ops/mcp/src/tools/decisions.ts; canonical location: docs/adr/, established by PR #1637)"
references:
  - "ADR-0017 (Monorepo Consolidation)"
  - "PR #1637 (ADR canonicalization under docs/adr/)"
  - "docs/adr/template.md"
---

# ADR 0018: ADR Lifecycle and Canonical Decision Index

## Status

**Accepted (2026-04-26).** Establishes the lifecycle vocabulary, metadata convention, and tooling contract for ICN ADRs. PR #1637 already moved the canonical location to `docs/adr/`; this ADR records the conventions that location follows.

## Context

ICN's ADRs grew up across two physical locations (`docs/adr/` and the now-retired `ops/state/decisions/`) with three different metadata styles:

- **Classic markdown**: `**Date**`, `**Status**`, `**Tags**` as bold labels (ADR-0001 through ADR-0009, 0011, 0015, 0016).
- **YAML frontmatter**: `---` block at the top with `id`, `title`, `status`, `date`, etc. (ADR-0012, 0013, 0014).
- **Bullet list**: `- Status: Proposed`, `- Date: ...` (ADR-0010).

The MCP indexer in `ops/mcp/src/tools/decisions.ts` recognizes only the classic format. ADRs in the other styles fail to populate `decision_index.title`/`tags`/`date` correctly. This makes the institutional memory brittle.

In addition, several decisions had implicit lifecycles that were never written down:

- "Proposed" ADRs that were silently being depended on as accepted.
- "Accepted" ADRs whose original decision had since been superseded by reality (multi-repo → monorepo) without a follow-up ADR being filed.
- "Implementation status" being tangled up with "decision status" — they are not the same thing. An accepted decision may be unimplemented, partially implemented, or fully implemented; the decision is still accepted.

## Decision

### 1. Canonical location

Human-readable ADRs live under `docs/adr/` only. `ops/state/decisions/` is retired and the directory holds only a redirect README (PR #1637). Operational state may *index* ADRs (the MCP `log_decision` / `search_decisions` / `get_decision` tools), but indexing is not authoring. There is one canonical decision tree, and it is `docs/adr/`.

### 2. ADR lifecycle vocabulary

Every ADR has a `status` from this closed list:

| Status | Meaning |
|---|---|
| `proposed` | Documented and circulating for review. Not yet the project's decision. May still be revised in body. |
| `accepted` | The decision is in force. Body is frozen except for amendment notes. New behavior must conform. |
| `amended` | An accepted ADR with one or more amendment notes added after acceptance. The original decision still holds; the amendments narrow, clarify, or extend it without overturning it. |
| `superseded` | An accepted-or-amended ADR whose decision has been replaced by a successor ADR. The body remains as institutional memory. The successor is named in `superseded_by`. |
| `deprecated` | An accepted-or-amended ADR whose decision no longer applies, but no successor ADR replaces it (the entire concern went away). Rare. |

### 3. Decision status is separate from implementation status

A decision can be `accepted` without code; code can exist without a decision being recorded. The two statuses are tracked independently:

- `status` (decision lifecycle, above).
- `implementation_status` (free-form text or a closed enum: `proposed`, `partially implemented`, `implemented`, `not yet implemented`, `needs verification`, `unknown`).

When code lands that proves an accepted ADR's intent, update `implementation_status` and cite the evidence (PR number, file path, test name). Do NOT silently flip `status` from `proposed` to `accepted` because code was written. The decision lifecycle moves only when the project decides; the implementation status moves when the code does.

### 4. Supersession is explicit and traceable

When a new ADR replaces an old one:

- The new ADR's frontmatter sets `supersedes: [ADR-NNNN]`.
- The old ADR's frontmatter sets `superseded_by: ADR-NNNN`.
- The old ADR keeps its body. **History is preserved.** Add a short header note explaining the supersession; do not rewrite the body to match the new decision.
- Searching for either ADR in the MCP index returns the chain.

When a new ADR amends without replacing:

- The new ADR's frontmatter sets `amends: [ADR-NNNN]`.
- The old ADR's status changes from `accepted` to `amended` and gets an amendment note pointing to the new ADR.

### 5. Metadata convention going forward

**New ADRs use YAML frontmatter** with at minimum:

```yaml
---
id: "NNNN"
title: "Short Decision Title"
status: "proposed | accepted | amended | superseded | deprecated"
date: "YYYY-MM-DD"
deciders: ["…"]
tags: ["…", "…"]
supersedes: []      # optional; list of "ADR-NNNN"
superseded_by: []   # optional; "ADR-NNNN" or empty list
amends: []          # optional; list of "ADR-NNNN"
implementation_status: "…"  # optional; free-form or one of the closed values above
references:
  - "ADR-NNNN (relationship description)"
  - "docs/path/to/related.md"
---
```

**Existing ADRs keep their original metadata format.** They are not mass-rewritten. The MCP indexer is taught to parse all three formats so the index stays consistent across the historical record. When an existing ADR is touched for an amendment or supersession, the touch may include a metadata-format upgrade in the same edit, but a doc-only mass migration is out of scope for normal work.

### 6. Tooling contract

`ops/mcp/src/tools/decisions.ts` indexes `docs/adr/` on boot and on each ADR write:

- Recognizes filenames `ADR-NNNN-slug.md` (canonical) and `NNNN-slug.md` (legacy, for safety).
- Parses metadata in three forms: YAML frontmatter, classic `**Field**:` markdown, bullet list.
- Extracts: `id`, `title`, `status`, `date`, `tags`, `supersedes`, `superseded_by`, `amends`, `implementation_status`.
- Re-points stale `file_path` rows to the canonical `docs/adr/` location on every boot (`INSERT OR REPLACE`).
- `get_decision` falls through to a filesystem scan when a stored row's `file_path` no longer exists.

### 7. Template

`docs/adr/template.md` is the source of truth for new-ADR shape. It uses the metadata convention above. Authors are not required to use every field, but the schema is the schema.

## Consequences

- ADR drift between `docs/adr/` and any other location is impossible by rule (only one canonical location exists) and indexed by tooling.
- Old decisions remain readable as written; their relevance to today is encoded in `status` + `superseded_by` + amendment notes, not in a rewrite.
- Searching the MCP decision index now works uniformly across all metadata formats. Brittleness from format inconsistency is gone.
- Decision-vs-implementation status separation prevents two failure modes: (a) "code lands so I'll quietly mark the ADR accepted," (b) "ADR is still proposed so I'll act as if no decision was made."
- Future authors have a written rule for "what do I do when the world changes after my ADR was accepted?" — file a new ADR, update the old one's `status` and frontmatter, point at the successor. No silent rewrites.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Mass-migrate every existing ADR to YAML frontmatter | Touches every historical record without changing institutional meaning. The brittleness cost is paid by the indexer once; the rewrite cost would be paid forever in noisy commits and lost git-blame. Indexer change is the right place. |
| Keep `ops/state/decisions/` as a parallel index | Was the source of the drift problem in the first place. PR #1637 retired it; this ADR records the principle behind that retirement. |
| Combine "decision status" and "implementation status" into one field | Conflates two genuinely independent variables. Loses the ability to say "we decided this, code is not yet here" or "we didn't decide this, but code shipped that depends on it." Both states need to exist; they need separate fields. |
| Skip writing this ADR | The lifecycle is real whether it is documented or not. Implicit conventions are how drift gets back in. |
