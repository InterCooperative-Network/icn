---
Status: operational
Canonical: no
Last Reviewed: 2026-05-19
---

# ICN Repository Map

> Cross-repo orientation. For **inside-the-icn-repo** structure see [`source-tree-map.md`](source-tree-map.md), [`rust-workspace-map.md`](rust-workspace-map.md), and [`repo-atlas.md`](repo-atlas.md).

This document maps the repositories under the `InterCooperative-Network` GitHub organization to their roles in the ICN ecosystem. Visibility, scope, and merge-order rules are noted so a reader knows where a given concern actually belongs.

## Repositories at a glance

| Repository | Visibility | Role | Notes |
|---|---|---|---|
| [`icn`](https://github.com/InterCooperative-Network/icn) | **Public** | Canonical substrate. Kernel, apps, daemon, CLI, gateway, TypeScript SDK, public website. | This repo. The truth lives here. |
| [`nycn`](https://github.com/InterCooperative-Network/nycn) | Private | NYCN institution ecosystem package. First application package built on ICN. | First "institution-in-a-box" seed. Downstream of `icn`. |
| [`icn-learn`](https://github.com/InterCooperative-Network/icn-learn) | Private | ICN Academy. Role-based learning and onboarding. | Teaching layer. Downstream of `icn` for facts, `nycn` for application examples. |
| [`icn-community-bridge`](https://github.com/InterCooperative-Network/icn-community-bridge) | Private | Discord-to-Matrix onboarding bridge. Scaffold/docs only, not deployed. | See `docs/BRIDGE_POLICY.md` in that repo. |

If you came in expecting a separate `website` repository: the public site lives **inside this repo** at [`website/`](../../../website/). Treat it as part of `icn`, not a separate property.

## Roles by repo

### `icn` (this repo) — canonical substrate

This is the public, citable, archivable core. Everything ICN claims to be technically lives here.

- **Rust workspace**: [`icn/`](../../../icn/) — kernel crates, app crates, daemons, CLI, gateway. See [`rust-workspace-map.md`](rust-workspace-map.md).
- **TypeScript SDK**: [`sdk/typescript/`](../../../sdk/typescript/).
- **Public website**: [`website/`](../../../website/) — what `intercooperative.network` serves. See [`website-truth-map.md`](website-truth-map.md).
- **Documentation**: [`docs/`](../../) — architecture, design, spec, planning, strategy. Navigate via [`docs/INDEX.md`](../../INDEX.md).
- **Deployment manifests**: [`deploy/`](../../../deploy/) — K3s and homelab.

ICN-canonical changes (kernel primitives, public contracts, claims on `intercooperative.network`) start here.

### `nycn` — institution ecosystem package

NYCN is the first concrete cooperative-network application built on top of ICN. It is **not** a fork; it consumes ICN primitives and adds institution-specific meaning (governance flows, member journeys, organizing tooling).

The kernel/app meaning firewall (see [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md)) is the contract between this repo and NYCN: ICN owns primitives; NYCN owns institution semantics.

### `icn-learn` — Academy / teaching layer

Role-based learning material for organizers, members, contributors, and operators. Authoritative for *how to learn ICN and NYCN*; not authoritative for the facts being taught.

When the facts in `icn-learn` and the source repos disagree, the source repos win and `icn-learn` is the one that has to update.

### `icn-community-bridge` — Discord ↔ Matrix bridge

A community-coordination utility, currently scaffold-and-docs only. Not deployed, not load-bearing. Treated as a small operational tool, not as a substrate component. See that repo's `docs/BRIDGE_POLICY.md`.

## Merge order across repos

When a change crosses repo boundaries, the canonical merge order is:

1. **`icn`** — primitives, public contracts, public claims.
2. **`nycn`** — application semantics built on those primitives.
3. **`icn-learn`** — teaching that reflects what shipped above.

See [`ops/coordination/PR_STACK_PROTOCOL.md`](../../../ops/coordination/PR_STACK_PROTOCOL.md) and the PR template's "Cross-repo dependency status" section. Out-of-order merges create teaching that describes something that does not yet exist, or institution behavior whose primitive moved underneath it.

## Truth and confidentiality rules per repo

| Repo | Truth class | What lives there that is **not** elsewhere |
|---|---|---|
| `icn` | Public, canonical | The substrate itself; the only place public claims can be grounded. |
| `nycn` | Private, application-canonical | Institution-specific decisions, member data models, organizing flows. Not exported to `icn`. |
| `icn-learn` | Private, derivative | Curriculum, exercises, narrative. Re-derives from `icn`/`nycn`; not a source of truth. |
| `icn-community-bridge` | Private, operational | Bridge config and policy. Operational only. |

If a fact appears in `nycn` or `icn-learn` and is meant to be public, it has to find its way back to `icn` (typically `docs/`, `website/`, or a code change) before it can be cited externally.

## Adjacent identifiers

A few external surfaces are referenced from ICN material but are not GitHub repos:

- **`intercooperative.network`** — the public site, built from [`website/`](../../../website/) in this repo.
- **GitHub Sponsors** — `github.com/sponsors/InterCooperative-Network`. See [`.github/FUNDING.yml`](../../../.github/FUNDING.yml).
- **Live K3s deployment** — see [`docs/operations/deployment/HOMELAB_DEPLOYMENT.md`](../../operations/deployment/HOMELAB_DEPLOYMENT.md). Operational, not a separate repo.

## Where this map fits

This document is **navigation**, not state. For state, see [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). For more granular maps inside this repo, see the rest of [`docs/reference/project-index/`](README.md).
