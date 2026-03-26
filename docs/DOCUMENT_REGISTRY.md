---
Status: descriptive
Canonical: no
Last Reviewed: 2026-03-26
---

# ICN Document Registry (human summary)

This file is a **companion** to the machine-readable [`registry.toml`](registry.toml). Regenerate after registry or corpus changes:

```bash
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml \
  --write-document-registry docs/DOCUMENT_REGISTRY.md
```

## Truth classes (merged onto every doc)

| Value | Meaning |
|-------|---------|
| `normative` | How the system is supposed to work (specs, binding ADRs, control policy). |
| `descriptive` | What exists or what the repo claims today (inventories, architecture narrative). |
| `operational` | How to run, verify, or recover (runbooks, checklists). |
| `historical` | Point-in-time or superseded; not current truth. Prefer `docs/archive/YYYY/`. |
| `draft` | Proposal / WIP; do not treat as frozen. |

## Canonical vs non-canonical

- **Control-plane canonical:** exactly the paths in `[control].canonical_doc_paths` in `registry.toml`. Those files must carry `Status` / `Canonical` / `Last Reviewed` in YAML (or HTML comment) matching the merged registry row.
- **Elsewhere:** `canonical = "yes"` in a `[docs."…"]` row does **not** mean control-plane canon; prefer `canonical = "no"` for domain hubs unless you are intentionally promoting a doc (and then add it to `canonical_doc_paths`).
- **Non-canonical:** still useful, but not in the four-path control set, or historical/draft.
- **History:** paths under `docs/archive/` are historical by convention; active paths with `Status: historical` or registry `truth_class = historical` must not override canon without explicit review.

## Canonical documentation (control plane)

| Path | Role | Truth class |
|------|------|-------------|
| `docs/INDEX.md` | reference | operational |
| `docs/STATE.md` | status | descriptive |
| `docs/ARCHITECTURE.md` | architecture | normative |
| `docs/DOCUMENTATION_CONTROL_SYSTEM.md` | internal | normative |

## Corpus coverage

- **Markdown files under docs/**: 638
- **By truth_class (merged):**
  - `descriptive`: 375
  - `draft`: 33
  - `historical`: 69
  - `normative`: 20
  - `operational`: 141

## Schema and policy

- Full field definitions: [`internal/documentation-control-system/02_DOCUMENT_REGISTRY_SPEC.md`](internal/documentation-control-system/02_DOCUMENT_REGISTRY_SPEC.md)
- Placement rules: [`internal/documentation-control-system/05_DOC_PLACEMENT_AND_TRUTH_CLASS_RULES.md`](internal/documentation-control-system/05_DOC_PLACEMENT_AND_TRUTH_CLASS_RULES.md)
- Top-level control spec: [`DOCUMENTATION_CONTROL_SYSTEM.md`](DOCUMENTATION_CONTROL_SYSTEM.md)

## Defaults vs explicit `[docs."…"]` rows

1. **Prefix defaults:** `[[doc_path_defaults]]` apply by longest matching `prefix` (e.g. `docs/plans/`). Every file under `docs/**/*.md` gets a merged record.
2. **Explicit row:** add `[docs."docs/relative/path.md"]` only when you need to override defaults or attach `title`, `superseded_by`, audiences, etc.
3. **Do not** fork a second registry format; extend `registry.toml` only.

## How to update the registry (checklist)

1. Pick the correct directory (see maintenance guide + placement rules doc).
2. Add or adjust `[[doc_path_defaults]]` if a **whole subtree** should change class/role.
3. Otherwise add `[docs."docs/…"]` with `truth_class`, `role`, `canonical`, `last_reviewed` / `last_updated`, `recommended_action`, `domain_tags` as needed.
4. For new top-level folders under `docs/`, extend `[control].allowlisted_docs_subdirs` or CI will fail.
5. Run `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` (and `--strict` before merge if you touched contested areas).
6. Refresh this file with `--write-document-registry` when you want counts and the canon table up to date.

## CI vs local enforcement

What fails in CI, what warns, what `--strict` adds, and when to promote strict to CI: [`DOCUMENTATION_MAINTENANCE.md`](DOCUMENTATION_MAINTENANCE.md#documentation-control-plane-start-here). Policy toggles live in `[control.enforcement]` in `registry.toml`.

## Registry coverage (this snapshot)

- **Files scanned:** 638 Markdown files under `docs/`
- **Explicit `[docs."…"]` rows under `docs/`:** 239 (remaining files rely on `[[doc_path_defaults]]` only)
