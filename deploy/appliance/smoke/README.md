# Appliance Smoke (real local one-VM)

> **Status: real local boot smoke.** `smoke-local.sh --real` boots the
> appliance QCOW2 under QEMU user-mode networking, waits for SSH,
> verifies firstboot ran, confirms `icnd` is active, and confirms
> `/v1/health` returns 200 on port 8080 — all inside the disposable VM.
> A PASS here means the local dev image works. It does NOT mean the
> appliance is production, signed, or fit for partner federation.

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
bash deploy/appliance/smoke/smoke-local.sh --real
```

## Cross-references

- [`../README.md`](../README.md) — appliance scaffold overview.
- [`../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md)
  — full appliance model and acceptance gates.
- [`../build-image.sh`](../build-image.sh) — image build scaffold (also
  dry-run-only today).
- [`../../devnet/`](../../devnet/) — the existing 3-node devnet, which
  is what appliance smoke will eventually converge with.
