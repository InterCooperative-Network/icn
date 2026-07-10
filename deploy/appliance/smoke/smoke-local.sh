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
# Demo-profile add-on (requires an image built with
# ICN_APPLIANCE_DEMO_PROFILE=1):
#   --demo     After the base checks pass, additionally:
#                - forward the gateway and member-shell ports to the host
#                  (hostfwd 127.0.0.1:GW_FWD->8080, 127.0.0.1:SHELL_FWD->8090)
#                - verify the member-shell static server answers (host-side,
#                  the stranger path)
#                - run `sudo icn-demo-seed --json` in the VM
#                - drive the member loop from the HOST through the forwarded
#                  gateway: standing -> action card -> complete -> receipt,
#                  with the receipt binding check (32-byte record_hash)
#                - block guest-initiated outbound networking by DEFAULT
#                  (QEMU user-net restrict=on; explicitly set forwarding
#                  rules are unaffected per the QEMU manual) and prove it
#                  with an in-guest canary probe: a host-loopback listener
#                  started by this script must be reachable from the HOST
#                  and unreachable from the GUEST (via the 10.0.2.2 slirp
#                  host alias — no public internet host involved, so an
#                  offline runner cannot false-pass)
#              A --demo pass means the demo loop works end-to-end against
#              this image from a clean boot. It still does NOT mean
#              production, pilot, or federation.
#
# Test hook:
#   --print-netdev  Print the exact -netdev string this invocation would
#                   pass to QEMU (honoring --demo and env) and exit. Used
#                   by smoke/net-restrict-check.sh; no VM, no env needed.
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
#   ICN_APPLIANCE_VM_MEMORY      Default: 1024 (MiB; consider 2048 with --demo).
#   ICN_APPLIANCE_VM_CPUS        Default: 2.
#   ICN_APPLIANCE_VM_TIMEOUT     Default: 300 (seconds; total smoke budget).
#   ICN_APPLIANCE_GW_FWD_PORT    Default: 18080 (host port forwarded to the
#                                VM gateway :8080; --demo only).
#   ICN_APPLIANCE_SHELL_FWD_PORT Default: 18090 (host port forwarded to the
#                                VM member-shell :8090; --demo only).
#   ICN_APPLIANCE_ALLOW_OUTBOUND Set to 1 to SKIP the --demo default of
#                                isolating the guest network (restrict=on).
#                                The run then permits guest-initiated
#                                outbound traffic and SKIPS the canary
#                                probe; the output says so loudly.
#   ICN_APPLIANCE_CANARY_PORT    Default: 18099. Host loopback port for the
#                                outbound-isolation canary listener
#                                (--demo restricted runs only).
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
    awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$0"
    exit 0
}

DEMO=0
PRINT_NETDEV=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) MODE="dry-run" ; shift ;;
        --real)    MODE="real"    ; shift ;;
        --demo)    DEMO=1         ; shift ;;
        --print-netdev) PRINT_NETDEV=1 ; shift ;;
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
GW_FWD_PORT="${ICN_APPLIANCE_GW_FWD_PORT:-18080}"
SHELL_FWD_PORT="${ICN_APPLIANCE_SHELL_FWD_PORT:-18090}"
ALLOW_OUTBOUND="${ICN_APPLIANCE_ALLOW_OUTBOUND:-0}"
CANARY_PORT="${ICN_APPLIANCE_CANARY_PORT:-18099}"

# Single source of truth for the QEMU -netdev string. --demo defaults to an
# ISOLATED guest: user-net restrict=on blocks guest-initiated traffic to the
# host and to the outside, while explicitly set hostfwd rules keep working
# (QEMU manual, `restrict=on|off`). Base (non-demo) smoke is UNCHANGED.
build_netdev() {
    local nd="user,id=net0,hostfwd=tcp:127.0.0.1:${SSH_PORT}-:22"
    if [ "$DEMO" = 1 ]; then
        nd="${nd},hostfwd=tcp:127.0.0.1:${GW_FWD_PORT}-:8080,hostfwd=tcp:127.0.0.1:${SHELL_FWD_PORT}-:8090"
        if [ "$ALLOW_OUTBOUND" != "1" ]; then
            nd="${nd},restrict=on"
        fi
    fi
    printf '%s\n' "$nd"
}

# Test hook: print the constructed netdev and exit before any logging, env
# requirement, or tool check — deterministic and VM-free.
if [ "$PRINT_NETDEV" = 1 ]; then
    build_netdev
    exit 0
fi

log "Mode: $MODE"
log "Demo add-on:      $DEMO"
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
       - with --demo: gateway + member-shell hostfwd, and guest outbound
         BLOCKED by default (user-net restrict=on; set
         ICN_APPLIANCE_ALLOW_OUTBOUND=1 to permit outbound). A restricted
         --demo run ends with an isolation canary: a host-loopback
         listener must be reachable from the host and unreachable from
         the guest.

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
    if [ -n "${CANARY_PID:-}" ] && kill -0 "$CANARY_PID" 2>/dev/null; then
        kill "$CANARY_PID" 2>/dev/null || true
    fi
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
# Demo add-on forwards the gateway and member-shell so the loop can be driven
# from the host — the same path a stranger's browser takes — and isolates the
# guest by default (see build_netdev).
NETDEV="$(build_netdev)"
if [ "$DEMO" = 1 ]; then
    log "Launching QEMU (user-mode net, hostfwd ${SSH_PORT}->22, ${GW_FWD_PORT}->8080, ${SHELL_FWD_PORT}->8090)..."
    if [ "$ALLOW_OUTBOUND" != "1" ]; then
        log "Guest outbound: BLOCKED by default (user-net restrict=on; hostfwd unaffected)."
        log "                Set ICN_APPLIANCE_ALLOW_OUTBOUND=1 to permit guest outbound."
    else
        warn "Guest outbound: ALLOWED (ICN_APPLIANCE_ALLOW_OUTBOUND=1) — the isolation canary probe will be SKIPPED."
    fi
else
    log "Launching QEMU (user-mode net, hostfwd ${SSH_PORT}->22)..."
fi
qemu-system-x86_64 \
    -machine accel=kvm:tcg \
    -m "$VM_MEMORY" \
    -smp "$VM_CPUS" \
    -display none \
    -serial file:"$WORK_DIR/serial.log" \
    -drive "if=virtio,format=qcow2,file=$OVERLAY" \
    -drive "if=virtio,format=raw,file=$SEED_ISO,readonly=on" \
    -netdev "$NETDEV" \
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

# ---------- --demo add-on: drive the member loop from the host ----------
if [ "$DEMO" = 1 ]; then
    command -v jq >/dev/null 2>&1 || { err "--demo requires jq on the host"; exit 10; }

    log "[demo] Verifying member-shell static server from the HOST (stranger path)..."
    SHELL_DEADLINE=$(( $(date +%s) + 60 ))
    SHELL_OK=0
    while [ "$(date +%s)" -lt "$SHELL_DEADLINE" ]; do
        if curl -sf -m 5 "http://127.0.0.1:${SHELL_FWD_PORT}/member-shell/index.html" >/dev/null 2>&1; then
            SHELL_OK=1
            break
        fi
        sleep 3
    done
    if [ "$SHELL_OK" -ne 1 ]; then
        err "[demo] member-shell never answered on host port ${SHELL_FWD_PORT} (is the image built with ICN_APPLIANCE_DEMO_PROFILE=1?)"
        run_in_vm "sudo systemctl status icn-member-shell.service --no-pager" || true
        run_in_vm "sudo journalctl -u icn-member-shell.service --no-pager -n 50" || true
        exit 11
    fi
    curl -sf -m 5 "http://127.0.0.1:${SHELL_FWD_PORT}/pilot-ui/fixtures/icn-organizer-demo/standing.json" >/dev/null 2>&1 \
        || { err "[demo] fixture pack missing from the served payload"; exit 11; }
    log "[demo] member-shell + fixture pack served."

    # curl does not enforce CORS, but the browser this smoke stands in for
    # does (the shell's Authorization header forces a preflight). Assert the
    # gateway actually allows the shell origin for the configured forward
    # port, so a custom SHELL_FWD_PORT outside the image's CORS allowlist
    # cannot produce a DEMO PASS that a real browser would contradict.
    # Assert BOTH loopback spellings for the configured port: the smoke
    # prints 127.0.0.1 URLs while docs use localhost, and the browser sends
    # whichever the operator typed. The demo drop-in allowlists both for
    # the supported ports; a custom port must too.
    for ORIGIN in "http://localhost:${SHELL_FWD_PORT}" "http://127.0.0.1:${SHELL_FWD_PORT}"; do
        log "[demo] CORS preflight for browser origin ${ORIGIN}..."
        CORS_HEADERS="$(curl -s -m 10 -X OPTIONS -D - -o /dev/null \
            -H "Origin: ${ORIGIN}" \
            -H "Access-Control-Request-Method: GET" \
            -H "Access-Control-Request-Headers: authorization" \
            "http://127.0.0.1:${GW_FWD_PORT}/v1/gov/me/standing" 2>/dev/null)"
        if ! grep -qiE "access-control-allow-origin: *(${ORIGIN}|\*)" <<<"$CORS_HEADERS"; then
            err "[demo] gateway CORS does not allow ${ORIGIN} — a real browser at that URL would fail even though curl checks pass."
            err "[demo] the demo image allowlists both loopback spellings for shell ports 8090/18090; use those, or extend ICN_CORS_ORIGINS in the image's icnd drop-in."
            exit 11
        fi
    done
    log "[demo] CORS preflight ok for both loopback origins on port ${SHELL_FWD_PORT}."

    log "[demo] Seeding the demo loop in the VM (sudo icn-demo-seed --json)..."
    SEED_JSON="$(run_in_vm "sudo icn-demo-seed --json" 2>/dev/null)" || {
        err "[demo] icn-demo-seed failed."
        run_in_vm "sudo journalctl -u icnd.service --no-pager -n 100" || true
        exit 12
    }
    DEMO_JWT="$(jq -r '.jwt // empty' <<<"$SEED_JSON")"
    DEMO_ITEM="$(jq -r '.item_id // empty' <<<"$SEED_JSON")"
    DEMO_DOMAIN="$(jq -r '.domain // empty' <<<"$SEED_JSON")"
    DEMO_DID="$(jq -r '.did // empty' <<<"$SEED_JSON")"
    if [ -z "$DEMO_JWT" ] || [ -z "$DEMO_ITEM" ] || [ -z "$DEMO_DOMAIN" ]; then
        err "[demo] seed output incomplete: $SEED_JSON"
        exit 12
    fi
    # Fail closed if the standing bootstrap silently degraded — the standing
    # pane is step 1 of the demo, and the seed downgrades a failed bootstrap
    # to a warning that a happy-path check would never see.
    STANDING_NOTE="$(jq -r '.standing_note // empty' <<<"$SEED_JSON")"
    if [ "$STANDING_NOTE" != "bootstrap-standing: ok" ]; then
        err "[demo] standing bootstrap did not succeed: $STANDING_NOTE"
        exit 12
    fi
    log "[demo] seeded: item $DEMO_ITEM in $DEMO_DOMAIN for $DEMO_DID (standing bootstrap ok)"

    GWH="http://127.0.0.1:${GW_FWD_PORT}"
    AUTH="Authorization: Bearer $DEMO_JWT"

    log "[demo] 1/4 standing (host-side GET /v1/gov/me/standing)..."
    curl -sf -m 10 -H "$AUTH" "$GWH/v1/gov/me/standing" | jq -e '.did' >/dev/null \
        || { err "[demo] standing fetch failed"; exit 13; }

    log "[demo] 2/4 action card visible (GET /v1/gov/me/action-cards)..."
    CARDS="$(curl -sf -m 10 -H "$AUTH" "$GWH/v1/gov/me/action-cards")" \
        || { err "[demo] action-cards fetch failed"; exit 13; }
    N_BEFORE="$(jq --arg id "$DEMO_ITEM" '[.cards[]? | select(.source_id==$id)] | length' <<<"$CARDS")"
    [ "$N_BEFORE" = "1" ] || { err "[demo] expected 1 open card for $DEMO_ITEM, found $N_BEFORE"; exit 13; }

    log "[demo] 3/4 discharge (PUT .../status {\"status\":\"completed\"} — the member-shell's documented call)..."
    curl -sf -m 10 -X PUT -H "$AUTH" -H 'Content-Type: application/json' \
        -d '{"status":"completed"}' \
        "$GWH/v1/gov/domains/$DEMO_DOMAIN/action-items/$DEMO_ITEM/status" >/dev/null \
        || { err "[demo] completion PUT failed"; exit 13; }

    log "[demo] 4/4 receipt (GET completion-receipt + binding check)..."
    RECEIPT="$(curl -sf -m 10 -H "$AUTH" \
        "$GWH/v1/gov/domains/$DEMO_DOMAIN/action-items/$DEMO_ITEM/completion-receipt")" \
        || { err "[demo] receipt fetch failed"; exit 13; }
    jq -e --arg id "$DEMO_ITEM" --arg dom "$DEMO_DOMAIN" --arg did "$DEMO_DID" \
        '(.record_hash | type == "array" and length == 32 and all(.[]; type == "number" and . >= 0 and . <= 255))
         and .item_id == $id and .domain_id == $dom and .actor_did == $did
         and .transition == "completed"' <<<"$RECEIPT" >/dev/null \
        || { err "[demo] receipt failed the binding check: $RECEIPT"; exit 13; }

    N_AFTER="$(curl -sf -m 10 -H "$AUTH" "$GWH/v1/gov/me/action-cards" \
        | jq --arg id "$DEMO_ITEM" '[.cards[]? | select(.source_id==$id)] | length')"
    [ "$N_AFTER" = "0" ] || { err "[demo] card did not clear after completion"; exit 13; }
    log "[demo] loop complete: standing -> card -> discharge -> receipt -> card cleared."

    # ---------- outbound-isolation canary (restricted runs only) ----------
    if [ "$ALLOW_OUTBOUND" != "1" ]; then
        log "[demo] outbound-isolation canary: guest must NOT reach a host listener..."
        # 10.0.2.2 is the QEMU user-net (slirp) alias for the host. Without
        # restrict=on a guest can open TCP connections to it; with restrict=on
        # the QEMU-documented isolation must drop them. The listener is
        # started by THIS script and verified reachable host-side first, so a
        # guest-side connection failure is attributable to the isolation, not
        # to a dead listener — and no public internet host is involved, so an
        # offline runner cannot produce a false pass.
        # Serve a dedicated directory containing only a per-run marker file, and
        # validate the exact marker content — so a pre-existing squatter on the
        # port (or a python that failed to bind) can never stand in as the
        # control. The spawned PID is also re-checked AFTER the guest probe:
        # a server that died mid-window would make "guest could not connect"
        # unattributable, so that case fails closed instead of passing.
        CANARY_DIR="$WORK_DIR/canary"
        CANARY_MARKER="icn-canary-$$-$(date +%s)"
        mkdir -p "$CANARY_DIR"
        printf '%s' "$CANARY_MARKER" > "$CANARY_DIR/canary-marker.txt"
        ( cd "$CANARY_DIR" && exec python3 -m http.server "$CANARY_PORT" --bind 127.0.0.1 ) >/dev/null 2>&1 &
        CANARY_PID=$!
        CANARY_UP=0
        for _ in 1 2 3 4 5; do
            if [ "$(curl -sf -m 2 "http://127.0.0.1:${CANARY_PORT}/canary-marker.txt" 2>/dev/null)" = "$CANARY_MARKER" ]; then
                CANARY_UP=1
                break
            fi
            sleep 1
        done
        if [ "$CANARY_UP" -ne 1 ] || ! kill -0 "$CANARY_PID" 2>/dev/null; then
            err "[demo] canary listener did not serve this run's marker on 127.0.0.1:${CANARY_PORT}"
            err "[demo] (port in use by another process, or the listener failed to start)."
            err "[demo] Set ICN_APPLIANCE_CANARY_PORT to a free host port and re-run."
            exit 14
        fi
        if run_in_vm "curl -s -o /dev/null -m 4 http://10.0.2.2:${CANARY_PORT}/canary-marker.txt" 2>/dev/null; then
            err "[demo] FAIL-OPEN: the guest REACHED the host canary listener."
            err "[demo] Guest-initiated outbound is not blocked (restrict=on ineffective?)."
            exit 14
        fi
        if ! kill -0 "$CANARY_PID" 2>/dev/null; then
            err "[demo] canary listener died during the probe window — the guest's"
            err "[demo] connection failure cannot be attributed to isolation. Re-run."
            exit 14
        fi
        kill "$CANARY_PID" 2>/dev/null || true
        CANARY_PID=""
        log "[demo] outbound-isolation canary held: per-run marker served to the host, unreachable from the guest, listener alive throughout."
    else
        warn "[demo] outbound isolation OVERRIDDEN (ICN_APPLIANCE_ALLOW_OUTBOUND=1) — canary probe SKIPPED; the guest may reach external networks."
    fi
fi

cat <<EOF_PASS
[appliance-smoke] PASS
  image:   $ICN_APPLIANCE_IMAGE
  ssh:     ${SSH_USER}@127.0.0.1:${SSH_PORT}
  health:  http://127.0.0.1:${HEALTH_PORT}/v1/health (via SSH-in-VM)
  budget:  ${VM_TIMEOUT}s wall, used $(( $(date +%s) - HEALTH_DEADLINE + 60 ))s past start of health wait

NOTE: PASS means the local dev image boots and icnd is healthy. It does
NOT mean the appliance is production, signed, or fit for partner federation.
EOF_PASS

if [ "$DEMO" = 1 ]; then
    cat <<EOF_DEMO
[appliance-smoke] DEMO PASS
  shell:    http://127.0.0.1:${SHELL_FWD_PORT}/member-shell/ (host-forwarded)
  gateway:  http://127.0.0.1:${GW_FWD_PORT} (host-forwarded, JWT-auth)
  loop:     standing -> action card -> discharge -> receipt (verified host-side)
  outbound: $( [ "$ALLOW_OUTBOUND" != "1" ] \
      && echo "BLOCKED (restrict=on; in-guest canary probe held)" \
      || echo "ALLOWED (ICN_APPLIANCE_ALLOW_OUTBOUND=1; canary skipped)" )

NOTE: DEMO PASS means the member loop works end-to-end on this image from a
clean boot, over the same forwarded ports a stranger's browser would use.
Fictional fixture institution, dev gates, test posture — NOT production,
NOT a pilot, NOT federation.
EOF_DEMO
fi
