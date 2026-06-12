#!/usr/bin/env bash
# ============================================================================
# icn-demo-reset — wipe this demo VM's node state and reseed from scratch.
# ----------------------------------------------------------------------------
# DEMO PROFILE ONLY. Installed by build-image.sh when
# ICN_APPLIANCE_DEMO_PROFILE=1. Run as root inside the appliance VM:
#
#     sudo icn-demo-reset
#
# DESTRUCTIVE — by design. This removes the VM's node state (identity,
# keystore, journal, receipts, config) and per-instance secrets, then
# re-runs firstboot so the VM provisions itself fresh, exactly like a
# first boot. Old receipts/tokens stop being valid. That is the point:
# a clean, rerunnable demo.
#
# Cheaper alternative when running from a QEMU overlay (the smoke script
# pattern): just discard the overlay file and boot again — that resets the
# whole disk, not only ICN state, and needs no in-VM action.
# ============================================================================
set -euo pipefail

log(){ echo "[demo-reset] $*"; }
[ "$(id -u)" -eq 0 ] || { echo "[demo-reset] run as root: sudo icn-demo-reset" >&2; exit 1; }

log "stopping icnd (and member shell)..."
systemctl stop icnd.service 2>/dev/null || true
systemctl stop icn-member-shell.service 2>/dev/null || true

log "wiping node state in /var/lib/icn and per-instance secrets..."
rm -rf /var/lib/icn/* /var/lib/icn/.firstboot-complete 2>/dev/null || true
rm -f /etc/icn/icnd.env

log "re-running firstboot provisioning..."
systemctl restart icn-appliance-firstboot.service
if ! systemctl is-active --quiet icn-appliance-firstboot.service \
   && ! [ -f /var/lib/icn/.firstboot-complete ]; then
  echo "[demo-reset] firstboot did not complete; inspect: journalctl -u icn-appliance-firstboot" >&2
  exit 1
fi

log "starting icnd + member shell..."
systemctl start icnd.service
systemctl start icn-member-shell.service 2>/dev/null || true

log "done. Reseed the demo with: sudo icn-demo-seed"
