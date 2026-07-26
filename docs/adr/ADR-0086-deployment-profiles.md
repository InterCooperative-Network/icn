---
id: "0086"
title: "ICN deployment profiles and public/private infrastructure boundary"
status: "proposed"
date: "2026-07-26"
deciders: ["Matt Faherty"]
tags: ["deployment", "appliance", "systemd", "compose", "kubernetes", "k3s", "operations"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partially implemented"
references:
  - "docs/architecture/DEBIAN_APPLIANCE_MODEL.md"
  - "deploy/appliance/README.md"
  - "deploy/appliance/build-image.sh"
  - "deploy/devnet/docker-compose.yml"
  - "Dockerfile"
  - "deploy/kubernetes/"
  - "deploy/k8s/"
  - "deploy/helm/icn/"
  - "PR #2455 (public/private K3s workflow boundary)"
  - "PR #2456 (assembled appliance demo-payload mode fix and runtime witness)"
---

# ADR-0086: ICN deployment profiles and public/private infrastructure boundary

## Status

`proposed` — this record states the deployment decision for review. It does not
adopt itself, authorize a production deployment, or certify any profile as
production-ready.

`implementation_status: partially implemented` — repository paths exist for all
four profiles, but only the assembled Debian appliance has a current, retained
build-and-boot witness. Backup/restoration, artifact signing, reproducibility,
two-node institutional operation, and generic Kubernetes reconciliation remain
incomplete.

## Context

ICN currently exposes several overlapping ways to run `icnd`: a Debian appliance
builder, multiple Docker Compose stacks, a generic OCI Dockerfile, two
Kubernetes trees, a Helm chart, and native Linux instructions. Until PR #2455,
the public repository also had a workflow that automatically deployed to a
specific private K3s homelab.

Those paths do not have the same purpose or evidence. Treating them as peers has
made private cluster liveness look like product readiness, left developers with
several conflicting Compose entry points, and obscured the custody boundary for
institution-owned identity and durable state.

An exact integrated appliance witness at
`67a6566e2335be108ca69bb5d60e0cfb761e63b5`, which includes merged K3s-lane
retirement `75d157503f168fb534c34cd16edf9bf6b8721254`, established a stronger
factual baseline:

- one Debian QCOW2 was built from repository code and a pinned base-image hash;
- its typed manifest re-hashed the image, base, `icnd`, and `icnctl`;
- a clean VM boot completed first-boot initialization, generated a per-instance
  identity and secrets, started `icnd` under systemd, and returned health;
- a fixture organizer/member loop and its negative capability checks passed;
- the same retained overlay preserved node identity and a completion receipt
  across `icnd` restart and VM reboot;
- the rehearsal workspace itself was process-local and did not survive restart;
- the artifact remained explicitly non-production, unsigned, mutable, and
  dependent on networked package installation during the build.

The decision must preserve those exact claim boundaries.

## Decision

ICN has four deployment profiles with distinct purposes and no implied promotion
between them.

### Profile A: sovereign appliance node

The Debian appliance VM is the canonical ICN node artifact.

Its baseline is one role-neutral image running native systemd services. Each
booted instance creates its own identity and secrets and owns its own durable
state. Post-boot configuration may specialize that image as an institution node,
member node, witness/archive node, domain host, or service host. Separate image
forks are not created for each role unless verified technical evidence requires
them.

The canonical appliance contract requires:

1. a Debian-based QCOW2 or raw image;
2. native systemd startup and restart behavior;
3. per-instance identity and secret generation;
4. institution-owned durable state;
5. explicit genesis, activation, or enrollment;
6. local backup and export;
7. independent restoration, including the configuration secrets needed to open
   the restored keystore;
8. manifest hashes and a release path that states signing and immutability
   honestly;
9. no Kubernetes dependency.

Items 1–5 have partial runtime evidence. Item 6 exists as generic `icnctl`
backup tooling, but it writes a data-directory tar archive and is not yet an
encrypted appliance recovery contract. Item 7 is open: the current data backup
does not include `/etc/icn/icnd.env`, so restoring `/var/lib/icn` alone is not
an independently operable appliance recovery. Items 8–9 are only partially
met: manifests exist and Kubernetes is not required, but appliance artifacts
are not yet signed or reproducibly built.

Fixture/demo services are optional add-ons. Their state and claims are not the
sovereign-node durability contract.

### Profile B: disposable local development network

`deploy/devnet/docker-compose.yml` is the proposed canonical Compose entry point
for fast multi-node development, protocol and gossip tests, integration work,
and disposable demos.

This profile carries no institutional-custody, restoration, production,
adoption, or federation claim. Other Compose files are compatibility or
historical material until a separate cleanup maps or retires them; this ADR does
not delete them.

### Profile C: optional hosted cluster

Kubernetes or K3s is optional operator infrastructure. It is appropriate for
hosted gateways, relays, monitoring, logging, registries, and operators that
already require orchestration.

It is not the canonical sovereign-node path and is not required to run ICN.
Generic Helm/manifests and generic validation may remain public. A Kubernetes
deployment does not, by itself, prove institutional custody, correct authority,
state restoration, peer enrollment, or federation.

The current generic entry points are `deploy/kubernetes/` and
`deploy/helm/icn/`. The separate `deploy/k8s/` tree contains legacy
homelab-specific defaults and is not a generic operator entry point. This ADR
does not migrate or execute that private material; it requires a later bounded
archive, genericization, or private-repository move.

### Profile D: advanced native Linux

Direct Linux installation remains available for experienced operators. It must
preserve the same identity, secret, authority, durable-state, backup, and
restoration invariants as Profile A. It is an advanced installation form, not a
second canonical artifact.

## Public/private repository boundary

The public ICN repository owns generic product artifacts:

- appliance build, verification, and release logic;
- generic OCI image build and validation;
- generic Kubernetes examples and Helm material;
- disposable development environments;
- documentation and non-private test evidence.

A private infrastructure repository owns deployment-specific operations:

- private addresses, registries, SSH targets, node names, credentials, and
  storage topology;
- actual private-cluster rollout and rollback automation;
- private monitoring endpoints and operational schedules;
- the current state of any private deployment.

A merge to public `main` must not automatically SSH into, push to, restart, or
otherwise mutate private infrastructure. Private cluster health must not be a
product build signal. A private repository may invoke a future generic reusable
workflow, but supplies and protects all deployment-specific values itself.

## Evidence and claim matrix

| Claim | Current evidence | Disposition |
|---|---|---|
| Appliance builds and boots | Exact assembled QCOW2 witness at integrated PR #2456 head `67a6566e` | Partially proven for local non-production use |
| Identity survives restart/reboot | Stable retained-overlay identity/config/genesis hashes | Proven for the witnessed bytes |
| Durable receipt survives restart/reboot | Exact completion receipt re-fetched after both transitions | Proven for the witnessed fixture receipt |
| Full demo workspace is durable | Read-only status became uninitialized after process restart | Not proven; currently false |
| Independent appliance restoration | Plain data-directory tar omits the secret-bearing environment file | Open blocker |
| Signed immutable appliance release | Manifest states `signed: false`, `immutable: false` | Not implemented |
| Generic OCI image builds | Local cold build plus merged hosted workflow from PR #2455 | Build evidence only |
| Kubernetes production readiness | Conflicting/stale generic and homelab material | Not claimed |
| Two independent institutions operate | No two-appliance authority/enrollment witness | Not proven |
| Federation | No qualifying two-institution witness | Not proven |

## Consequences

- Product proof centers on an artifact an institution can own, restart, back up,
  and eventually restore without a cluster.
- Developers retain a fast disposable multi-node environment without confusing
  it with institutional custody.
- Hosted operators retain Kubernetes/Helm options without making K3s mandatory.
- Private homelab failures stop presenting as public product failures.
- The repository must reconcile duplicate Compose and Kubernetes paths over time.
- Appliance recovery and signed distribution become explicit product blockers
  rather than undocumented operator assumptions.
- A later acceptance decision may adopt or amend this ADR only after review; the
  current `proposed` status is deliberate.

## Alternatives considered

| Alternative | Disposition |
|---|---|
| K3s as the canonical node | Rejected: couples sovereign operation to cluster infrastructure and private liveness |
| OCI container as the only canonical artifact | Rejected for now: current image proves a build, not appliance-equivalent identity, systemd, backup, or restoration |
| Native Linux as the only canonical path | Rejected: increases operator variance and weakens artifact-level review evidence |
| Keep all deployment paths equal | Rejected: preserves contradictory purposes and unbounded readiness claims |
| Remove generic Kubernetes support | Rejected: hosted/operator use remains legitimate when kept optional and generic |

## Non-goals

- No live cluster change and no private infrastructure migration.
- No adoption of this proposed decision without governance review.
- No production-readiness, pilot, institutional-adoption, or federation claim.
- No `COMMUNITY_TOPIC`, composition-root, ledger, or B1/B2 architecture change.
- No assertion that a fixture rehearsal proves institution-owned durable
  workflow state.
