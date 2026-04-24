#!/bin/bash
# Starts a local icnd gateway for NYCN bootstrap live validation.
# Data dir: /tmp/icn-bootstrap-test (ephemeral)
# Gateway:  127.0.0.1:8085 (avoid 8080 which may be reserved)
#
# Override the binary via ICND_BIN, e.g.:
#   ICND_BIN=$(pwd)/icn/target/release/icnd ./start-gateway-test.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICND_BIN="${ICND_BIN:-$SCRIPT_DIR/icn/target/debug/icnd}"

if [ ! -x "$ICND_BIN" ]; then
  echo "error: icnd binary not found or not executable: $ICND_BIN" >&2
  echo "build it first: (cd '$SCRIPT_DIR/icn' && cargo build -p icnd)" >&2
  exit 1
fi

DATA_DIR="/tmp/icn-bootstrap-test"
JWT_SECRET_FILE="$DATA_DIR/jwt-secret.txt"

mkdir -p "$DATA_DIR"
chmod 700 "$DATA_DIR"

umask 077
if [ ! -s "$JWT_SECRET_FILE" ]; then
  openssl rand -hex 32 > "$JWT_SECRET_FILE"
  chmod 600 "$JWT_SECRET_FILE"
fi

JWT_SECRET="$(cat "$JWT_SECRET_FILE")"
if [ -z "$JWT_SECRET" ]; then
  echo "error: JWT secret file '$JWT_SECRET_FILE' is empty" >&2
  exit 1
fi

exec "$ICND_BIN" \
  --data-dir "$DATA_DIR" \
  --gateway-enable \
  --gateway-bind 127.0.0.1:8085 \
  --gateway-jwt-secret "$JWT_SECRET" \
  --log-level warn
