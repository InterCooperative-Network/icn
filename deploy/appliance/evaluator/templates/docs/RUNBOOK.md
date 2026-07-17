# ICN Common Sense Vertical Slice Runbook

## Purpose

This runbook explains how to verify, boot, run, inspect, and stop the ICN
Common Sense bootable vertical slice.

The package is local after download. It does not require access to the sender's
LAN or node.

## Recommended Host

Use Debian or Ubuntu Linux.

Minimum practical resources:

- 2 CPU cores,
- 4 GB RAM,
- 4 GB free disk space.

## Package Map

```text
@PKG_NAME@/
  README.md
  SHA256SUMS
  @IMAGE_BASENAME@
  @MANIFEST_BASENAME@
  start-icn-demo-vm.sh
  scripts/
    run-demo.sh
    verify.sh
  docs/
    START-HERE.md
    RUNBOOK.md
    WALKTHROUGH.md
    (a matching .pdf for each may be present if the build rendered one)
  setup-and-run.sh
```

## One-Command Path

On Debian or Ubuntu:

```bash
./setup-and-run.sh
```

That script:

```text
check host tools
   |
install missing apt packages with sudo
   |
verify checksums
   |
start demo VM
   |
print browser URL
```

Useful options:

```bash
./setup-and-run.sh --setup-only
./setup-and-run.sh --verify-only
./setup-and-run.sh --no-install
```

## Manual Install Requirements

```bash
sudo apt-get update
sudo apt-get install -y qemu-system-x86 qemu-utils cloud-image-utils openssh-client curl coreutils
```

The launcher uses:

- `qemu-system-x86_64`
- `qemu-img`
- `cloud-localds`
- `ssh`
- `curl`
- `sha256sum`

## Verify Files

From the package root:

```bash
./scripts/verify.sh
```

Expected shape:

```text
README.md: OK
docs/START-HERE.md: OK
docs/RUNBOOK.md: OK
docs/WALKTHROUGH.md: OK
@IMAGE_BASENAME@: OK
...
```

Every line must end in `OK`. If the package includes `.pdf` docs, they appear as
additional `OK` lines; if it ships Markdown only, they are simply absent.

If any line does not end in `OK`, stop and re-download.

## Start The Demo

```bash
./setup-and-run.sh
```

The launcher does this:

```text
create disposable SSH key
   |
create cloud-init seed
   |
create throwaway overlay disk
   |
boot qcow2 with QEMU
   |
wait for SSH
   |
wait for firstboot
   |
wait for icnd
   |
wait for gateway health
   |
wait for member shell
   |
open local tunnels
```

When ready, it prints the browser URL.

## Browser URL

Default:

```text
http://localhost:18090/member-shell/?mode=live&demo=launcher&gw=18080&session=18091
```

Expected browser path:

```text
member shell loads
   |
click Start local demo
   |
review action card
   |
complete action
   |
receipt appears
```

## Health Check

After the launcher says ready:

```bash
curl http://localhost:18080/v1/health
```

Expected:

```json
{"status":"ok","version":"0.1.0"}
```

## Stop The Demo

In the launcher terminal:

```text
Ctrl-C
```

The launcher stops the VM and removes the temporary overlay. The source image
is not modified.

## If Ports Are Busy

Default ports:

- `22222` VM SSH
- `18080` gateway
- `18090` member shell
- `18091` session service

Alternative ports (SSH, gateway, session only):

```bash
ICN_DEMO_SSH_PORT=32222 \
ICN_DEMO_GW_PORT=28080 \
ICN_DEMO_SESSION_PORT=28091 \
./setup-and-run.sh
```

The member-shell port is **fixed at 18090** and cannot be changed: the demo image
only allowlists the `localhost:18090` browser origin, so the shell must load from
there or the session/gateway calls fail their origin checks. If `18090` is in use,
free it (see Troubleshooting) rather than remapping it. The launcher prints the
exact URL to open; it always uses `localhost:18090`.

## Troubleshooting

### Missing QEMU

Install the host requirements.

### Missing `cloud-localds`

```bash
sudo apt-get install -y cloud-image-utils
```

### VM boot is slow

Wait. If hardware virtualization is unavailable, QEMU may use slower emulation.

### Browser does not load

Confirm the launcher printed that the demo VM is ready. Then run the health
check above.

### Verification fails

Do not run the image. Re-download the zip and verify again.

## Provenance

Image version:

```text
@IMAGE_VERSION@
```

Source commit recorded in manifest:

```text
@SOURCE_COMMIT@
```

Image SHA256:

```text
@IMAGE_SHA256@
```

## Scope

This is a bootable vertical slice of what exists today. It is unsigned,
fixture-backed, and meant to show the local VM plus browser demo path.
