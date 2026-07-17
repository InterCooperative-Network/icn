#!/usr/bin/env bash
# Self-contained launcher for the ICN demo appliance image.
#
# Put this script next to:
#   @IMAGE_BASENAME@
#
# It boots a clean throwaway overlay, waits for the services, and prints the
# browser URL. The source image is never modified.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE="${ICN_DEMO_IMAGE:-$SCRIPT_DIR/@IMAGE_BASENAME@}"
# Per-run private work dir (holds the disposable SSH key + cloud-init seed). When
# ICN_DEMO_WORK_ROOT is not set we mktemp a fresh 0700 dir per run and delete it on
# exit — never a fixed /tmp path that another local user on a shared host could
# pre-create (predictable-path / pre-seeded-key attack). An explicit override is
# honored as-is (its lifecycle is the caller's).
if [ -n "${ICN_DEMO_WORK_ROOT:-}" ]; then
  WORK_ROOT="$ICN_DEMO_WORK_ROOT"
  WORK_ROOT_EPHEMERAL=0
  mkdir -p "$WORK_ROOT"
  chmod 700 "$WORK_ROOT" 2>/dev/null || true
else
  WORK_ROOT="$(mktemp -d -t icn-demo-work.XXXXXX)"
  WORK_ROOT_EPHEMERAL=1
fi
SSH_PORT="${ICN_DEMO_SSH_PORT:-22222}"
GW_PORT="${ICN_DEMO_GW_PORT:-18080}"
# The member-shell host-forward port is FIXED at 18090 and intentionally not
# overridable: the demo image's CORS/origin allowlist only permits the fixed
# localhost:8090/18090 origins, so the browser must load the shell from
# localhost:18090 for the session mint + authenticated gateway calls to pass.
# (Gateway/session forward ports are the request TARGETS, not the browser Origin,
# so those remain overridable.)
SHELL_PORT=18090
SESSION_PORT="${ICN_DEMO_SESSION_PORT:-18091}"
SSH_USER="${ICN_DEMO_SSH_USER:-debian}"
VM_MEMORY="${ICN_DEMO_VM_MEMORY:-2048}"
VM_CPUS="${ICN_DEMO_VM_CPUS:-2}"
VM_TIMEOUT="${ICN_DEMO_VM_TIMEOUT:-420}"

KEY="$WORK_ROOT/demo-key"
SEED_ISO="$WORK_ROOT/seed.iso"
RUN_DIR=""
QEMU_PID=""
TUNNEL_PID=""

log() { printf '[icn-demo] %s\n' "$*"; }
err() { printf '[icn-demo] ERROR: %s\n' "$*" >&2; }

cleanup() {
  local rc=$?
  if [ -n "${TUNNEL_PID:-}" ] && kill -0 "$TUNNEL_PID" 2>/dev/null; then
    log "Closing browser tunnels."
    kill "$TUNNEL_PID" 2>/dev/null || true
  fi
  if [ -n "${QEMU_PID:-}" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    log "Stopping VM."
    kill "$QEMU_PID" 2>/dev/null || true
    sleep 1
    kill -9 "$QEMU_PID" 2>/dev/null || true
  fi
  if [ -n "${RUN_DIR:-}" ] && [ -d "$RUN_DIR" ]; then
    rm -rf "$RUN_DIR"
  fi
  if [ "${WORK_ROOT_EPHEMERAL:-0}" = "1" ] && [ -n "${WORK_ROOT:-}" ] && [ -d "$WORK_ROOT" ]; then
    rm -rf "$WORK_ROOT"
  fi
  exit "$rc"
}
trap cleanup EXIT INT TERM

require_tool() {
  command -v "$1" >/dev/null 2>&1 || {
    err "Missing required tool: $1"
    exit 2
  }
}

port_free() {
  local port="$1"
  if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
    exec 3>&- 3<&- 2>/dev/null || true
    err "Port $port is already in use. Free it, then rerun this launcher."
    exit 3
  fi
}

prepare_seed() {
  mkdir -p "$WORK_ROOT"
  if [ ! -f "$KEY" ]; then
    log "Creating disposable VM SSH key at $KEY"
    ssh-keygen -t ed25519 -f "$KEY" -N "" -C icn-demo-vm >/dev/null
  fi

  local pub user_data meta_data
  pub="$(cat "$KEY.pub")"
  user_data="$WORK_ROOT/user-data"
  meta_data="$WORK_ROOT/meta-data"

  cat >"$user_data" <<EOF_USER_DATA
#cloud-config
hostname: icn-demo-vm
manage_etc_hosts: true
users:
  - name: debian
    sudo: ALL=(ALL) NOPASSWD:ALL
    groups: [sudo]
    shell: /bin/bash
    lock_passwd: true
    ssh_authorized_keys:
      - "$pub"
ssh_pwauth: false
disable_root: true
packages: []
final_message: |
  ICN demo VM booted. Non-production demo appliance.
EOF_USER_DATA

  cat >"$meta_data" <<EOF_META_DATA
instance-id: icn-demo-vm
local-hostname: icn-demo-vm
EOF_META_DATA

  cloud-localds "$SEED_ISO" "$user_data" "$meta_data"
}

wait_for_ssh() {
  local deadline=$(( $(date +%s) + VM_TIMEOUT ))
  log "Waiting for VM SSH on 127.0.0.1:$SSH_PORT ..."
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      err "QEMU exited before SSH became reachable."
      tail -80 "$RUN_DIR/serial.log" 2>/dev/null || true
      exit 6
    fi
    if ssh "${SSH_OPTS[@]}" "${SSH_USER}@127.0.0.1" "true" 2>/dev/null; then
      log "VM SSH is ready."
      return 0
    fi
    sleep 3
  done
  err "Timed out waiting for SSH."
  tail -80 "$RUN_DIR/serial.log" 2>/dev/null || true
  exit 6
}

run_in_vm() {
  ssh "${SSH_OPTS[@]}" "${SSH_USER}@127.0.0.1" "$@"
}

wait_in_vm() {
  local label="$1"
  local seconds="$2"
  local command="$3"
  local deadline=$(( $(date +%s) + seconds ))
  log "Waiting for $label ..."
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if run_in_vm "$command" >/dev/null 2>&1; then
      log "$label ready."
      return 0
    fi
    sleep 3
  done
  err "$label did not become ready."
  exit 7
}

[ -f "$IMAGE" ] || { err "Image not found: $IMAGE"; exit 2; }

require_tool qemu-system-x86_64
require_tool qemu-img
require_tool cloud-localds
require_tool ssh
require_tool curl
require_tool sha256sum

port_free "$SSH_PORT"
port_free "$GW_PORT"
port_free "$SHELL_PORT"
port_free "$SESSION_PORT"

prepare_seed

RUN_DIR="$(mktemp -d -t icn-demo-vm.XXXXXX)"
OVERLAY="$RUN_DIR/vm-overlay.qcow2"
log "Creating clean VM overlay."
qemu-img create -f qcow2 -b "$IMAGE" -F qcow2 "$OVERLAY" >/dev/null

log "Booting ICN demo VM."
qemu-system-x86_64 \
  -machine accel=kvm:tcg \
  -m "$VM_MEMORY" \
  -smp "$VM_CPUS" \
  -display none \
  -serial file:"$RUN_DIR/serial.log" \
  -drive "if=virtio,format=qcow2,file=$OVERLAY" \
  -drive "if=virtio,format=raw,file=$SEED_ISO,readonly=on" \
  -netdev "user,id=net0,hostfwd=tcp:127.0.0.1:${SSH_PORT}-:22" \
  -device "virtio-net-pci,netdev=net0" \
  -nographic \
  >"$RUN_DIR/qemu.stdout" 2>"$RUN_DIR/qemu.stderr" &
QEMU_PID=$!

SSH_OPTS=(
  -o UserKnownHostsFile=/dev/null
  -o StrictHostKeyChecking=no
  -o PasswordAuthentication=no
  -o ConnectTimeout=5
  -o BatchMode=yes
  -i "$KEY"
  -p "$SSH_PORT"
)

wait_for_ssh
wait_in_vm "firstboot" 120 "sudo test -f /var/lib/icn/.firstboot-complete"
wait_in_vm "icnd" 120 "systemctl is-active icnd"
wait_in_vm "gateway health" 60 "curl -sf http://127.0.0.1:8080/v1/health"
wait_in_vm "member shell" 60 "curl -sf http://127.0.0.1:8090/member-shell/index.html"

log "Opening localhost tunnels to the VM services."
ssh -N \
  -o ExitOnForwardFailure=yes \
  -o UserKnownHostsFile=/dev/null \
  -o StrictHostKeyChecking=no \
  -o ServerAliveInterval=30 \
  -o GatewayPorts=no \
  -L "127.0.0.1:${GW_PORT}:127.0.0.1:8080" \
  -L "127.0.0.1:${SHELL_PORT}:127.0.0.1:8090" \
  -L "127.0.0.1:${SESSION_PORT}:127.0.0.1:8091" \
  -i "$KEY" \
  -p "$SSH_PORT" \
  "${SSH_USER}@127.0.0.1" &
TUNNEL_PID=$!

# The forwards MUST stay loopback-only. Explicit 127.0.0.1 bind addresses plus
# GatewayPorts=no defeat a host ssh_config that sets `GatewayPorts yes` (which
# would otherwise bind these on wildcard interfaces and expose the demo to the LAN).

TUNNEL_UP=0
for _ in $(seq 1 30); do
  if ! kill -0 "$TUNNEL_PID" 2>/dev/null; then
    err "The browser tunnel exited before it became reachable (a local process may have won the port bind race). Free the ports and rerun."
    exit 8
  fi
  if curl -sf -m 3 "http://127.0.0.1:${SHELL_PORT}/member-shell/index.html" >/dev/null 2>&1; then
    TUNNEL_UP=1
    break
  fi
  sleep 1
done
if [ "$TUNNEL_UP" -ne 1 ]; then
  err "The browser surface did not become reachable at http://127.0.0.1:${SHELL_PORT}; not printing a dead URL."
  exit 8
fi

URL="http://localhost:${SHELL_PORT}/member-shell/?mode=live&demo=launcher&gw=${GW_PORT}&session=${SESSION_PORT}"

cat <<EOF_READY

ICN demo VM is ready.

Open this in the browser:
  $URL

Health check:
  http://localhost:${GW_PORT}/v1/health

In the page, click "Start local demo".
Leave this terminal open during the demo. Press Ctrl-C here to stop the VM.

EOF_READY

if [ "${ICN_DEMO_NO_BROWSER:-0}" != "1" ]; then
  for opener in xdg-open gio open; do
    if command -v "$opener" >/dev/null 2>&1; then
      if [ "$opener" = "gio" ]; then
        "$opener" open "$URL" >/dev/null 2>&1 || true
      else
        "$opener" "$URL" >/dev/null 2>&1 || true
      fi
      break
    fi
  done
fi

if [ "${ICN_DEMO_SMOKE_ONLY:-0}" = "1" ]; then
  log "Smoke-only mode complete; stopping VM."
  exit 0
fi

wait "$QEMU_PID"
