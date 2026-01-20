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
- CI status docs last updated 2025-12-18; re-run CI baseline to refresh timestamps.
- Local CI baseline 2026-01-20 passed with rustc 1.89.0 override in icn/. (fmt/clippy/test).
- K3s/self-hosted runner node down (2026-01-20 per user report); deploy workflows blocked.
- Demo status doc (2025-12-18) reports daemon/gateway/UI running; re-validate before presenting.
- Homelab deployment (docs/HOMELAB_DEPLOYMENT.md): K3s cluster running with self-hosted runner and monitoring stack (deployed 2025-12-03).


## Code review findings (2026-01-20)
- Repo-wide TODO scan across icn/ captured items below.
- icn-core supervisor: personhood store and anchor rate limit config are TODOs (icn-core/src/supervisor/mod.rs:499-500).
- icn-core governance handlers: treasury disbursement/redemption/bond issuance TODOs (icn-core/src/supervisor/governance_handlers.rs:392-419).
- icn-core notifications: evidence validator wiring TODO (icn-core/src/supervisor/init_notifications.rs:89).
- icn-governance proposal cleanup: metrics TODO and archive store stub (icn-governance/src/proposal_cleanup.rs:303-357).
- icn-gateway agreements: compensation_model parsing and terms fields TODOs (icn-gateway/src/api/agreements.rs:255-275, 721-725).
- icn-gateway governance domains: performance note for large domain counts (icn-gateway/src/api/governance.rs:174-176).
- icn-gateway SDIS enrollment: temporary VUI hash and steward rate limit TODOs (icn-gateway/src/api/sdis/simple_enrollment.rs:11-77, 477-484).
- icn-steward: message-level signatures TODO for VUI sync (icn-steward/src/actor.rs:679-689).
- icn-compute: federated task settlement TODO (icn-compute/src/actor/placement.rs:983-988).
- icn-gossip storage challenge: missing ContentNotFound response TODO (icn-gossip/src/handlers/storage_challenge.rs:94).
- icnctl: automatic daemon start TODO (bins/icnctl/src/main.rs:6532-6541).
- deny.toml: add proper license field to all crates TODO (icn/deny.toml:68).

## References
- docs/dev-journal/ROADMAP.md
- docs/ci/CI_CURRENT_STATUS.md
- docs/status/TESTS_FIXED_STATUS.md
- docs/status/CURRENT_SYSTEM_STATUS.md
- docs/HOMELAB_DEPLOYMENT.md
- deploy/README.md
