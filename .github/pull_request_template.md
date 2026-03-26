# Pull Request

## Summary
What changed and why?

## Related
- Issues: <!-- Fixes # / Closes # -->
- Specs / ADRs / RFCs: <!-- paths under docs/ -->

## Work mode
- [ ] Discovery output
- [ ] Delivery tranche
- [ ] Mixed, but bounded

## Risk
What could break? Edge cases or known limitations?

## Documentation control (required when `docs/**` changes)
- [ ] New or moved docs declare **truth class** and **role** (see `docs/DOCUMENTATION_CONTROL_SYSTEM.md`; vocabulary: `normative` / `descriptive` / `operational` / `historical` / `draft`)
- [ ] `docs/registry.toml` updated (explicit `[docs."path"]` entry when defaults are wrong)
- [ ] Placement matches allowlisted `docs/` subtree (see `[control].allowlisted_docs_subdirs` in `registry.toml`)
- [ ] Control-plane canonical paths (exact set: `[control].canonical_doc_paths` — same four files as below) unchanged **or** YAML headers + merged registry row updated **together**
- [ ] Ran: `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` (add `--strict` if you touched registry structure, canon paths, or supersession; see `docs/DOCUMENTATION_MAINTENANCE.md`)
- [ ] If `DOCUMENT_REGISTRY.md` should reflect corpus stats: same command with `--write-document-registry docs/DOCUMENT_REGISTRY.md`

## Structural changes (docs migrations)
- Files added:
- Files moved:
- Files archived:
- Files marked superseded:

## Type of change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation
- [ ] Tests
- [ ] Refactor (no behavior change)

## Verification
What you actually ran (commands and result):
- [ ] `python3 docs/scripts/doc_control_check.py` (if docs touched)
- [ ] Tests / clippy / fmt as applicable

## Non-goals
What this PR intentionally avoids.

## Remaining unknowns
What still needs human review?
