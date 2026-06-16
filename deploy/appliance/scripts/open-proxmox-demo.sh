#!/usr/bin/env bash
# ============================================================================
# open-proxmox-demo.sh — one command to open the ICN member-shell DEV/DEMO.
# ----------------------------------------------------------------------------
# DEV/DEMO ONLY. Run this from your workstation (your laptop or dev box). It:
#   1. opens SSH tunnels so the browser uses the sealed demo origins and you
#      reach the node through forwards you control:
#        localhost:18080 -> node instance gateway      :8080  (binds 0.0.0.0)
#        localhost:18090 -> node instance member-shell :8090  (binds 0.0.0.0)
#        localhost:18091 -> node instance demo-session 127.0.0.1:8091 (loopback)
#      Binding note: only the demo-session endpoint binds loopback. In the demo
#      profile the gateway (--gateway-bind 0.0.0.0:8080) and member-shell
#      (--bind 0.0.0.0) bind all interfaces, so on a BRIDGED Proxmox/cloud VM
#      they are reachable on that VM's network — run the demo node on a trusted
#      or isolated network. The tunnel works regardless; this is a DEV/DEMO
#      image, not a hardened deployment.
#   2. waits for the member-shell to answer through the tunnel,
#   3. opens your browser to the member-shell in live mode.
# After the browser opens you do NOT touch the terminal again: click
# "Start local demo" in the page — standing and an action card load with no
# gateway typing and no JWT copy/paste. Ctrl-C here later to close the tunnel.
#
# Terminology: the appliance is a reproducible VM image; this connects to a
# RUNNING NODE INSTANCE (a VM booted from it) on a hypervisor host. Not a
# physical box, not production, not a pilot.
#
# Connectivity (two routes):
#   * Direct: this machine can SSH the node instance and has the demo key.
#       ICN_DEMO_VM_IP   (required — your running node instance IP)
#       ICN_DEMO_SSH_KEY (path to the demo private key)
#   * Jump:   this machine cannot SSH the node directly (e.g. the key lives on
#       a dev host). Set ICN_DEMO_JUMP=<user@dev-host> and the tunnel is built
#       through it; the dev host reaches the node instance and holds the key.
#       ICN_DEMO_JUMP        (e.g. user@dev-host or an ssh-config alias)
#       ICN_DEMO_REMOTE_KEY  (key path ON the jump host; required for the jump route)
#
# Env (all optional; sensible defaults):
#   ICN_DEMO_VM_IP        running node instance IP (required, e.g. 192.0.2.50)
#   ICN_DEMO_SSH_USER     ssh user on the node instance   (default debian)
#   ICN_DEMO_SSH_KEY      demo private key (direct route)
#   ICN_DEMO_JUMP         user@host of a jump host (jump route)
#   ICN_DEMO_REMOTE_KEY   demo key path on the jump host
#   ICN_DEMO_GW_PORT      host port -> gateway   (default 18080)
#   ICN_DEMO_SESSION_PORT host port -> session   (default 18091)
#   ICN_DEMO_NO_BROWSER=1 set up tunnels + print the URL, do not open a browser
# The shell host port is FIXED at 18090: it is the member-shell page Origin,
# which the node's gateway (ICN_CORS_ORIGINS) and demo session endpoint both
# pin. GW/SESSION are tunnel destinations (never an Origin), so they stay
# overridable; the shell port is not.
# ============================================================================
set -uo pipefail

VM_IP="${ICN_DEMO_VM_IP:?set ICN_DEMO_VM_IP to your running node instance IP (e.g. 192.0.2.50)}"
SSH_USER="${ICN_DEMO_SSH_USER:-debian}"
GW_PORT="${ICN_DEMO_GW_PORT:-18080}"
# FIXED, deliberately not overridable. The member-shell page Origin is
# http://localhost:18090; the node's gateway (ICN_CORS_ORIGINS) and the demo
# session endpoint both pin that origin. A custom shell port would pass the
# session seed but fail every authenticated gateway fetch (CORS), so we do not
# expose a shell-port knob. GW_PORT / SESSION_PORT are tunnel destinations, not
# Origins, and remain overridable.
SHELL_PORT=18090
SESSION_PORT="${ICN_DEMO_SESSION_PORT:-18091}"
JUMP="${ICN_DEMO_JUMP:-}"
# Carry the chosen ports to the shell so it talks to the gateway + session
# endpoint on whatever ports this launcher actually forwarded (honoring the
# ICN_DEMO_*_PORT overrides), not hard-coded defaults.
SHELL_URL="http://localhost:${SHELL_PORT}/member-shell/?mode=live&demo=launcher&gw=${GW_PORT}&session=${SESSION_PORT}"
GATEWAY_URL="http://localhost:${GW_PORT}"

log()  { printf '[demo-open] %s\n' "$*"; }
err()  { printf '[demo-open] ERROR: %s\n' "$*" >&2; }

# curl is used for the post-tunnel readiness wait — require it up front.
command -v curl >/dev/null 2>&1 || { err "curl is required but not found on PATH."; exit 2; }

# ---- preflight: required host ports free ----
for p in "$GW_PORT" "$SHELL_PORT" "$SESSION_PORT"; do
  if (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then
    exec 3>&- 3<&- 2>/dev/null || true
    err "host port $p is already in use. Override ICN_DEMO_GW_PORT (gateway) or ICN_DEMO_SESSION_PORT (demo-session) to a free tunnel port; the shell port 18090 is FIXED (the gateway + demo-session CORS allow-lists pin the shell origin), so if 18090 is the conflict, free that port instead of overriding it."
    exit 2
  fi
done

# NOTE: no pre-tunnel reachability probe. The node's services bind 127.0.0.1
# inside the VM, so a probe to ${VM_IP}:8080 from here would (correctly) fail
# on a properly sealed node even when SSH works perfectly. The direct route
# only requires SSH + the demo key, as documented; the jump route requires SSH
# to the jump host. We validate end-to-end reachability AFTER the tunnel is up,
# over the real path (curl localhost:${SHELL_PORT}), which is what actually
# matters. SSH/connectivity failures surface there with an actionable message.

# ---- build the SSH tunnel command for the chosen route ----
FWD=( -L "${GW_PORT}:127.0.0.1:8080" -L "${SHELL_PORT}:127.0.0.1:8090" -L "${SESSION_PORT}:127.0.0.1:8091" )
COMMON=( -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=30 )

if [ -n "$JUMP" ]; then
  # Jump route: run the per-node tunnel ON the jump host (which holds the demo
  # key and can reach the node), and forward those local binds back here.
  REMOTE_KEY="${ICN_DEMO_REMOTE_KEY:?set ICN_DEMO_REMOTE_KEY to the demo key path on the jump host}"
  log "Route: jump through ${JUMP} -> ${SSH_USER}@${VM_IP} (key on jump host)."
  # Build the remote command with shell-escaping so a key path / user / IP
  # containing spaces or metacharacters cannot break out of the inner command.
  # The forward flags are numeric ports (safe); the path/user/host are %q-quoted.
  printf -v INNER 'ssh -N %s -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=accept-new -i %q %q@%q' \
    "${FWD[*]}" "$REMOTE_KEY" "$SSH_USER" "$VM_IP"
  TUNNEL_CMD=( ssh "${COMMON[@]}"
    -L "${GW_PORT}:127.0.0.1:${GW_PORT}"
    -L "${SHELL_PORT}:127.0.0.1:${SHELL_PORT}"
    -L "${SESSION_PORT}:127.0.0.1:${SESSION_PORT}"
    "$JUMP" "$INNER" )
else
  KEY="${ICN_DEMO_SSH_KEY:?set ICN_DEMO_SSH_KEY to the demo private key path}"
  if [ ! -f "$KEY" ]; then
    err "demo SSH key not found at $KEY. Set ICN_DEMO_SSH_KEY, or use ICN_DEMO_JUMP to route through a host that holds it."
    exit 4
  fi
  log "Route: direct ${SSH_USER}@${VM_IP} (key $KEY)."
  TUNNEL_CMD=( ssh -N "${COMMON[@]}" "${FWD[@]}" -i "$KEY" "${SSH_USER}@${VM_IP}" )
fi

# ---- start tunnel in the background, wait for the shell to answer ----
"${TUNNEL_CMD[@]}" &
TUN_PID=$!
cleanup() { log "Closing tunnel (pid $TUN_PID)."; kill "$TUN_PID" 2>/dev/null; }
trap cleanup EXIT INT TERM

log "Establishing tunnel (pid $TUN_PID); waiting for the member-shell ..."
ok=0
for _ in $(seq 1 30); do
  if ! kill -0 "$TUN_PID" 2>/dev/null; then
    err "SSH tunnel exited before it came up. Check: can you 'ssh ${SSH_USER}@${VM_IP}'"
    [ -n "$JUMP" ] && err "  via jump host ${JUMP}, with ICN_DEMO_REMOTE_KEY?" \
                   || err "  with the key in ICN_DEMO_SSH_KEY? (or set ICN_DEMO_JUMP to route through a host that can)."
    exit 5
  fi
  if curl -sf -m 3 "http://localhost:${SHELL_PORT}/member-shell/index.html" >/dev/null 2>&1; then ok=1; break; fi
  sleep 1
done
[ "$ok" = 1 ] || { err "tunnel is up but the member-shell did not answer on localhost:${SHELL_PORT}. Is the node booted and the demo profile running (systemctl status icnd icn-demo-session)?"; exit 6; }
log "Tunnel up. Shell reachable on localhost:${SHELL_PORT}."

# ---- open the browser ----
open_browser() {
  for opener in xdg-open gio open kde-open5 kde-open open; do
    if command -v "$opener" >/dev/null 2>&1; then
      [ "$opener" = "gio" ] && { "$opener" open "$1" >/dev/null 2>&1 && return 0 || continue; }
      "$opener" "$1" >/dev/null 2>&1 && return 0
    fi
  done
  return 1
}

cat <<EOF

  ============================ ICN LOCAL DEMO OPEN ============================
   Member shell : ${SHELL_URL}
   Gateway      : ${GATEWAY_URL}  (pre-filled in the page; do not type it)
   In the browser: click "Start local demo" — standing + an action card load.
   No JWT to copy. No gateway to type. No terminal needed after this.
   Stop the demo: press Ctrl-C in this terminal to close the tunnel.
  ============================================================================

EOF

if [ "${ICN_DEMO_NO_BROWSER:-0}" = "1" ]; then
  log "ICN_DEMO_NO_BROWSER=1 — not opening a browser. Open the URL above yourself."
else
  if open_browser "$SHELL_URL"; then
    log "Opened your browser to the member-shell."
  else
    log "Could not auto-open a browser. Open this URL manually: ${SHELL_URL}"
  fi
fi

log "Tunnel is running. Leave this terminal open during the demo; Ctrl-C to stop."
wait "$TUN_PID"
