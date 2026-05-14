---
Status: operational
Canonical: no
Last Reviewed: 2026-04-29
---

# CI / Ops / Deploy Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map is a routing layer. Substantive runbooks live under `docs/guides/operations/` and `deploy/`.

## GitHub Actions workflows (`.github/workflows/`)

| Workflow file | What it does |
|---|---|
| `ci.yml` | The main Rust CI. Format check, Clippy, Test, Build Release, TypeScript SDK, Web UI, Accessibility Tests, Kernel Forbidden Dependencies, Firewall Contract Enforcement, Meaning Firewall Check, Regulatory Compliance Linter, Secrets Placeholder Check, Change Scope, Pilot Provenance Invariant, Backup Validation, Test Coverage, Security Audit. |
| `docs-freshness.yml` | "Documentation Maintenance" suite: Documentation Control Plane, Generate Documentation Index, Lint Architecture Doc, Check Documentation Freshness, Documentation CI Summary. |
| `agent-drift-check.yml` | Verifies agent tooling and configuration are not drifting against expectations. |
| `api-types.yml` | Checks TypeScript API types are regenerated when the gateway API changes. |
| `claude-code-review.yml` | Posts a Claude review comment on PRs. **15-minute timeout; failures here are infra flakes and never block merge.** |
| `claude.yml` | Auxiliary Claude integration. |
| `opencode.yml` | OpenCode integration workflow. |
| `docker-build-deploy.yml` | Builds and deploys Docker images for the daemon and gateway. |
| `release.yml` | Release tagging and artifact publishing. |
| `npm-publish.yml` | Publishes the TypeScript SDK to npm. |
| `website-deploy.yml` | Deploys [`intercooperative.network`](https://intercooperative.network). |
| `security-audit.yml` | Cargo / npm dependency audit. |
| `benchmark.yml` | Criterion benchmarks. CI runs on `ubuntu-latest`; absolute numbers compare to CI baselines, not local runs. |
| `fuzz.yml` | Fuzz harnesses. |
| `sync-stats.yml` | Repo statistics sync. |
| `issue-label-enforcer.yml` | Enforces the issue label policy in [`.github/ISSUE_POLICY.md`](../../../.github/ISSUE_POLICY.md). |

### CI failure index (compressed)

For most CI failures, fix the smallest thing CI is asking for. The full table lives in [`CLAUDE.md`](../../../CLAUDE.md) under "CI Failure Index"; the gist:

- **Check API Types Drift** → regenerate TS types (`cd sdk/typescript && npm ci && npm run generate-types`). Commit only `sdk/typescript/src/generated/api-types.ts`.
- **Clippy** → fix the warning in changed code. Don't `#[allow]` unless pre-existing.
- **claude-review** → 15-min infra flake. Never blocks merge.
- **Test Coverage at `pending / 0s`** → queue-stalled, not running. Safe to `--admin` merge when other required gates are green.

## Deploy paths

| Target | Where | Notes |
|---|---|---|
| Native / systemd | [`deploy/icnd.service`](../../../deploy/icnd.service), [`deploy/install.sh`](../../../deploy/install.sh) | Single-node systemd unit. |
| Docker Compose | [`deploy/compose/`](../../../deploy/compose/), [`deploy/docker-compose.yml`](../../../deploy/docker-compose.yml) | Local multi-node deployment. |
| Local devnet | [`deploy/devnet/`](../../../deploy/devnet/) | Local 3-node Docker Compose cluster. See the `devnet` skill at [`.claude/skills/devnet/SKILL.md`](../../../.claude/skills/devnet/SKILL.md). |
| Kubernetes | [`deploy/k8s/`](../../../deploy/k8s/), [`deploy/kubernetes/`](../../../deploy/kubernetes/) | Plain manifests. |
| Helm | [`deploy/helm/`](../../../deploy/helm/) | Helm chart for ICN. |
| **Live K3s cluster** | (homelab) | Deployed since 2025-12-03. See [`docs/operations/deployment/HOMELAB_DEPLOYMENT.md`](../../operations/deployment/HOMELAB_DEPLOYMENT.md). |

```bash
cd deploy/k8s && make full-deploy-dev   # deploy a new image
make status                              # check pod status
make logs                                # tail logs
```

## K3s smoke and proof-path runbooks

| Runbook | Path |
|---|---|
| Local HTTP proof loop (action-item completion receipt) | `docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md` |
| K3s smoke proof closure (operator-authorized) | `docs/dev/NYCN_K3S_PROOF_PATH.md` |
| Pilot smoke runbook | [`docs/guides/operations/pilot-smoke.md`](../../guides/operations/pilot-smoke.md) |
| Backup and recovery | [`docs/guides/operations/backup-and-recovery.md`](../../guides/operations/backup-and-recovery.md) |
| Replication operations | [`docs/guides/operations/replication-operations.md`](../../guides/operations/replication-operations.md) |
| Operations general | [`docs/guides/operations/operations-guide.md`](../../guides/operations/operations-guide.md) |
| Troubleshooting | [`docs/guides/operations/troubleshooting.md`](../../guides/operations/troubleshooting.md) |

## Monitoring

| Surface | Where |
|---|---|
| Prometheus configuration | [`monitoring/`](../../../monitoring/) |
| Atlas-backed Prometheus persistent storage | Configured 2026-04-22 (#1614). See `STATE.md`. |
| Atlas-backed sccache for ci-runner | Configured 2026-04-23 (#1618). |
| Soft pod anti-affinity for ICN daemons | Configured 2026-04-23 (#1619). |

## Verification by area touched

Per [`AGENTS.md`](../../../AGENTS.md):

| If you touched... | Run from `icn/`... |
|---|---|
| Rust crates (`icn/crates/**`) | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test` (scoped to crate or `--workspace --lib`) |
| Gateway API (`icn-gateway`) | `cargo test -p icn-gateway --features sled-storage`; if API changed, regenerate OpenAPI + TS types |
| TypeScript SDK | `cd sdk/typescript && npm ci && npm run build && npm test && npm run lint` |
| React Native SDK | `cd sdk/react-native && npm test && npm run build` |
| Pilot UI | `cd web/pilot-ui && npm run test && npm run test:e2e && npm run test:a11y` |
| Deploy manifests | Manual: ensure no secrets committed; keep placeholders. |
| Documentation | `python3 docs/scripts/doc_control_check.py --strict`; vocabulary scan; `git diff --check`. |

## Port reference

| Service | Port | Protocol | Purpose |
|---|---|---|---|
| Peer transport | 7777 | QUIC/UDP | P2P |
| RPC API | 5601 | HTTP | `icnctl` control |
| Metrics | 9100 | HTTP | Prometheus exporter |
| Gateway / health | **8080** | HTTP | Member-facing REST + WebSocket. **Never 8000.** |

## Where to read deeper

| Topic | Doc |
|---|---|
| Live deployment overview | [`docs/operations/deployment/HOMELAB_DEPLOYMENT.md`](../../operations/deployment/HOMELAB_DEPLOYMENT.md) |
| Production hardening | [`docs/security/production-hardening.md`](../../security/production-hardening.md) |
| Issue and label policy | [`.github/ISSUE_POLICY.md`](../../../.github/ISSUE_POLICY.md) |
| Per-area verification routing (full table) | [`AGENTS.md`](../../../AGENTS.md) §"Change routing" |
