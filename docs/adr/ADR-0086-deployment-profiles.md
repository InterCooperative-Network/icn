---
id: "0086"
title: "ICN deployment profiles and public/private infrastructure boundary"
status: "accepted"
date: "2026-07-26"
accepted_date: "2026-09-15"
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

**`accepted` (2026-09-15).** The four-profile decision and the public/private
boundary are in force; new deployment work must conform. Accepted by the decider
named above, as a deliberate act — merging this record did not perform it, and
ADR-0018 is explicit that the decision lifecycle moves only when the project
decides.

**Frozen Alpha profile revision: `860c6f6c22f18de0f7bc4cfb55e35b1143b3b4f1`.**
That is the exact `main` revision at which the Profile A artifacts were reviewed
and witnessed. The profile means those bytes, not "whatever `deploy/` currently
contains".

What acceptance settles, and what it does not:

* it settles the **classification** — appliance as canonical sovereign-node
  artifact, Compose devnet as disposable, Kubernetes/K3s as optional operator
  infrastructure, native Linux as an advanced install form — and the
  public/private repository boundary;
* it settles that the native service consumes the configuration the institution
  was provisioned with, at the frozen revision, across every shipped profile
  override;
* it does **not** certify production readiness, authorize a production
  deployment, or adopt any profile for pilot or partner use;
* it does **not** convert an incomplete item into a complete one. Every non-claim
  below still holds, and item 7 of the Profile A contract remains open.

`implementation_status: partially implemented` is unchanged and is a separate
axis from the decision status (ADR-0018 § "Decision status is separate from
implementation status"). Repository paths exist for all four profiles, but only
the assembled Debian appliance has a current, retained build-and-boot witness.
Independent restoration, artifact signing, reproducibility, two-node
institutional operation, and generic Kubernetes reconciliation remain
incomplete.

### What the freeze added since this record was written

At the frozen revision, and witnessed against the shipped artifacts rather than
hand-composed invocations:

* the native unit and **every** shipped `icnd` drop-in pass the provisioned
  configuration, so the effective ledger treasury is the institutional principal
  and not a node-DID fallback (icn#2755);
* `<data_dir>/icn.toml` is the single canonical native configuration path, owned
  by `icn_core::config::config_file_path`;
* both units that create `icnd`'s files — `icnd.service` and
  `icn-appliance-firstboot.service` — pin `UMask=0077`;
* `<data_dir>/config.toml` is deprecated and **not** migrated. There is no
  legacy upgrade path: when the canonical configuration is absent and a legacy
  `config.toml` sits beside it, `icnd` refuses to start and names both remedies.
  It does not read the legacy file, and no dual-path discovery exists.

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
encrypted appliance recovery contract. Item 7 is open for two independent
reasons, and closing either alone does not close the item. First, the current
data backup does not include `/etc/icn/icnd.env`, so restoring `/var/lib/icn`
alone is not an independently operable appliance recovery. Second, a restored
ledger cannot be shown to be *complete*: a `db` truncated to a partial length
reopens cleanly as a valid short prefix, and nothing commits to the extent it
should have had. Nothing available to a verifier distinguishes that from a
ledger that always held that many, so restoration cannot be shown to restore a
*complete* ledger (icn#2746). icn#2787 has landed and stops the verifier
overclaiming: `verify-backup --verify-ledger` now reports completeness as
`unresolved` and fails closed instead of certifying it. That removed a false
positive; it is not detection, and it did not make restoration provable.
icn#2786 owns the requirement for an independent extent/frontier commitment
that would make completeness checkable at all. It remains open, and the
mechanism is deliberately **not yet selected** — a durable count, a monotonic
frontier and a digest accumulator differ in what they can prove, and a count
alone can refute completeness but never establish it. **Item 7 closes only when
BOTH causes are closed** — the secret-bearing environment file must travel with
the backup, *and* icn#2786 must land. Closing either alone leaves an appliance
that cannot be independently restored, which is what item 7 asserts. Items
8–9 are only partially
met: manifests exist and Kubernetes is not required, but appliance artifacts
are not yet signed or reproducibly built.

Fixture/demo services are optional add-ons. Their state and claims are not the
sovereign-node durability contract.

### Profile B: disposable local development network

`deploy/devnet/docker-compose.yml` is the canonical Compose entry point
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
| Independent appliance restoration | Two independent causes: the plain data-directory tar omits the secret-bearing environment file, and ledger completeness cannot be established at all — a truncated journal reopens as a valid short prefix and nothing commits to its expected extent (icn#2746; the verifier stopped overclaiming in icn#2787, which has landed, but the mechanism that would make completeness checkable is icn#2786 and remains open) | Open blocker |
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
- This decision was adopted on 2026-09-15 after review, at the frozen revision
  named in § Status. Amending it now requires an amendment note under ADR-0018's
  lifecycle rather than an edit.

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
- No production-readiness or pilot adoption follows from accepting this
  decision. Acceptance settles the classification and the frozen profile
  revision; it authorizes no deployment.
- No production-readiness, pilot, institutional-adoption, or federation claim.
- No `COMMUNITY_TOPIC`, composition-root, ledger, or B1/B2 architecture change.
- No assertion that a fixture rehearsal proves institution-owned durable
  workflow state.
