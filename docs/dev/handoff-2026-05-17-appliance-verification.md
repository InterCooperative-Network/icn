# Session Handoff — 2026-05-17 — Appliance Verification

**Topic:** Operator handoff recording verified host toolchain, missing base-image staging, and the exact next authorized verification step for the Debian appliance dev image (PR #1865 substrate).
**Branch:** `docs/appliance-verification-handoff-2026-05-17`
**Base:** `origin/main @ f9a98a2f4` (`test(devnet): add RedundancyProof Slice B fixture`)
**Closes:** none.
**Refs:** PR #1865 (appliance substrate, squash-merged), PR #1864 (abuse-case strategy), PR #1874 (RedundancyProof Slice B fixture).

This handoff is docs-only. **No runtime change, no script change, no schema mint, no ADR, no RFC, no new contract URN, no production-readiness claim, no live-federation claim, no NYCN activation claim, no real QCOW2 build claim, no smoke-pass claim.** The deliverable is one new file under `docs/dev/` that records what is verifiably true about appliance build/smoke readiness on the `icn-dev` host as of `main @ f9a98a2f4`, and the single next operator-authorized verification command.

---

## Session Goal

Inspect the appliance substrate landed by PR #1865 against the live `icn-dev` host, record exactly which `--real` prerequisites are present and which are missing, and produce one handoff doc so the next session can resume Option C (real local build + smoke) without re-running discovery.

---

## Decisive Test

This handoff fails if any of the following holds:

1. It claims the appliance is production-ready, signed, immutable, or fit for partner federation.
2. It claims a real QCOW2 was built in this session.
3. It claims a real local smoke (`smoke-local.sh --real`) was executed or passed in this session.
4. It claims NYCN activation, live federation contact, or any partner data was touched.
5. It mutates K3s, DNS, Forgejo, devnet, homelab networking, or any host package state.
6. It installs host packages, downloads a base image, or stages any artifact outside the repo working tree.
7. It introduces any secret material into the repo, logs, generated images, cloud-init examples, or docs.
8. It uses payment / wallet / currency / balance / token / blockchain / crypto / timebank framing for ICN-native primitives outside negation or legacy-code discussion.
9. It modifies any runtime code, gateway/kernel/actor/handler, or any `deploy/appliance/` script or unit file.
10. The commit SHA, tool versions, or glibc reading recorded below cannot be reproduced from `main @ f9a98a2f4` on the same `icn-dev` host.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands run from inside the repo. -->

### `main` HEAD at session start

`f9a98a2f40f4d47d4d262133b67fb5a570895a35` — `test(devnet): add RedundancyProof Slice B fixture`

`git pull --ff-only origin main` reported `Already up to date.` against `origin/main`. Working tree was clean before any new file was added in this session.

### Files inspected (read-only, no mutation)

| Path | Purpose |
|---|---|
| `deploy/appliance/README.md` | Scaffold overview, two-path usage, glibc and WSL2 notes |
| `deploy/appliance/build-image.sh` | `--dry-run` and `--real` QCOW2 builder (PR #1865) |
| `deploy/appliance/check.sh` | Local pre-commit validation (bash -n, dry-runs, YAML, vocab, secrets) |
| `deploy/appliance/smoke/smoke-local.sh` | `--dry-run` and `--real` one-VM QEMU+SSH smoke (PR #1865) |
| `deploy/appliance/smoke/README.md` | Smoke acceptance bar, non-claims, gotchas |
| `deploy/appliance/appliance.manifest.example.yaml` | Manifest template (existence only) |
| `deploy/appliance/roles/*.example.yaml` | Five role examples (existence only) |
| `deploy/appliance/scripts/icn-appliance-firstboot.sh` | Firstboot oneshot (existence only) |
| `deploy/appliance/systemd/icn-appliance-firstboot.service` | Firstboot unit (existence only) |
| `deploy/appliance/systemd/icnd.service.d/10-firstboot-gate.conf` | Drop-in gating `icnd.service` on firstboot (existence only) |
| `docs/architecture/DEBIAN_APPLIANCE_MODEL.md` | Existence verified; not re-read in full this session |
| `docs/dev/HANDOFF_TEMPLATE.md` | Template used for this handoff |

### Host inventory (verified by command this session)

| Attribute | Value | Source command |
|---|---|---|
| Host | `icn-dev` | `hostname` |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) | `cat /etc/os-release` |
| Kernel | `Linux 6.8.0-111-generic x86_64` | `uname -srm` |
| glibc | `2.39` | `ldd --version \| head -1` |
| Working dir | `/home/ubuntu/projects/icn` | `pwd` |
| Repo HEAD | `f9a98a2f4` | `git log -1` |

### Tool inventory for `--real` (verified by command this session)

| Tool | Present | Path | Version |
|---|---|---|---|
| `qemu-img` | yes | `/usr/bin/qemu-img` | `8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.16)` |
| `virt-customize` | yes | `/usr/bin/virt-customize` | `1.52.0` |
| `virt-sysprep` | yes | `/usr/bin/virt-sysprep` | (libguestfs-tools 1.52.0 series) |
| `cloud-localds` | yes | `/usr/bin/cloud-localds` | (cloud-image-utils) |
| `qemu-system-x86_64` | yes | `/usr/bin/qemu-system-x86_64` | `8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.16)` |
| `cargo` | yes | `/home/ubuntu/.cargo/bin/cargo` | `1.95.0 (f2d3ce0bd 2026-03-21)` |
| `sha256sum` | yes | `/usr/bin/sha256sum` | (coreutils) |
| `ssh` | yes | `/usr/bin/ssh` | (openssh-client) |
| `curl` | yes | `/usr/bin/curl` | (curl) |
| `python3` | yes | `/usr/bin/python3` | (python3) |

Every tool listed in `deploy/appliance/README.md` §"Real local build + boot smoke" and in `smoke-local.sh` §"Required tools for --real" is present on `icn-dev`. No host package install is required to proceed to Option C.

### Base image staging — NOT present

Searched the obvious locations; no Debian / Ubuntu cloud `qcow2` is staged on this host:

- `~/icn-appliance-build/` — does not exist.
- `/var/lib/libvirt/images/*.qcow2` — no matches.
- `find ~ -maxdepth 4 -type f \( -name '*genericcloud*.qcow2' -o -name 'debian-12*.qcow2' -o -name 'debian-13*.qcow2' \)` — no matches.

`build-image.sh --real` requires `ICN_APPLIANCE_BASE_IMAGE` to point at a manually-staged base image. The script does not download anything. Staging is an operator decision.

### glibc skew implication (recorded, not resolved)

Host glibc is `2.39`. Per `deploy/appliance/README.md` §"Host / image compatibility", `icnd` is dynamically linked, so the build host's glibc must be `<=` the appliance base image's glibc, or `icnd` will restart-loop in the booted VM with `libc.so.6: version 'GLIBC_2.x' not found`.

| Candidate base | Base glibc | Compatible with host glibc 2.39? |
|---|---|---|
| Debian 12 bookworm genericcloud | 2.36 | No — needs containerized icnd build, OR pick a newer base. |
| Debian 13 trixie genericcloud | 2.41 | Yes. |
| Ubuntu 22.04 jammy cloud-image | 2.35 | No — same as Debian 12. |
| Ubuntu 24.04 noble cloud-image | 2.39 | Yes (equal, satisfies `<=`). |

This handoff does not pick a base. It records the constraint so the next operator action picks one deliberately.

### CI / workflow surface (verified)

`rg -n "qcow2|virt-customize|virt-sysprep|cloud-localds|appliance" .github` returned no matches. No CI workflow exercises the appliance build or smoke. This is consistent with `deploy/appliance/smoke/README.md` §"What this slice does NOT deliver" — "No CI integration." It does not need to change for this handoff.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. New file: `docs/dev/handoff-2026-05-17-appliance-verification.md`

This file. Docs-only. No other path was modified.

`git status --short` after the write shows exactly one new untracked file under `docs/dev/`. No edits to runtime code, no edits to scripts, no edits to other docs.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [ ] Operator decision: stage a Debian / Ubuntu cloud base image at a chosen path, or defer.
- [ ] Operator decision: pick a base (Debian 13 trixie genericcloud, Ubuntu 24.04 noble cloud-image, or a Debian-12-matching containerized build of `icnd`) that satisfies the glibc-skew constraint above.
- [ ] First real `build-image.sh --real` run on `icn-dev` against a staged base, with `ICN_APPLIANCE_BASE_SHA256` set so the script verifies the base checksum.
- [ ] First real `smoke-local.sh --real` run against the resulting QCOW2.
- [ ] (Adjacent, NOT taken in this session.) `deploy/appliance/README.md` §"Next implementation slice" still describes the `build-image.sh --real` work as future, even though that work landed in PR #1865 and the README's own opening paragraph already says so. This is a known doc inconsistency. The fix is a separate small docs-only PR; deliberately not bundled here to avoid scope creep.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- Assumed `deploy/appliance/scripts/icn-appliance-firstboot.sh`, `systemd/icn-appliance-firstboot.service`, and `systemd/icnd.service.d/10-firstboot-gate.conf` behave as their headers and PR #1865 commit message describe. Their existence was verified by `find`; their runtime behavior was NOT exercised (no VM was booted).
- Assumed `docs/architecture/DEBIAN_APPLIANCE_MODEL.md` is consistent with the README's stage labels. Only existence was verified; full content was not re-read this session.
- Assumed `cloud-localds` from `cloud-image-utils` on Ubuntu 24.04 produces a seed ISO that QEMU 8.2.2 will accept as a virtio drive on first boot. Not exercised this session.
- Assumed `qemu-system-x86_64 -machine accel=kvm:tcg` will fall back cleanly to TCG if `/dev/kvm` is unavailable or unreadable inside whatever environment `icn-dev` is running. `/dev/kvm` permissions were not inspected.
- Assumed the existing native install (`deploy/install.sh` + `deploy/icnd.service`) and the 3-node devnet (`deploy/devnet/`) remain the supported runtime paths, per `deploy/appliance/README.md` §"What this is not". State of those paths was not re-checked this session.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

Two paths, in order of operator-authorization cost.

### Path 1 — Confirm scaffold is dry-runnable without staging anything (safe today)

```bash
cd /home/ubuntu/projects/icn
bash deploy/appliance/build-image.sh        --dry-run
bash deploy/appliance/smoke/smoke-local.sh  --dry-run
bash deploy/appliance/scripts/icn-appliance-firstboot.sh --dry-run
bash deploy/appliance/check.sh
```

Each should exit `0`. No files are mutated. This re-verifies the dry-run plane without the operator having to authorize anything new. If the next session begins with this and any step fails, that is a regression worth investigating before staging a base image.

### Path 2 — First real local build + smoke (requires operator authorization)

This handoff explicitly does NOT authorize this path. The following is recorded so the next session can execute it without rediscovery, after the operator says yes.

Pre-step (operator decision, not done in this session):

1. Pick a base from the compatible row of the glibc table above.
2. Stage it manually outside the repo. Example (do not run without authorization):

   ```bash
   # Example only. Operator chooses base, mirror, and target path.
   mkdir -p ~/icn-appliance-build
   # Stage the chosen base image at ~/icn-appliance-build/<base>.qcow2
   # Capture its sha256 for later verification.
   sha256sum ~/icn-appliance-build/<base>.qcow2 > ~/icn-appliance-build/<base>.sha256
   ```

3. Generate a smoke-only SSH keypair outside the repo:

   ```bash
   ssh-keygen -t ed25519 -f /tmp/icn-smoke-key -N ""
   ```

   Edit a private copy of `deploy/appliance/smoke/cloud-init/user-data.example.yaml` to drop in the public key, save outside the repo, build a seed ISO with `cloud-localds`. Do NOT commit the edited file.

Then run:

```bash
cd /home/ubuntu/projects/icn

export ICN_APPLIANCE_BASE_IMAGE=~/icn-appliance-build/<base>.qcow2
export ICN_APPLIANCE_BASE_SHA256=$(awk '{print $1}' ~/icn-appliance-build/<base>.sha256)
export ICN_APPLIANCE_OUTPUT_DIR=~/icn-appliance-build
export ICN_APPLIANCE_VERSION=0.0.1-dev

bash deploy/appliance/build-image.sh --real
# -> $ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.1-dev-amd64.qcow2
# -> $ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.1-dev-amd64.manifest.json

export ICN_APPLIANCE_IMAGE=$ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.1-dev-amd64.qcow2
export ICN_APPLIANCE_SSH_KEY=/tmp/icn-smoke-key
export ICN_APPLIANCE_CLOUD_INIT_SEED=/path/to/built/seed.iso

bash deploy/appliance/smoke/smoke-local.sh --real
# Expected on PASS: SSH up -> firstboot marker -> icnd active -> /v1/health 200.
```

A PASS from `smoke-local.sh --real` would be the appropriate trigger for Option C (a separate, separately-justified PR that records the exact command, environment, artifact path, and host limits). It would NOT be a production-readiness claim, NOT a partner-federation claim, NOT an NYCN activation claim, and NOT generalizable past `icn-dev`.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

None. This handoff makes no architectural decision. It records verified host state and the next operator decision; the operator owns it.

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
# Position
hostname && date && pwd
git log --oneline -1
git status --short --branch

# Host inventory
cat /etc/os-release | head -4
ldd --version | head -1
uname -srm

# Tool inventory
for t in qemu-img virt-customize virt-sysprep cloud-localds qemu-system-x86_64 cargo sha256sum ssh curl python3; do
  printf '%-22s ' "$t"
  command -v "$t" || echo MISSING
done

# Appliance scaffold present
find deploy/appliance -maxdepth 4 -type f | sort

# Scaffold dry-runs still pass (no mutation)
bash deploy/appliance/build-image.sh        --dry-run >/dev/null && echo build-image:OK
bash deploy/appliance/smoke/smoke-local.sh  --dry-run >/dev/null && echo smoke-local:OK
bash deploy/appliance/scripts/icn-appliance-firstboot.sh --dry-run >/dev/null && echo firstboot:OK
bash deploy/appliance/check.sh

# Docs-only validation (this handoff)
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 .github/scripts/compliance_linter.py
git diff --check
```

If any of the above changes, this handoff has aged out and should be reconciled before staging a base image.

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** `deploy/appliance/README.md`, `deploy/appliance/smoke/README.md`, `docs/architecture/DEBIAN_APPLIANCE_MODEL.md` (existence). Status labels in those documents — "bootable dev image", "Unbuilt scaffold" — are not all internally consistent (see §"What's Open" item 5); this handoff does not reconcile them.
- **Implementation truth:** the appliance scaffold files exist at the paths PR #1865 placed them. Their *runtime* behavior was not exercised.
- **Execution truth:** branch `docs/appliance-verification-handoff-2026-05-17` was created from `main @ f9a98a2f4`. The only path mutated this session is the new file under `docs/dev/`. No remote was touched at the time this body was written.
- **Narrative truth:** PR #1865's commit message describes `--real` paths as already landed. The README's opening paragraph agrees. The README's "Next implementation slice" section still describes them as future work. The next session, or a separate small docs PR, should reconcile that. This handoff calls it out rather than silently rewriting it.
- **Known conflicts:**
  - README opening vs README "Next implementation slice" (see above).
  - None within this handoff itself.
