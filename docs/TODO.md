# ICN TODO (ordered)

Last reviewed: 2026-04-10

1) Main CI — stable as of 2026-04-10
   - Security Audit gate (RUSTSEC-2026-0095, wasmtime 24.0.6) resolved via PR #1522
   - Test gate (`test_treasury_nonce_survives_reopen` sled lock) resolved via PR #1522
   - PR #1520 (website cleanup) merged 2026-04-10
   - PR #1521 closed as superseded by #1522

2) Sprint: Pilot Vertical Slice Hardening (complete — all issues closed)
   - #1214 ✅ closed
   - #1221 ✅ closed
   - #1220 ✅ closed
   - #1222 ✅ closed

3) Docs reality-sync (active, non-archive/session scope)
   - Fix broken links and stale absolute paths under `docs/`
   - Normalize present-tense status claims to dated snapshot language where not re-verified
   - Keep canonical current pointers in `docs/INDEX.md`, `docs/README.md`, `docs/STATE.md`
   Acceptance: `.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh` reports no broken links in scanned docs.

4) Verification routing discipline
   - Rust changes: run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and targeted `cargo test` scopes
   - Gateway API changes: run `cargo test -p icn-gateway --features sled-storage`
   - If API schema changes: regenerate OpenAPI + TypeScript generated types
   Acceptance: verification output recorded in PR and matches touched subsystems.

5) Track remaining pilot-completion backlog separately
   - Milestone `Pilot Completion` retains open items (for example #1099 epic and child issues)
   - Keep this backlog distinct from active hardening sprint to avoid scope mixing
   Acceptance: sprint PRs link explicitly to milestone scope.
