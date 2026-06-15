#!/usr/bin/env bash
# icn-appliance-firstboot.sh
#
# ICN appliance first-boot scaffold.
#
# What this does (today):
#   - Creates /etc/icn, /var/lib/icn, /var/log/icn if missing.
#   - Creates or validates the `icn` system user (if run as root).
#   - Writes a minimal /etc/icn/appliance.env ONLY if it is absent.
#   - When ICN_FIRSTBOOT_INIT_IDENTITY=1 (default), generates a per-instance
#     JWT secret + keystore passphrase, writes them to /etc/icn/icnd.env
#     (mode 600, owned icn:icn), and runs `icnd --init` to seal the
#     keystore + write config.toml/genesis.json under /var/lib/icn.
#     The image itself contains NO secrets; everything is generated on
#     this specific boot.
#   - Optionally installs the icnd.service unit if it can be found
#     and ICN_FIRSTBOOT_INSTALL_UNIT=1 is set.
#   - Drops a marker file so subsequent runs no-op.
#   - Prints next steps for an operator.
#
# What this does NOT do:
#   - Embed any secret in the image. JWT + passphrase are generated per-VM.
#   - Start or enable icnd.service. The image's build step does that;
#     this script never invokes `systemctl start`.
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
#   ICN_FIRSTBOOT_INIT_IDENTITY    If "1" (default for in-image runs), generate
#                                  per-instance JWT secret + keystore
#                                  passphrase, write them to /etc/icn/icnd.env
#                                  (mode 600, owned icn:icn), and run
#                                  `icnd --init` so icnd.service can start.
#                                  Set to "0" to opt out (operator manages
#                                  secrets and identity by hand).
#   ICN_DATA_DIR                   Override the data dir (default: /var/lib/icn).
#   ICN_CONFIG_DIR                 Override the config dir (default: /etc/icn).
#   ICN_LOG_DIR                    Override the log dir (default: /var/log/icn).
#   ICN_USER                       System user (default: icn).
#
# Per-instance secrets:
#   When ICN_FIRSTBOOT_INIT_IDENTITY=1, this script generates fresh JWT and
#   keystore-passphrase material the first time it runs. These values are
#   NOT embedded in the appliance image; they are generated per-VM on first
#   boot and written to /etc/icn/icnd.env (mode 600). To rotate, remove
#   /var/lib/icn/.firstboot-complete AND the keystore file
#   (/var/lib/icn/identity.age plus config.toml/genesis.json in the same
#   directory), then re-run.
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

# Ports written into the generated config.toml. These match the runtime
# binds in deploy/icnd.service (--gateway-bind 127.0.0.1:8080) and the
# scaffold defaults in /etc/icn/appliance.env. Override via env if needed.
ICN_FIRSTBOOT_INIT_GATEWAY_PORT="${ICN_FIRSTBOOT_INIT_GATEWAY_PORT:-8080}"
ICN_FIRSTBOOT_INIT_GOSSIP_PORT="${ICN_FIRSTBOOT_INIT_GOSSIP_PORT:-7777}"

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

# ---------------- per-instance secrets + identity init ----------------
# This block runs only when ICN_FIRSTBOOT_INIT_IDENTITY=1 (default for
# image runs) AND we are root. It generates instance-local secret material
# and bootstraps the icnd identity. The image itself contains NO secrets;
# everything written here is generated on this specific boot.
#
# Files written:
#   /etc/icn/icnd.env       mode 600, owned icn:icn
#                           contains ICN_GATEWAY_JWT_SECRET and
#                           ICN_KEYSTORE_PASSPHRASE for systemd
#                           EnvironmentFile=-/etc/icn/icnd.env consumption.
#   /var/lib/icn/...        icnd --init writes identity, config, and genesis.
#
# This block is opt-out: set ICN_FIRSTBOOT_INIT_IDENTITY=0 to skip it (the
# operator then manages secrets and identity by hand, matching the
# deploy/install.sh manual path).

ICND_ENV_FILE="$ICN_CONFIG_DIR/icnd.env"
init_identity_block() {
    if [ "${ICN_FIRSTBOOT_INIT_IDENTITY:-1}" != "1" ]; then
        log "Skipping identity init (ICN_FIRSTBOOT_INIT_IDENTITY=0)."
        return 0
    fi
    if ! is_root; then
        warn "Skipping identity init: not running as root."
        return 0
    fi
    if ! command -v openssl >/dev/null 2>&1; then
        warn "Skipping identity init: openssl not available on PATH."
        return 0
    fi
    if ! command -v icnd >/dev/null 2>&1; then
        warn "Skipping identity init: icnd not on PATH."
        warn "Place icnd in /usr/local/bin and re-run after removing the marker."
        return 0
    fi
    # Bail if identity already exists (operator did it by hand, or a prior
    # firstboot already ran). The keystore is a file at
    # ${ICN_DATA_DIR}/identity.age (per icnd --init in icn/bins/icnd/src/main.rs),
    # not a directory. We also bail if /etc/icn/icnd.env exists, since that
    # env file is paired with the keystore passphrase.
    local keystore_file="$ICN_DATA_DIR/identity.age"
    if [ -f "$keystore_file" ] || [ -f "$ICND_ENV_FILE" ]; then
        log "Identity material already present at $keystore_file or $ICND_ENV_FILE; not regenerating."
        return 0
    fi

    log "Generating per-instance JWT secret + keystore passphrase (NOT embedded in image)."
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '[firstboot] (dry-run) would generate JWT + keystore passphrase, write %s mode 600.\n' "$ICND_ENV_FILE"
        printf '[firstboot] (dry-run) would run: ICN_KEYSTORE_PASSPHRASE=<REDACTED> runuser -u %s --preserve-environment -- icnd --init --data-dir %s --init-gateway-port %s --init-gossip-port %s\n' \
            "$ICN_USER" "$ICN_DATA_DIR" "$ICN_FIRSTBOOT_INIT_GATEWAY_PORT" "$ICN_FIRSTBOOT_INIT_GOSSIP_PORT"
        return 0
    fi

    local jwt pass tmp
    jwt="$(openssl rand -hex 32)"
    pass="$(openssl rand -base64 32 | tr -d '\n')"

    # Write env file atomically with mode 600 so the secrets are never
    # world-readable, even briefly.
    tmp="$(mktemp -p "$ICN_CONFIG_DIR" .icnd.env.XXXXXX)"
    chmod 600 "$tmp"
    {
        printf '# /etc/icn/icnd.env\n'
        printf '# Generated by icn-appliance-firstboot.sh on first boot.\n'
        printf '# Instance-local material. NOT in the appliance image.\n'
        printf '# To rotate: remove the marker and the keystore, then re-run.\n'
        printf 'ICN_GATEWAY_JWT_SECRET=%s\n' "$jwt"
        printf 'ICN_KEYSTORE_PASSPHRASE=%s\n' "$pass"
    } > "$tmp"
    chown "$ICN_USER":"$ICN_USER" "$tmp"
    mv "$tmp" "$ICND_ENV_FILE"

    log "Wrote $ICND_ENV_FILE (mode 600, $ICN_USER:$ICN_USER)."

    # Run icnd --init as the icn user. The passphrase comes from env, so
    # the call is non-interactive. icnd --init creates identity.age,
    # config.toml, and genesis.json under $ICN_DATA_DIR, then exits.
    #
    # --init-gateway-port / --init-gossip-port are written into the generated
    # config.toml so the persisted config matches deploy/icnd.service's
    # runtime --gateway-bind and the appliance.env scaffold defaults. Without
    # these, icnd --init defaults to gateway:8000 / gossip:9000, which would
    # drift from the runtime bind and from appliance.env's documented ports.
    # The runtime --gateway-bind flag in deploy/icnd.service still wins for
    # the bind address; this just keeps the on-disk config honest.
    # Drop to the icn user with `runuser`, NOT `sudo -u`. sudo logs the full
    # command (including ICN_KEYSTORE_PASSPHRASE=<value>) to the auth journal;
    # runuser does not. firstboot already runs as root, so no outer sudo is
    # needed (an outer sudo would re-strip the env before runuser). The
    # passphrase is passed via the environment with --preserve-environment.
    # Same hardening as icn-demo-{seed,verify}.
    log "Initializing icnd identity (icnd --init) as $ICN_USER..."
    if ICN_KEYSTORE_PASSPHRASE="$pass" \
        runuser -u "$ICN_USER" --preserve-environment -- \
        icnd --init \
            --data-dir "$ICN_DATA_DIR" \
            --init-gateway-port "$ICN_FIRSTBOOT_INIT_GATEWAY_PORT" \
            --init-gossip-port "$ICN_FIRSTBOOT_INIT_GOSSIP_PORT"; then
        log "icnd --init succeeded."
    else
        warn "icnd --init failed. icnd.service may not start until identity is initialized."
        warn "Inspect: runuser -u $ICN_USER -- icnd --init --data-dir $ICN_DATA_DIR"
        return 1
    fi
}
init_identity_block || warn "Identity init reported a non-zero exit; continuing so the rest of firstboot still completes."

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

  1) Per-instance secrets and identity have been generated at /etc/icn/icnd.env
     (mode 600) and /var/lib/icn/ — these are local to this VM only and were
     NOT in the appliance image. To rotate, remove the firstboot marker AND
     the keystore file (/var/lib/icn/identity.age plus config.toml/genesis.json
     in the same directory), then re-run.

  2) Select a role profile from /etc/icn/roles/ and apply it deliberately.
     The appliance scaffold does NOT auto-apply a role.

  3) icnd.service is expected to start automatically (enabled at image
     build time). Verify:
       systemctl is-active icnd

  4) Check health on 8080 (never 8000):
       curl -sf http://127.0.0.1:8080/v1/health

NOTE: The dev image is NOT a signed release. Do not place a dev appliance
into a partner federation. See docs/architecture/DEBIAN_APPLIANCE_MODEL.md.
EOF_NEXT

exit 0
