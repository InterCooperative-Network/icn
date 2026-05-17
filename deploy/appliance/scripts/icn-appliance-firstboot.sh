#!/usr/bin/env bash
# icn-appliance-firstboot.sh
#
# ICN appliance first-boot scaffold.
#
# What this does (today):
#   - Creates /etc/icn, /var/lib/icn, /var/log/icn if missing.
#   - Creates or validates the `icn` system user (if run as root).
#   - Writes a minimal /etc/icn/appliance.env ONLY if it is absent.
#   - Optionally installs the icnd.service unit if it can be found
#     and ICN_FIRSTBOOT_INSTALL_UNIT=1 is set.
#   - Drops a marker file so subsequent runs no-op.
#   - Prints next steps for an operator.
#
# What this does NOT do:
#   - Generate or store any secret. No JWT, no keystore passphrase.
#   - Initialize an `icnd` identity. The operator runs `icnctl id init`
#     deliberately, mirroring deploy/install.sh.
#   - Start or enable icnd.service. The operator does that explicitly.
#   - Replace existing config. Overwrite requires
#     ICN_FIRSTBOOT_FORCE_OVERWRITE=1.
#   - Talk to a real federation. No network calls.
#
# Run modes:
#   --dry-run     Print what would happen; touch nothing.
#   --help|-h     Print this help.
#
# Environment variables:
#   ICN_FIRSTBOOT_INSTALL_UNIT     If "1", install icnd.service from the
#                                  repo's deploy/icnd.service if present.
#   ICN_FIRSTBOOT_FORCE_OVERWRITE  If "1", allow overwriting an existing
#                                  /etc/icn/appliance.env. Default: refuse.
#   ICN_DATA_DIR                   Override the data dir (default: /var/lib/icn).
#   ICN_CONFIG_DIR                 Override the config dir (default: /etc/icn).
#   ICN_LOG_DIR                    Override the log dir (default: /var/log/icn).
#   ICN_USER                       System user (default: icn).
#
# This script is intended to be safe to re-run. It will no-op once the
# marker file at $ICN_DATA_DIR/.firstboot-complete exists, unless the
# marker is removed by an operator.

set -euo pipefail

# ---------------- defaults ----------------
ICN_DATA_DIR="${ICN_DATA_DIR:-/var/lib/icn}"
ICN_CONFIG_DIR="${ICN_CONFIG_DIR:-/etc/icn}"
ICN_LOG_DIR="${ICN_LOG_DIR:-/var/log/icn}"
ICN_USER="${ICN_USER:-icn}"

# Resolved at runtime; not pinned at script-author time.
MARKER_FILE="$ICN_DATA_DIR/.firstboot-complete"
APPLIANCE_ENV_FILE="$ICN_CONFIG_DIR/appliance.env"

DRY_RUN=0

# ---------------- helpers ----------------
log()   { printf '[firstboot] %s\n' "$*"; }
warn()  { printf '[firstboot] WARN: %s\n' "$*" >&2; }
err()   { printf '[firstboot] ERROR: %s\n' "$*" >&2; }

usage() {
    sed -n '2,40p' "$0"
    exit 0
}

run() {
    # Echo the command; execute only if not dry-run.
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '[firstboot] (dry-run) %s\n' "$*"
    else
        eval "$@"
    fi
}

is_root() {
    [ "$(id -u)" = "0" ]
}

# ---------------- arg parsing ----------------
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run)  DRY_RUN=1 ; shift ;;
        --help|-h)  usage ;;
        *)          err "unknown argument: $1" ; exit 2 ;;
    esac
done

# ---------------- marker / idempotency ----------------
if [ -f "$MARKER_FILE" ]; then
    log "Marker present at $MARKER_FILE — first-boot already completed."
    log "Remove the marker file to re-run first-boot."
    exit 0
fi

log "Starting first-boot scaffold (dry-run=$DRY_RUN)."
log "  data_dir   = $ICN_DATA_DIR"
log "  config_dir = $ICN_CONFIG_DIR"
log "  log_dir    = $ICN_LOG_DIR"
log "  icn_user   = $ICN_USER"

# ---------------- create directories ----------------
for d in "$ICN_DATA_DIR" "$ICN_CONFIG_DIR" "$ICN_LOG_DIR"; do
    if [ -d "$d" ]; then
        log "Directory exists: $d"
    else
        log "Creating directory: $d"
        run "mkdir -p '$d'"
    fi
done

# ---------------- system user ----------------
if is_root; then
    if id "$ICN_USER" >/dev/null 2>&1; then
        log "User '$ICN_USER' already exists."
    else
        log "Creating system user: $ICN_USER"
        # Mirror deploy/install.sh: system user, no shell, home in data dir.
        run "useradd --system --home-dir '$ICN_DATA_DIR' --shell /bin/false '$ICN_USER'"
    fi
    # Make sure data dir is owned by icn:icn so icnd can write to it.
    log "Ensuring ownership of $ICN_DATA_DIR and $ICN_LOG_DIR -> $ICN_USER:$ICN_USER"
    run "chown -R '$ICN_USER':'$ICN_USER' '$ICN_DATA_DIR' '$ICN_LOG_DIR' 2>/dev/null || true"
else
    warn "Not running as root; skipping user creation and chown steps."
    warn "Operators normally run this script via systemd as root."
fi

# ---------------- appliance.env ----------------
# Minimal environment file. Contains NO secrets. The operator is expected
# to add ICN_GATEWAY_JWT_SECRET and any keystore passphrase out-of-band.
write_appliance_env() {
    # POSIX-compatible heredoc; quoted limit string disables interpolation.
    cat <<'EOF_APPLIANCE_ENV'
# /etc/icn/appliance.env
# Written by deploy/appliance/scripts/icn-appliance-firstboot.sh.
#
# This file declares scaffold-stage defaults for the ICN appliance. It
# contains NO secrets. Operators must add ICN_GATEWAY_JWT_SECRET and any
# keystore passphrase via a secrets manager, an out-of-band drop-in
# (e.g. /etc/icn/icnd.env per deploy/install.sh), or systemd LoadCredential.
#
# Ports are anchored to icn-core native defaults. Never 8000.

# Data and log roots; match the existing native install layout.
ICN_DATA_DIR=/var/lib/icn
ICN_LOG_DIR=/var/log/icn

# Gateway / health binds to localhost by default. Operators expose the
# service through a reverse proxy or by changing this binding deliberately.
ICN_APPLIANCE_GATEWAY_BIND=127.0.0.1:8080

# Peer transport bind. Native default; QUIC/UDP.
ICN_APPLIANCE_PEER_BIND=[::]:7777

# Default role profile is unset on first boot. The operator selects a
# role profile from deploy/appliance/roles/ and applies it deliberately.
ICN_APPLIANCE_ROLE=

# Log level (matches deploy/icnd.env.example convention).
# RUST_LOG=info
EOF_APPLIANCE_ENV
}

if [ -f "$APPLIANCE_ENV_FILE" ]; then
    if [ "${ICN_FIRSTBOOT_FORCE_OVERWRITE:-0}" = "1" ]; then
        warn "$APPLIANCE_ENV_FILE exists; ICN_FIRSTBOOT_FORCE_OVERWRITE=1 set, overwriting."
        if [ "$DRY_RUN" -eq 1 ]; then
            printf '[firstboot] (dry-run) would overwrite %s with scaffold defaults.\n' "$APPLIANCE_ENV_FILE"
        else
            write_appliance_env > "$APPLIANCE_ENV_FILE"
            chmod 644 "$APPLIANCE_ENV_FILE"
        fi
    else
        log "Refusing to overwrite existing $APPLIANCE_ENV_FILE (set ICN_FIRSTBOOT_FORCE_OVERWRITE=1 to override)."
    fi
else
    log "Writing scaffold defaults to $APPLIANCE_ENV_FILE (no secrets)."
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '[firstboot] (dry-run) would write %s with scaffold defaults.\n' "$APPLIANCE_ENV_FILE"
    else
        write_appliance_env > "$APPLIANCE_ENV_FILE"
        chmod 644 "$APPLIANCE_ENV_FILE"
    fi
fi

# ---------------- optional: install icnd.service ----------------
# We DO NOT enable or start anything. We only copy the unit into
# /etc/systemd/system if explicitly opted in and the unit can be found.
if [ "${ICN_FIRSTBOOT_INSTALL_UNIT:-0}" = "1" ]; then
    if is_root; then
        # Try to locate icnd.service relative to this script.
        SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        # PROJECT-ROOT discovery: appliance/scripts -> appliance -> deploy -> repo root.
        REPO_GUESS="$(cd "$SCRIPT_DIR/../../.." && pwd 2>/dev/null || true)"
        CANDIDATE="$REPO_GUESS/deploy/icnd.service"
        if [ -f "$CANDIDATE" ]; then
            log "Copying $CANDIDATE -> /etc/systemd/system/icnd.service"
            run "cp '$CANDIDATE' /etc/systemd/system/icnd.service"
            run "systemctl daemon-reload"
            log "icnd.service installed (not enabled, not started — operator does that)."
        else
            warn "ICN_FIRSTBOOT_INSTALL_UNIT=1 set but $CANDIDATE not found."
            warn "Skipping unit install. Re-run after placing the unit, or do it manually."
        fi
    else
        warn "ICN_FIRSTBOOT_INSTALL_UNIT=1 set but not running as root. Skipping."
    fi
fi

# ---------------- marker ----------------
log "Writing first-boot marker: $MARKER_FILE"
if [ "$DRY_RUN" -eq 1 ]; then
    printf '[firstboot] (dry-run) would create marker %s\n' "$MARKER_FILE"
else
    : > "$MARKER_FILE"
fi

# ---------------- next steps ----------------
cat <<'EOF_NEXT'
[firstboot] DONE. Next steps for the operator:

  1) Add secrets out-of-band. Do NOT put secrets in /etc/icn/appliance.env.
     For native installs, see deploy/icnd.env.example for the existing
     ICN_GATEWAY_JWT_SECRET pattern.

  2) Initialize identity (this is a deliberate action, not first-boot):
       sudo -u icn icnctl --data-dir /var/lib/icn id init

  3) Select a role profile from deploy/appliance/roles/ and apply it
     deliberately. The appliance scaffold does NOT auto-apply a role.

  4) Start the daemon when you're ready:
       sudo systemctl enable --now icnd

  5) Check health on 8080 (never 8000):
       curl -sf http://127.0.0.1:8080/v1/health

NOTE: The scaffold image is NOT a signed release. Do not place a scaffold
appliance into a partner federation. See docs/architecture/DEBIAN_APPLIANCE_MODEL.md.
EOF_NEXT

exit 0
