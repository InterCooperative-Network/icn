#!/usr/bin/env bash
# build-image.sh
#
# ICN appliance image-build scaffold (dry-run-only today).
#
# This script does NOT build a real image. It prints the planned steps
# the future build path will execute, validates that minimum inputs are
# present, and exits.
#
# The first non-scaffold implementation will use Debian cloud image
# customization (virt-customize on debian-12-genericcloud-amd64). Later
# backends under consideration: Packer, debos, live-build. See
# docs/architecture/DEBIAN_APPLIANCE_MODEL.md §"Build path posture".
#
# Modes:
#   --dry-run    Print planned steps. Default when no real backend is wired.
#   --help|-h    Print this help.
#
# Required environment variables (when not dry-run, ie a future real build):
#   ICN_APPLIANCE_VERSION       Version label baked into the image manifest.
#   ICN_APPLIANCE_OUTPUT_DIR    Directory the built image is written to.
#   ICN_APPLIANCE_BASE_IMAGE    Path to (or URL of) the Debian cloud image.
#
# CI policy:
#   This script does NOT download external files. The Debian base image
#   must be staged by the operator before invocation. CI must not
#   trigger network fetches from this scaffold.

set -euo pipefail

MODE="dry-run"

usage() {
    sed -n '2,32p' "$0"
    exit 0
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) MODE="dry-run" ; shift ;;
        --real)    MODE="real"    ; shift ;;  # placeholder for the eventual real backend
        --help|-h) usage ;;
        *)
            printf '[build-image] ERROR: unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

log() { printf '[build-image] %s\n' "$*"; }

log "Mode: $MODE"
log "Planned backend (first slice): Debian cloud image + virt-customize."
log "  Base image:  ${ICN_APPLIANCE_BASE_IMAGE:-<unset>}"
log "  Version tag: ${ICN_APPLIANCE_VERSION:-<unset>}"
log "  Output dir:  ${ICN_APPLIANCE_OUTPUT_DIR:-<unset>}"
echo

cat <<'EOF_PLAN'
[build-image] Planned build steps (NOT executed in dry-run):

  1) Verify the Debian base image is present on disk.
     - Path comes from ICN_APPLIANCE_BASE_IMAGE.
     - We do NOT download the base image here. Stage it manually.
     - Verify SHA256 against an operator-supplied checksum file.

  2) Copy the base image to a working path under ICN_APPLIANCE_OUTPUT_DIR.

  3) Customize the working image using virt-customize:
       - Install required packages: ca-certificates, curl, openssl,
         systemd, python3 (for the firstboot heredoc validators).
       - Copy icnd, icnctl into /usr/local/bin (built from icn/ via
         `cargo build --release`).
       - Copy deploy/icnd.service           -> /etc/systemd/system/
       - Copy deploy/appliance/systemd/
            icn-appliance-firstboot.service -> /etc/systemd/system/
       - Copy deploy/appliance/scripts/
            icn-appliance-firstboot.sh      -> /usr/local/sbin/
                                               (and chmod +x)
       - Copy deploy/appliance/
            appliance.manifest.example.yaml -> /etc/icn/
       - Create the `icn` system user (handled at firstboot, not here,
         so the image is portable across hostnames).
       - Enable icn-appliance-firstboot.service.
       - Enable icnd.service (gated by firstboot marker via Before=).

  4) Truncate machine-id (so each booted instance gets its own).

  5) Run virt-sysprep --operations machine-id,bash-history,logfiles to
     scrub instance-specific state.

  6) Convert to qcow2 (or raw) per ICN_APPLIANCE_IMAGE_FORMAT.

  7) Emit a build manifest next to the image listing:
       - appliance_id (from appliance.manifest.example.yaml)
       - base_os version
       - icnd / icnctl version (cargo metadata)
       - manifest field hash
       - build host (operator-controlled, NOT a release identity)

  8) Print verification commands the operator can run.

[build-image] This is a scaffold. The above is a plan, not a result.
EOF_PLAN

if [ "$MODE" = "dry-run" ]; then
    log "Exiting cleanly. No image was built."
    exit 0
fi

# --- Real-build branch: refuse to pretend ---
# We do not have a real backend wired yet. Until the first slice lands,
# --real exits non-zero with a clear message so callers cannot
# accidentally believe an image was produced.
cat >&2 <<'EOF_REAL_REFUSED'
[build-image] --real backend not implemented.

The Debian cloud-image + virt-customize backend is the first planned
slice; it is not in this PR. Until that slice lands, --real refuses to
run rather than silently fake a build.

Track the next slice in: deploy/appliance/README.md §"Next implementation slice"
EOF_REAL_REFUSED
exit 3
