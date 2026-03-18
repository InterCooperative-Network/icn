# Reading Map

This map links each module to the highest-signal files and docs.

## Module 0: Setup and tooling
- `README.md`
- `docs/DEV_ENVIRONMENT.md`
- `CONTRIBUTING.md`
- `scripts/dev-setup.sh`

## Module 1: Rust fundamentals
- `icn/` crate structure
- `icn/crates/*/src/lib.rs` (public APIs)

## Module 2: ICN architecture overview
- `docs/ARCHITECTURE.md`
- `docs/README.md`
- `docs/architecture/`

## Module 3: Runtime and actor model
- `icn/bins/icnd/src/main.rs`
- `icn/crates/icn-core/src/runtime.rs`
- `icn/crates/icn-core/src/supervisor/mod.rs`

## Module 4: Identity and trust
- `icn/crates/icn-identity/`
- `icn/crates/icn-trust/`
- `docs/ARCHITECTURE.md` (Identity, Trust sections)
- `docs/multi-device-identity-design.md`

## Module 5: Network and gossip
- `icn/crates/icn-net/`
- `icn/crates/icn-gossip/`
- `docs/ARCHITECTURE.md` (Network, Gossip sections)
- `docs/gossip-signed-envelope-migration.md`

## Module 6: Ledger and contracts
- `icn/crates/icn-ledger/`
- `icn/crates/icn-ccl/`
- `docs/ARCHITECTURE.md` (Ledger, Contracts sections)
- `docs/governance-primitives.md`

## Module 7: Gateway API and SDK
- `icn/crates/icn-gateway/README.md`
- `sdk/typescript/README.md`
- `docs/api/`

## Module 8: Web UI integration
- `web/pilot-ui/README.md`
- `web/pilot-ui/GETTING-STARTED.md`
- `web/pilot-ui/SUMMARY.md`

## Module 9: Operations and deployment
- `deploy/README.md`
- `config/README.md`
- `docs/security/production-hardening.md`
- `docs/ops/`

## Module 10: Contributor workflow
- `CONTRIBUTING.md`
- `docs/ci/CI_ALL_GREEN_REPORT.md`
- `docs/testing/`

## Module 11: Federation (Advanced)
- `icn/crates/icn-federation/`
- `docs/ARCHITECTURE.md` (Federation section)

## Module 12: Observability and metrics
- `icn/crates/icn-obs/`
- `monitoring/`
- `docs/security/production-hardening.md`

## Module 13: Security and privacy
- `icn/crates/icn-security/`
- `icn/crates/icn-privacy/`
- `icn/crates/icn-net/src/envelope.rs`
- `docs/security/production-hardening.md`

## Module 14: Governance and CCL deep dive
- `icn/crates/icn-governance/`
- `icn/crates/icn-ccl/`
- `docs/governance-primitives.md`
