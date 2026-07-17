#!/usr/bin/env bash
set -euo pipefail

PACKAGE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<'EOF'
ICN Common Sense setup-and-run

Usage:
  ./setup-and-run.sh
  ./setup-and-run.sh --setup-only
  ./setup-and-run.sh --verify-only
  ./setup-and-run.sh --no-install

What it does by default:
  1. Checks for required host tools.
  2. Installs missing Debian/Ubuntu packages with apt, using sudo if needed.
  3. Verifies package checksums.
  4. Starts the local demo VM.

Notes:
  - Debian/Ubuntu Linux is the supported one-command path.
  - On other systems, install the listed tools manually, then run:
      ./scripts/verify.sh
      ./scripts/run-demo.sh
EOF
}

SETUP_ONLY=0
VERIFY_ONLY=0
NO_INSTALL=0

for arg in "$@"; do
  case "$arg" in
    --setup-only) SETUP_ONLY=1 ;;
    --verify-only) VERIFY_ONLY=1 ;;
    --no-install) NO_INSTALL=1 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "Unknown option: $arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

log() { printf '[icn-setup] %s\n' "$*"; }
err() { printf '[icn-setup] ERROR: %s\n' "$*" >&2; }

need_cmds=(
  qemu-system-x86_64
  qemu-img
  cloud-localds
  ssh
  curl
  sha256sum
)

apt_packages=(
  qemu-system-x86
  qemu-utils
  cloud-image-utils
  openssh-client
  curl
  coreutils
)

missing_cmds() {
  local missing=()
  local cmd
  for cmd in "${need_cmds[@]}"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      missing+=("$cmd")
    fi
  done
  if [ "${#missing[@]}" -gt 0 ]; then
    printf '%s\n' "${missing[@]}"
  fi
}

run_sudo() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    err "Missing sudo. Install these packages manually: ${apt_packages[*]}"
    exit 3
  fi
}

install_missing_tools() {
  mapfile -t missing < <(missing_cmds)
  if [ "${#missing[@]}" -eq 0 ]; then
    log "Host tools are already installed."
    return 0
  fi

  log "Missing host tools: ${missing[*]}"

  if [ "$NO_INSTALL" -eq 1 ]; then
    err "Required tools are missing and --no-install was requested."
    exit 3
  fi

  if ! command -v apt-get >/dev/null 2>&1; then
    err "This one-command installer supports Debian/Ubuntu apt-get only."
    err "Install these tools manually, then rerun: ${need_cmds[*]}"
    exit 3
  fi

  log "Installing Debian/Ubuntu packages. You may be asked for your password."
  run_sudo apt-get update
  run_sudo apt-get install -y "${apt_packages[@]}"

  mapfile -t missing_after < <(missing_cmds)
  if [ "${#missing_after[@]}" -ne 0 ]; then
    err "Still missing after install: ${missing_after[*]}"
    exit 3
  fi

  log "Host tools installed."
}

verify_package() {
  log "Verifying package checksums."
  "$PACKAGE_DIR/scripts/verify.sh"
  log "Checksums OK."
}

main() {
  cd "$PACKAGE_DIR"

  if [ "$VERIFY_ONLY" -eq 1 ]; then
    verify_package
    exit 0
  fi

  install_missing_tools

  if [ "$SETUP_ONLY" -eq 1 ]; then
    log "Setup complete."
    exit 0
  fi

  verify_package

  log "Starting the local ICN browser demo."
  exec "$PACKAGE_DIR/scripts/run-demo.sh"
}

main "$@"
