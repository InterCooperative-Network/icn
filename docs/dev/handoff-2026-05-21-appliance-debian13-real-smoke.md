# Appliance Debian 13 real-smoke verification, 2026-05-21

**Date / time (UTC):** 2026-05-22T02:00Z — 02:40Z
**Branch:** `docs/appliance-real-smoke-debian13-2026-05-21`
**Base:** `origin/main @ 00b3224aa778c518604a41f1dfd2b3ba0c1d1dec` (`feat(rpc,gateway): mint governance class-level scope constants (#1868 step 1) (#1881)`)
**Closes:** none.
**Refs:** PR #1865 (appliance scaffold, squash-merged), PR #1866 (real QCOW2 build + boot smoke, squash-merged into #1865), PR #1876 (prior operator handoff documenting verified host toolchain + base-image staging gap), PR #1879 (README drift fix).

This handoff is docs-only. **No runtime code change, no script change, no schema mint, no ADR, no RFC, no new contract URN, no production-readiness claim, no live-federation claim, no NYCN activation claim.** One real `--real` QCOW2 was built on `icn-dev`, one disposable QEMU VM was booted from it, and the four positive-path acceptance checks (`SSH` → `firstboot marker` → `icnd active` → `/v1/health 200`) all passed.

---

## Summary

Built a real local appliance QCOW2 on `icn-dev` from the official Debian 13 trixie `genericcloud` amd64 base image, then ran `deploy/appliance/smoke/smoke-local.sh --real` end-to-end. After one host-side preflight fix (kernel readability for `supermin`), both the build and the smoke passed. Total artifacts (base image, built appliance image, manifest JSON, smoke key, cloud-init seed, logs) live outside the repo under `~/icn-appliance-build/`; the repo working tree is clean and this handoff is the only file changed.

## Operator decision

Debian 13 trixie was selected as the primary ICN appliance base. Ubuntu 24.04 noble remains fallback only. Debian 13 was not blocked by any concrete failure; no fallback was needed.

## Repo state

- **Base branch:** `main`
- **Tested SHA:** `00b3224aa778c518604a41f1dfd2b3ba0c1d1dec` (origin/main HEAD at branch creation; same SHA also recorded in the appliance manifest's `git_commit`)
- **Working branch:** `docs/appliance-real-smoke-debian13-2026-05-21`
- **Relevant PRs already merged:**
  - #1865 — `scaffold(deploy): Debian appliance / installable node image substrate`
  - #1866 — `feat(deploy): real local QCOW2 build + one-VM boot smoke` (squashed into #1865)
  - #1876 — `docs(appliance): operator handoff — verified host toolchain + base-image staging gap`
  - #1879 — `docs(appliance): reconcile README with landed scaffold + real build/smoke`

## Base image

<!-- TRUTH TYPE: Implementation truth — only facts confirmed by `curl`, `sha512sum`, `sha256sum`, and `qemu-img info` run this session -->

| Field | Value |
|---|---|
| **Source** | `https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2` (official Debian cloud images) |
| **Filename** | `debian-13-genericcloud-amd64.qcow2` |
| **Local path** | `/home/ubuntu/icn-appliance-build/base-images/debian-13-genericcloud-amd64.qcow2` |
| **On-disk size** | 325 MiB (`340656128` bytes) |
| **Virtual size** | 3 GiB (`3221225472` bytes), `qcow2`, cluster 65536, zlib |
| **SHA512 (Debian-published)** | `7752ad2adce1bc49dd964dae8300ed7a239d0bf3c13112f55953b111447fe642d2cc01afeead234aa6ebe3605513f2e7c0e7c56785d675c38ff40110d5c8332b` |
| **SHA256 (computed locally; used as `ICN_APPLIANCE_BASE_SHA256`)** | `f8573792e38e6d8a5ba701759e5ff96792e4c7ebca3721394f548106f42aeb34` |
| **Download verified at** | `2026-05-22T02:07:52Z` |

**Verification command (executed this session):**

```bash
curl -fsSL -o SHA512SUMS https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS
sha512sum -c SHA512SUMS --ignore-missing
# debian-13-genericcloud-amd64.qcow2: OK
sha256sum debian-13-genericcloud-amd64.qcow2
# f8573792e38e6d8a5ba701759e5ff96792e4c7ebca3721394f548106f42aeb34  debian-13-genericcloud-amd64.qcow2
qemu-img info debian-13-genericcloud-amd64.qcow2
# file format: qcow2; virtual size: 3 GiB; disk size: 325 MiB
```

**Limitation:** `https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS.sign` returned HTTP 404 from the same official source, so the SHA512SUMS file itself was not PGP-verified. Trust chain for this run rests on TLS to `cloud.debian.org` plus per-file SHA512 verification. Recorded as an unsafe assumption below; not resolved in this session.

## Host environment

<!-- TRUTH TYPE: Implementation truth — captured by `uname`, `ldd`, `--version` flags run this session -->

| Field | Value | Source |
|---|---|---|
| Host | `icn-dev` | `hostname` |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) | `cat /etc/os-release` (from prior handoff #1876, unchanged) |
| Kernel (running) | `6.8.0-111-generic x86_64` | `uname -r` |
| Kernel (newest installed) | `6.8.0-117-generic` | `ls /boot/vmlinuz-*` |
| glibc | `2.39` (Ubuntu GLIBC 2.39-0ubuntu8.7) | `ldd --version` |
| `qemu-img` | `8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.16)` | `qemu-img --version` |
| `virt-customize` | `1.52.0` | `virt-customize --version` |
| `virt-sysprep` | `1.52.0` | `virt-sysprep --version` |
| `qemu-system-x86_64` | `8.2.2` | `qemu-system-x86_64 --version` |
| `cloud-localds` | from `cloud-image-utils` (no `--version` flag; presence confirmed by `--help`) | `cloud-localds --help` |
| `rustc` | `1.95.0 (59807616e 2026-04-14)` | `rustc --version` (after `source ~/.cargo/env`) |
| `cargo` | `1.95.0 (f2d3ce0bd 2026-03-21)` | `cargo --version` |
| Rust toolchain pin | `1.95.0` | `icn/rust-toolchain.toml` |
| `/dev/kvm` | `crw-rw---- root kvm` (user `ubuntu` not in group `kvm`) | `ls -l /dev/kvm` |

## Commands run

<!-- TRUTH TYPE: Execution truth — every state-changing or evidence-gathering command in this session -->

```bash
# Step 0–1: orient, branch
git fetch origin && git checkout -b docs/appliance-real-smoke-debian13-2026-05-21 origin/main

# Step 2: stage Debian 13 trixie base image (outside repo)
mkdir -p ~/icn-appliance-build/base-images
cd ~/icn-appliance-build/base-images
curl -fsSL -o SHA512SUMS https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS
curl -fsSL -o SHA512SUMS.sign https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS.sign   # 404, not published
curl -fSL  -o debian-13-genericcloud-amd64.qcow2 \
    https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2
sha512sum -c SHA512SUMS --ignore-missing
sha256sum debian-13-genericcloud-amd64.qcow2
qemu-img info debian-13-genericcloud-amd64.qcow2

# Step 3–4: appliance preflight
bash deploy/appliance/check.sh                                 # 16 / 16 OK
bash deploy/appliance/build-image.sh   --dry-run               # plan printed, exit 0
bash deploy/appliance/smoke/smoke-local.sh --dry-run           # plan printed, exit 0

# Smoke key + cloud-init seed (outside repo, real public key, never committed)
mkdir -p ~/icn-appliance-build/smoke
ssh-keygen -t ed25519 -f ~/icn-appliance-build/smoke/icn-smoke-key -N '' -C 'icn-appliance-smoke-2026-05-21'
cp deploy/appliance/smoke/cloud-init/user-data.example.yaml  ~/icn-appliance-build/smoke/user-data.smoke.yaml
# replaced INVALIDREPLACEME line with the real ed25519 public key (python3 substitution)
cp deploy/appliance/smoke/cloud-init/meta-data.example.yaml  ~/icn-appliance-build/smoke/meta-data.smoke.yaml
cloud-localds ~/icn-appliance-build/smoke/seed.iso \
              ~/icn-appliance-build/smoke/user-data.smoke.yaml \
              ~/icn-appliance-build/smoke/meta-data.smoke.yaml

# Step 5 (first attempt): real build — failed on virt-customize / supermin / kernel readability
env LIBGUESTFS_BACKEND=direct PATH="$HOME/.cargo/bin:$PATH" \
    ICN_APPLIANCE_BASE_IMAGE=~/icn-appliance-build/base-images/debian-13-genericcloud-amd64.qcow2 \
    ICN_APPLIANCE_BASE_SHA256=f8573792e38e6d8a5ba701759e5ff96792e4c7ebca3721394f548106f42aeb34 \
    ICN_APPLIANCE_OUTPUT_DIR=~/icn-appliance-build/images \
    ICN_APPLIANCE_VERSION=0.0.1-dev-trixie-2026-05-21 \
    bash deploy/appliance/build-image.sh --real
# cargo release build: OK in 15m23s; virt-customize: FAIL
# supermin: cp -p '/boot/vmlinuz-6.8.0-117-generic' ...: Permission denied
# (mode 600, root-only — supermin auto-picks the highest installed kernel)

# Step 5 (diagnosis): probe with supermin env-override pointed at the running kernel
SUPERMIN_KERNEL=/boot/vmlinuz-$(uname -r) \
SUPERMIN_MODULES=/lib/modules/$(uname -r) \
LIBGUESTFS_BACKEND=direct \
libguestfs-test-tool   # ===== TEST FINISHED OK =====

# Step 5 (fix, operator-authorized minimal host mutation):
stat -c '%a %U %G %n' /boot/vmlinuz-$(uname -r)   # before: 600 root root /boot/vmlinuz-6.8.0-111-generic
sudo chmod 0644       /boot/vmlinuz-$(uname -r)
stat -c '%a %U %G %n' /boot/vmlinuz-$(uname -r)   # after:  644 root root /boot/vmlinuz-6.8.0-111-generic
rm -f ~/icn-appliance-build/images/icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.qcow2

# Step 5 (retry): real build — passed
env LIBGUESTFS_BACKEND=direct \
    SUPERMIN_KERNEL=/boot/vmlinuz-$(uname -r) \
    SUPERMIN_MODULES=/lib/modules/$(uname -r) \
    PATH="$HOME/.cargo/bin:$PATH" \
    ICN_APPLIANCE_BASE_IMAGE=~/icn-appliance-build/base-images/debian-13-genericcloud-amd64.qcow2 \
    ICN_APPLIANCE_BASE_SHA256=f8573792e38e6d8a5ba701759e5ff96792e4c7ebca3721394f548106f42aeb34 \
    ICN_APPLIANCE_OUTPUT_DIR=~/icn-appliance-build/images \
    ICN_APPLIANCE_VERSION=0.0.1-dev-trixie-2026-05-21 \
    bash deploy/appliance/build-image.sh --real

# Step 6: real one-VM smoke
env ICN_APPLIANCE_IMAGE=~/icn-appliance-build/images/icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.qcow2 \
    ICN_APPLIANCE_SSH_KEY=~/icn-appliance-build/smoke/icn-smoke-key \
    ICN_APPLIANCE_CLOUD_INIT_SEED=~/icn-appliance-build/smoke/seed.iso \
    ICN_APPLIANCE_VM_TIMEOUT=600 \
    bash deploy/appliance/smoke/smoke-local.sh --real
```

No `git push`, no `gh pr create`, no `kubectl`, no `make` against `deploy/k8s/`, no devnet touch, no host package install or removal, no DNS / Forgejo / K3s / networking mutation. The only host-state change this session made was the operator-authorized `chmod 0644 /boot/vmlinuz-6.8.0-111-generic`.

## Build result

<!-- TRUTH TYPE: Implementation truth — last 25 lines of the build log were inspected this session -->

**PASS** on the retry. First attempt failed on a host-side libguestfs/supermin permission, not on a script bug, image incompatibility, or glibc skew.

| Field | Value |
|---|---|
| Output image | `/home/ubuntu/icn-appliance-build/images/icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.qcow2` |
| Output image size | 1.1 GiB |
| Output image SHA256 | `e6888dd512d4cf718a7b9d2bb208a0a743654aca1c0fcda1d4c3fa79aa4e6f51` |
| Manifest JSON | `/home/ubuntu/icn-appliance-build/images/icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.manifest.json` |
| `git_commit` in manifest | `00b3224aa778c518604a41f1dfd2b3ba0c1d1dec` |
| `non_production` flag | `true` (matches expectation) |
| `signed` flag | `false` (matches expectation) |
| `immutable` flag | `false` (matches expectation) |
| `icnd` source SHA256 | `71151408b09c6bda9157c1ea025c2e35a761d6f8f6350768178f2898c5d6783e` |
| `icnctl` source SHA256 | `3c595aae2a274b2b1e2494797c00822297e4dec59f770b6122219ef0dffa3d3b` |
| Wall clock (1st attempt: cargo OK, virt-customize FAIL) | 15m 35s |
| Wall clock (2nd attempt: cargo cache hot, full success) | 2m 58s |
| Total elapsed across both attempts | ≈ 18 min |
| Final exit code | `0` |

Build log: `~/icn-appliance-build/logs/build-20260522T023239Z.log` (retained on host, not committed).

**Non-fatal warning emitted by `virt-sysprep`:**

```
libguestfs: warning: current user is not a member of the KVM group (group ID 993).
This user cannot access /dev/kvm, so libguestfs may run very slowly.
```

Confirmed: `ubuntu` is in `ubuntu,adm,cdrom,sudo,dip,lxd,docker` — not in `kvm` or `libvirt`. libguestfs falls back to TCG; build still completes. Matches the README's existing "KVM permission denied is non-fatal" note.

## Smoke result

<!-- TRUTH TYPE: Implementation truth — full smoke log inspected this session -->

**PASS** end-to-end.

| Field | Value |
|---|---|
| Mode | `--real` |
| VM SSH | `debian@127.0.0.1:2222` (default) |
| Health surface | `http://127.0.0.1:8080/v1/health` (via SSH inside VM) |
| VM memory | 1024 MiB (default) |
| VM cpus | 2 (default) |
| QEMU acceleration | `accel=kvm:tcg` — fell back to TCG (no `kvm` group); cleanly handled |
| Cloud-init seed | operator-supplied (`~/icn-appliance-build/smoke/seed.iso`); placeholder check passed because the example was copied + edited outside the repo |
| Wall clock (start → PASS) | 1m 38s (`0s past start of health wait` — health succeeded on first try) |
| Final exit code | `0` |

**Per-check evidence (smoke log excerpts):**

```
[appliance-smoke] Working dir: /tmp/icn-smoke.2YPf56
[appliance-smoke] Using operator-supplied cloud-init seed: …/seed.iso
[appliance-smoke] Creating disposable overlay … (backing format: qcow2) ...
[appliance-smoke] Launching QEMU (user-mode net, hostfwd 2222->22)...
[appliance-smoke] QEMU pid: 715741
[appliance-smoke] Waiting for SSH (up to 600s)...
[appliance-smoke] SSH is up.
[appliance-smoke] Verifying icn-appliance-firstboot.service ran (oneshot; check marker)...
[appliance-smoke] firstboot marker present.
[appliance-smoke] Waiting for icnd.service to become active (bounded)...
[appliance-smoke] icnd.service is active.
[appliance-smoke] Verifying /v1/health on port 8080 from inside the VM...
[appliance-smoke] /v1/health returned 200.
[appliance-smoke] PASS
[appliance-smoke] Terminating QEMU (pid 715741)...
```

**Benign warning observed inside the VM:**
```
bash: warning: setlocale: LC_ALL: cannot change locale (en_US.UTF-8): No such file or directory
```
Debian cloud images ship `C.UTF-8` only; the smoke script does not require `en_US.UTF-8`. Not a failure.

Smoke log: `~/icn-appliance-build/logs/smoke-20260522T023611Z.log` (retained on host, not committed).

## Firstboot / icnd gate result

<!-- TRUTH TYPE: Implementation truth — the positive path was exercised; the negative path was NOT -->

**Positive path verified.** The image's `icn-appliance-firstboot.service` ran on first boot, wrote `/var/lib/icn/.firstboot-complete`, and the `icnd.service.d/10-firstboot-gate.conf` drop-in's `Requires=` / `After=` / `ConditionPathExists=` allowed `icnd.service` to start. `systemctl is-active icnd` returned active, and `/v1/health` returned 200.

**Negative path NOT verified this session.** The fail-closed property (i.e. "if firstboot fails, `icnd` must not start in a half-provisioned state") was not directly exercised — we did not intentionally break firstboot. This is recorded under "Follow-ups" as a useful next test that does not block the slice.

The per-instance secrets contract was indirectly verified by smoke success: the image contains no embedded keystore, so `icnd` could only have reached "active" because firstboot's `openssl rand` / `icnd --init` path produced a valid `/etc/icn/icnd.env` and keystore on this specific VM instance. No `/etc/icn/icnd.env` content was inspected this session.

## Artifacts

All generated artifacts live **outside the repo** under `~/icn-appliance-build/` on `icn-dev`. None are committed.

```
~/icn-appliance-build/
├── base-images/
│   ├── debian-13-genericcloud-amd64.qcow2        # 325 MiB, official Debian 13 trixie
│   ├── SHA512SUMS                                # from cloud.debian.org
│   ├── SHA256_LOCAL.txt                          # SHA256 computed this session
│   └── download-timestamp.txt
├── images/
│   ├── icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.qcow2          # 1.1 GiB, built this session
│   └── icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.manifest.json  # 1.3 KiB
├── smoke/
│   ├── icn-smoke-key       # ed25519 private; mode 600; smoke-only
│   ├── icn-smoke-key.pub   # ed25519 public; mode 644
│   ├── user-data.smoke.yaml # edited copy of the repo example, real pubkey inlined
│   ├── meta-data.smoke.yaml # unchanged copy of the repo example
│   └── seed.iso            # cidata cloud-init seed
└── logs/
    ├── build-20260522T021135Z.log         # first build attempt (cargo OK, virt-customize FAIL)
    ├── build-20260522T021135Z.log.timing  # /usr/bin/time -v
    ├── build-20260522T023239Z.log         # successful build
    ├── build-20260522T023239Z.log.timing
    ├── smoke-20260522T023611Z.log         # successful smoke
    └── smoke-20260522T023611Z.log.timing
```

**Verified clean:** `git status --short` is empty on this branch; `find . -name '*.qcow2' -o -name 'seed.iso' -o -name 'icn-smoke-key*'` returns nothing inside the repo working tree.

## Changes made

| Plane | What |
|---|---|
| Repo files | **One new doc** — this handoff (`docs/dev/handoff-2026-05-21-appliance-debian13-real-smoke.md`). No code, no script, no schema, no other docs touched. |
| Host state (operator-authorized) | `chmod 0644 /boot/vmlinuz-6.8.0-111-generic` (was `600 root root`, now `644 root root`). Mirrors the doctrine the appliance README already records under WSL2 quirks; recorded explicitly so the change can be reverted if undesired. |
| Local-only working artifacts | Everything under `~/icn-appliance-build/` (see "Artifacts"). None committed. |

## Unsafe assumptions

<!-- Explicitly named anything this session relied on but did not verify -->

- **`SHA512SUMS.sign` not available.** `https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS.sign` returned HTTP 404. The trust chain for the base image is therefore "TLS to `cloud.debian.org`" + per-file SHA512 match, not a PGP-verified `SHA512SUMS`. If a stronger chain of trust is required, the next operator should fetch the signed manifest from a directory that publishes it and re-verify.
- **Negative-path firstboot gate not exercised.** We verified that `icnd` *does* start when firstboot succeeds; we did **not** verify that `icnd` *refuses* to start when firstboot fails. The fail-closed property is structurally guaranteed by the systemd `Requires=` / `ConditionPathExists=` semantics, but it was not directly tested in this session.
- **`SUPERMIN_KERNEL` pinning is per-session.** The build worked because we pinned supermin to `/boot/vmlinuz-$(uname -r)` and chmod'd that specific file. If the host reboots into `6.8.0-117-generic`, the chmod no longer matches what supermin auto-picks, and the next operator either re-runs the same authorized fix on the new running kernel or repeats the `SUPERMIN_KERNEL`/`SUPERMIN_MODULES` override.
- **`/etc/icn/icnd.env` content not inspected.** Smoke success implies firstboot wrote a valid env file (otherwise `icnd` would not have started), but the file's mode/owner/contents were not explicitly read this session.
- **No verification across reboots.** The VM was booted once and torn down. We did not exercise a second boot to confirm the firstboot marker correctly short-circuits a re-run, or that a re-issued JWT is not produced.
- **No CI signal.** No CI workflow exercises appliance build or smoke. The smoke log lives only on `icn-dev`. The next operator either re-runs the smoke locally or accepts this session's evidence.

## Follow-ups

Kept tight; no roadmap, no broadening.

- **Immediate (none).** The slice succeeded; nothing is blocking the next consumer of this work.
- **Next hardening step (docs-only, separate PR).** Broaden the README §"WSL2 quirks" section into a generic "Host quirks (libguestfs / supermin)" section that records:
  - Some Ubuntu hosts ship `/boot/vmlinuz-*` mode `600` regardless of WSL2 (icn-dev demonstrated this on Ubuntu 24.04 native). The `chmod 0644` recipe is not WSL2-specific.
  - `SUPERMIN_KERNEL=/boot/vmlinuz-$(uname -r)` plus `SUPERMIN_MODULES=/lib/modules/$(uname -r)` is the no-broader-chmod alternative when supermin's auto-pick chooses a newer-installed kernel the operator does not want to chmod.
  - `libvirtd` is not required if `LIBGUESTFS_BACKEND=direct` is exported.
  - This is a docs-only follow-up; it is **deliberately not bundled here** to avoid scope creep beyond verification.
- **Future hardening (separate PR, not started).** Add a negative-path firstboot-gate test (intentionally fail the firstboot oneshot, confirm `icnd` does not start, capture `journalctl`) to lock in the fail-closed property as evidence rather than as a derived guarantee.
- **Future CI dry-run gate.** Wiring `deploy/appliance/check.sh` + `build-image.sh --dry-run` + `smoke-local.sh --dry-run` into the CI matrix would prevent silent bit-rot of the scaffold. **Explicitly out of scope for this slice** (slice contract: no CI integration).

---

## Truth-Plane Notes

- **Implementation truth** in this handoff is derived from commands run on `icn-dev` between `2026-05-22T02:00Z` and `2026-05-22T02:40Z` against `origin/main @ 00b3224aa`. Re-running the same recipe against the same SHA on the same host should reproduce the same SHA256s for the input base image and the built appliance image (modulo `git_commit`, `build_timestamp_utc`, and binary hashes if the cargo build cache differs).
- **Execution truth** is the command sequence in §"Commands run".
- **Declared project truth** is **unchanged**. This session ratified no ADR, no architectural primitive, and no acceptance bar beyond what `deploy/appliance/smoke/README.md` already declares.
- **Honest non-claims:** appliance is still not production, not signed, not immutable, not partner-distributable. No federation contact occurred. NYCN was not activated. No K3s, DNS, devnet, or homelab networking was touched.
