#!/bin/bash
# Starts a local icnd gateway for development / NYCN bootstrap validation.
#
# Override the binary via ICND_BIN, e.g.:
#   ICND_BIN=$(pwd)/icn/target/release/icnd ./start-gateway.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICND_BIN="${ICND_BIN:-$SCRIPT_DIR/icn/target/debug/icnd}"

if [ ! -x "$ICND_BIN" ]; then
  echo "error: icnd binary not found or not executable: $ICND_BIN" >&2
  echo "build it first: (cd '$SCRIPT_DIR/icn' && cargo build -p icnd)" >&2
  exit 1
fi

DATA_DIR="${ICN_DATA_DIR:-$HOME/.icn}"
JWT_SECRET_FILE="$DATA_DIR/gateway-jwt-secret"

mkdir -p "$DATA_DIR"
chmod 700 "$DATA_DIR"

umask 077
if [ ! -s "$JWT_SECRET_FILE" ]; then
  openssl rand -base64 32 > "$JWT_SECRET_FILE"
  chmod 600 "$JWT_SECRET_FILE"
fi

JWT_SECRET="$(cat "$JWT_SECRET_FILE")"
if [ -z "$JWT_SECRET" ]; then
  echo "error: JWT secret file '$JWT_SECRET_FILE' is empty" >&2
  exit 1
fi

export ICN_KEYSTORE_PASSPHRASE=''
export ICN_GATEWAY_JWT_SECRET="$JWT_SECRET"
exec "$ICND_BIN" \
  --data-dir "$DATA_DIR" \
  --gateway-enable \
  --gateway-bind 127.0.0.1:8080 \
  --log-level warn
