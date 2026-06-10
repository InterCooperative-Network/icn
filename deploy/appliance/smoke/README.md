# Appliance Smoke (real local one-VM)

> **Status: real local boot smoke, positive + one negative scenario.**
> `smoke-local.sh --real` boots the appliance QCOW2 under QEMU user-mode
> networking, waits for SSH, verifies firstboot ran, confirms `icnd` is
> active, and confirms `/v1/health` returns 200 on port 8080 — all
> inside the disposable VM. `negative-firstboot-smoke.sh --real` proves
> the opposite direction of the same gate: with required firstboot
> material missing, `icnd` must NOT start and health must NOT answer.
> A PASS on either means the local dev image behaves; it does NOT mean
> the appliance is production, signed, or fit for partner federation.

## Future acceptance path

The first real (non-scaffold) appliance image is accepted when, on a
disposable local VM, the following succeeds end-to-end:

1. **Boot.** The image boots Debian to a usable system. The
   `icn-appliance-firstboot.service` unit runs once and marks itself
   complete (`/var/lib/icn/.firstboot-complete`).
2. **SSH or health surface reachable.** Either SSH is available for the
   operator, or `/v1/health` on port `8080` is reachable from the host.
3. **`icnd` is running.** `systemctl status icnd` reports `active
   (running)`.
4. **Health passes.** `curl -sf http://<vm-ip>:8080/v1/health` returns
   HTTP 200.
5. **Institution smoke fixture (future).** Once available, an institution
   smoke fixture is applied and the standing → action → receipt loop is
   verified.

The first three items are the **minimum acceptance bar** for promoting
the appliance from "Unbuilt scaffold" to "Bootable dev image" per
[`DEBIAN_APPLIANCE_MODEL.md`](../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md).

## What this slice delivers

- `smoke-local.sh --dry-run` (unchanged behavior; still prints the plan).
- `smoke-local.sh --real`: boots the appliance QCOW2 in QEMU user-mode
  networking, waits for SSH, runs verification commands inside the VM
  via SSH, captures journalctl on failure, kills the VM via trap.
- `cloud-init/{user-data,meta-data}.example.yaml` — smoke-only cloud-init
  seed examples. The placeholder SSH key in `user-data.example.yaml` is
  intentionally invalid; the smoke script refuses to use the example
  unless the operator has replaced the placeholder with a real
  smoke-only key.

## What this slice does NOT deliver

- No real federation contact. The VM uses user-mode networking only.
- No partner / NYCN fixture application.
- No CI integration. Running QEMU on CI requires hardware-assisted virt
  or accepting nested-virt + slow runs; we are not taking that on yet.
- No SSH key embedded in the appliance image. Operator supplies their
  own via cloud-init, per-VM, never committed.

## How to run

Dry-run (no tools required):

```bash
bash deploy/appliance/smoke/smoke-local.sh --dry-run
```

Real (see `../README.md` §"Real local build + boot smoke" for the full
recipe):

```bash
export ICN_APPLIANCE_IMAGE=/path/to/built/qcow2
export ICN_APPLIANCE_SSH_KEY=/path/to/smoke-private-key
export ICN_APPLIANCE_CLOUD_INIT_SEED=/path/to/seed.iso
# WSL2: 2222/2223 are reserved by Windows on many setups — override:
# export ICN_APPLIANCE_SSH_PORT=22222
bash deploy/appliance/smoke/smoke-local.sh --real
```

## Negative fail-closed smoke

`negative-firstboot-smoke.sh` completes the proof matrix the positive
smoke leaves at positive-only. It exercises the appliance-only drop-in
[`../systemd/icnd.service.d/10-firstboot-gate.conf`](../systemd/icnd.service.d/10-firstboot-gate.conf):

- `Requires=icn-appliance-firstboot.service` — firstboot failure must
  propagate and cancel icnd's start job.
- `ConditionPathExists=/var/lib/icn/.firstboot-complete` — without the
  marker, icnd must be skipped even if started manually.

**Scenario (exactly one): `missing-firstboot-exec`.** The script
creates a disposable qcow2 overlay of the image, deletes
`/usr/local/sbin/icn-appliance-firstboot` (the firstboot unit's
`ExecStart=`) from the overlay with `virt-customize`, boots the
tampered overlay, and asserts fail-closed:

1. `icn-appliance-firstboot.service` reaches `failed`.
2. The marker `/var/lib/icn/.firstboot-complete` does not exist.
3. `icnd` is never `active` during the observation window
   (default 60s) — any `active` exits non-zero as **FAIL-OPEN**.
4. `/v1/health` never answers during the window — any success exits
   non-zero as **FAIL-OPEN**.

The source image is never modified; the tamper exists only on the
disposable overlay. Evidence (unit statuses, journals, hashes,
verdict summary) is preserved in the `--out` directory.

Dry-run (no tools required):

```bash
bash deploy/appliance/smoke/negative-firstboot-smoke.sh --dry-run
```

Real:

```bash
export ICN_APPLIANCE_IMAGE=/path/to/built/qcow2
export ICN_APPLIANCE_SSH_KEY=/path/to/smoke-private-key
export ICN_APPLIANCE_CLOUD_INIT_SEED=/path/to/seed.iso
bash deploy/appliance/smoke/negative-firstboot-smoke.sh --real \
  --out /path/to/evidence-dir
# If /dev/kvm is not accessible, add --force to accept a slow TCG run.
```

Additional requirement over the positive smoke: `virt-customize`
(libguestfs-tools). libguestfs needs a readable `/boot/vmlinuz-*` on
the host; if it errors out as a non-root user, the operator can
`chmod 0644` the running kernel image (the same host-state change the
2026-05-21 positive smoke session recorded).

**What a PASS does NOT mean:** one verified negative scenario does not
certify every appliance failure mode, and does not make the appliance
production-ready, signed, immutable, or partner-distributable.

### Known smoke gotchas

- **glibc skew between build host and base image.** If `icnd`
  restart-loops with `libc.so.6: version 'GLIBC_2.x' not found`, the
  base image's glibc is older than the build host's. See
  [`../README.md`](../README.md) §"Host / image compatibility".
- **WSL2 + QEMU host port reservations.** Ports `2222`/`2223` may be
  held by Windows-side exclusions. Use `ICN_APPLIANCE_SSH_PORT=22222`
  or similar.
- **KVM permission denied is non-fatal.** QEMU falls back to TCG.
  Smoke still works; it's slower.

## Cross-references

- [`../README.md`](../README.md) — appliance scaffold overview.
- [`../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md)
  — full appliance model and acceptance gates.
- [`../build-image.sh`](../build-image.sh) — image build scaffold (also
  dry-run-only today).
- [`../../devnet/`](../../devnet/) — the existing 3-node devnet, which
  is what appliance smoke will eventually converge with.
