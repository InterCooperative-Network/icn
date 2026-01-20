# ICN State (living doc)

## Architecture notes
- Repo root is not a Cargo workspace; Rust workspace lives in icn/.
- Workspace members (icn/Cargo.toml):
  - Crates: icn-core, icn-identity, icn-trust, icn-net, icn-gossip, icn-ledger, icn-ccl, icn-store, icn-rpc, icn-obs, icn-snapshot, icn-testkit, icn-gateway, icn-governance, icn-compute, icn-security, icn-time, icn-privacy, icn-federation, icn-crypto-pq, icn-steward, icn-zkp, icn-community, icn-coop, icn-entity, icn-encoding.
  - Binaries: icnd, icnctl, icn-console.
- Web UI:
  - web/pilot-ui: static PWA; tests via npm run test, test:e2e, test:a11y, test:all.
  - web/dashboard: static admin dashboard; start via python3 -m http.server 8080.
- SDKs:
  - sdk/typescript: npm run build/dev/test/lint; generate-types uses docs/api/openapi.generated.yaml.
  - sdk/react-native: npm run build/dev/test.
- Deployment options (deploy/README.md): native/systemd, Docker Compose, Kubernetes, Helm. Secrets required (JWT/Grafana/keystore).

## Decisions
- Mutual TLS with client certificates enabled (2025-12-18 session).
- DID-TLS binding verification enabled; IdentityBundle TLS certs used by SessionManager.
- Some QUIC/chaos tests are ignored in CI due to timing; run manually as needed.

## Constraints
- Run Rust build/test commands from icn/.
- Tokio async only; avoid blocking operations in async paths.
- No panics in protocol/network/actor runtime paths.
- Demo status docs note STUN discovery disabled for local-only testing; re-validate before demo.

## Current status (2026-01-20 snapshot)
- Roadmap (docs/dev-journal/ROADMAP.md): Phase 18 complete, Phase 19 (Release Infrastructure) next; implementation ~75% (272K LOC, 2,287 tests).
- CI status docs (docs/ci/CI_CURRENT_STATUS.md) last updated 2026-01-20.
- Local CI baseline 2026-01-20 passed with rustc 1.89.0 override in icn/. (fmt/clippy/test).
- K3s/self-hosted runner node down (2026-01-20 per user report); deploy workflows blocked.
- Demo status doc (2025-12-18) reports daemon/gateway/UI running; re-validate before presenting.
- Homelab deployment (docs/HOMELAB_DEPLOYMENT.md): K3s cluster running with self-hosted runner and monitoring stack (deployed 2025-12-03).


## Code review findings (2026-01-20)
- Repo-wide TODO scan across icn/ captured items below.
- icn-core supervisor: personhood store and anchor rate limit config are TODOs (icn-core/src/supervisor/mod.rs:499-500). [Requires config schema extension]
- ✅ icn-core governance handlers: treasury disbursement/redemption/bond issuance implemented (icn-core/src/supervisor/governance_handlers.rs).
- ✅ icn-core notifications: evidence validator wired up (icn-core/src/supervisor/init_notifications.rs).
- ✅ icn-governance proposal cleanup: metrics implemented, archive store stubbed (icn-governance/src/proposal_cleanup.rs).
- ✅ icn-gateway agreements: compensation_model and terms parsing implemented (icn-gateway/src/api/agreements.rs).
- icn-gateway governance domains: performance note for large domain counts (icn-gateway/src/api/governance.rs:174-176).
- icn-gateway SDIS enrollment: temporary VUI hash and steward rate limit TODOs (icn-gateway/src/api/sdis/simple_enrollment.rs:11-77, 477-484).
- ✅ icn-steward: message-level signatures implemented for VUI sync (icn-steward/src/gossip.rs).
- icn-compute: federated task settlement TODO (icn-compute/src/actor/placement.rs:983-988).
- ✅ icn-gossip storage challenge: ContentNotFound response implemented (icn-gossip/src/handlers/storage_challenge.rs).
- icnctl: automatic daemon start TODO (bins/icnctl/src/main.rs:6532-6541).
- ✅ deny.toml: license field added to all crates (icn/deny.toml).

## Code quality notes (2026-01-20)
- No dead code warnings from clippy or rustc.
- Large module candidates for future modularization:
  - bins/icnctl/src/main.rs (9445 lines) - CLI should be split into subcommand modules
  - crates/icn-ledger/src/ledger.rs (5447 lines)
  - crates/icn-gateway/src/api/governance.rs (4650 lines)
  - crates/icn-core/src/supervisor/governance_handlers.rs (4243 lines)

## References
- docs/dev-journal/ROADMAP.md
- docs/ci/CI_CURRENT_STATUS.md
- docs/status/TESTS_FIXED_STATUS.md
- docs/status/CURRENT_SYSTEM_STATUS.md
- docs/HOMELAB_DEPLOYMENT.md
- deploy/README.md
