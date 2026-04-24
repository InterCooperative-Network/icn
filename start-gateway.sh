#!/bin/bash
set -e
JWT_SECRET=$(openssl rand -base64 32)
echo "$JWT_SECRET" > /tmp/icnd-jwt-secret.txt
export ICN_KEYSTORE_PASSPHRASE=''
export ICN_GATEWAY_JWT_SECRET="$JWT_SECRET"
exec /home/matt/projects/icn/icn/target/debug/icnd \
  --data-dir ~/.icn \
  --gateway-enable \
  --gateway-bind 127.0.0.1:8080 \
  --log-level warn
