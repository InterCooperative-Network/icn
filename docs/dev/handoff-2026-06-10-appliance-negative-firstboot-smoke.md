---
Status: descriptive
Authority: session
Canonical: no
Last verified: 2026-06-10
---

# Session Handoff — 2026-06-10 — Appliance Negative Firstboot Smoke (fail-closed verification)

**Topic:** Operator-run verification of the appliance fail-closed firstboot path — the proof-matrix gap the 2026-05-21 positive smoke (#1900) explicitly left unverified.
**Branch:** `test/appliance-negative-firstboot-smoke` (icn-dev worktree model, created off `origin/main @ a9ee3d2a`).
**Refs:** PR #1865 (appliance substrate incl. the `10-firstboot-gate.conf` drop-in), #1866 (real build+boot smoke), #1900 (Debian-13 positive real-smoke handoff), #2019 (truth-sync that restated the negative-path gap as still open).

This handoff records ONE negative fail-closed scenario, operator-run on the same host and the same built image as the #1900 positive smoke. **No production-readiness claim, no signed-image claim, no immutable-image claim, no partner-distributable claim, no live-federation claim, no NYCN activation claim, no Phase 2 completion claim.** One negative scenario does not certify every appliance failure mode.

---

## Session Goal

Add a runnable negative firstboot smoke (`deploy/appliance/smoke/negative-firstboot-smoke.sh`) and operator-run it: deliberately remove required firstboot material from a disposable overlay, boot it, and prove the appliance fails closed — firstboot unit failed, marker absent, `icnd` never active, `/v1/health` never answering.

## Decisive Test

This handoff fails if any of the following holds:

1. It claims the negative path is verified but the recorded run did not actually execute, or executed with assertions weakened.
2. It claims more than one scenario was verified, or implies the appliance is fail-closed-certified in general.
3. It claims production readiness, signed/immutable/partner-distributable images, live federation, or NYCN activation.
4. The tamper touched the source image rather than the disposable overlay.
5. The host-state changes attributed to this session are not exactly the one listed under §"Host-state changes".
6. It commits a QCOW2, seed ISO, SSH key, or any secret into the repo.

## What #1900 verified vs. what this session adds

| Path | #1900 (2026-05-21) | This session (2026-06-10) |
|---|---|---|
| Positive: build real QCOW2 | verified | not repeated (same artifact reused) |
| Positive: boot → SSH → marker → icnd active → health 200 | verified | **re-verified as baseline** (same image, same host, fresh ephemeral key/seed) |
| Negative: missing firstboot material → fail closed | **explicitly NOT verified** | **verified, one scenario** (`missing-firstboot-exec`) |

## The gate under test

`deploy/appliance/systemd/icnd.service.d/10-firstboot-gate.conf` (in-image at `/etc/systemd/system/icnd.service.d/`, confirmed present in the artifact image via read-only `guestfish` inspection before the run):

- `Requires=icn-appliance-firstboot.service` — firstboot failure propagates; icnd's start job is cancelled.
- `After=icn-appliance-firstboot.service` — ordering.
- `ConditionPathExists=/var/lib/icn/.firstboot-complete` — belt: no marker, no start.

**Injected negative condition (`missing-firstboot-exec`):** delete `/usr/local/sbin/icn-appliance-firstboot` — the `ExecStart=` of `icn-appliance-firstboot.service` — from a disposable qcow2 overlay via `virt-customize`. The oneshot then fails at start (203/EXEC); both gate mechanisms must hold.

## Host environment

| Field | Value | Source |
|---|---|---|
| Host | `icn-dev` (same host class as #1900) | `hostname` |
| OS | Ubuntu 24.04.4 LTS (Noble Numbat) | `/etc/os-release` |
| Kernel | `6.8.0-124-generic x86_64` | `uname -r` |
| QEMU | 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.16) | `qemu-system-x86_64 --version` |
| qemu-img | 8.2.2 | `qemu-img --version` |
| virt-customize | 1.52.0 | `virt-customize --version` |
| OpenSSH | 9.6p1 Ubuntu-3ubuntu13.16 | `ssh -V` |
| cloud-localds | cloud-image-utils (usage-confirmed) | `cloud-localds` |
| shellcheck | 0.9.0 | `shellcheck --version` |
| KVM | `/dev/kvm` not accessible to the operator user; runs executed under TCG fallback (`--force`) | `ls -l /dev/kvm` |

### Host-state changes (exactly one)

- `sudo chmod 0644 /boot/vmlinuz-6.8.0-124-generic` — required so libguestfs (virt-customize/guestfish) works as a non-root user. This is the same class of change #1900 recorded for the then-running kernel (`6.8.0-111`); the host has since rotated kernels, so it had to be re-applied for the current one.

## Image under test (same artifact #1900 built and verified)

| Field | Value |
|---|---|
| Image | `/home/ubuntu/artifacts/icn/appliance/debian13-real-smoke-20260521/images/icn-appliance-0.0.1-dev-trixie-2026-05-21-amd64.qcow2` |
| Image SHA256 (recomputed this session) | `e6888dd512d4cf718a7b9d2bb208a0a743654aca1c0fcda1d4c3fa79aa4e6f51` — matches the manifest written at build time |
| Image SHA512 (recomputed this session, pre-run; the source image is never modified by either smoke) | `fc37b6b168d78aa0450424bf3d329bd46c820e7f9f8ea1da495205890e1ca38ee411bc0fccb44e5ae6ae17d632f3610de691bd38cc94718c09ce6c3b288a257e` |
| Manifest `git_commit` | `00b3224aa778c518604a41f1dfd2b3ba0c1d1dec` (the #1900 build) |
| Manifest flags | `non_production: true`, `signed: false`, `immutable: false` |
| Base image | `debian-13-genericcloud-amd64.qcow2`, SHA256 `f8573792e38e6d8a5ba701759e5ff96792e4c7ebca3721394f548106f42aeb34` (per #1900 record) |

In-image preconditions confirmed by read-only `guestfish` before any run: `/usr/local/sbin/icn-appliance-firstboot` present; `/etc/systemd/system/icnd.service.d/10-firstboot-gate.conf` present; both `icn-appliance-firstboot.service` and `icnd.service` enabled in `multi-user.target.wants`.

## Run record

Working assets (all outside the repo, none committed): staging at
`/home/ubuntu/artifacts/icn/appliance/negative-firstboot-20260610/`
(`keys/` ephemeral ed25519 smoke key generated this session; `seed/`
cloud-init user-data + `seed.iso` built with `cloud-localds` from the
repo examples with the placeholder key replaced; `logs/`;
`evidence-negative/`).

### Step 1 — Positive baseline (sanity: plumbing + image still good today)

```
ICN_APPLIANCE_IMAGE=<artifact image> ICN_APPLIANCE_SSH_KEY=<ephemeral key> \
ICN_APPLIANCE_CLOUD_INIT_SEED=<seed.iso> ICN_APPLIANCE_VM_TIMEOUT=900 \
bash deploy/appliance/smoke/smoke-local.sh --real
```

Result: **PASS**, exit 0 — SSH up → firstboot marker present → `icnd.service` active → `/v1/health` 200. Log: `logs/positive-baseline-20260610T*.log`. This re-validates the harness (seed, key, ports) and the image on this host today, so the negative run differs from this baseline in exactly one variable: the tamper.

### Step 2 — Negative run (`missing-firstboot-exec`)

```
ICN_APPLIANCE_IMAGE=<same artifact image> ICN_APPLIANCE_SSH_KEY=<same key> \
ICN_APPLIANCE_CLOUD_INIT_SEED=<same seed.iso> ICN_APPLIANCE_VM_TIMEOUT=900 \
bash deploy/appliance/smoke/negative-firstboot-smoke.sh --real --force \
  --out /home/ubuntu/artifacts/icn/appliance/negative-firstboot-20260610/evidence-negative
```

Result: **PASS (fail-closed verified)**, exit 0, run completed `2026-06-10T17:17:07Z`.
Log: `logs/negative-20260610T*.log`. Evidence: `evidence-negative/` (summary.txt, tamper.log, hashes.txt, firstboot-status/journal, icnd-status/journal, depfail-journal, cloudinit-and-marker, serial.log).

Tamper record: `virt-customize --delete /usr/local/sbin/icn-appliance-firstboot` applied to the disposable overlay only; post-tamper overlay SHA512 `48b8a65c57a0a534d606c219171b145693e4a44fddc8ed3d58bbdefd9dd4694d5226b01993974a6056e795388f9afa3830b92bd1e57223c25f39bed1f0a1c1e5`; source-image SHA512 unchanged (recorded pre-run in `hashes.txt`).

Assertion-by-assertion, with journal evidence:

1. **Firstboot unit failed** — `systemctl status icn-appliance-firstboot.service`:
   `Active: failed (Result: exit-code)`; `Process: 606 ExecStart=/usr/local/sbin/icn-appliance-firstboot (code=exited, status=203/EXEC)`; journal: `Unable to locate executable '/usr/local/sbin/icn-appliance-firstboot': No such file or directory` → `Failed at step EXEC` → `Failed with result 'exit-code'.`
2. **Marker absent** — `sudo test ! -f /var/lib/icn/.firstboot-complete` held; evidence file records `MARKER ABSENT`.
3. **icnd never active (60s window)** — `systemctl show icnd`: `ActiveState=inactive`, `SubState=dead`. The blocking mechanism is the `Requires=` propagation, captured verbatim in the journal:
   `Dependency failed for icnd.service - ICN Daemon - Intercooperative Network Node.`
   `icnd.service: Job icnd.service/start failed with result 'dependency'.`
   The `ConditionPathExists=` marker belt was therefore not even reached on the boot path (the start job was cancelled first); the marker's absence (assertion 2) is what would hold the line against a later manual `systemctl start icnd`.
4. **Health never answered (60s window, port 8080)** — `curl -sf http://127.0.0.1:8080/v1/health` failed on every poll.

Cloud-init reported `status: done` — SSH provisioning is independent of the ICN units, as designed, which is what makes the in-VM assertions observable in the failure case.

## What remains unverified (preserved non-claims)

- Every other negative path: tampered (not missing) firstboot script, corrupted `/etc/icn` inputs, partial identity material, marker pre-seeded without identity (the firstboot script itself treats a pre-existing marker as "already completed" — a separate scenario), `icnd --init` failure mid-firstboot (the script currently warns-and-continues on identity-init failure, which is a *different* path from unit failure and is NOT exercised here), disk-full, clock-skew.
- Signed images, immutable images, partner-distributable images: not claimed, unchanged.
- Production readiness: not claimed, unchanged.
- The PGP trust-chain limitation on the Debian base image recorded by #1900: unchanged.

## Unsafe Assumptions

- The archived #1900 artifact image is the unit under test; its SHA256 was re-verified against its build manifest before use.
- TCG (no KVM for the operator user) affects speed only, not gate semantics.
- SSH reachability in the negative scenario comes from cloud-init, which is independent of the ICN units — confirmed by the run itself.

## Cleanup performed

- QEMU processes terminated by each script's trap handler; disposable overlays and seed work dirs removed by the same.
- The ephemeral smoke keypair, seed, evidence, and logs remain under `/home/ubuntu/artifacts/icn/appliance/negative-firstboot-20260610/` (outside the repo, uncommitted) for replay/audit.
- No change made to the source image, the base image, host networking, K3s, DNS, Forgejo, or GitHub settings.

## Checks run for the PR

See the PR body test plan: shellcheck (clean; one intentional SC2029 suppressed with justification), doc-control strict, compliance linter, readiness-overclaim linter, INDEX drift mirror, `git diff --stat` scope confirmation.
