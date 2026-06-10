#!/usr/bin/env bash
# negative-firstboot-smoke.sh
#
# Negative (fail-closed) firstboot smoke for the ICN appliance.
#
# The positive smoke (smoke-local.sh --real) proves the happy path:
# firstboot runs, the marker appears, icnd becomes active, /v1/health
# returns 200. This script proves the OPPOSITE direction of the same
# gate: when required firstboot material is missing, the appliance must
# refuse to bring icnd up at all.
#
# Scenario implemented (exactly one):
#   missing-firstboot-exec
#     Delete /usr/local/sbin/icn-appliance-firstboot from a DISPOSABLE
#     OVERLAY of the image before first boot. That path is the
#     ExecStart= of icn-appliance-firstboot.service. With it missing,
#     the oneshot fails at start (203/EXEC), and the appliance-only
#     drop-in systemd/icnd.service.d/10-firstboot-gate.conf must hold
#     the line two ways:
#       Requires=icn-appliance-firstboot.service  -> failure propagates,
#         icnd's start job is cancelled ("Dependency failed").
#       ConditionPathExists=/var/lib/icn/.firstboot-complete -> the
#         marker is never written, so even a manual `systemctl start
#         icnd` is skipped by condition.
#     PASS for this script means: firstboot unit failed, marker absent,
#     icnd never active during the observation window, /v1/health never
#     returned 200. If icnd comes up anyway, this script exits non-zero
#     and labels the run FAIL-OPEN.
#
# Two modes:
#   --dry-run  Print the planned smoke and exit. No mutation. (default)
#   --real     Tamper a disposable overlay, boot it, assert fail-closed.
#
# Flags:
#   --scenario NAME   Only `missing-firstboot-exec` is implemented. The
#                     flag exists so future scenarios are additive, not
#                     breaking. Unknown names are refused.
#   --out DIR         Evidence directory (created if missing; preserved
#                     on exit). Default: mktemp under $TMPDIR, path
#                     printed at start. Holds serial log, journals,
#                     unit statuses, hashes, and summary.txt.
#   --force           Allow --real without /dev/kvm access (pure-TCG
#                     run; works, much slower). Without --force the
#                     script refuses so an operator does not start a
#                     30-minute boot by accident.
#
# Required for --real (same convention as smoke-local.sh):
#   ICN_APPLIANCE_IMAGE      Path to the QCOW2 produced by build-image.sh.
#                            NEVER modified; tamper happens on an overlay.
#   ICN_APPLIANCE_SSH_KEY    Path to the smoke-only SSH private key. The
#                            matching public key MUST be in the cloud-init
#                            seed. SSH must still work in the negative
#                            scenario — cloud-init provisions the user
#                            independently of the ICN units.
#
# Optional (same defaults as smoke-local.sh):
#   ICN_APPLIANCE_SSH_USER         Default: debian.
#   ICN_APPLIANCE_SSH_PORT         Default: 2222 (host-side).
#   ICN_APPLIANCE_HEALTH_PORT      Default: 8080 (never 8000).
#   ICN_APPLIANCE_CLOUD_INIT_SEED  Pre-built seed ISO; if unset, built
#                                  from smoke/cloud-init examples and the
#                                  placeholder key is refused.
#   ICN_APPLIANCE_VM_MEMORY        Default: 1024 (MiB).
#   ICN_APPLIANCE_VM_CPUS          Default: 2.
#   ICN_APPLIANCE_VM_TIMEOUT       Default: 600 (seconds; SSH wait budget).
#   ICN_APPLIANCE_NEGATIVE_WINDOW  Default: 60 (seconds). After firstboot
#                                  failure is confirmed, icnd and health
#                                  are polled this long; ANY success in
#                                  the window is a FAIL-OPEN verdict.
#
# Required tools for --real:
#   qemu-system-x86_64, qemu-img, ssh, curl, sha512sum, virt-customize
#   Optional: cloud-localds (only if the seed ISO is not pre-built).
#
# What this smoke does NOT do:
#   - Modify the source image (tamper is overlay-only).
#   - Connect to any real ICN federation. User-mode networking only.
#   - Mutate host networking, K3s, DNS, or host systemd state.
#   - Require root on the host (libguestfs needs a readable
#     /boot/vmlinuz-*; see smoke README).
#   - Touch partner / member / organizer data.
#
# Honest non-claims:
#   A PASS here verifies ONE negative scenario of the firstboot gate on
#   a local dev image. It does NOT certify every appliance failure mode,
#   and it does NOT make the appliance production-ready, signed,
#   immutable, or partner-distributable.
#
# Exit codes:
#   0   fail-closed verified (negative smoke PASS)
#   2   usage / required env missing
#   3   required tool missing
#   4   input file missing
#   5   placeholder SSH key in example user-data
#   6   SSH never came up (infrastructure failure, not a verdict)
#   7   tamper step failed (virt-customize)
#   10  FAIL-OPEN: icnd became active despite failed firstboot
#   11  FAIL-OPEN: /v1/health returned success despite failed firstboot
#   12  gate precondition broken: firstboot did not fail / marker present

set -euo pipefail

MODE="dry-run"
SCENARIO="missing-firstboot-exec"
OUT_DIR=""
FORCE=0

log()  { printf '[neg-smoke] %s\n' "$*"; }
warn() { printf '[neg-smoke] WARN: %s\n' "$*" >&2; }
err()  { printf '[neg-smoke] ERROR: %s\n' "$*" >&2; }

usage() {
    sed -n '2,100p' "$0"
    exit 0
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run)  MODE="dry-run" ; shift ;;
        --real)     MODE="real"    ; shift ;;
        --scenario) SCENARIO="${2:-}" ; shift 2 ;;
        --out)      OUT_DIR="${2:-}"  ; shift 2 ;;
        --force)    FORCE=1 ; shift ;;
        --help|-h)  usage ;;
        *) err "unknown argument: $1" ; exit 2 ;;
    esac
done

if [ "$SCENARIO" != "missing-firstboot-exec" ]; then
    err "unknown scenario: $SCENARIO (implemented: missing-firstboot-exec)"
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SSH_USER="${ICN_APPLIANCE_SSH_USER:-debian}"
SSH_PORT="${ICN_APPLIANCE_SSH_PORT:-2222}"
HEALTH_PORT="${ICN_APPLIANCE_HEALTH_PORT:-8080}"
VM_MEMORY="${ICN_APPLIANCE_VM_MEMORY:-1024}"
VM_CPUS="${ICN_APPLIANCE_VM_CPUS:-2}"
VM_TIMEOUT="${ICN_APPLIANCE_VM_TIMEOUT:-600}"
NEG_WINDOW="${ICN_APPLIANCE_NEGATIVE_WINDOW:-60}"

# In-image paths under test. These mirror deploy/appliance/systemd/*.
FIRSTBOOT_EXEC="/usr/local/sbin/icn-appliance-firstboot"
MARKER="/var/lib/icn/.firstboot-complete"

log "Mode:        $MODE"
log "Scenario:    $SCENARIO"
log "Tamper:      delete $FIRSTBOOT_EXEC from a disposable overlay"
log "Gate under test: icnd.service.d/10-firstboot-gate.conf (Requires= + ConditionPathExists=$MARKER)"
log "VM SSH:      ${SSH_USER}@127.0.0.1:${SSH_PORT}"
log "Health port: ${HEALTH_PORT} (never 8000)"
log "SSH budget:  ${VM_TIMEOUT}s; negative observation window: ${NEG_WINDOW}s"
log "Image:       ${ICN_APPLIANCE_IMAGE:-<unset>}"
echo

print_plan() {
cat <<'EOF_PLAN'
[neg-smoke] Planned negative smoke steps:

  1) Validate tools: qemu-system-x86_64, qemu-img, ssh, curl, sha512sum,
     virt-customize (+ cloud-localds if no seed ISO supplied).

  2) Record SHA512 of the source image. The source image is NEVER
     modified by this script.

  3) Create a disposable qcow2 overlay backed by the source image.

  4) TAMPER (overlay only): virt-customize --delete the firstboot
     ExecStart binary. This is the "missing firstboot material"
     injection. Record overlay SHA512 after tamper.

  5) Prepare/validate the cloud-init seed (same rules as the positive
     smoke; the placeholder key is refused).

  6) Boot the TAMPERED overlay under QEMU user-mode networking and wait
     for SSH. SSH comes from cloud-init, not from the ICN units, so it
     must still work.

  7) Assert fail-closed, in order:
       a. icn-appliance-firstboot.service reaches state "failed"
          (bounded wait).
       b. The marker /var/lib/icn/.firstboot-complete does NOT exist.
       c. For the whole observation window, `systemctl is-active icnd`
          is never "active". Any "active" => exit 10 (FAIL-OPEN).
       d. For the whole observation window, /v1/health never succeeds.
          Any success => exit 11 (FAIL-OPEN).

  8) Capture evidence into the --out directory: unit statuses, journals
     (firstboot, icnd, "Dependency failed" lines), marker check,
     cloud-init status, serial log, hashes, and a summary.txt verdict.

  9) Exit 0 only if every fail-closed assertion held.
EOF_PLAN
}

print_plan

if [ "$MODE" = "dry-run" ]; then
    log "Exiting cleanly. No VM was booted, nothing was tampered."
    exit 0
fi

# ---------- --real path ----------

require_env() {
    local name="$1"
    if [ -z "${!name:-}" ]; then
        err "$name is required for --real. See script header."
        exit 2
    fi
}
require_env ICN_APPLIANCE_IMAGE
require_env ICN_APPLIANCE_SSH_KEY

MISSING=0
for t in qemu-system-x86_64 qemu-img ssh curl sha512sum virt-customize; do
    if ! command -v "$t" >/dev/null 2>&1; then
        err "required tool not found on PATH: $t"
        MISSING=1
    fi
done
if [ "$MISSING" -ne 0 ]; then
    err "One or more required tools missing. On Debian/Ubuntu:"
    err "  qemu-system-x86_64 -> qemu-system-x86"
    err "  qemu-img           -> qemu-utils"
    err "  virt-customize     -> libguestfs-tools (needs readable /boot/vmlinuz-*)"
    exit 3
fi

# Host-class guard: a pure-TCG run works but is slow enough that an
# operator should opt in deliberately.
if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    if [ "$FORCE" -eq 1 ]; then
        warn "/dev/kvm not accessible; continuing under TCG because --force was given."
    else
        err "/dev/kvm is not accessible for this user. QEMU would fall back to"
        err "TCG (much slower boot). Re-run with --force to accept that, or fix"
        err "KVM access first."
        exit 3
    fi
fi

if [ ! -f "$ICN_APPLIANCE_IMAGE" ]; then
    err "Appliance image not found: $ICN_APPLIANCE_IMAGE"
    exit 4
fi
if [ ! -f "$ICN_APPLIANCE_SSH_KEY" ]; then
    err "SSH key not found: $ICN_APPLIANCE_SSH_KEY"
    exit 4
fi

# Evidence directory: preserved on exit, including on failure.
if [ -z "$OUT_DIR" ]; then
    OUT_DIR="$(mktemp -d -t icn-neg-smoke-out.XXXXXX)"
fi
mkdir -p "$OUT_DIR"
log "Evidence dir: $OUT_DIR (preserved on exit)"

WORK_DIR="$(mktemp -d -t icn-neg-smoke.XXXXXX)"
log "Working dir:  $WORK_DIR (removed on exit)"

cleanup() {
    local rc=$?
    if [ -n "${QEMU_PID:-}" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        log "Terminating QEMU (pid $QEMU_PID)..."
        kill "$QEMU_PID" 2>/dev/null || true
        sleep 1
        kill -9 "$QEMU_PID" 2>/dev/null || true
    fi
    # Preserve boot diagnostics in the evidence dir before the workdir goes.
    for f in serial.log qemu.stdout qemu.stderr; do
        [ -f "$WORK_DIR/$f" ] && cp -f "$WORK_DIR/$f" "$OUT_DIR/$f" 2>/dev/null
    done
    [ -d "$WORK_DIR" ] && rm -rf "$WORK_DIR"
    log "Evidence preserved in: $OUT_DIR"
    exit $rc
}
trap cleanup EXIT INT TERM

# ---------- hashes (before) ----------
log "Recording SHA512 of the source image (never modified)..."
sha512sum "$ICN_APPLIANCE_IMAGE" | tee "$OUT_DIR/hashes.txt"

# ---------- overlay ----------
BACKING_FORMAT="$(qemu-img info --output=json "$ICN_APPLIANCE_IMAGE" 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("format","qcow2"))' \
    2>/dev/null || echo qcow2)"
case "$BACKING_FORMAT" in
    qcow2|raw|vmdk|vdi|vhdx) ;;
    *) warn "Unrecognized backing format '$BACKING_FORMAT'; defaulting to qcow2."
       BACKING_FORMAT="qcow2" ;;
esac
OVERLAY="$WORK_DIR/vm-overlay-tampered.qcow2"
log "Creating disposable overlay $OVERLAY (backing format: $BACKING_FORMAT)..."
qemu-img create -f qcow2 -b "$ICN_APPLIANCE_IMAGE" -F "$BACKING_FORMAT" "$OVERLAY" >/dev/null

# ---------- tamper (overlay only) ----------
# The negative condition: the firstboot unit's ExecStart binary is gone.
# Everything else in the image is untouched, so any icnd start-up after
# this point can only mean the gate failed open.
log "TAMPER: deleting $FIRSTBOOT_EXEC from the overlay (source image untouched)..."
if ! virt-customize -a "$OVERLAY" --delete "$FIRSTBOOT_EXEC" >"$OUT_DIR/tamper.log" 2>&1; then
    err "virt-customize failed; see $OUT_DIR/tamper.log"
    err "(libguestfs commonly fails when /boot/vmlinuz-* is not readable by this user.)"
    exit 7
fi
log "Tamper applied. Recording overlay SHA512 after tamper..."
sha512sum "$OVERLAY" | tee -a "$OUT_DIR/hashes.txt"

# ---------- cloud-init seed ----------
SEED_ISO=""
if [ -n "${ICN_APPLIANCE_CLOUD_INIT_SEED:-}" ]; then
    if [ ! -f "$ICN_APPLIANCE_CLOUD_INIT_SEED" ]; then
        err "Cloud-init seed not found: $ICN_APPLIANCE_CLOUD_INIT_SEED"
        exit 4
    fi
    SEED_ISO="$ICN_APPLIANCE_CLOUD_INIT_SEED"
    log "Using operator-supplied cloud-init seed: $SEED_ISO"
else
    EX_USER="$SCRIPT_DIR/cloud-init/user-data.example.yaml"
    EX_META="$SCRIPT_DIR/cloud-init/meta-data.example.yaml"
    if [ ! -f "$EX_USER" ] || [ ! -f "$EX_META" ]; then
        err "Cloud-init example files missing under $SCRIPT_DIR/cloud-init/"
        exit 4
    fi
    if grep -q "INVALIDREPLACEME" "$EX_USER"; then
        err "user-data still has the placeholder SSH key; see smoke-local.sh header."
        exit 5
    fi
    if ! command -v cloud-localds >/dev/null 2>&1; then
        err "No seed ISO supplied and cloud-localds is missing (cloud-image-utils)."
        exit 3
    fi
    SEED_ISO="$WORK_DIR/seed.iso"
    cloud-localds "$SEED_ISO" "$EX_USER" "$EX_META"
    log "Built seed ISO: $SEED_ISO"
fi

# ---------- boot the tampered overlay ----------
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
        sleep 5
    done
    err "Timed out waiting for SSH."
    return 1
}

if ! wait_for_ssh; then
    err "Negative smoke could not run: SSH never came up (infrastructure failure, NOT a fail-closed verdict)."
    exit 6
fi

run_in_vm() {
    # shellcheck disable=SC2029  # client-side expansion is intentional here
    ssh "${SSH_OPTS[@]}" "${SSH_USER}@127.0.0.1" "$@"
}

# ---------- assertion a: firstboot unit failed ----------
log "Asserting icn-appliance-firstboot.service reaches 'failed' (bounded wait)..."
FB_DEADLINE=$(( $(date +%s) + 180 ))
FB_STATE=""
while [ "$(date +%s)" -lt "$FB_DEADLINE" ]; do
    FB_STATE="$(run_in_vm "systemctl is-failed icn-appliance-firstboot.service 2>/dev/null" || true)"
    [ "$FB_STATE" = "failed" ] && break
    sleep 5
done
run_in_vm "sudo systemctl status icn-appliance-firstboot.service --no-pager" \
    >"$OUT_DIR/firstboot-status.txt" 2>&1 || true
run_in_vm "sudo journalctl -u icn-appliance-firstboot.service --no-pager -n 100" \
    >"$OUT_DIR/firstboot-journal.txt" 2>&1 || true
if [ "$FB_STATE" != "failed" ]; then
    err "icn-appliance-firstboot.service did not reach 'failed' (last state: '${FB_STATE:-unknown}')."
    err "The injected negative condition did not produce a failing gate; cannot assert fail-closed."
    exit 12
fi
log "firstboot unit state: failed (as expected under tamper)."

# ---------- assertion b: marker absent ----------
log "Asserting the firstboot marker is absent..."
if ! run_in_vm "sudo test ! -f $MARKER"; then
    err "Marker $MARKER exists despite failed firstboot. Gate precondition broken."
    exit 12
fi
log "Marker absent (as expected)."

# ---------- assertions c+d: icnd never active, health never 200 ----------
log "Observing icnd + /v1/health for ${NEG_WINDOW}s; ANY success is FAIL-OPEN..."
OBS_DEADLINE=$(( $(date +%s) + NEG_WINDOW ))
while [ "$(date +%s)" -lt "$OBS_DEADLINE" ]; do
    ICND_STATE="$(run_in_vm "systemctl is-active icnd 2>/dev/null" || true)"
    if [ "$ICND_STATE" = "active" ]; then
        err "FAIL-OPEN: icnd.service became ACTIVE despite failed firstboot."
        run_in_vm "sudo journalctl -u icnd.service --no-pager -n 200" \
            >"$OUT_DIR/icnd-journal.txt" 2>&1 || true
        exit 10
    fi
    if run_in_vm "curl -sf --max-time 4 http://127.0.0.1:${HEALTH_PORT}/v1/health" \
        >/dev/null 2>&1; then
        err "FAIL-OPEN: /v1/health answered successfully despite failed firstboot."
        exit 11
    fi
    sleep 5
done
log "Observation window over: icnd never active, health never answered."

# ---------- evidence capture ----------
log "Capturing evidence..."
run_in_vm "systemctl is-active icnd 2>/dev/null || true; systemctl show icnd -p ActiveState,SubState,Result,LoadState --no-pager" \
    >"$OUT_DIR/icnd-status.txt" 2>&1 || true
run_in_vm "sudo systemctl status icnd.service --no-pager" \
    >>"$OUT_DIR/icnd-status.txt" 2>&1 || true
run_in_vm "sudo journalctl -u icnd.service --no-pager -n 100" \
    >"$OUT_DIR/icnd-journal.txt" 2>&1 || true
run_in_vm "sudo journalctl -b --no-pager | grep -iE 'dependency failed|firstboot' | tail -40" \
    >"$OUT_DIR/depfail-journal.txt" 2>&1 || true
run_in_vm "cloud-init status 2>/dev/null; sudo test -f $MARKER && echo 'MARKER PRESENT' || echo 'MARKER ABSENT'" \
    >"$OUT_DIR/cloudinit-and-marker.txt" 2>&1 || true

{
    echo "verdict: PASS (fail-closed verified)"
    echo "scenario: $SCENARIO"
    echo "tampered_path: $FIRSTBOOT_EXEC"
    echo "gate: /etc/systemd/system/icnd.service.d/10-firstboot-gate.conf"
    echo "firstboot_unit_state: failed"
    echo "marker: absent ($MARKER)"
    echo "icnd_active_during_window: no (${NEG_WINDOW}s)"
    echo "health_200_during_window: no (${NEG_WINDOW}s, port ${HEALTH_PORT})"
    echo "image: $ICN_APPLIANCE_IMAGE (UNMODIFIED; tamper on overlay only)"
    echo "host: $(hostname) / $(uname -r)"
    echo "date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} | tee "$OUT_DIR/summary.txt"

cat <<EOF_PASS
[neg-smoke] PASS (fail-closed verified)
  scenario:  $SCENARIO
  evidence:  $OUT_DIR
  image:     $ICN_APPLIANCE_IMAGE (not modified)

NOTE: This verifies ONE negative firstboot scenario on a local dev
image. It does NOT certify every appliance failure mode, and it does
NOT make the appliance production-ready, signed, immutable, or
partner-distributable.
EOF_PASS
