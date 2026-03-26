# ICN Document Registry Specification

## Purpose

The document registry is the machine- and human-readable control plane for the docs corpus.

Its job is to answer, for every important document:

- what it is
- what kind of truth it contains
- whether it is canonical
- how fresh it is
- what subsystem it belongs to
- what should happen to it during migration

This spec is intentionally aligned with the existing `docs/registry.toml`, `docs/freshness.toml`, and `docs/status.toml` pattern already present in the repo.

---

## Required fields

Each registry entry should include at minimum:

| Field | Meaning |
|---|---|
| `path` | Repository-relative path |
| `title` | Human-readable title |
| `truth_class` | `normative` / `descriptive` / `operational` / `historical` / `draft` |
| `role` | `architecture` / `design` / `spec` / `planning` / `strategy` / `guide` / `status` / `archive` / `internal` / `template` / `reference` |
| `canonical` | `yes` / `no` |
| `status` | `active` / `living` / `superseded` / `archived` / `draft` / `missing` |
| `freshness` | `current` / `stale` / `unknown` |
| `owner` | Responsible human or team |
| `audience` | Intended audience(s) |
| `summary` | One-line description |
| `recommended_action` | `keep` / `move` / `archive` / `split` / `merge` / `mark-superseded` / `review` |

---

## Optional fields

| Field | Meaning |
|---|---|
| `superseded_by` | Replacement path if superseded |
| `related_docs` | Related paths |
| `related_issues` | GitHub issues |
| `related_prs` | Pull requests |
| `last_reviewed` | Date of last human review |
| `evidence_type` | `code` / `test` / `demo` / `adr` / `human-asserted` |
| `notes` | Migration comments |

---

## Truth classes

### `normative`
How the system is supposed to work.

### `descriptive`
What the system currently is.

### `operational`
How to run, verify, or recover it.

### `historical`
Preserved for context. Not current truth.

### `draft`
Proposal, not yet frozen.

---

## Canonical policy

A document can be **useful** without being **canonical**.

Mark a doc canonical only if it is:
- actively maintained
- intended as current reference
- the preferred source for its topic

Historical docs should remain queryable, but should not sit in canonical locations without explicit labeling.

---

## Storage model recommendation

### Human-facing
Maintain:
- `docs/DOCUMENT_REGISTRY.md`

### Machine-facing
Maintain:
- `docs/registry.toml`
- `docs/freshness.toml`
- `docs/status.toml`

The markdown registry is for contributors and agents reading the repo.
The TOML files are for scripts, linting, and future automation.

---

## Minimal migration requirement

During the docs-system PR, the agent should produce at least:

1. a normalized markdown registry for high-value docs
2. an updated machine-readable registry for moved docs
3. canonical/superseded labels for ambiguous docs
4. a list of unresolved classifications requiring human review

