# ICN Debian Appliance (dev image)

> **Status: bootable dev image.** The scaffold from PR #1865 plus a real
> local QCOW2 build path (`build-image.sh --real`) and a real one-VM
> boot smoke (`smoke/smoke-local.sh --real`). The dev image is **not**
> production: unsigned, not immutable, no A-B updates, no claim flow, no
> partner federation activation. The scaffold contents from PR #1865 are
> unchanged; this slice adds the build and smoke implementations.

## What this is

The ICN appliance is the **node unit**: one bootable, installable Debian image
that runs `icnd`, exposes a health endpoint, and can be claimed and
role-profiled into a member node, witness, domain host, or future service
host. The same image is used everywhere; the role is applied after the image
boots.

Today this directory establishes the **vocabulary, layout, and honest
non-claims** for that work so that the next slice — a real local QCOW2 build
and a one-VM boot smoke — can drop into a settled structure.

For the full design including lifecycle stages, node states, role-profile
vocabulary, and runtime-provider roadmap, see
[`docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md).

## What this is not

- Not a production-ready operating system release.
- Not an immutable image. A-B updates, signed updates, TPM, and measured boot
  are all future work.
- Not a signed release. No release artifact is produced by this scaffold.
- Not a production-deployed network. NYCN is not activated under these
  primitives. No partner federation is operating on appliance images.
- Not Phase 2 completion. See [`docs/STATE.md`](../../docs/STATE.md): the
  recent architecture-spec sprint landed docs/control-plane work only. No
  runtime implementation lives here.
- Not a re-implementation of [`docs/spec/governed-service-binding.md`](../../docs/spec/governed-service-binding.md).
  `GovernedServiceBinding`, `WorkloadManifest`, and `RuntimeProvider` remain
  forward-direction primitives. The appliance is the **substrate** beneath
  them, not the runtime that executes them.
- Not a replacement for the existing native install (`deploy/install.sh` +
  `deploy/icnd.service`) or the existing 3-node devnet
  (`deploy/devnet/docker-compose.yml`). Those continue to be the supported
  paths until the appliance reaches a comparable acceptance bar.

## Where this sits in the ICN stack

```text
public / learning surfaces
        ↓
institution packages
        ↓
governed service hosting layer       ← docs/architecture/SERVICE_HOSTING_MODEL.md
        ↓                              docs/spec/governed-service-binding.md
ICN runtime apps                       (forward-direction)
        ↓
ICN substrate (icnd)
        ↓
appliance image                      ← this directory
        ↓
ops / infrastructure
```

`SERVICE_HOSTING_MODEL.md` defines the hosted → governed → ICN-native service
progression. `governed-service-binding.md` defines the abstract primitives a
governed workload binds against. This appliance is one node-shaped substrate
on which those bindings will eventually run — through a `RuntimeProvider` the
appliance hosts, not through any logic in the image itself.

## Future scope: governed service hosting

Once the image can boot, run `icnd`, and pass health, the appliance is
intended to host governed services on the same node it runs the daemon on.
Candidate services include:

- `forge` — sovereign code forge
- `auth` — identity-provider bridge
- `status` — public status surface
- `metrics` — internal observability
- `registry` — artifact / release / receipt registry
- `docs` — documentation publishing surface
- institution-hosted services declared by partner packages

The appliance will not implement these. It hosts the `RuntimeProvider` that a
`GovernedServiceBinding` selects. Authority remains in ICN; the appliance
provides the substrate.

## Runtime posture

Initial runtime posture, in priority order:

1. **systemd / native** — `icnd.service` plus per-role unit files. First-class.
2. **OCI container** — a future provider for hosted services (forge, auth,
   status, metrics, registry, docs). Not baseline.
3. **k3s** — optional provider for some role profiles. Not baseline. Not a
   requirement to run an ICN node.
4. **WASM** — future provider for deterministic legitimacy compute and
   utility computation workloads.
5. **microVM** — future stronger-isolation provider.

The appliance does not collapse Linux administration into ICN authority.
Operator scope (who runs the box) stays distinct from governance scope (who
authorizes what runs on it).

## Port discipline

The appliance uses native code defaults — **not** the devnet overrides:

| Surface | Port | Source |
|---|---|---|
| Gateway / health | **8080** | `icn-core/src/config/gateway.rs` default `[::1]:8080`; matches `deploy/icnd.service` |
| Peer transport | **7777** | `icn-core/src/config/mod.rs` default `[::]:7777` (QUIC/UDP) |
| RPC | 5601 | `docs/reference/project-index/ci-ops-deploy-map.md` |
| Metrics | 9100 | `docs/reference/project-index/ci-ops-deploy-map.md` |

**Never 8000.** The devnet `entrypoint.sh` has a legacy `8000` fallback if
`ICN_GATEWAY_PORT` is unset; the appliance does not inherit that. Gateway and
health endpoint are 8080.

## Initial acceptance target

```text
boot → first-boot config → icnd starts → /v1/health responds on 8080
```

That single end-to-end path is the gate for promoting this scaffold to a real
buildable image. Everything else (claim flow, role profile application,
hosted-service installation, smoke fixtures) is layered on after.

## Lifecycle stages this scaffold supports

| Stage | What the appliance is |
|---|---|
| **Unbuilt scaffold** (today) | Docs, templates, scripts, no image artifact. |
| **Bootable dev image** | Local QCOW2 image that boots, runs `icnd`, passes health. |
| **Claimed devnet node** | The image after a devnet operator runs first-boot and joins a local devnet. |
| **Role-profiled node** | Same image, declared role applied via the appliance manifest. |
| **Governed service host** | A `RuntimeProvider` hosts services under `GovernedServiceBinding`. |
| **Production-signed appliance** | Signed updates, immutable rootfs, A-B updates, measured boot. |

Today is **Unbuilt scaffold**. Next slice is **Bootable dev image** plus a
boot smoke that verifies `/v1/health` on 8080. See the suggested next slice
at the bottom of `DEBIAN_APPLIANCE_MODEL.md`.

## Layout

```
deploy/appliance/
├── README.md                                   # this file
├── appliance.manifest.example.yaml             # declarative manifest template
├── build-image.sh                              # dry-run build scaffold (no real build yet)
├── roles/
│   ├── genesis.example.yaml
│   ├── witness-archive.example.yaml
│   ├── domain-host.example.yaml
│   ├── service-host.example.yaml
│   └── sandbox.example.yaml
├── scripts/
│   └── icn-appliance-firstboot.sh              # POSIX bash, idempotent, no secrets
├── smoke/
│   ├── README.md
│   └── smoke-local.sh                          # dry-run scaffold for next PR
└── systemd/
    └── icn-appliance-firstboot.service         # oneshot unit, runs before icnd.service
```

## How to use this slice

Two paths, depending on what you have locally:

### Dry-run (no tools required)

```bash
bash deploy/appliance/build-image.sh --dry-run
bash deploy/appliance/smoke/smoke-local.sh --dry-run
bash deploy/appliance/scripts/icn-appliance-firstboot.sh --dry-run
bash deploy/appliance/check.sh
```

Each prints the planned steps and exits cleanly. No files are mutated.

### Real local build + boot smoke

Required tools (Debian/Ubuntu package in parentheses):

| Tool | Package |
|---|---|
| `qemu-img` | `qemu-utils` |
| `virt-customize` | `libguestfs-tools` |
| `virt-sysprep` | `libguestfs-tools` |
| `qemu-system-x86_64` | `qemu-system-x86` |
| `cloud-localds` | `cloud-image-utils` |
| `sha256sum` | `coreutils` |
| `cargo` | `rustup` or `rust-toolchain` |
| `ssh`, `curl` | `openssh-client`, `curl` |

Required inputs:

- A staged Debian cloud base image (e.g. `debian-12-genericcloud-amd64.qcow2`).
  The build script does **not** download anything; you stage it manually.
- An SSH keypair dedicated to disposable smoke VMs (not your daily key).

Build a local dev image:

```bash
export ICN_APPLIANCE_BASE_IMAGE=/path/to/debian-12-genericcloud-amd64.qcow2
export ICN_APPLIANCE_OUTPUT_DIR=$HOME/icn-appliance-build
export ICN_APPLIANCE_VERSION=0.0.1-dev
# Optional but recommended:
export ICN_APPLIANCE_BASE_SHA256=$(sha256sum "$ICN_APPLIANCE_BASE_IMAGE" | awk '{print $1}')

bash deploy/appliance/build-image.sh --real
# -> $ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.1-dev-amd64.qcow2
# -> $ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.1-dev-amd64.manifest.json
```

Boot smoke the image:

```bash
# Prepare your smoke-only SSH keypair:
ssh-keygen -t ed25519 -f /tmp/icn-smoke-key -N ""
# Edit deploy/appliance/smoke/cloud-init/user-data.example.yaml,
# replace the placeholder with the contents of /tmp/icn-smoke-key.pub,
# and save somewhere safe (NOT in the repo). Then build a seed ISO:
cp deploy/appliance/smoke/cloud-init/user-data.example.yaml /tmp/user-data
$EDITOR /tmp/user-data
cloud-localds /tmp/seed.iso /tmp/user-data deploy/appliance/smoke/cloud-init/meta-data.example.yaml

export ICN_APPLIANCE_IMAGE=$HOME/icn-appliance-build/icn-appliance-0.0.1-dev-amd64.qcow2
export ICN_APPLIANCE_SSH_KEY=/tmp/icn-smoke-key
export ICN_APPLIANCE_CLOUD_INIT_SEED=/tmp/seed.iso

bash deploy/appliance/smoke/smoke-local.sh --real
# Expected: SSH up -> firstboot marker present -> icnd active -> /v1/health 200 -> PASS.
```

### Per-instance secrets

The image itself contains **zero** secrets. On first boot,
`icn-appliance-firstboot.service` (oneshot) generates:

- A random JWT secret (`openssl rand -hex 32`).
- A random keystore passphrase (`openssl rand -base64 32`).

Both are written to `/etc/icn/icnd.env` (mode `600`, owned `icn:icn`).
`icnd --init` runs once to create the keystore. Then `icnd.service` picks
up the env file and starts normally.

To rotate: remove `/var/lib/icn/.firstboot-complete` AND the keystore at
`/var/lib/icn/.icn/`, then reboot or rerun firstboot.

## Security posture (scaffold-only)

- No secrets are committed in this directory.
- The first-boot script never writes a passphrase or JWT into a file.
- The devnet's `devnet-insecure` shared secrets are explicitly **not** used
  by the appliance. The appliance has no embedded credentials.
- First-boot material (keystore passphrase, JWT secret) is expected to be
  generated locally by an operator, per `deploy/install.sh`'s existing
  pattern. The appliance does not pretend to manage operator secrets yet.
- Signed updates, A-B updates, immutable rootfs, TPM-backed keys, and
  measured boot are all named in the model document as future work. None of
  that is implemented here.

## Next implementation slice

After this scaffold lands:

1. Turn `build-image.sh` into a real local QCOW2 build path using Debian
   cloud-image customization (`virt-customize` or equivalent).
2. Add a one-VM boot smoke under `smoke/` that boots the image and verifies
   `icnd` is alive and `/v1/health` responds on 8080.
3. Decide on Packer vs debos vs live-build as the longer-term backend after
   the first qcow2 path is working.

## Cross-references

- [`docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md) — full appliance model.
- [`docs/architecture/SERVICE_HOSTING_MODEL.md`](../../docs/architecture/SERVICE_HOSTING_MODEL.md) — hosted → governed → native stages.
- [`docs/spec/governed-service-binding.md`](../../docs/spec/governed-service-binding.md) — abstract workload primitives the appliance will eventually host.
- [`deploy/icnd.service`](../icnd.service) — the existing native systemd unit the appliance reuses.
- [`deploy/install.sh`](../install.sh) — the existing native install path.
- [`deploy/devnet/`](../devnet/) — the existing 3-node Docker devnet. Appliance behavior should eventually converge with devnet behavior but does not replace it.
