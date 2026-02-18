# ICN TODO (ordered)

Last reviewed: 2026-02-18

1) Sprint: Pilot Vertical Slice Hardening (active)
   - #1214 `refactor(api): governance + ledger shared service parity in icn-api`
   - #1221 `refactor(gateway): enforce adapter/view-model boundary for decision+ledger reads`
   - #1220 `refactor(governance): migrate pilot-required proposal types off legacy handlers`
   - #1222 `docs(ops): pilot runbook + one-command smoke verification`
   Acceptance: all four issues merged with required crate checks and invariant-safe behavior.

2) Docs reality-sync (active, non-archive/session scope)
   - Fix broken links and stale absolute paths under `docs/`
   - Normalize present-tense status claims to dated snapshot language where not re-verified
   - Keep canonical current pointers in `docs/INDEX.md`, `docs/README.md`, `docs/STATE.md`
   Acceptance: `.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh` reports no broken links in scanned docs.

3) Verification routing discipline
   - Rust changes: run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and targeted `cargo test` scopes
   - Gateway API changes: run `cargo test -p icn-gateway --features sled-storage`
   - If API schema changes: regenerate OpenAPI + TypeScript generated types
   Acceptance: verification output recorded in PR and matches touched subsystems.

4) Track remaining pilot-completion backlog separately
   - Milestone `Pilot Completion` retains open items (for example #1099 epic and child issues)
   - Keep this backlog distinct from active hardening sprint to avoid scope mixing
   Acceptance: sprint PRs link explicitly to milestone scope.
