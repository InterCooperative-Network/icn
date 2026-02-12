---
name: icn-docs-reality-sync
description: Audit ICN documentation against code, manifests, and CI workflows; produce a dated mismatch report; apply minimal doc fixes; and verify alignment. Use when requests include unstaling docs, aligning docs to code, doc drift cleanup, architecture/status accuracy checks, or documentation truth-sync work.
---

# ICN Docs Reality Sync

Run a code-first documentation alignment workflow. Treat documentation as derived artifacts.

## Core Rules

- Use code and workflow files as canonical truth.
- Prefer minimal, reviewable doc diffs.
- Never claim "current" status without an explicit date and evidence path.
- If docs and code disagree, update docs to match code or mark docs historical.
- Preserve ICN invariants and topology rules from `AGENTS.md`.

## Canonical Sources

Read these first:

1. `AGENTS.md`
2. `icn/Cargo.toml`
3. `.github/workflows/*.yml`
4. Entry points and wiring:
- `icn/bins/icnd/Cargo.toml`
- `icn/bins/icnctl/Cargo.toml`
- `icn/bins/icn-console/Cargo.toml`
5. Subsystem manifests:
- `icn/crates/*/Cargo.toml`
- `icn/apps/*/Cargo.toml`
- `apps/*/Cargo.toml`
- `sdk/*/package.json`
- `web/*/package.json`
6. Documentation index/state:
- `docs/INDEX.md`
- `docs/README.md`
- `docs/STATE.md`

## Workflow

1. Build inventory.
- Map docs to subsystem owners and volatility class:
  - Stable: architecture/spec/reference
  - Volatile: status/roadmap/session reports/CI snapshots

2. Detect mismatches.
- Broken internal links and bad file paths
- Missing/moved roadmap/status references
- Command/path drift (especially `cd icn` requirement)
- Topology drift (`apps/` vs `icn/apps/`)
- CI claim drift (blocking vs continue-on-error)

3. Score each mismatch.
- `blocker`: wrong security/architecture/CI claims; broken primary navigation
- `high`: wrong commands/paths; contradictory "current status"
- `medium`: stale metrics/dates lacking context; low-risk wording drift

4. Patch in small batches.
- Batch A: nav + broken links
- Batch B: architecture/topology truth
- Batch C: commands/verification guidance
- Batch D: status normalization and date banners
- Batch E: subsystem READMEs

5. Verify after each batch.
- Run link/path scan
- Re-open changed docs and truth sources
- Confirm no newly introduced drift

## Recursive Improvement Trigger

Use this loop continuously while editing:

1. Plan: state intended doc truth source for each change.
2. Check: compare the change against code/workflow files.
3. Score: `accuracy`, `completeness`, `consistency`, `verifiability` from 0-5.
4. Trigger: if any score is below 4, stop and correct before continuing.
5. Re-check: rerun checks and rescore until all are at least 4.

Keep an audit ledger line per updated doc:

`doc_path | truth_source | verification_command | result | reviewed_on(YYYY-MM-DD)`

## Output Contract

Return:

1. Dated mismatch table by severity
2. Files changed with short rationale
3. Verification commands run and outcomes
4. Remaining accepted debt with explicit owners/next action

## Resources

- Use `references/reality-sources.md` for repo-specific truth mapping.
- Use `references/mismatch-rubric.md` for severity and scoring rules.
- Use `scripts/doc_reality_scan.sh` for deterministic link/path drift detection.
