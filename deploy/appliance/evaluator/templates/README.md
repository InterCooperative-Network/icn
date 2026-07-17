# ICN Common Sense Bootable Vertical Slice

This zip is a self-contained bootable vertical slice of what exists today.

It lets a reviewer boot a local ICN demo VM and see the current browser-facing
member shell working against services inside that VM.

## Start Here

Open this first:

- `docs/START-HERE.md`

Then run:

```bash
./setup-and-run.sh
```

On Debian/Ubuntu, that script installs missing host tools, verifies checksums,
and starts the local VM.

## What Is Included

- `@IMAGE_BASENAME@` - bootable VM image.
- `@MANIFEST_BASENAME@` - build and provenance manifest.
- `start-icn-demo-vm.sh` - QEMU launcher.
- `setup-and-run.sh` - one-command Debian/Ubuntu setup, verify, and run path.
- `scripts/verify.sh` - verifies every packaged file.
- `scripts/run-demo.sh` - starts the demo from the package root.
- `docs/START-HERE.md` - visual setup guide.
- `docs/RUNBOOK.md` - detailed operations runbook.
- `docs/WALKTHROUGH.md` - browser click-through.
- `SHA256SUMS` - checksums for the package contents.

The docs are Markdown. A matching `.pdf` for each is included **only when the build
environment can render one** (best-effort); it is never required to run the demo.

## What This Shows

```text
download zip
   |
   v
extract package
   |
   v
verify checksums
   |
   v
boot local VM
   |
   v
open member shell in browser
   |
   v
start local demo
   |
   v
complete action card
   |
   v
see receipt
```

## Scope

This is intentionally narrow:

- it is a bootable vertical slice,
- it runs locally on the reviewer's machine,
- it demonstrates the current browser path that exists today,
- it is not a production federation deployment,
- it is not a signed release,
- it is not a claim that the full cooperative storage or distributed compute
  product is finished.

## Recommended Host

Debian or Ubuntu Linux.

Install host tools:

```bash
sudo apt-get update
sudo apt-get install -y qemu-system-x86 qemu-utils cloud-image-utils openssh-client curl coreutils
```

Or just run:

```bash
./setup-and-run.sh
```
