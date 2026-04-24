#!/usr/bin/env bash
#
# Mount the shared Atlas-backed sccache on a GitHub Actions self-hosted runner.
#
# Converts the runner from a local 10G cache (~/.cache/sccache) to the shared
# NFS cache at /mnt/icn-sccache (Atlas 10.8.10.25, ~892G pool), matching
# icn-dev (10.8.30.45) which already uses this cache.
#
# Context: issue #1597. The NFS export is:
#     10.8.10.25:/mnt/ssd_pool/icn-vols/sccache-cache
#
# Apply on the ci-runner host (10.8.30.46) during a quiet window when no
# GitHub Actions jobs are running. Check with:
#
#     ps aux | grep -E 'Runner.Worker|cargo|rustc' | grep -v grep
#
# The final `systemctl restart` interrupts in-progress jobs.

set -euo pipefail

ATLAS_EXPORT="${ATLAS_EXPORT:-10.8.10.25:/mnt/ssd_pool/icn-vols/sccache-cache}"
MOUNT_POINT="${MOUNT_POINT:-/mnt/icn-sccache}"
MOUNT_OPTS="${MOUNT_OPTS:-nfs4 rw,hard,noatime,nofail,_netdev,vers=4.2}"
CACHE_SIZE="${CACHE_SIZE:-100G}"
RUNNER_USER="${RUNNER_USER:-ubuntu}"
RUNNER_HOME="/home/${RUNNER_USER}"
RUNNER_ENV="${RUNNER_HOME}/actions-runner/.env"
RUNNER_BASHRC="${RUNNER_HOME}/.bashrc"
RUNNER_SERVICE="${RUNNER_SERVICE:-actions.runner.InterCooperative-Network-icn.ci-runner.service}"

log() { printf '[atlas-sccache-setup] %s\n' "$*"; }
fail() { printf '[atlas-sccache-setup] ERROR: %s\n' "$*" >&2; exit 1; }

require_root() {
  [[ $EUID -eq 0 ]] || fail "run as root (sudo)"
}

snapshot() {
  local ts
  ts="$(date +%Y%m%d-%H%M%S)"
  local dir="/tmp/ci-runner-sccache-snapshot-${ts}"
  mkdir -p "$dir"
  cp /etc/fstab "$dir/fstab.before"
  mount > "$dir/mount.before"
  df -hT > "$dir/df.before"
  findmnt -a > "$dir/findmnt.before" || true
  [[ -f "$RUNNER_ENV" ]] && cp "$RUNNER_ENV" "$dir/actions-runner.env.before" || true
  [[ -f "$RUNNER_BASHRC" ]] && cp "$RUNNER_BASHRC" "$dir/bashrc.before" || true
  log "snapshot at $dir"
  echo "$dir"
}

install_nfs_client() {
  if ! dpkg -s nfs-common >/dev/null 2>&1; then
    log "installing nfs-common"
    apt-get update -q
    apt-get install -y nfs-common
  else
    log "nfs-common already installed"
  fi
}

ensure_mountpoint() {
  install -d -o "$RUNNER_USER" -g "$RUNNER_USER" -m 0755 "$MOUNT_POINT"
}

add_fstab() {
  local entry="${ATLAS_EXPORT} ${MOUNT_POINT} ${MOUNT_OPTS} 0 0"
  if grep -qsE "^[^#]*${MOUNT_POINT}[[:space:]]" /etc/fstab; then
    log "fstab entry for ${MOUNT_POINT} already present, leaving alone"
  else
    log "appending fstab entry"
    printf '\n# ICN shared sccache (issue #1597)\n%s\n' "$entry" >> /etc/fstab
  fi
}

mount_cache() {
  if mountpoint -q "$MOUNT_POINT"; then
    log "${MOUNT_POINT} already mounted"
    return
  fi
  log "mounting ${ATLAS_EXPORT} → ${MOUNT_POINT}"
  mount "$MOUNT_POINT"
}

verify_write() {
  local probe="${MOUNT_POINT}/.ci-runner-write-probe"
  sudo -u "$RUNNER_USER" touch "$probe" || fail "runner user cannot write to ${MOUNT_POINT}"
  sudo -u "$RUNNER_USER" rm -f "$probe"
  log "verified runner user can write to ${MOUNT_POINT}"
}

update_env() {
  local tmp
  tmp="$(mktemp)"
  if [[ -f "$RUNNER_ENV" ]]; then
    grep -v -E '^(RUSTC_WRAPPER|SCCACHE_DIR|SCCACHE_CACHE_SIZE)=' "$RUNNER_ENV" > "$tmp" || true
  fi
  {
    echo "RUSTC_WRAPPER=sccache"
    echo "SCCACHE_DIR=${MOUNT_POINT}"
    echo "SCCACHE_CACHE_SIZE=${CACHE_SIZE}"
  } >> "$tmp"
  install -o "$RUNNER_USER" -g "$RUNNER_USER" -m 0644 "$tmp" "$RUNNER_ENV"
  rm -f "$tmp"
  log "wrote ${RUNNER_ENV}"
}

update_bashrc() {
  local tag='# === sccache: shared compile cache on Atlas via NFS (issue #1597) ==='
  if grep -qsF "$tag" "$RUNNER_BASHRC"; then
    log "bashrc already tagged, leaving alone"
    return
  fi
  cat >> "$RUNNER_BASHRC" <<BRC

${tag}
if [ -d ${MOUNT_POINT} ] && [ -w ${MOUNT_POINT} ]; then
  export RUSTC_WRAPPER=sccache
  export SCCACHE_DIR=${MOUNT_POINT}
  export SCCACHE_CACHE_SIZE=${CACHE_SIZE}
fi
BRC
  chown "$RUNNER_USER:$RUNNER_USER" "$RUNNER_BASHRC"
  log "appended sccache block to ${RUNNER_BASHRC}"
}

restart_runner() {
  if [[ "${SKIP_RESTART:-0}" == "1" ]]; then
    log "SKIP_RESTART=1 set — not restarting runner; apply new env by running:"
    log "    sudo systemctl restart ${RUNNER_SERVICE}"
    return
  fi
  if pgrep -f 'Runner.Worker' >/dev/null 2>&1; then
    fail "Runner.Worker is active — refusing to restart. Re-run later or export SKIP_RESTART=1."
  fi
  log "restarting ${RUNNER_SERVICE}"
  systemctl restart "$RUNNER_SERVICE"
  systemctl --no-pager --lines=0 status "$RUNNER_SERVICE" || true
}

main() {
  require_root
  snapshot >/dev/null
  install_nfs_client
  ensure_mountpoint
  add_fstab
  mount_cache
  verify_write
  update_env
  update_bashrc
  restart_runner
  log "done. df -hT ${MOUNT_POINT}:"
  df -hT "$MOUNT_POINT"
}

main "$@"
