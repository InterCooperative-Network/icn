---
name: icn-shipping
description: Prepare ICN branches for merge with required checks, doc/spec sync, clean git hygiene, and high-quality PR metadata.
metadata:
  short-description: ICN ship checklist
---

# ICN Shipping Skill (Codex)

Use this skill when preparing a branch for merge.

## Purpose

- Ensure code, docs, and PR metadata are release-quality.
- Prevent false-green claims and drift between behavior and docs.

## Preconditions

- Implementation changes are complete.
- Required checks for touched areas are known from `AGENTS.md`.

## Shipping Checklist

1. Verification
   - Run required checks for touched paths.
   - Capture command outputs before claiming success.
2. Diff Quality
   - Remove unrelated edits.
   - Keep patch focused and reviewable.
3. Documentation
   - If semantics changed, update docs/spec in same PR.
4. Git Hygiene
   - `git status --short` is clean except intended files.
   - Commit message is clear and scoped.
   - Branch pushed to remote.
5. Pull Request Quality
   - Title: concise, behavior-oriented.
   - Body includes:
     - Problem
     - What changed
     - Invariants/safety notes
     - Verification commands and outcomes
     - Follow-ups (if any)

## Standard Verification Commands

- Rust crates:
  - `cd icn && cargo fmt --all --check`
  - `cd icn && cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cd icn && cargo test` (or narrower scoped tests with justification)
- Docs-only changes:
  - Link/consistency checks with `rg` and manual spot validation.

## Stop Conditions

Do not ship if any required check fails, invariants are unclear, or docs drift is unresolved.
