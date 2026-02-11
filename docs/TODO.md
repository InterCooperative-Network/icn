# ICN TODO (ordered)

1) ✅ DONE - Standardize Rust toolchain to 1.88.0:
   - rust-toolchain.toml added in icn/ with toolchain = "1.88.0"
   Acceptance: repo clearly specifies required rustc and baseline runs without manual overrides.

2) Refresh status docs with current timestamps and outcomes:
   - docs/ci/CI_CURRENT_STATUS.md (updated 2026-01-20)
   - docs/status/FINAL_SESSION_STATUS.md
   - docs/status/TESTS_FIXED_STATUS.md
   - docs/status/CURRENT_SYSTEM_STATUS.md
   Acceptance: each file reflects latest verification date and status.

3) Demo runtime validation (if demo is needed):
   - Start icnd/gateway/pilot-ui
   - Verify /v1/health returns 200 and UI loads
   Acceptance: health check succeeds; UI renders without console errors.

4) PRs:
   - chore/opencode-pilot-ui branch (opencode + pilot-ui changes)
   - docs/status-sync-2026-01-19 branch (status doc sync + STATE/TODO updates)
   Acceptance: PRs opened with summaries and reviewers assigned.

5) Phase 19 planning (Release Infrastructure):
   - #183 Binary signing and SBOM generation
   - #184 Pre-deployment health validation
   - #186 Benchmark regression detection in CI
   - #223 Horizontal Pod Autoscaling for icnd
   - #224 Backup validation tests
   Acceptance: choose next issue and write a short execution plan.

6) ✅ DONE (2026-02-11) - Gateway API changes, regenerate TS types:
   - cd icn && ./target/debug/icnctl api export-openapi -o ../docs/api/openapi.generated.yaml
   - cd sdk/typescript && npm run generate-types && npm run check-types
   Acceptance: generated types are committed and check-types passes.
   Note: Updated for Sprint 8-10 Economics Consolidation (new receipt endpoints + ledger provenance).

7) Code review follow-ups:
   - Consider secondary index/unsorted pagination for large domain counts (icn-gateway/src/api/governance.rs:174-176).
   - Wire personhood store + anchor rate limit config (icn-core/src/supervisor/mod.rs:499-500).
   - Implement treasury disbursement/redemption/bond issuance (icn-core/src/supervisor/governance_handlers.rs:392-419).
   - Wire evidence validator in trust attestation handling (icn-core/src/supervisor/init_notifications.rs:89).
   - Implement governance proposal archive storage (icn-governance/src/proposal_cleanup.rs:319-357).
   - Parse agreements compensation_model + terms fields (icn-gateway/src/api/agreements.rs:255-275, 721-725).
   - Replace temporary VUI hash + enforce steward vouch rate limits (icn-gateway/src/api/sdis/simple_enrollment.rs).
   - Add steward message-level signatures for VUI sync (icn-steward/src/actor.rs:679-689).
   - Trigger federated task settlement (icn-compute/src/actor/placement.rs:983-988).
   - Add ContentNotFound response for storage challenges (icn-gossip/src/handlers/storage_challenge.rs:94).
   - Implement daemon auto-start in icnctl (bins/icnctl/src/main.rs:6532).
   - Address license field TODO in deny.toml (icn/deny.toml:68).
   Acceptance: each TODO above is either implemented or tracked as an issue with owner and target phase.
