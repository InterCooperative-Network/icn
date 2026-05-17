---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-16
---

# Debian Appliance Model

> **Status: design direction, scaffold-only.** This document names how ICN
> should distribute a node: as a single Debian-based appliance image that
> boots, runs `icnd`, and can later be claimed, role-profiled, and used as a
> substrate for governed service hosting. It is not an implementation
> report. It does not declare any appliance live and does not authorize
> mutation of K3s, DNS, Forgejo, GitHub, or homelab state.

## Purpose

ICN currently ships as a daemon that an operator installs into an existing
Debian/Ubuntu host (`deploy/install.sh`, `deploy/icnd.service`) or runs as a
multi-container devnet (`deploy/devnet/`). Neither of those is a node unit
that a partner cooperative can claim and place into operation without
becoming an operating-system administrator.

The appliance addresses that gap: **one bootable image, one node, one
operator action to bring it on-line**. The same image is used for every
role; the role is a declarative profile applied to the running appliance,
not a different image.

This document records the model. The appliance itself does not exist yet.

## Non-goals

This document and the scaffold it accompanies explicitly do **not**:

- Introduce any Rust types or wire formats.
- Re-implement `GovernedServiceBinding`, `WorkloadManifest`, or
  `RuntimeProvider` from `docs/spec/governed-service-binding.md`. Those
  remain forward-direction primitives.
- Re-implement the service-hosting stages from
  `docs/architecture/SERVICE_HOSTING_MODEL.md`.
- Replace the existing native install (`deploy/install.sh`,
  `deploy/icnd.service`) or the existing 3-node devnet
  (`deploy/devnet/`). Both continue to be supported until the appliance
  reaches a comparable acceptance bar.
- Claim production readiness.
- Claim a signed release.
- Claim a live federation or activated partner cooperative.
- Claim Phase 2 completion.
- Treat NYCN as production-activated.
- Embed secrets, keystore passphrases, JWT secrets, or any other private
  material in the image, in repo, or in build artifacts.

## Relationship to existing architecture

This document extends, but does not replace:

- [`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md) — hosted →
  governed → ICN-native service progression. Appliance hosts the
  substrate; service hosting layers above it.
- [`../spec/governed-service-binding.md`](../spec/governed-service-binding.md)
  — abstract `GovernedServiceBinding`, `WorkloadManifest`,
  `RuntimeProvider`. The appliance hosts one or more runtime providers; it
  does not redefine the primitives.
- [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) — meaning
  firewall. The appliance image is a substrate concern; nothing in the
  image teaches the kernel to read domain types.
- [`../../deploy/icnd.service`](../../deploy/icnd.service) — the systemd
  unit the appliance reuses unmodified for `icnd` itself.
- [`../../deploy/install.sh`](../../deploy/install.sh) — the existing
  native install path. The appliance's first-boot scaffold mirrors its
  directory layout (`/var/lib/icn`, `/etc/icn`, `icn` system user).
- [`../../deploy/devnet/`](../../deploy/devnet/) — the existing Docker
  devnet. Appliance behavior should eventually converge with devnet
  behavior (3-node bootstrap, gossip convergence, health) but the first
  acceptance target is a single boot.

## Lifecycle stages

A given appliance moves through stages. The image artifact and the running
node have separate stage progressions; both are listed below.

### Image artifact stages

| Stage | What exists | What does not |
|---|---|---|
| **Unbuilt scaffold** (today) | Docs, manifest templates, role examples, first-boot scaffold, dry-run build script. | No image. No partner consumer. |
| **Bootable dev image** | A local QCOW2 image that boots Debian + `icnd`, exposes `/v1/health` on 8080. | Not signed. Not reproducible. Not for partner hands. |
| **Reproducible build** | A build path (Packer / debos / live-build) that produces the same image hash from the same inputs. | Still not signed. Still not immutable. |
| **Signed release** | The reproducible build emits a signature operators can verify. | Still not immutable. No A-B updates. |
| **Production-signed appliance** | Signed updates, immutable rootfs, A-B updates, measured boot, TPM-backed identity material. | — |

Today is **Unbuilt scaffold**. The first implementation slice after this
scaffold lands is **Bootable dev image** plus a one-VM boot smoke.

### Node-instance states

Once an appliance is running, the node moves through:

| State | What it means |
|---|---|
| **unclaimed** | The appliance has booted but no operator has claimed it. `icnd` may not be running, or is running in an unconfigured state. |
| **claimed** | An operator has performed the claim action: identity initialized, keystore created, role profile selected. The node knows what it is. |
| **configured** | The role profile's expected configuration has been applied (peers, gateway binding, observability, role-specific units). |
| **running** | `icnd` is running, `/v1/health` returns OK, role-specific services are alive. |
| **degraded** | One or more role-specific services are unhealthy, or the node has diverged from federation state, or anti-entropy probes are failing. The node continues to serve what it can. |
| **suspended** | Authority to act has been revoked (operator pause, governance decision, or emergency stop). The node holds state but does not dispatch effects. |
| **decommissioned** | The node has been removed from operation. Custody handoff and data export have been performed per the appliance manifest's exit policy. |

State transitions are operational facts. None of them is an authority
transition by itself; authority changes flow through governance acts per
`docs/spec/governed-service-binding.md`.

## Role profiles

Initial role-profile vocabulary. A role is a **declared shape** the
appliance is being used as, not a different image and not a different
binary. The role is applied as a manifest reference; the appliance reads
the role profile and starts (or refuses to start) the matching unit set.

| Profile | Purpose | Initial expected services |
|---|---|---|
| **genesis** | First node in a new local domain. Bootstraps identity and the local genesis state. | `icnd` only. No peers yet. |
| **witness-archive** | Observes a federation, retains state, serves no policy authority. | `icnd`, observability, retention. |
| **commons-executor** | Runs commons compute workloads under the existing compute admission policy (ADR-0031). | `icnd`, compute provider. |
| **domain-host** | Runs the substrate for one institutional domain (`docs/spec/institutional-domain.md`). | `icnd`, domain-scoped storage, member-shell endpoint. |
| **service-host** | Hosts governed services declared by `GovernedServiceBinding`s once those land. Today: no services. | `icnd`, future runtime providers. |
| **ingress** | Public-facing entry point for institutional domains. Routes member-shell and steward-cockpit traffic. | `icnd`, reverse-proxy adapter. |
| **storage** | Holds durable storage for a domain (custody-class storage per `docs/spec/storage-durability-policies.md`). | `icnd`, storage backend. |
| **forge-build** | Build runner for the sovereign forge once it exists. Today: not implemented. | `icnd`, build executor (future). |
| **observability** | Aggregates metrics, traces, and audit evidence for a federation. | `icnd`, Prometheus, log shipper. |
| **sandbox** | Disposable node for rehearsals, fixture work, and local proof-loops. **Never production.** | `icnd`, devnet-style overrides allowed. |

Roles are **not authority**. A node profiled as `service-host` does not gain
authority to host services; it gains the capacity to host them once a
`GovernedServiceBinding` selects it as the substrate. The kernel does not
consult role profiles. Role profiles are a substrate-layer config artifact.

## Runtime provider roadmap

Each runtime class named in `docs/spec/governed-service-binding.md`
§"Runtime classes" eventually has at least one provider implementation on
the appliance. The appliance does **not** implement providers; it hosts
them.

| Provider | Status | Notes |
|---|---|---|
| **systemd / native** | First-class baseline | `icnd.service` plus role-specific unit files. The appliance ships with this. |
| **oci-container** | Future | Hosts containerized governed services (forge, auth, status, metrics, registry, docs). Likely backed by `podman` or `containerd`, not Docker, to keep the substrate auditable. |
| **k3s** | Optional, future | For roles that need clustered orchestration. Not baseline. An appliance does not require k3s to run. |
| **wasm** | Future | Deterministic legitimacy compute (per ADR-0021) and utility computation. Runs sandboxed in-process or via a dedicated runtime. |
| **microvm** | Future | Stronger-isolation provider for untrusted code paths. |
| **accelerator** | Future | For hardware-accelerated workloads where the appliance is provisioned with the relevant hardware. |
| **external-bridge** | Future | Integrations with non-ICN systems, granted only by explicit policy. |

The appliance manifest declares which providers are present on a given
image. Adding a provider is an image change; granting a binding to use a
provider is a governance act.

## Service-hosting future

The appliance is not just `icnd`. The longer-term posture is:

```text
appliance image
        ↓
icnd (substrate daemon)        ← already runs here
        ↓
runtime providers              ← hosted by the appliance
        ↓
GovernedServiceBindings        ← chosen by governance
        ↓
governed services              ← forge, auth, status, metrics, registry, docs, institution-hosted
```

The discipline is the same one `SERVICE_HOSTING_MODEL.md` already states:

> **ICN owns authority. Services expose functions. Adapters translate
> service events. Receipts prove transitions. Compute produces evidence.
> Governance authorizes power.**

The appliance is below all of that. It boots, runs the daemon, exposes the
provider surface, and lets governance decide what to do with it. Operator
scope (who runs the appliance) does **not** become institutional authority
(who authorizes what runs on it).

## Security notes

These are scaffold-stage statements. They define the posture the appliance
must hold; they do not declare that posture implemented.

| Concern | Posture |
|---|---|
| **Embedded secrets** | None. No JWT, keystore passphrase, TLS key, or partner-shared material is in the image, in this repo, or in any build output. |
| **Private data** | Not in repo. Any institution-specific material lives in partner repos under partner-controlled access. |
| **First-boot material** | Generated locally on the appliance, after first boot, by the operator. Mirrors `deploy/install.sh` behavior; the appliance does not generate or vend passphrases. |
| **Signed updates** | Future work. Until signed updates land, an appliance is updated by replacing the image. |
| **Immutable rootfs** | Future work. The first dev image is mutable. |
| **A-B updates** | Future work. |
| **TPM-backed identity** | Future work. Identity material is held in `~icn/.icn/keystore.age` (per the existing pattern), Age-encrypted, passphrase-derived. |
| **Measured boot** | Future work. |
| **Devnet shared secrets** | Explicitly **never** used by the appliance. The devnet's `devnet-insecure` shared secrets exist only in `deploy/devnet/`. The appliance does not import them, copy them, or accept them as defaults. |

## Build path posture

The first build target is a single local QCOW2 image. The longer-term
posture supports multiple backends; the choice is deferred until the first
local build is working.

| Candidate backend | Strength | Weakness |
|---|---|---|
| **Debian cloud image + `virt-customize`** | Smallest jump from existing native install; reuses official Debian images. | Single-shot; less reproducible. |
| **Packer (HashiCorp / community)** | Mature, well-documented, multi-format output. | Tooling overhead; license/governance review required for HashiCorp lineage. |
| **debos** | Native Debian; designed for declarative image builds. | Smaller community; more bespoke pipeline. |
| **live-build** | Official Debian project. | Older tooling style; documentation rougher. |

The dry-run build scaffold (`deploy/appliance/build-image.sh`) does not pick
yet. The first slice will use Debian cloud image + `virt-customize` because
it is the smallest credible step.

## Acceptance for the first real image

The first non-scaffold appliance image is accepted when, on a disposable
local VM:

1. The image boots Debian to a usable login state.
2. The first-boot unit runs once and is marked complete (does not run on
   subsequent boots).
3. `icnd.service` starts.
4. `/v1/health` on port `8080` returns HTTP `200` to a request from the
   host.
5. The appliance can be cleanly destroyed and rebuilt without manual
   intervention.

That is the gate for promoting from **Unbuilt scaffold** to **Bootable dev
image**. Nothing in this PR claims that gate is met.

## Honest non-claims (repeated for clarity)

- No production image exists.
- No signed release exists.
- No `GovernedServiceBinding` runtime implementation lives here.
- No partner-federation activation exists.
- NYCN is not production-activated under appliance primitives.
- No K3s, DNS, Forgejo, GitHub, or homelab state is mutated by this
  document or the accompanying scaffold.
- No claim that the existing systemd / Docker / K8s deploy paths are
  deprecated; they remain primary.

## Related documents

- [`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md) — hosted →
  governed → native stages, service definition shape, authority model,
  data custody classes.
- [`../spec/governed-service-binding.md`](../spec/governed-service-binding.md)
  — `WorkloadManifest`, `GovernedServiceBinding`, `RuntimeProvider`,
  runtime classes, lifecycle.
- [`../spec/institutional-domain.md`](../spec/institutional-domain.md) —
  `InstitutionalDomain` and `DomainPolicy`. Role profiles `domain-host`,
  `ingress`, `storage` exist to host these.
- [`../spec/storage-durability-policies.md`](../spec/storage-durability-policies.md)
  — custody classes the `storage` role profile must honor.
- [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) — kernel never
  imports appliance, role profile, or runtime-provider types.
- [`../../deploy/icnd.service`](../../deploy/icnd.service),
  [`../../deploy/install.sh`](../../deploy/install.sh),
  [`../../deploy/devnet/`](../../deploy/devnet/) — the existing deploy
  surfaces the appliance reuses or layers above.
- [`../../deploy/appliance/`](../../deploy/appliance/) — the accompanying
  scaffold directory (manifest template, role examples, first-boot, build
  scaffold, smoke scaffold).
