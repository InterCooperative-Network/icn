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

This directory establishes the **vocabulary, layout, and honest
non-claims** for that work. The scaffold (PR #1865) and the real local QCOW2
build + one-VM boot smoke (PR #1866) have both landed. The repo does not
ship a prebuilt QCOW2 artifact; the operator must stage a Debian base
image and invoke `build-image.sh --real` to produce one. The script does
not download the base image, but the `--real` path's in-image
`virt-customize --update --install` step does fetch Debian apt packages
from the base image's configured repos — so the real path is not
network-free. See "Real local build + boot smoke" below.

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
| **Unbuilt scaffold** | Docs, templates, scripts, no image artifact. |
| **Bootable dev image** (today, for operators who stage a base image) | Local QCOW2 image that boots, runs `icnd`, passes health. |
| **Claimed devnet node** | The image after a devnet operator runs first-boot and joins a local devnet. |
| **Role-profiled node** | Same image, declared role applied via the appliance manifest. |
| **Governed service host** | A `RuntimeProvider` hosts services under `GovernedServiceBinding`. |
| **Production-signed appliance** | Signed updates, immutable rootfs, A-B updates, measured boot. |

The scripts for **Bootable dev image** (real build) and the one-VM boot
smoke that verifies `/v1/health` on 8080 are present and landed via PR
#1866. Whether a given clone has produced an actual QCOW2 artifact depends
on operator action: the build does not run automatically and requires an
operator-staged base image plus explicit invocation with `--real`. The
build script itself does not download the base image; the `--real` path's
in-image `virt-customize --update --install` step does fetch apt packages
from the base image's configured repos. See the suggested follow-on
stages at the bottom of `DEBIAN_APPLIANCE_MODEL.md` and the next-step list
at the bottom of this README.

## Layout

```
deploy/appliance/
├── README.md                                   # this file
├── appliance.manifest.example.yaml             # declarative manifest template
├── build-image.sh                              # dry-run by default; `--real` builds a QCOW2 from an operator-staged base image
├── check.sh                                    # script + manifest sanity check
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
│   ├── cloud-init/                             # example user-data / meta-data for the smoke VM (placeholders only)
│   └── smoke-local.sh                          # dry-run by default; `--real` boots the built QCOW2 and checks `/v1/health`
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

### Host / image compatibility

`icnd` is dynamically linked. The **build host's glibc version must be
less than or equal to the appliance base image's glibc**, or `icnd` will
restart-loop on the image with:

```
/usr/local/bin/icnd: /lib/x86_64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found
```

Reference points (as of 2026-05):

| Environment | glibc |
|---|---|
| Debian 12 bookworm (genericcloud) | 2.36 |
| Debian 13 trixie (genericcloud) | 2.41 |
| Ubuntu 22.04 jammy (cloud-image) | 2.35 |
| Ubuntu 24.04 noble (cloud-image) | 2.39 |

If your build host is Ubuntu 24.04 / Debian trixie / similar, either:

- Use a base image with matching-or-newer glibc (e.g. Debian trixie
  cloud image), or
- Build `icnd` inside a Debian-12-matching container so the binary
  links against an older glibc, or
- Defer until a static / musl build target is wired (future work).

Either path is the operator's call; the build script doesn't pick.

### WSL2 quirks

If you're building on WSL2 (Ubuntu/Debian under Windows), the following
have been observed:

- **`virt-customize` / `virt-sysprep` may need a real kernel.** WSL2's
  default kernel may not be enough for libguestfs's appliance. Install
  `linux-image-generic` so `/boot/vmlinuz-*` exists.
- **`/boot/vmlinuz-*` may need `chmod 0644`** so the non-root user
  running `virt-customize` can read it.
- **`LIBGUESTFS_BACKEND=direct`** may be required: WSL2 doesn't run
  `libvirtd`, so libguestfs's libvirt backend fails to start. Set
  `export LIBGUESTFS_BACKEND=direct` before running `build-image.sh
  --real`.
- **`/dev/kvm` permission denied is non-fatal.** WSL2 may expose
  `/dev/kvm` as `root:kvm 0660`. If your user is not in the `kvm`
  group, libguestfs and QEMU fall back to TCG. The build still works;
  it's just slower.
- **Windows reserves ephemeral ports.** Ports `2222` and `2223` are
  commonly held by Windows-side Hyper-V / NAT exclusions and `qemu`
  cannot bind to them even though Linux `ss -ltn` shows nothing.
  Override the smoke SSH port to something higher:
  `export ICN_APPLIANCE_SSH_PORT=22222` before `smoke-local.sh --real`.

### Per-instance secrets

The image itself contains **zero** secrets. On first boot,
`icn-appliance-firstboot.service` (oneshot) generates:

- A random JWT secret (`openssl rand -hex 32`).
- A random keystore passphrase (`openssl rand -base64 32`).

Both are written to `/etc/icn/icnd.env` (mode `600`, owned `icn:icn`).
`icnd --init` runs once to create the keystore. Then `icnd.service` picks
up the env file and starts normally.

To rotate: remove `/var/lib/icn/.firstboot-complete` AND the keystore
file `/var/lib/icn/identity.age` (plus `config.toml` / `genesis.json` in
the same directory), then reboot or rerun firstboot.

## Demo profile (DEV/DEMO image variant)

`ICN_APPLIANCE_DEMO_PROFILE=1` at build time produces a **demo image**: the
base appliance plus everything needed to run the member loop
**standing -> action card -> discharge -> receipt -> evidence** against the
VM's own node, from a stranger's browser on the host.

What it adds (and the base image does NOT have):

| Piece | Where | What it is |
|---|---|---|
| member-shell + pilot-ui fixtures | `/usr/share/icn/static/web/` | static reference client (#2026), fixture + live-local modes |
| `icn-member-shell.service` | `:8090` | python3 stdlib static server, dev-only |
| `20-demo-profile.conf` drop-in | `icnd.service.d/` | gateway bind `0.0.0.0:8080` (hostfwd reachability), dev gates (`ICN_ENABLE_ADMIN_ENDPOINTS`, `ICN_GOVERNANCE_BUILD_MODE=test`), `ICN_CORS_ORIGINS` for the shell |
| `icn-demo-seed` | `/usr/local/sbin/` | seeds NYCN fixture institution + one open action item; prints the dev session JWT + URLs |
| `icn-demo-verify` | `/usr/local/sbin/` | receipt binding check; `--chain` runs the bundled 13/13 governed receipt-chain rehearsal on an ephemeral in-VM node |
| `icn-demo-reset` | `/usr/local/sbin/` | destructive reset: wipe node state, re-run firstboot, reseed |
| NYCN package + dogfood kit + 13/13 scripts | `/usr/share/icn/demo/` | fixture institution and bundled evidence tooling |

Build and smoke it:

```bash
export ICN_APPLIANCE_BASE_IMAGE=/path/to/debian-13-genericcloud-amd64.qcow2
export ICN_APPLIANCE_OUTPUT_DIR=$HOME/icn-appliance-build
export ICN_APPLIANCE_VERSION=0.0.2-demo
export ICN_APPLIANCE_DEMO_PROFILE=1
bash deploy/appliance/build-image.sh --real

ICN_APPLIANCE_IMAGE=$ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.2-demo-amd64.qcow2 \
ICN_APPLIANCE_SSH_KEY=... ICN_APPLIANCE_CLOUD_INIT_SEED=... \
bash deploy/appliance/smoke/smoke-local.sh --real --demo
```

`--demo` forwards the gateway (host `18080`) and shell (host `18090`), seeds
the loop in-VM, then drives **standing -> card -> complete -> receipt** from
the host through the forwarded ports — the same path a stranger's browser
takes — and checks the receipt's 32-byte `record_hash` binding.

Honesty labels for the demo image:

- **Live-local:** node boot, health, standing/action-card/receipt endpoints,
  action-item discharge, completion receipt, 13/13 receipt-chain rehearsal
  (`icn-demo-verify --chain`). All on the VM's own node.
- **Fixture-backed:** the shell's `?mode=demo` surfaces (self-labeled), the
  NYCN institution package (fictional data).
- **Not present / not claimed:** production posture, signed image, pilot
  adoption, federation, multi-org deployment, real member data. The dev
  gates in the drop-in are the same labeled non-production gates the local
  devnet uses — never enable them outside a disposable demo VM.

## Security posture (dev-image, not production)

- **No secrets are committed in this directory or embedded in the image.**
  `appliance.env`, role examples, cloud-init examples, and the firstboot
  script all use placeholders or generate values at runtime.
- **Per-instance secrets are generated on first boot, not in the image.**
  `icn-appliance-firstboot.service` writes a fresh JWT secret and keystore
  passphrase to `/etc/icn/icnd.env` (mode `600`, owned `icn:icn`) and runs
  `icnd --init` to seal the keystore. Two different VMs from the same
  image get two different identities and two different JWT secrets.
- **`devnet-insecure` shared secrets are explicitly NOT used** by the
  appliance. The appliance has no embedded credentials.
- **`icnd.service` is enabled at image build time** but cannot start until
  firstboot has run; the systemd `Before=icnd.service` dependency enforces
  ordering. There is no auto-pairing, no federation contact, no remote
  enrollment.
- **Not implemented here, named in `DEBIAN_APPLIANCE_MODEL.md` as future
  work:** signed updates, A-B updates, immutable rootfs, TPM-backed keys,
  measured boot, attested federation enrollment. This image is a local
  dev-VM artifact, not a partner-distributable appliance.

## What has landed and what is still ahead

What has landed:

1. The scaffold — docs, manifest template, role examples, firstboot script,
   systemd unit, dry-run check (PR #1865).
2. A real local QCOW2 build path implemented in `build-image.sh --real`
   using Debian cloud-image customization via `virt-customize` (PR #1866).
3. A one-VM boot smoke under `smoke/smoke-local.sh --real` that boots the
   built image and verifies `icnd` is alive and `/v1/health` responds on
   8080 (PR #1866).

Caveats on the real path:

- The repo does not ship a prebuilt QCOW2 artifact. The scripts are
  present, but producing one requires the operator to stage a Debian
  base image and invoke `--real` explicitly.
- The build script does not download the base image. The `--real` path's
  in-image `virt-customize --update --install` step does fetch apt
  packages from the base image's configured repos, so the real path is
  not network-free even though the script never downloads the base
  image itself.

What is still ahead:

1. Decide on Packer vs debos vs live-build as the longer-term backend now
   that the first qcow2 path is working.
2. Signed releases, A-B updates, immutable rootfs, TPM-backed keys,
   measured boot, attested federation enrollment — see the future-work
   list in `DEBIAN_APPLIANCE_MODEL.md`. These are not implemented here.
3. Convergence between appliance and devnet behavior; the appliance does
   not yet replace `deploy/install.sh` + `deploy/icnd.service` or
   `deploy/devnet/`.

## Cross-references

- [`docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md) — full appliance model.
- [`docs/architecture/SERVICE_HOSTING_MODEL.md`](../../docs/architecture/SERVICE_HOSTING_MODEL.md) — hosted → governed → native stages.
- [`docs/spec/governed-service-binding.md`](../../docs/spec/governed-service-binding.md) — abstract workload primitives the appliance will eventually host.
- [`deploy/icnd.service`](../icnd.service) — the existing native systemd unit the appliance reuses.
- [`deploy/install.sh`](../install.sh) — the existing native install path.
- [`deploy/devnet/`](../devnet/) — the existing 3-node Docker devnet. Appliance behavior should eventually converge with devnet behavior but does not replace it.
