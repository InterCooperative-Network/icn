#!/usr/bin/env bash
# ============================================================================
# icn-rehearsal-node.sh — ICN Rehearsal Node v0.1: one named operator
# entrypoint for the existing appliance DEV/DEMO paths.
# ----------------------------------------------------------------------------
# DEV/DEMO ONLY. Not production, not a pilot, not federation.
#
# This wrapper does not implement a new node path. It delegates to the
# appliance DEV/DEMO substrate that already exists:
#   - smoke/smoke-local.sh --real --demo   (headless proof on a disposable VM)
#   - scripts/open-proxmox-demo.sh         (browser shell for a running node)
#   - in-VM helpers: icn-demo-seed / icn-demo-verify / icn-demo-reset
#
# Usage:
#   icn-rehearsal-node.sh --help
#   icn-rehearsal-node.sh --dry-run
#   icn-rehearsal-node.sh smoke-image          # needs an already-built
#                                              # DEMO-profile image
#   icn-rehearsal-node.sh open-running-node    # needs an already-running
#                                              # DEMO-profile node instance
#   icn-rehearsal-node.sh verify-running-node  # prints verification steps
#
# Env (smoke-image — same contract as smoke-local.sh):
#   ICN_APPLIANCE_IMAGE            required  path to DEMO-profile QCOW2
#   ICN_APPLIANCE_SSH_KEY          required  smoke-only SSH private key
#   ICN_APPLIANCE_CLOUD_INIT_SEED  seed ISO. May be left unset ONLY after
#                                  editing smoke/cloud-init/user-data.example.yaml
#                                  with your smoke-only PUBLIC key (smoke-local
#                                  then builds a seed via cloud-localds; it
#                                  refuses the shipped placeholder key and does
#                                  NOT derive the key from ICN_APPLIANCE_SSH_KEY)
#   ICN_APPLIANCE_ALLOW_OUTBOUND   optional. The demo smoke ISOLATES the guest
#                                  network by default (QEMU user-net
#                                  restrict=on; loopback hostfwd unaffected)
#                                  and proves it with an in-guest canary
#                                  probe. Set 1 to permit guest outbound
#                                  (skips the canary; loud in output).
#
# Env (open-running-node / verify-running-node — same contract as
# open-proxmox-demo.sh):
#   ICN_DEMO_VM_IP        required            running node instance IP
#   ICN_DEMO_SSH_KEY      direct route        SSH key on this workstation
#   ICN_DEMO_JUMP         jump route          user@jump-host
#   ICN_DEMO_REMOTE_KEY   jump route          key path on the jump host
#   ICN_DEMO_SSH_USER     optional            VM ssh user (default: debian)
#   Other launcher knobs (ICN_DEMO_GW_PORT, ICN_DEMO_SESSION_PORT,
#   ICN_DEMO_NO_BROWSER, ...) pass through to open-proxmox-demo.sh unchanged.
#
# Network posture (be precise about which route guarantees what):
#   smoke-image        guest outbound is BLOCKED BY DEFAULT and canary-proven
#                      (see ICN_APPLIANCE_ALLOW_OUTBOUND above).
#   open-running-node  network isolation is OPERATOR-PROVIDED. This launcher
#                      only opens SSH tunnels; it does not seal the VM. A
#                      bounded preflight WARNS if the node's gateway port is
#                      directly reachable from this workstation.
#
# What this wrapper does NOT do:
#   - does not build or download images (see build-image.sh)
#   - does not create the cloud-init seed by itself
#   - does not add a runtime mode or alter icnd flags
#   - does not store or print credentials
#   - does not bypass the demo-session loopback/dev-gate boundary
#
# Honest non-claims: single-node, fictional fixture institution, dev gates,
# test posture — NOT production, NOT a pilot, NOT federation.
#
# Docs: deploy/appliance/DEMO_QUICKSTART.md
#       docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md
# ============================================================================
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APPLIANCE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

log()   { printf '[rehearsal-node] %s\n' "$*"; }
warn()  { printf '[rehearsal-node] WARN: %s\n' "$*" >&2; }
err()   { printf '[rehearsal-node] ERROR: %s\n' "$*" >&2; }
fatal() { err "$*"; exit 2; }

usage() { awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$0"; }

DOCS_HINT="See deploy/appliance/DEMO_QUICKSTART.md and docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md."

banner() {
  log "ICN Rehearsal Node v0.1 — wrapper over the appliance DEV/DEMO profile."
  log "Not production, not a pilot, not federation. Fictional fixture data only."
}

require_smoke_env() {
  local missing=0
  if [ -z "${ICN_APPLIANCE_IMAGE:-}" ]; then
    err "ICN_APPLIANCE_IMAGE is not set (path to an already-built DEMO-profile QCOW2)."
    missing=1
  fi
  if [ -z "${ICN_APPLIANCE_SSH_KEY:-}" ]; then
    err "ICN_APPLIANCE_SSH_KEY is not set (smoke-only SSH private key)."
    missing=1
  fi
  if [ "$missing" -ne 0 ]; then
    err "smoke-image needs an image built with ICN_APPLIANCE_DEMO_PROFILE=1."
    fatal "$DOCS_HINT"
  fi
  # Fail fast on the seed precondition instead of mid-run: without a pre-built
  # seed ISO, smoke-local builds one from smoke/cloud-init/*.example.yaml and
  # refuses the shipped placeholder key (it never derives the public key from
  # ICN_APPLIANCE_SSH_KEY).
  local ex_user="${APPLIANCE_DIR}/smoke/cloud-init/user-data.example.yaml"
  if [ -z "${ICN_APPLIANCE_CLOUD_INIT_SEED:-}" ] && [ -f "$ex_user" ] \
     && grep -q "INVALIDREPLACEME" "$ex_user"; then
    err "no ICN_APPLIANCE_CLOUD_INIT_SEED set, and ${ex_user}"
    err "still contains the placeholder SSH key — smoke-local will refuse it."
    err "Either point ICN_APPLIANCE_CLOUD_INIT_SEED at a pre-built seed ISO, or"
    err "edit that file to carry the smoke-only PUBLIC key matching ICN_APPLIANCE_SSH_KEY."
    fatal "$DOCS_HINT"
  fi
}

require_open_env() {
  if [ -z "${ICN_DEMO_VM_IP:-}" ]; then
    err "ICN_DEMO_VM_IP is not set (IP of an already-running DEMO-profile node instance)."
    fatal "$DOCS_HINT"
  fi
  if [ -z "${ICN_DEMO_SSH_KEY:-}" ] && { [ -z "${ICN_DEMO_JUMP:-}" ] || [ -z "${ICN_DEMO_REMOTE_KEY:-}" ]; }; then
    err "no usable SSH route: set ICN_DEMO_SSH_KEY (direct route),"
    err "or BOTH ICN_DEMO_JUMP and ICN_DEMO_REMOTE_KEY (jump route)."
    fatal "$DOCS_HINT"
  fi
}

print_verify_steps() {
  banner
  log "Verification runs INSIDE the running node instance (VM):"
  log "  sudo icn-demo-verify <item-id>   # re-fetch + consistency check of one completion receipt"
  log "  sudo icn-demo-verify --chain     # deeper 13-of-13 governed receipt-chain rehearsal"
  log "Neither command re-derives hashes client-side; the hash binding is created"
  log "server-side when the receipt is emitted. Chain evidence lands under"
  log "/var/lib/icn-demo/receipt-chain-13of13/ in the VM."
  if [ -n "${ICN_DEMO_VM_IP:-}" ]; then
    local ssh_user="${ICN_DEMO_SSH_USER:-debian}"
    if [ -n "${ICN_DEMO_SSH_KEY:-}" ]; then
      log "Ready-to-copy (direct route):"
      log "  ssh -i \"${ICN_DEMO_SSH_KEY}\" ${ssh_user}@${ICN_DEMO_VM_IP} 'sudo icn-demo-verify --chain'"
    fi
    if [ -n "${ICN_DEMO_JUMP:-}" ] && [ -n "${ICN_DEMO_REMOTE_KEY:-}" ]; then
      log "Ready-to-copy (jump route — the demo key lives on the jump host, so"
      log "the inner ssh runs THERE, mirroring open-proxmox-demo.sh):"
      log "  ssh -o StrictHostKeyChecking=accept-new \"${ICN_DEMO_JUMP}\" \\"
      log "    \"ssh -o StrictHostKeyChecking=accept-new -i '${ICN_DEMO_REMOTE_KEY}' ${ssh_user}@${ICN_DEMO_VM_IP} 'sudo icn-demo-verify --chain'\""
    fi
  else
    log "Set ICN_DEMO_VM_IP (plus a route, as for open-running-node) to get a"
    log "ready-to-copy ssh command printed here."
  fi
  log "This command prints instructions only; it does not run anything remotely."
}

dry_run() {
  banner
  log "--dry-run: nothing is executed, no VM is booted, no network is used."
  log ""
  log "smoke-image would run:"
  log "  bash ${APPLIANCE_DIR}/smoke/smoke-local.sh --real --demo"
  log "  (boots a disposable local QEMU overlay from ICN_APPLIANCE_IMAGE and"
  log "   drives the loop headlessly: health -> member shell -> seed ->"
  log "   standing -> action card -> complete -> receipt)"
  log "  requires: ICN_APPLIANCE_IMAGE, ICN_APPLIANCE_SSH_KEY"
  log "  seed:     ICN_APPLIANCE_CLOUD_INIT_SEED, or an edited"
  log "            smoke/cloud-init/user-data.example.yaml carrying your real"
  log "            smoke public key (the shipped placeholder is refused)"
  log "  network:  guest outbound is BLOCKED by default (QEMU user-net"
  log "            restrict=on; loopback hostfwd unaffected) and proven by an"
  log "            in-guest canary probe; ICN_APPLIANCE_ALLOW_OUTBOUND=1"
  log "            permits outbound and skips the canary"
  log ""
  log "open-running-node would run:"
  log "  bash ${SCRIPT_DIR}/open-proxmox-demo.sh"
  log "  (SSH-tunnels gateway/shell/session ports 18080/18090/18091 to an"
  log "   already-running DEMO-profile node instance and opens the member"
  log "   shell with the no-paste 'Start local demo' launcher)"
  log "  requires: ICN_DEMO_VM_IP + ICN_DEMO_SSH_KEY (direct)"
  log "        or: ICN_DEMO_VM_IP + ICN_DEMO_JUMP + ICN_DEMO_REMOTE_KEY (jump)"
  log "  network:  isolation is OPERATOR-PROVIDED (tunnels do not seal the"
  log "            VM); a bounded preflight WARNS if the node's gateway port"
  log "            is directly reachable from this workstation"
  log ""
  log "verify-running-node would print the in-VM verification commands"
  log "(sudo icn-demo-verify <item-id> | --chain); it never executes remotely."
}

cmd="${1:---help}"
case "$cmd" in
  -h|--help|help)
    usage
    ;;
  --dry-run)
    dry_run
    ;;
  smoke-image)
    banner
    require_smoke_env
    log "delegating to smoke-local.sh --real --demo ..."
    exec bash "${APPLIANCE_DIR}/smoke/smoke-local.sh" --real --demo
    ;;
  open-running-node)
    banner
    require_open_env
    log "network isolation for an already-running node is OPERATOR-PROVIDED:"
    log "this launcher only opens SSH tunnels; it does not seal the VM's network."
    # Bounded exposure preflight (warn-only). The demo profile binds services
    # beyond loopback inside the VM; if the gateway port answers this
    # workstation directly, the node is reachable from this network segment.
    # The inverse is NOT proof of isolation — it only means no direct path
    # from here.
    if timeout 2 bash -c "exec 3<>/dev/tcp/${ICN_DEMO_VM_IP}/8080" 2>/dev/null; then
      warn "exposure preflight: gateway port 8080 on ${ICN_DEMO_VM_IP} is DIRECTLY reachable from this workstation."
      warn "Run this node only on a trusted/isolated network or behind an operator-provided firewall. Continuing (warning only)."
    else
      log "exposure preflight: no direct gateway reachability from here (NOT proof of isolation)."
    fi
    log "delegating to open-proxmox-demo.sh ..."
    exec bash "${SCRIPT_DIR}/open-proxmox-demo.sh"
    ;;
  verify-running-node)
    print_verify_steps
    ;;
  *)
    err "unknown command: ${cmd}"
    err "commands: smoke-image | open-running-node | verify-running-node | --dry-run | --help"
    fatal "$DOCS_HINT"
    ;;
esac
