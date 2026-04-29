---
Status: operational
Canonical: no
Last Reviewed: 2026-04-29
---

# Source Tree Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map describes *what is where*, not *what works today*.

The repo is a monorepo with two important roots: the **monorepo root** (everything in this directory tree) and the **Rust workspace root** at `icn/`. Mixing them up causes subtle failures.

```
git rev-parse --show-toplevel    # repo root
test -f Cargo.toml && echo "Rust root" || echo "Not Rust root"
```

## Top-level surfaces

### Code

| Path | What it is | Maturity / caution | Where to start |
|---|---|---|---|
| `icn/` | The Rust workspace. Contains `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/`, `apps/`, `bins/`. **All Rust commands run from here.** | Mature. Toolchain is pinned; do not upgrade. | [`rust-workspace-map.md`](rust-workspace-map.md), [`AGENTS.md`](../../../AGENTS.md) |
| `icn/crates/` | 35 Rust crates: kernel mechanics, identity, networking, ledger, governance, gateway, observability, etc. | Mature; see kernel/app boundary docs. | [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md) |
| `icn/apps/` | Runtime-integrated app crates (`charter`, `governance`, `membership`, `ledger`). PolicyOracle implementations live here. | Mature. New runtime apps go here, **not** at top-level `apps/`. | [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md) |
| `icn/bins/` | Binaries: `icnd` (daemon), `icnctl` (CLI), `icn-console` (TUI). | Mature. | `cargo run -p icnctl -- --help` |
| `apps/` (top level) | Example or tool crates that have not yet been migrated to `icn/apps/`. Per `AGENTS.md`, do not add new runtime-integrated crates here. | Frozen for new work. | [`AGENTS.md`](../../../AGENTS.md) §"App topology rule" |
| `sdk/typescript/` | TypeScript SDK. Generates types from the gateway OpenAPI. | Active. | `cd sdk/typescript && npm ci && npm run build` |
| `sdk/react-native/` | React Native SDK. Mobile member-facing surfaces. | Earlier; mobile UX spec at [`docs/mobile/icn-mobile-ux-spec-v1.md`](../../mobile/icn-mobile-ux-spec-v1.md) supersedes earlier docs. | `cd sdk/react-native && npm test` |
| `web/pilot-ui/` | Vanilla-JS PWA used in the pilot demo. | Active. | `cd web/pilot-ui && npm ci && npm run test` |
| `web/dashboard/` | Static dashboard. | Active. | `cd web/dashboard && npm run dev` |
| `website/` | Public site for [`intercooperative.network`](https://intercooperative.network). | Active. | `website/README.md` |

### Documentation, contracts, institution data

| Path | What it is | Where to start |
|---|---|---|
| `docs/` | Architecture, reference, contributor, and operator documentation. | [`docs/INDEX.md`](../../INDEX.md), [`docs/STATE.md`](../../STATE.md), [`docs-control-map.md`](docs-control-map.md) |
| `docs/adr/` | Architecture decision records. | [`docs/INDEX.md`](../../INDEX.md) §"Architecture Decision Records" |
| `docs/rfcs/` | Proposals not yet decided. | [`docs/INDEX.md`](../../INDEX.md) |
| `institutions/` | In-monorepo institution packages. Boundary-clean for later extraction. | `institutions/README.md` |
| `institutions/nycn/` | The in-monorepo NYCN institution package (`bootstrap.yaml`, `charter/`, `config/`, `definitions/`, `seed/`, etc.). **Distinct from the separate operator repo at `fahertym/nycn`** — see [`pilot-and-nycn-map.md`](pilot-and-nycn-map.md). | `institutions/nycn/README.md` |
| `contracts/` | CCL contract templates and policies. | `contracts/templates/` (worker-coop, consumer-coop, housing-coop, community-org, federation) |
| `demo/` | Demo orchestration: scripts, runbooks, fixtures. | `demo/RUNBOOK.md` |

### Operations and deployment

| Path | What it is | Where to start |
|---|---|---|
| `deploy/` | Deployment manifests: native/systemd, Docker Compose, Kubernetes, Helm. | [`deploy/README.md`](../../../deploy/README.md) |
| `ops/` | Operations tooling, including `ops/mcp` (the MCP server used by Claude Code sessions). | [`ci-ops-deploy-map.md`](ci-ops-deploy-map.md) |
| `monitoring/` | Prometheus / Grafana configuration. | [`docs/operations/deployment/HOMELAB_DEPLOYMENT.md`](../../operations/deployment/HOMELAB_DEPLOYMENT.md), [`monitoring/`](../../../monitoring/) |
| `docker/` | Dockerfiles and image build context. | `docker/README.md` if present, or `Dockerfile.fast` |
| `scripts/` | Repo-wide shell scripts (multi-agent worktree helper, demo helpers, sync utilities). | `scripts/` listing |
| `config/` | Sample/runtime configuration. | `config/` listing |
| `examples/` | Worked examples (policies, configurations). | `examples/policies/README.md` |
| `sims/` | Simulations (economic, network) for validating mechanisms. | `sims/README.md` if present |

### CI and agent infrastructure

| Path | What it is | Where to start |
|---|---|---|
| `.github/workflows/` | 16 GitHub Actions workflows: CI, docs maintenance, release, security audit, npm publish, website deploy, etc. | [`ci-ops-deploy-map.md`](ci-ops-deploy-map.md) |
| `.github/agents/` | Specialized Copilot/agent definitions (orchestrator + 21 specialists). | `.github/agents/README.md` |
| `.github/ISSUE_POLICY.md` | Issue taxonomy and labeling rules. | [`.github/ISSUE_POLICY.md`](../../../.github/ISSUE_POLICY.md) |

## What lives at the repo root (only)

```
README.md         CONTRIBUTING.md   AGENTS.md
CHANGELOG.md      CLAUDE.md         CODE_OF_CONDUCT.md
LICENSE
```

Per `AGENTS.md` and `CLAUDE.md`, do not add documentation files at the repo root. New docs go under `docs/`. The five files above plus `LICENSE` are the only sanctioned root markdown.

## "Where am I?" quick check

```bash
git rev-parse --show-toplevel                       # repo root
ls -d icn/Cargo.toml && echo "Rust workspace OK"    # confirms Rust root
git branch --show-current
```

For Rust commands, `cd icn` first. For SDK/OpenAPI commands, run from the SDK directory under `sdk/`.
