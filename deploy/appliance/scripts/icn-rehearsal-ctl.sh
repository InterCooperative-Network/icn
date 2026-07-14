#!/usr/bin/env bash
# ============================================================================
# icn-rehearsal-ctl — operator control for a deployed LAN Rehearsal Node VM.
# ----------------------------------------------------------------------------
# Runs FROM an operator machine, over SSH, against the deployed appliance VM
# (LAN profile). Matt-facing: one command instead of remembering systemd /
# QEMU / crate specifics. The normal browser flow needs none of this.
#
# Configuration (environment, or ~/.config/icn-rehearsal/ctl.env):
#   REHEARSAL_SSH     ssh target for the VM, e.g. icnops@rehearsal-vm  (required)
#   REHEARSAL_ORIGIN  browser origin, e.g. https://rehearsal.example.internal
#                     (used by `status` for an end-to-end probe; optional)
#
# Subcommands:
#   status    health summary: services, gateway health, sanitized workspace
#             state (via icn-demo-status), origin probe, last deploy commit
#   start|stop|restart   the rehearsal services (icnd, member-shell, session, nginx)
#   logs [unit]          recent journal (default: icnd)
#   reset     FULL appliance state reset (destructive; asks for confirmation;
#             wraps `sudo icn-demo-reset`). The browser "Start a new rehearsal"
#             is the normal, non-destructive per-run reset.
#   verify    run `sudo icn-demo-verify --rehearsal` on the VM (steward check)
#   update    print the supported update procedure (image rebuild + reimport);
#             deliberately NOT automated from here — see
#             docs/testing/LAN_REHEARSAL_DEPLOYMENT.md
# ============================================================================
set -euo pipefail

CONF="${XDG_CONFIG_HOME:-$HOME/.config}/icn-rehearsal/ctl.env"
# shellcheck disable=SC1090
[ -f "$CONF" ] && . "$CONF"

[ -n "${REHEARSAL_SSH:-}" ] || { echo "REHEARSAL_SSH not set (e.g. icnops@rehearsal-vm); set it in the environment or $CONF" >&2; exit 2; }

UNITS="icnd icn-member-shell icn-demo-session nginx"
run(){ ssh -o BatchMode=yes -o ConnectTimeout=6 "$REHEARSAL_SSH" "$@"; }

cmd="${1:-status}"
case "$cmd" in
  status)
    echo "== services =="
    run "for u in $UNITS; do printf '%-18s %s\n' \"\$u:\" \"\$(systemctl is-active \$u 2>/dev/null)\"; done" || { echo "VM unreachable over SSH ($REHEARSAL_SSH)"; exit 1; }
    echo "== rehearsal (sanitized) =="
    run "sudo icn-demo-status" || echo "icn-demo-status failed"
    echo "== deploy provenance =="
    run "cat /usr/share/icn/static/web/build-info.json 2>/dev/null; cat /etc/icn/lan-profile.env 2>/dev/null" || true
    if [ -n "${REHEARSAL_ORIGIN:-}" ]; then
      echo "== origin probe =="
      code="$(curl -s -o /dev/null -m 6 -w '%{http_code}' "$REHEARSAL_ORIGIN/v1/health" || echo 000)"
      echo "$REHEARSAL_ORIGIN/v1/health => HTTP $code"
    fi
    ;;
  start|stop|restart)
    run "sudo systemctl $cmd $UNITS"
    echo "$cmd: done"
    ;;
  logs)
    unit="${2:-icnd}"
    run "sudo journalctl -u $unit -n 100 --no-pager"
    ;;
  reset)
    echo "FULL appliance reset: wipes /var/lib/icn (receipts, identity) and re-runs firstboot."
    echo "The browser 'Start a new rehearsal' is the normal per-run reset — use this only to rebuild local state."
    printf 'Type RESET to proceed: '
    read -r answer
    [ "$answer" = "RESET" ] || { echo "aborted"; exit 1; }
    run "sudo icn-demo-reset"
    ;;
  verify)
    run "sudo icn-demo-verify --rehearsal"
    ;;
  update)
    cat <<'EOF'
Update = rebuild the image from the new commit and re-import the disk:
  1. On the build host: deploy/appliance/build-image.sh --real with the demo +
     LAN profile env (see docs/testing/LAN_REHEARSAL_DEPLOYMENT.md).
  2. Copy the new qcow2 to the hypervisor, stop the VM, re-import the disk,
     boot, and re-run the workstation smoke.
Note: replacing the disk starts a FRESH node (new identity, empty receipts) —
that is the supported, honest update for a non-production rehearsal appliance.
EOF
    ;;
  *)
    echo "usage: icn-rehearsal-ctl {status|start|stop|restart|logs [unit]|reset|verify|update}" >&2
    exit 2
    ;;
esac
