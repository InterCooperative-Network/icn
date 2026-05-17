#!/usr/bin/env bash
# smoke-local.sh
#
# Local one-VM smoke for the ICN appliance.
#
# Two modes:
#   --dry-run  Print the planned smoke and exit. No mutation.
#   --real     Boot the supplied QCOW2 in a disposable QEMU VM under
#              user-mode networking, wait for SSH, verify icnd is alive,
#              and confirm /v1/health on port 8080 from inside the VM.
#
# Required for --real:
#   ICN_APPLIANCE_IMAGE      Path to the QCOW2 produced by build-image.sh.
#   ICN_APPLIANCE_SSH_KEY    Path to the smoke-only SSH private key. The
#                            matching public key MUST be present in the
#                            cloud-init seed.
#
# Optional:
#   ICN_APPLIANCE_SSH_USER       Default: debian (Debian cloud image default).
#   ICN_APPLIANCE_SSH_PORT       Default: 2222 (host-side).
#   ICN_APPLIANCE_HEALTH_PORT    Default: 8080.
#   ICN_APPLIANCE_CLOUD_INIT_SEED  Path to a cloud-init seed ISO. If unset,
#                                  the script tries to build one from
#                                  smoke/cloud-init/*.example.yaml using
#                                  cloud-localds (operator must replace
#                                  the placeholder SSH key first; the
#                                  script refuses the placeholder).
#   ICN_APPLIANCE_VM_MEMORY      Default: 1024 (MiB).
#   ICN_APPLIANCE_VM_CPUS        Default: 2.
#   ICN_APPLIANCE_VM_TIMEOUT     Default: 300 (seconds; total smoke budget).
#
# Required tools for --real:
#   qemu-system-x86_64, ssh, curl, sha256sum
#   Optional: cloud-localds (only if seed ISO is not pre-built).
#
# What this smoke does NOT do:
#   - Connect to any real ICN federation.
#   - Apply NYCN or partner fixtures.
#   - Mutate host networking, K3s, DNS, or systemd state.
#   - Require root on the host.
#
# Honest non-claims:
#   A pass here means the local dev image boots and reaches a healthy
#   icnd. It does NOT mean the appliance is production-ready, signed,
#   or fit for partner federation.

set -euo pipefail

MODE="dry-run"

log()  { printf '[appliance-smoke] %s\n' "$*"; }
warn() { printf '[appliance-smoke] WARN: %s\n' "$*" >&2; }
err()  { printf '[appliance-smoke] ERROR: %s\n' "$*" >&2; }

usage() {
    sed -n '2,42p' "$0"
    exit 0
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) MODE="dry-run" ; shift ;;
        --real)    MODE="real"    ; shift ;;
        --help|-h) usage ;;
        *)
            err "unknown argument: $1"
            exit 2
            ;;
    esac
done

# ---------- common context ----------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SSH_USER="${ICN_APPLIANCE_SSH_USER:-debian}"
SSH_PORT="${ICN_APPLIANCE_SSH_PORT:-2222}"
HEALTH_PORT="${ICN_APPLIANCE_HEALTH_PORT:-8080}"
VM_MEMORY="${ICN_APPLIANCE_VM_MEMORY:-1024}"
VM_CPUS="${ICN_APPLIANCE_VM_CPUS:-2}"
VM_TIMEOUT="${ICN_APPLIANCE_VM_TIMEOUT:-300}"

log "Mode: $MODE"
log "VM SSH:           ${SSH_USER}@127.0.0.1:${SSH_PORT}"
log "Health port:      ${HEALTH_PORT} (checked inside VM via SSH; never 8000)"
log "VM memory:        ${VM_MEMORY} MiB"
log "VM cpus:          ${VM_CPUS}"
log "Total budget:     ${VM_TIMEOUT}s"
log "Image:            ${ICN_APPLIANCE_IMAGE:-<unset>}"
log "SSH key:          ${ICN_APPLIANCE_SSH_KEY:-<unset>}"
log "Cloud-init seed:  ${ICN_APPLIANCE_CLOUD_INIT_SEED:-<not provided; will try to build from examples>}"
echo

# ---------- dry-run plan (shown in both modes) ----------
print_plan() {
cat <<'EOF_PLAN'
[appliance-smoke] Planned smoke steps:

  1) Validate tools: qemu-system-x86_64, ssh, curl, sha256sum.
     If no cloud-init seed ISO is provided, also require cloud-localds.

  2) Validate the appliance image exists at ICN_APPLIANCE_IMAGE.

  3) Stage a disposable working overlay so the original image is not
     mutated:
       qemu-img create -f qcow2 -b "$ICN_APPLIANCE_IMAGE" -F qcow2 vm-overlay.qcow2

  4) Prepare cloud-init seed:
       - If ICN_APPLIANCE_CLOUD_INIT_SEED is set, use it.
       - Else, refuse to use the example unless the placeholder SSH key
         has been replaced by the operator. If replaced, run
         cloud-localds against the example files to build a seed ISO.

  5) Launch qemu-system-x86_64 under user-mode networking with:
       - SSH hostfwd 127.0.0.1:${SSH_PORT}->22
       - drive: working overlay
       - drive: cloud-init seed ISO (read-only)
       - no display; serial-on-stdout for diagnostics

  6) Wait for SSH on 127.0.0.1:${SSH_PORT}, bounded by VM_TIMEOUT.

  7) Over SSH, verify:
       systemctl is-active icn-appliance-firstboot.service  || cat marker
       systemctl is-active icnd
       curl -sf http://127.0.0.1:${HEALTH_PORT}/v1/health

  8) On failure, capture journalctl for icn-appliance-firstboot.service
     and icnd. Print to stdout. Exit non-zero.

  9) On success, print one-line PASS summary.

  10) Trap-handler kills QEMU and removes the overlay on exit.
EOF_PLAN
}

print_plan

# ---------- dry-run exit ----------
if [ "$MODE" = "dry-run" ]; then
    log "Exiting cleanly. No VM was booted."
    exit 0
fi

# ---------- --real path ----------

# Required env vars
require_env() {
    local name="$1"
    if [ -z "${!name:-}" ]; then
        err "$name is required for --real. See script header."
        exit 2
    fi
}
require_env ICN_APPLIANCE_IMAGE
require_env ICN_APPLIANCE_SSH_KEY

# Required tools
require_tool() {
    local t="$1"
    if ! command -v "$t" >/dev/null 2>&1; then
        err "required tool not found on PATH: $t"
        return 1
    fi
}
MISSING=0
# sha256sum was previously listed but never used; the smoke validates the
# image by booting it, not by hashing it. Re-add if we ever log/verify the
# image hash here.
for t in qemu-system-x86_64 ssh curl qemu-img; do
    if ! require_tool "$t"; then
        MISSING=1
    fi
done
if [ "$MISSING" -ne 0 ]; then
    err "One or more required tools missing. On Debian/Ubuntu:"
    err "  qemu-system-x86_64 -> qemu-system-x86"
    err "  qemu-img           -> qemu-utils"
    err "  ssh                -> openssh-client"
    err "  curl               -> curl"
    err "  cloud-localds (if needed) -> cloud-image-utils"
    exit 3
fi

# Validate inputs
if [ ! -f "$ICN_APPLIANCE_IMAGE" ]; then
    err "Appliance image not found: $ICN_APPLIANCE_IMAGE"
    exit 4
fi
if [ ! -f "$ICN_APPLIANCE_SSH_KEY" ]; then
    err "SSH key not found: $ICN_APPLIANCE_SSH_KEY"
    exit 4
fi

# Cloud-init seed
WORK_DIR="$(mktemp -d -t icn-smoke.XXXXXX)"
log "Working dir: $WORK_DIR"

cleanup() {
    local rc=$?
    if [ -n "${QEMU_PID:-}" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        log "Terminating QEMU (pid $QEMU_PID)..."
        kill "$QEMU_PID" 2>/dev/null || true
        # Give it a moment then force-kill if still up.
        sleep 1
        kill -9 "$QEMU_PID" 2>/dev/null || true
    fi
    if [ -d "$WORK_DIR" ]; then
        rm -rf "$WORK_DIR"
    fi
    exit $rc
}
trap cleanup EXIT INT TERM

SEED_ISO=""
if [ -n "${ICN_APPLIANCE_CLOUD_INIT_SEED:-}" ]; then
    if [ ! -f "$ICN_APPLIANCE_CLOUD_INIT_SEED" ]; then
        err "Cloud-init seed not found: $ICN_APPLIANCE_CLOUD_INIT_SEED"
        exit 4
    fi
    SEED_ISO="$ICN_APPLIANCE_CLOUD_INIT_SEED"
    log "Using operator-supplied cloud-init seed: $SEED_ISO"
else
    # Build a seed ISO from the example files. Refuse the placeholder key.
    EX_USER="$SCRIPT_DIR/cloud-init/user-data.example.yaml"
    EX_META="$SCRIPT_DIR/cloud-init/meta-data.example.yaml"
    if [ ! -f "$EX_USER" ] || [ ! -f "$EX_META" ]; then
        err "Cloud-init example files missing under $SCRIPT_DIR/cloud-init/"
        exit 4
    fi
    if grep -q "INVALIDREPLACEME" "$EX_USER"; then
        err "user-data still has the placeholder SSH key."
        err "Copy $EX_USER, paste your smoke-only SSH PUBLIC key under"
        err "ssh_authorized_keys, save it somewhere outside the repo, and"
        err "either point ICN_APPLIANCE_CLOUD_INIT_SEED at a pre-built seed"
        err "ISO or supply the edited user-data via that path."
        exit 5
    fi
    if ! command -v cloud-localds >/dev/null 2>&1; then
        err "No seed ISO supplied and cloud-localds is missing."
        err "Install cloud-image-utils, or pre-build a seed ISO and set"
        err "ICN_APPLIANCE_CLOUD_INIT_SEED."
        exit 3
    fi
    SEED_ISO="$WORK_DIR/seed.iso"
    cloud-localds "$SEED_ISO" "$EX_USER" "$EX_META"
    log "Built seed ISO: $SEED_ISO"
fi

# Image overlay so we never mutate the source qcow2.
# The overlay itself is always qcow2 (we need copy-on-write semantics), but
# the BACKING image format must match whatever build-image.sh produced —
# ICN_APPLIANCE_IMAGE_FORMAT defaults to qcow2 but can be raw. Detect the
# real format from the file so `-F` is honest; otherwise qemu-img errors out
# with "Backing file specified without explicit format" on a raw base.
BACKING_FORMAT="$(qemu-img info --output=json "$ICN_APPLIANCE_IMAGE" 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("format","qcow2"))' \
    2>/dev/null || echo qcow2)"
case "$BACKING_FORMAT" in
    qcow2|raw|vmdk|vdi|vhdx) ;;
    *)
        warn "Unrecognized backing-image format '$BACKING_FORMAT' from qemu-img info; defaulting to qcow2."
        BACKING_FORMAT="qcow2"
        ;;
esac
OVERLAY="$WORK_DIR/vm-overlay.qcow2"
log "Creating disposable overlay $OVERLAY (backing format: $BACKING_FORMAT) ..."
qemu-img create -f qcow2 -b "$ICN_APPLIANCE_IMAGE" -F "$BACKING_FORMAT" "$OVERLAY" >/dev/null

# Launch QEMU under user-mode networking. We do NOT touch host networking.
log "Launching QEMU (user-mode net, hostfwd ${SSH_PORT}->22)..."
qemu-system-x86_64 \
    -machine accel=kvm:tcg \
    -m "$VM_MEMORY" \
    -smp "$VM_CPUS" \
    -display none \
    -serial file:"$WORK_DIR/serial.log" \
    -drive "if=virtio,format=qcow2,file=$OVERLAY" \
    -drive "if=virtio,format=raw,file=$SEED_ISO,readonly=on" \
    -netdev "user,id=net0,hostfwd=tcp:127.0.0.1:${SSH_PORT}-:22" \
    -device "virtio-net-pci,netdev=net0" \
    -nographic \
    >"$WORK_DIR/qemu.stdout" 2>"$WORK_DIR/qemu.stderr" &
QEMU_PID=$!
log "QEMU pid: $QEMU_PID"

# Wait for SSH on the host-forwarded port.
SSH_OPTS=(
    -o "UserKnownHostsFile=/dev/null"
    -o "StrictHostKeyChecking=no"
    -o "PasswordAuthentication=no"
    -o "ConnectTimeout=5"
    -o "BatchMode=yes"
    -i "$ICN_APPLIANCE_SSH_KEY"
    -p "$SSH_PORT"
)

wait_for_ssh() {
    local deadline=$(( $(date +%s) + VM_TIMEOUT ))
    log "Waiting for SSH (up to ${VM_TIMEOUT}s)..."
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            err "QEMU exited before SSH became reachable."
            return 1
        fi
        if ssh "${SSH_OPTS[@]}" "${SSH_USER}@127.0.0.1" "true" 2>/dev/null; then
            log "SSH is up."
            return 0
        fi
        sleep 3
    done
    err "Timed out waiting for SSH."
    return 1
}

if ! wait_for_ssh; then
    err "Smoke failed: SSH never came up."
    echo "----- last 50 lines of serial.log -----"
    tail -50 "$WORK_DIR/serial.log" 2>/dev/null || true
    echo "----- qemu stderr -----"
    cat "$WORK_DIR/qemu.stderr" 2>/dev/null || true
    exit 6
fi

# Run verifications inside the VM.
run_in_vm() {
    ssh "${SSH_OPTS[@]}" "${SSH_USER}@127.0.0.1" "$@"
}

log "Verifying icn-appliance-firstboot.service ran (oneshot; check marker)..."
# Bounded wait. icn-appliance-firstboot.service is Before=icnd.service in the
# image's systemd unit, but that ordering is NOT relative to cloud-init / SSH.
# On slower boots SSH can be reachable before the oneshot finishes writing
# the marker. Mirror the bounded-wait pattern used below for icnd.service and
# /v1/health so a healthy image doesn't fail on the marker check.
FIRSTBOOT_DEADLINE=$(( $(date +%s) + 120 ))
FIRSTBOOT_OK=0
while [ "$(date +%s)" -lt "$FIRSTBOOT_DEADLINE" ]; do
    if run_in_vm "sudo test -f /var/lib/icn/.firstboot-complete"; then
        FIRSTBOOT_OK=1
        break
    fi
    sleep 3
done
if [ "$FIRSTBOOT_OK" -ne 1 ]; then
    err "firstboot marker still missing at /var/lib/icn/.firstboot-complete after 120s."
    run_in_vm "sudo systemctl status icn-appliance-firstboot.service --no-pager" || true
    run_in_vm "sudo journalctl -u icn-appliance-firstboot.service --no-pager -n 100" || true
    exit 7
fi
log "firstboot marker present."

log "Waiting for icnd.service to become active (bounded)..."
ICND_DEADLINE=$(( $(date +%s) + 120 ))
ICND_OK=0
while [ "$(date +%s)" -lt "$ICND_DEADLINE" ]; do
    if run_in_vm "systemctl is-active icnd >/dev/null"; then
        ICND_OK=1
        break
    fi
    sleep 3
done
if [ "$ICND_OK" -ne 1 ]; then
    err "icnd never reached active state."
    run_in_vm "sudo journalctl -u icnd.service --no-pager -n 200" || true
    exit 8
fi
log "icnd.service is active."

log "Verifying /v1/health on port ${HEALTH_PORT} from inside the VM..."
HEALTH_DEADLINE=$(( $(date +%s) + 60 ))
HEALTH_OK=0
while [ "$(date +%s)" -lt "$HEALTH_DEADLINE" ]; do
    if run_in_vm "curl -sf http://127.0.0.1:${HEALTH_PORT}/v1/health" >/dev/null 2>&1; then
        HEALTH_OK=1
        break
    fi
    sleep 3
done
if [ "$HEALTH_OK" -ne 1 ]; then
    err "/v1/health never returned 200."
    run_in_vm "sudo journalctl -u icnd.service --no-pager -n 200" || true
    exit 9
fi
log "/v1/health returned 200."

cat <<EOF_PASS
[appliance-smoke] PASS
  image:   $ICN_APPLIANCE_IMAGE
  ssh:     ${SSH_USER}@127.0.0.1:${SSH_PORT}
  health:  http://127.0.0.1:${HEALTH_PORT}/v1/health (via SSH-in-VM)
  budget:  ${VM_TIMEOUT}s wall, used $(( $(date +%s) - HEALTH_DEADLINE + 60 ))s past start of health wait

NOTE: PASS means the local dev image boots and icnd is healthy. It does
NOT mean the appliance is production, signed, or fit for partner federation.
EOF_PASS
