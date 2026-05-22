# Session Handoff — 2026-05-21 — Appliance Debian 13 Real-Smoke Verification

**Topic:** Operator-authorized end-to-end verification of the appliance dev-image path landed by PRs #1865 / #1866, against the official Debian 13 trixie cloud base.
**Branch:** `docs/appliance-real-smoke-debian13-2026-05-21`
**Base:** `origin/main @ 00b3224aa778c518604a41f1dfd2b3ba0c1d1dec` (`feat(rpc,gateway): mint governance class-level scope constants (#1868 step 1) (#1881)`)
**Closes:** none.
**Refs:** PR #1865 (appliance substrate, squash-merged), PR #1866 (real QCOW2 build + boot smoke, squashed into #1865), PR #1876 (prior operator handoff documenting verified host toolchain + base-image staging gap), PR #1879 (README drift fix).

This handoff is docs-only. **No runtime code change, no script change, no schema mint, no ADR, no RFC, no new contract URN, no production-readiness claim, no live-federation claim, no NYCN activation claim.** One real `--real` QCOW2 was built on `icn-dev`, one disposable QEMU VM was booted from it, and the four positive-path acceptance checks (`SSH` → `firstboot marker` → `icnd active` → `/v1/health 200`) all passed.

---

## Session Goal

Stage the official Debian 13 trixie genericcloud base image on `icn-dev`, run `deploy/appliance/build-image.sh --real` and `deploy/appliance/smoke/smoke-local.sh --real` end-to-end against it, and produce one operator handoff recording exactly what was verified and what blockers (if any) appeared — so the substrate-acceptance bar for the appliance moves from "Unbuilt scaffold" to "Bootable dev image" on the Debian 13 path the operator selected.

## Decisive Test

This handoff fails if any of the following holds:

1. It claims the appliance is production-ready, signed, immutable, or fit for partner federation.
2. It claims a real `--real` QCOW2 was *not* built or that the build did not pass — when in fact it did.
3. It claims the smoke passed but cannot reproduce the four positive-path checks (SSH → firstboot marker → `icnd` active → `/v1/health 200`).
4. It claims NYCN activation, live federation contact, or any partner data was touched.
5. It mutates K3s, DNS, Forgejo, devnet, or homelab networking.
6. It commits a generated QCOW2, manifest, seed ISO, SSH key, or build/smoke log.
7. It introduces any secret material into the repo, logs, generated images, cloud-init examples, or docs.
8. It uses payment / wallet / currency / balance / token / blockchain / crypto / timebank framing for ICN-native primitives outside negation or legacy-code discussion.
9. The recorded command transcript in §"Verification Commands" cannot be replayed against `origin/main @ 00b3224aa` on the same `icn-dev` host to reproduce the same SHA256s for the input base image and the built appliance image (modulo `git_commit`, `build_timestamp_utc`, and binary hashes if the cargo cache differs).
10. The host-state change attributed to operator authorization is not exactly `chmod 0644 /boot/vmlinuz-$(uname -r)` — i.e., it does not silently chmod more files than that.

## Operator decision

Debian 13 trixie was selected as the primary ICN appliance base. Ubuntu 24.04 noble remains fallback only. Debian 13 was not blocked by any concrete failure in this session; no fallback was needed.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands run from inside the repo or by inspection of artifacts produced by those commands -->

### `main` HEAD at session start

`00b3224aa778c518604a41f1dfd2b3ba0c1d1dec` — `feat(rpc,gateway): mint governance class-level scope constants (#1868 step 1) (#1881)`.

`git fetch origin && git checkout -b docs/appliance-real-smoke-debian13-2026-05-21 origin/main` created this branch from that SHA. The same SHA is recorded as `git_commit` in the built appliance image's manifest JSON.

### Open PRs

| PR | Branch | State | CI status (at handoff) |
|----|--------|-------|------------------------|
| **#1900** (this) | `docs/appliance-real-smoke-debian13-2026-05-21` | OPEN | all required checks green; pending only review feedback resolution |

### Branches

- `docs/appliance-real-smoke-debian13-2026-05-21` — working branch, head SHA at first push `0782b10dd` (this handoff doc as first commit).
- Previously merged appliance PRs (unchanged): #1865, #1866, #1876, #1879.

### Base image

| Field | Value |
|---|---|
| **Source** | `https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2` (official Debian cloud images) |
| **Filename** | `debian-13-genericcloud-amd64.qcow2` |
| **Local path (outside repo)** | `/home/ubuntu/icn-appliance-build/base-images/debian-13-genericcloud-amd64.qcow2` |
| **On-disk size** | 325 MiB (`340656128` bytes) |
| **Virtual size** | 3 GiB (`3221225472` bytes), `qcow2`, cluster 65536, zlib |
| **SHA512 (Debian-published)** | `7752ad2adce1bc49dd964dae8300ed7a239d0bf3c13112f55953b111447fe642d2cc01afeead234aa6ebe3605513f2e7c0e7c56785d675c38ff40110d5c8332b` |
| **SHA256 (computed locally; used as `ICN_APPLIANCE_BASE_SHA256`)** | `f8573792e38e6d8a5ba701759e5ff96792e4c7ebca3721394f548106f42aeb34` |
| **Download verified at** | `2026-05-22T02:07:52Z` |

**Trust-chain limitation:** `https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS.sign` returned HTTP 404 from the same official source, so the `SHA512SUMS` file itself was not PGP-verified. Trust chain for this run rests on TLS to `cloud.debian.org` plus per-file SHA512 verification against `SHA512SUMS`. Recorded under §"Unsafe Assumptions"; not resolved in this session.

### Host environment

| Field | Value | Source command |
|---|---|---|
| Host | `icn-dev` | `hostname` |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) | `cat /etc/os-release` (carried from prior handoff #1876, unchanged) |
| Kernel (running) | `6.8.0-111-generic x86_64` | `uname -r` |
| Kernel (newest installed) | `6.8.0-117-generic` | `ls /boot/vmlinuz-*` |
| glibc | `2.39` (Ubuntu GLIBC 2.39-0ubuntu8.7) | `ldd --version` |
| `qemu-img` | `8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.16)` | `qemu-img --version` |
| `virt-customize` | `1.52.0` | `virt-customize --version` |
| `virt-sysprep` | `1.52.0` | `virt-sysprep --version` |
| `qemu-system-x86_64` | `8.2.2` | `qemu-system-x86_64 --version` |
| `cloud-localds` | from `cloud-image-utils` (no `--version` flag; presence confirmed via `--help`) | `cloud-localds --help` |
| `rustc` | `1.95.0 (59807616e 2026-04-14)` | `rustc --version` (after `source ~/.cargo/env`) |
| `cargo` | `1.95.0 (f2d3ce0bd 2026-03-21)` | `cargo --version` |
| Rust toolchain pin | `1.95.0` | `icn/rust-toolchain.toml` |
| `/dev/kvm` | `crw-rw---- root kvm` (user `ubuntu` not in group `kvm`) | `ls -l /dev/kvm` |

### Build result

**PASS** on retry (first attempt failed on a host-side libguestfs/supermin permission, not a script bug, image incompatibility, or glibc skew).

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
| Wall clock — 1st attempt (cargo OK, virt-customize FAIL) | 15m 35s |
| Wall clock — 2nd attempt (cargo cache hot, full success) | 2m 58s |
| Total elapsed across both attempts | ≈ 18 min |
| Final exit code | `0` |

Build log: `~/icn-appliance-build/logs/build-20260522T023239Z.log` (retained on host, not committed).

**Non-fatal warning emitted by `virt-sysprep`:**

```
libguestfs: warning: current user is not a member of the KVM group (group ID 993).
This user cannot access /dev/kvm, so libguestfs may run very slowly.
```

Confirmed: `ubuntu` is in `ubuntu,adm,cdrom,sudo,dip,lxd,docker` — not in `kvm` or `libvirt`. libguestfs falls back to TCG; build still completes. Matches the README's existing "KVM permission denied is non-fatal" note.

### Smoke result

**PASS** end-to-end.

| Field | Value |
|---|---|
| Mode | `--real` |
| VM SSH | `debian@127.0.0.1:2222` (default) |
| Health surface | `http://127.0.0.1:8080/v1/health` (via SSH inside VM) |
| VM memory | 1024 MiB (default) |
| VM cpus | 2 (default) |
| QEMU acceleration | `accel=kvm:tcg` — fell back to TCG (host user not in `kvm` group); cleanly handled |
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

### Firstboot / icnd gate result

**Positive path verified.** The image's `icn-appliance-firstboot.service` ran on first boot, wrote `/var/lib/icn/.firstboot-complete`, and the `icnd.service.d/10-firstboot-gate.conf` drop-in's `Requires=` / `After=` / `ConditionPathExists=` allowed `icnd.service` to start. `systemctl is-active icnd` returned active and `/v1/health` returned 200.

**Negative path NOT verified this session.** The fail-closed property (i.e. "if firstboot fails, `icnd` must not start in a half-provisioned state") was not directly exercised — we did not intentionally break firstboot. This is recorded under §"What's Open" as the next useful test that does not block this slice.

The per-instance-secrets contract is indirectly verified by smoke success: the image contains no embedded keystore, so `icnd` could only have reached "active" because firstboot's `openssl rand` / `icnd --init` path produced a valid `/etc/icn/icnd.env` and keystore on this specific VM instance. No `/etc/icn/icnd.env` content was inspected this session.

### Artifacts (not committed)

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

The exclusion-from-repo claim is itself verified — see §"Verification Commands" (`git status --short` + `find ... -name '*.qcow2' …`).

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. New file: `docs/dev/handoff-2026-05-21-appliance-debian13-real-smoke.md`

This file. Docs-only. Initial commit `0782b10dd docs(appliance): record Debian 13 real-smoke verification` on branch `docs/appliance-real-smoke-debian13-2026-05-21`, opened as PR #1900. No other path was modified in the repo.

### 2. Operator-authorized host-state change

`chmod 0644 /boot/vmlinuz-6.8.0-111-generic` (was `600 root root`, now `644 root root`). Mirrors the doctrine the appliance README already records under WSL2 quirks; recorded explicitly so the change can be reverted if undesired. No other files chmod'd. No package installed or removed. No sysctl / systemd unit / firewall mutated.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [ ] **Negative-path firstboot-gate test.** Intentionally fail the firstboot oneshot, confirm `icnd` does not start, capture `journalctl`. Converts the fail-closed property from "structurally guaranteed by systemd `Requires=` semantics" to "exercised evidence." Not started.
- [ ] **README §"Host quirks (libguestfs / supermin)" generalization.** Broaden the README's WSL2-quirks section to make explicit that the `chmod 0644 /boot/vmlinuz-*` recipe applies on native Ubuntu hosts too (icn-dev demonstrated this on Ubuntu 24.04), and that `SUPERMIN_KERNEL=/boot/vmlinuz-$(uname -r)` + `SUPERMIN_MODULES=/lib/modules/$(uname -r)` is the no-broader-chmod alternative when supermin's auto-pick chooses a newer-installed kernel the operator does not want to chmod. Deliberately separate from this verification PR.
- [ ] **CI dry-run gate for appliance scaffold.** Wiring `deploy/appliance/check.sh` + `--dry-run` of `build-image.sh` and `smoke-local.sh` into the CI matrix would prevent silent bit-rot. Explicitly out of scope per the current slice contract (slice declares "No CI integration").
- [ ] **Trust-chain follow-up.** If a stronger chain of trust than "TLS + SHA512" is required for the base image, locate a Debian cloud directory that publishes `SHA512SUMS.sign` for the chosen trixie build and re-verify there.

---

## Unsafe Assumptions
<!-- Explicitly named anything this session relied on but did not verify -->

- **`SHA512SUMS.sign` not available.** `https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS.sign` returned HTTP 404. The trust chain for the base image is therefore "TLS to `cloud.debian.org`" + per-file SHA512 match, not a PGP-verified `SHA512SUMS`.
- **Negative-path firstboot gate not exercised.** We verified that `icnd` *does* start when firstboot succeeds; we did **not** verify that `icnd` *refuses* to start when firstboot fails. The fail-closed property is structurally guaranteed by the systemd `Requires=` / `ConditionPathExists=` semantics, but it was not directly tested in this session.
- **`SUPERMIN_KERNEL` pinning is per-session.** The build worked because we pinned supermin to `/boot/vmlinuz-$(uname -r)` and chmod'd that specific file. If the host reboots into `6.8.0-117-generic`, the chmod on 111 no longer matches what supermin auto-picks, and the next operator either re-runs the same authorized fix on the new running kernel or repeats the `SUPERMIN_KERNEL` / `SUPERMIN_MODULES` override.
- **`/etc/icn/icnd.env` content not inspected.** Smoke success implies firstboot wrote a valid env file (otherwise `icnd` would not have started), but the file's mode / owner / contents were not explicitly read this session.
- **No verification across reboots.** The VM was booted once and torn down. We did not exercise a second boot to confirm the firstboot marker correctly short-circuits a re-run, or that a fresh JWT is not re-issued.
- **No CI signal.** No CI workflow exercises appliance build or smoke. The build / smoke logs live only on `icn-dev`. The next operator either re-runs the smoke locally or accepts this session's evidence.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Resolve any remaining review feedback on PR #1900 (this PR). At handoff time, all required CI checks were green; the Copilot review surfaced three substantive comments addressed by the second commit on this branch.
2. Once review threads are resolved and CI remains green, squash-merge PR #1900.
3. (Optional next slice — recommended.) Open a new branch `feat/appliance-negative-path-firstboot-smoke` or similar, extend `smoke-local.sh` (or add a sibling script under `deploy/appliance/smoke/`) to exercise the negative path: boot a disposable VM, deliberately fail firstboot (e.g. inject a sentinel file the firstboot script treats as fatal, or remove a prerequisite), and assert `icnd.service` stays inactive. Bound the wait, capture `journalctl`, exit non-zero only if `icnd` started.
4. (Separate optional slice.) Open a docs-only PR broadening the README §"WSL2 quirks" into a generic §"Host quirks (libguestfs / supermin)" section per §"What's Open" above.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

**None.** This session ratified no ADR, no new architectural primitive, no acceptance bar beyond what `deploy/appliance/smoke/README.md` already declares, and no schema mint. Declared project truth is unchanged. The Debian-13 base-image selection is recorded under §"Operator decision" but is a verification-substrate choice, not a project-architecture commitment.

---

## Verification Commands
<!-- TRUTH TYPE: Execution truth — every state-changing or evidence-gathering command in this session, in order, including the `cd` transitions needed to reproduce -->

```bash
# Repo root, used as $REPO_ROOT below.
cd /home/ubuntu/projects/icn/.claude/worktrees/wonderful-sammet-c03619
REPO_ROOT="$(git rev-parse --show-toplevel)"

# Step 0–1: orient, branch
git fetch origin
git checkout -b docs/appliance-real-smoke-debian13-2026-05-21 origin/main

# Step 2: stage Debian 13 trixie base image (outside repo)
mkdir -p ~/icn-appliance-build/base-images
cd ~/icn-appliance-build/base-images
date -u +'%Y-%m-%dT%H:%M:%SZ' > download-timestamp.txt
curl -fsSL -o SHA512SUMS https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS
curl -fsSL -o SHA512SUMS.sign https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS.sign   # HTTP 404 (not published in trixie/latest)
curl -fSL  -o debian-13-genericcloud-amd64.qcow2 \
    https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2
sha512sum -c SHA512SUMS --ignore-missing      # debian-13-genericcloud-amd64.qcow2: OK
sha256sum debian-13-genericcloud-amd64.qcow2 | tee SHA256_LOCAL.txt
qemu-img info debian-13-genericcloud-amd64.qcow2

# Return to the repo working tree before invoking any repo-relative tools.
cd "$REPO_ROOT"

# Step 3: record host toolchain (uname, ldd, qemu, virt-customize, virt-sysprep, qemu-system, cloud-localds, rustc, cargo, ssh, curl, sha256sum, python3, /dev/kvm, group membership).
. ~/.cargo/env   # rustc/cargo not on default PATH; rust-toolchain.toml pin is 1.95.0

# Step 4: appliance preflight (all from repo root)
bash deploy/appliance/check.sh                                 # 16 / 16 OK
bash deploy/appliance/build-image.sh   --dry-run               # plan printed, exit 0
bash deploy/appliance/smoke/smoke-local.sh --dry-run           # plan printed, exit 0

# Smoke key + cloud-init seed (outside repo; real public key never committed)
mkdir -p ~/icn-appliance-build/smoke
ssh-keygen -t ed25519 -f ~/icn-appliance-build/smoke/icn-smoke-key -N '' -C 'icn-appliance-smoke-2026-05-21'
cp deploy/appliance/smoke/cloud-init/user-data.example.yaml  ~/icn-appliance-build/smoke/user-data.smoke.yaml
# Replace the INVALIDREPLACEME line in ~/icn-appliance-build/smoke/user-data.smoke.yaml with the real ed25519 public key (done via a python3 substitution; key body intentionally not reproduced here).
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
#   supermin: cp -p '/boot/vmlinuz-6.8.0-117-generic' …: Permission denied
#   (file mode 600 root-only; supermin auto-picks the highest installed kernel)

# Step 5 (diagnosis): probe with supermin env-override pointed at the running kernel
SUPERMIN_KERNEL=/boot/vmlinuz-$(uname -r) \
SUPERMIN_MODULES=/lib/modules/$(uname -r) \
LIBGUESTFS_BACKEND=direct \
libguestfs-test-tool                                          # ===== TEST FINISHED OK =====

# Step 5 (fix, operator-authorized minimal host mutation):
stat -c '%a %U %G %n' /boot/vmlinuz-$(uname -r)               # before: 600 root root /boot/vmlinuz-6.8.0-111-generic
sudo chmod 0644       /boot/vmlinuz-$(uname -r)
stat -c '%a %U %G %n' /boot/vmlinuz-$(uname -r)               # after:  644 root root /boot/vmlinuz-6.8.0-111-generic
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

# Step 7: inspect artifacts and git state. The artifact-exclusion claim in §"Artifacts" is the output of these two commands.
git status --short                                            # empty (no working-tree changes outside the new handoff file once staged)
find . -maxdepth 5 \( -name '*.qcow2' -o -name 'seed.iso' -o -name 'icn-smoke-key*' \) \
    2>/dev/null | grep -v node_modules | grep -v target       # empty (no stray artifacts inside the repo)
ls -la ~/icn-appliance-build/                                 # confirms artifact dirs live outside repo
du -sh ~/icn-appliance-build/*/                               # confirms sizes recorded in §"Artifacts"

# Step 9: docs-only validation chain (same chain used by the prior handoff PR #1876)
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 .github/scripts/compliance_linter.py
git diff --check
```

No `git push` line is included here; pushing this branch is recorded in PR #1900's commit log directly, and `/push` ran the scoped preflight gates (drift-check PASS; no Rust files touched, so Rust gates were correctly skipped per scope). No `kubectl`, no `make` against `deploy/k8s/`, no devnet touch, no host package install or removal, no DNS / Forgejo / K3s / networking mutation occurred this session. The only host-state change is the operator-authorized `chmod 0644` recorded above.

---

## Truth-Plane Notes

- **Implementation truth** in this handoff is derived from commands run on `icn-dev` between `2026-05-22T02:00Z` and `2026-05-22T02:40Z` against `origin/main @ 00b3224aa`. Re-running the recipe in §"Verification Commands" against the same SHA on the same host should reproduce the same SHA256s for the input base image and the built appliance image (modulo `git_commit`, `build_timestamp_utc`, and binary hashes if the cargo build cache differs).
- **Execution truth** is the command sequence in §"Verification Commands".
- **Declared project truth** is **unchanged.** This session ratified no ADR, no architectural primitive, and no acceptance bar beyond what `deploy/appliance/smoke/README.md` already declares.
- **Honest non-claims:** appliance is still not production, not signed, not immutable, not partner-distributable. No federation contact occurred. NYCN was not activated. No K3s, DNS, devnet, or homelab networking was touched.
