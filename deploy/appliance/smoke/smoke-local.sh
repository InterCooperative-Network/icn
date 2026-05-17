#!/usr/bin/env bash
# smoke-local.sh
#
# Dry-run scaffold for the ICN appliance boot smoke.
#
# This script does NOT boot a VM, does NOT install anything, and does
# NOT exercise a real ICN health endpoint. It prints what the future
# real smoke will do and exits.
#
# Modes:
#   --dry-run   Print planned smoke. Default. The only supported mode today.
#   --real      Reserved. Refused until the real boot smoke lands.
#   --help|-h   Print this help.

set -euo pipefail

MODE="dry-run"
TARGET_HOST="${ICN_SMOKE_TARGET:-127.0.0.1}"
TARGET_PORT="${ICN_SMOKE_GATEWAY_PORT:-8080}"  # never 8000

usage() {
    sed -n '2,16p' "$0"
    exit 0
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) MODE="dry-run" ; shift ;;
        --real)    MODE="real"    ; shift ;;
        --help|-h) usage ;;
        *)
            printf '[appliance-smoke] ERROR: unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

log() { printf '[appliance-smoke] %s\n' "$*"; }

log "Mode: $MODE"
log "Target host: $TARGET_HOST"
log "Target gateway/health port: $TARGET_PORT (never 8000)"
echo

cat <<EOF_PLAN
[appliance-smoke] Planned smoke steps (NOT executed in dry-run):

  1) Locate or boot a disposable VM with the appliance image attached.
     - Local libvirt / Proxmox / qemu-system path.
     - The image path comes from a separate operator step (likely the
       output of deploy/appliance/build-image.sh once it is real).

  2) Wait for the appliance to reach a usable state:
     - SSH on the VM's IP responds, OR
     - HTTP on the gateway port returns *something*.

  3) Verify the firstboot marker:
       ls -la /var/lib/icn/.firstboot-complete
     (over SSH on the VM)

  4) Verify icnd is running:
       systemctl is-active icnd

  5) Verify the gateway / health endpoint:
       curl -sf http://${TARGET_HOST}:${TARGET_PORT}/v1/health
     Expected: HTTP 200.

  6) (Future) Apply an institution smoke fixture and verify the standing
     -> action -> receipt loop. The fixture path is not yet specified
     and is not part of this scaffold.

  7) Print a one-line summary: PASS/FAIL plus the gate that was the
     deciding signal.

[appliance-smoke] This is a scaffold. The above is a plan, not a result.
EOF_PLAN

if [ "$MODE" = "dry-run" ]; then
    log "Exiting cleanly. No VM was booted, no endpoint was contacted."
    exit 0
fi

cat >&2 <<'EOF_REAL_REFUSED'
[appliance-smoke] --real not implemented.

The real boot smoke depends on:
  - A buildable appliance image (deploy/appliance/build-image.sh --real)
  - A VM launch path (libvirt / qemu / Proxmox)
  - A health-check loop with bounded timeout

None of those are in this PR. Until the next slice lands, --real
refuses to run rather than fake a result.
EOF_REAL_REFUSED
exit 3
