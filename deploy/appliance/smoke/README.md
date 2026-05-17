# Appliance Smoke (scaffold)

> **Status: scaffold.** This directory describes the acceptance path the
> first real ICN appliance image must satisfy. Nothing here builds or
> boots an image today.

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

## What this PR delivers

- This README explaining the path.
- `smoke-local.sh` as a dry-run scaffold that prints the planned smoke
  steps. It does **not** boot a VM or run any health checks today.

## What this PR does NOT deliver

- No real VM boot.
- No real health verification.
- No real fixture application.
- No CI integration. The smoke is intentionally non-CI for now: it
  needs hardware-assisted virt or qemu-system on the runner, neither of
  which we want to take on in a scaffold PR.

## How to run today

```bash
bash deploy/appliance/smoke/smoke-local.sh --dry-run
```

The script will print what the future real smoke will do, then exit
cleanly without booting anything.

## Cross-references

- [`../README.md`](../README.md) — appliance scaffold overview.
- [`../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../../../docs/architecture/DEBIAN_APPLIANCE_MODEL.md)
  — full appliance model and acceptance gates.
- [`../build-image.sh`](../build-image.sh) — image build scaffold (also
  dry-run-only today).
- [`../../devnet/`](../../devnet/) — the existing 3-node devnet, which
  is what appliance smoke will eventually converge with.
