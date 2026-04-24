#!/bin/bash
# Starts a local icnd gateway for NYCN bootstrap live validation
# Data dir: /tmp/icn-bootstrap-test
# Gateway: 127.0.0.1:8085 (avoid 8080 which may be reserved)

JWT_SECRET="$(cat /tmp/icn-bootstrap-test/jwt-secret.txt)"

exec /home/matt/projects/icn/icn/target/debug/icnd \
  --data-dir /tmp/icn-bootstrap-test \
  --gateway-enable \
  --gateway-bind 127.0.0.1:8085 \
  --gateway-jwt-secret "$JWT_SECRET" \
  --log-level warn
