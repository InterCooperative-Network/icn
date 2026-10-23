#!/usr/bin/env bash
# install-icn-disk-guard.sh — install the ICN dev-host disk guard and its systemd timer.
#
# WHAT THIS PROTECTS AGAINST
#   Every agent worktree under ~/icn-dev/worktrees/<repo>/<name> builds into its own Cargo
#   `target/`. Each is 3-26 GB, and a full workspace build across feature sets can exceed
#   100 GB. Nothing reclaimed them, so on 2026-08-23 the icn-dev root filesystem reached 96%
#   with 372 GB of build output across 19 worktrees. The guard bounds that growth; this
#   script is what makes the protection survive a rebuild of the host.
#
# WHY IT LIVES HERE, AND THE GAP IT DOCUMENTS
#   This repository has no dev-host provisioning subsystem. `deploy/appliance/systemd/` is the
#   shipped *product* appliance, not the development VM, and there is no ansible/, provision/
#   or hosts/ tree. Rather than invent one for a single guard, these files sit in the narrowest
#   existing location that already matches the convention: `ops/scripts/`, alongside
#   `setup-skill-symlinks.sh`, which is likewise tracked-in-repo tooling that configures
#   machine-local state on a developer host.
#
#   THE GAP IS REAL AND WORTH NAMING: icn-dev host configuration is otherwise unmanaged. If a
#   dev-host provisioning home is ever established, these four files should move there
#   wholesale. Until then this script is the only reproducible path, and it must be run by
#   hand after reprovisioning.
#
# IDEMPOTENT
#   Safe to re-run. It installs or updates in place, reloads systemd, and re-enables the timer.
#   Re-running after no change is a no-op apart from the reload.
#
# USAGE
#   ops/scripts/install-icn-disk-guard.sh              install/update (needs sudo for units)
#   ops/scripts/install-icn-disk-guard.sh --uninstall   remove timer/service, keep the script
#   ops/scripts/install-icn-disk-guard.sh --check       report installed state, change nothing
#
set -euo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEV_USER="${ICN_DEV_USER:-ubuntu}"
DEV_HOME="$(getent passwd "$DEV_USER" | cut -d: -f6)"
: "${DEV_HOME:?cannot resolve home directory for $DEV_USER}"
SCRIPT_DST="$DEV_HOME/icn-dev/scripts/icn-disk-guard"
BIN_LINK="$DEV_HOME/bin/icn-disk-guard"
UNIT_DIR=/etc/systemd/system

case "${1:-install}" in
  --check)
    echo "script : $([ -x "$SCRIPT_DST" ] && echo "installed ($SCRIPT_DST)" || echo MISSING)"
    echo "symlink: $([ -L "$BIN_LINK" ] && echo "installed -> $(readlink "$BIN_LINK")" || echo MISSING)"
    for u in icn-disk-guard.service icn-disk-guard.timer; do
      echo "$u: $([ -f "$UNIT_DIR/$u" ] && echo installed || echo MISSING)"
    done
    echo "timer  : $(systemctl is-enabled icn-disk-guard.timer 2>/dev/null || echo not-enabled) / $(systemctl is-active icn-disk-guard.timer 2>/dev/null || echo inactive)"
    systemctl list-timers icn-disk-guard.timer --all --no-pager 2>/dev/null | sed -n 2p
    exit 0 ;;
  --uninstall)
    sudo systemctl disable --now icn-disk-guard.timer 2>/dev/null || true
    sudo rm -f "$UNIT_DIR/icn-disk-guard.service" "$UNIT_DIR/icn-disk-guard.timer"
    sudo systemctl daemon-reload
    echo "removed timer and service; left $SCRIPT_DST in place"
    exit 0 ;;
  install) ;;
  *) echo "unknown argument: $1" >&2; exit 2 ;;
esac

echo "installing icn-disk-guard for user '$DEV_USER' (home $DEV_HOME)"

install -d -m 0755 -o "$DEV_USER" -g "$DEV_USER" "$DEV_HOME/icn-dev/scripts" "$DEV_HOME/bin" \
  2>/dev/null || mkdir -p "$DEV_HOME/icn-dev/scripts" "$DEV_HOME/bin"
install -m 0755 "$SRC_DIR/icn-disk-guard" "$SCRIPT_DST"
ln -sfn "$SCRIPT_DST" "$BIN_LINK"
echo "  script  -> $SCRIPT_DST"
echo "  symlink -> $BIN_LINK"

# The guard must pass its own tests before it is allowed to be scheduled. Installing a guard
# whose policy logic is broken is worse than installing none, because the timer makes it look
# supervised.
if ! "$SCRIPT_DST" --self-test >/dev/null 2>&1; then
  echo "ERROR: icn-disk-guard --self-test failed; refusing to schedule it." >&2
  "$SCRIPT_DST" --self-test || true
  exit 1
fi
echo "  self-test: passed"

# System-level units, not user units. A user timer only runs while a login session exists
# (this host has Linger=no), so it would silently stop after an unattended reboot. Enabling
# lingering was the alternative, but on this host that would also start dotfiles-sync.service
# — a `git pull` — unattended at boot, which is outside this guard's remit. One system unit
# running as the dev user gives boot-persistent housekeeping with the smallest blast radius.
# The units are checked in with icn-dev's own user baked in, because systemd does not
# interpolate variables in User=/ExecStart=. Substituting here keeps them consistent with
# ICN_DEV_USER instead of silently installing a unit that runs as the wrong account.
tmp_unit="$(mktemp)"; trap 'rm -f "$tmp_unit"' EXIT
sed -e "s|^User=.*|User=$DEV_USER|" \
    -e "s|^Group=.*|Group=$DEV_USER|" \
    -e "s|^Environment=HOME=.*|Environment=HOME=$DEV_HOME|" \
    -e "s|^Environment=PATH=.*|Environment=PATH=/usr/local/bin:/usr/bin:/bin:$DEV_HOME/bin|" \
    -e "s|^WorkingDirectory=.*|WorkingDirectory=$DEV_HOME|" \
    -e "s|^ExecStart=.*|ExecStart=$SCRIPT_DST --auto --quiet|" \
    -e "s|^ReadWritePaths=.*|ReadWritePaths=$DEV_HOME/icn-dev|" \
    "$SRC_DIR/icn-disk-guard.service" > "$tmp_unit"
sudo install -m 0644 "$tmp_unit" "$UNIT_DIR/icn-disk-guard.service"
sudo install -m 0644 "$SRC_DIR/icn-disk-guard.timer" "$UNIT_DIR/icn-disk-guard.timer"
echo "  units   -> $UNIT_DIR/icn-disk-guard.{service,timer} (User=$DEV_USER)"

sudo systemctl daemon-reload
sudo systemctl enable --now icn-disk-guard.timer
echo "  timer   -> enabled"
echo
systemctl list-timers icn-disk-guard.timer --all --no-pager | sed -n '1,2p'
echo
echo "Audit now with:  icn-disk-guard            (dry-run, never deletes)"
echo "Policy lives in the CONFIG block at the top of $SCRIPT_DST"
