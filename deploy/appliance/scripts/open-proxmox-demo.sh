#!/usr/bin/env bash
# ============================================================================
# open-proxmox-demo.sh — one command to open the ICN member-shell DEV/DEMO.
# ----------------------------------------------------------------------------
# DEV/DEMO ONLY. Run this from your workstation (e.g. Zenith). It:
#   1. confirms the demo node instance is reachable,
#   2. opens SSH tunnels so the browser uses the sealed demo origins:
#        localhost:18080 -> node instance gateway      127.0.0.1:8080
#        localhost:18090 -> node instance member-shell 127.0.0.1:8090
#        localhost:18091 -> node instance demo-session  127.0.0.1:8091
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
#   ICN_DEMO_SHELL_PORT   host port -> shell     (default 18090)
#   ICN_DEMO_SESSION_PORT host port -> session   (default 18091)
#   ICN_DEMO_NO_BROWSER=1 set up tunnels + print the URL, do not open a browser
# ============================================================================
set -uo pipefail

VM_IP="${ICN_DEMO_VM_IP:?set ICN_DEMO_VM_IP to your running node instance IP (e.g. 192.0.2.50)}"
SSH_USER="${ICN_DEMO_SSH_USER:-debian}"
GW_PORT="${ICN_DEMO_GW_PORT:-18080}"
SHELL_PORT="${ICN_DEMO_SHELL_PORT:-18090}"
SESSION_PORT="${ICN_DEMO_SESSION_PORT:-18091}"
JUMP="${ICN_DEMO_JUMP:-}"
SHELL_URL="http://localhost:${SHELL_PORT}/member-shell/?mode=live&demo=launcher"
GATEWAY_URL="http://localhost:${GW_PORT}"

log()  { printf '[demo-open] %s\n' "$*"; }
err()  { printf '[demo-open] ERROR: %s\n' "$*" >&2; }

# ---- preflight: required host ports free ----
for p in "$GW_PORT" "$SHELL_PORT" "$SESSION_PORT"; do
  if (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then
    exec 3>&- 3<&- 2>/dev/null || true
    err "host port $p is already in use. Set ICN_DEMO_{GW,SHELL,SESSION}_PORT to free ports, or stop the process holding it."
    exit 2
  fi
done

# ---- reachability ----
log "Checking the running node instance gateway at ${VM_IP}:8080 ..."
if command -v curl >/dev/null 2>&1; then
  if ! curl -sf -m 6 "http://${VM_IP}:8080/v1/health" >/dev/null 2>&1; then
    err "node instance gateway not reachable at http://${VM_IP}:8080/v1/health from here."
    err "If you are off the node's network, run this from a host that can reach it, or use ICN_DEMO_JUMP."
    [ -z "$JUMP" ] && exit 3
    log "Continuing via jump host ${JUMP} (it will reach the node)."
  else
    log "Gateway healthy."
  fi
fi

# ---- build the SSH tunnel command for the chosen route ----
FWD=( -L "${GW_PORT}:127.0.0.1:8080" -L "${SHELL_PORT}:127.0.0.1:8090" -L "${SESSION_PORT}:127.0.0.1:8091" )
COMMON=( -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=30 )

if [ -n "$JUMP" ]; then
  # Jump route: run the per-node tunnel ON the jump host (which holds the demo
  # key and can reach the node), and forward those local binds back here.
  REMOTE_KEY="${ICN_DEMO_REMOTE_KEY:?set ICN_DEMO_REMOTE_KEY to the demo key path on the jump host}"
  log "Route: jump through ${JUMP} -> ${SSH_USER}@${VM_IP} (key on jump host)."
  INNER="ssh -N ${FWD[*]} -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=accept-new -i ${REMOTE_KEY} ${SSH_USER}@${VM_IP}"
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
  if ! kill -0 "$TUN_PID" 2>/dev/null; then err "tunnel exited early."; exit 5; fi
  if curl -sf -m 3 "http://localhost:${SHELL_PORT}/member-shell/index.html" >/dev/null 2>&1; then ok=1; break; fi
  sleep 1
done
[ "$ok" = 1 ] || { err "member-shell did not answer on localhost:${SHELL_PORT}."; exit 6; }
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
