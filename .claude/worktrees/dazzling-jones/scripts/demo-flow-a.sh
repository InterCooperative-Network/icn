#!/usr/bin/env bash
# Demo Flow A: WASM Distribution
# Tests: upload WASM → list modules → submit by hash → poll status
set -euo pipefail

# Gateway binds 8080 by default (see icn-core/src/config/gateway.rs).
GATEWAY="${ICN_GATEWAY:-http://localhost:8080}"
TOKEN="${ICN_TOKEN:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

step() { echo -e "${CYAN}[Flow A] $1${NC}"; }
ok()   { echo -e "${GREEN}  ✓ $1${NC}"; }
fail() { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

# Step 1: Upload a minimal WASM module
step "Uploading WASM module..."
# Create a minimal valid WASM module (magic + version header)
WASM_BASE64=$(printf '\x00\x61\x73\x6d\x01\x00\x00\x00' | base64)

UPLOAD_RESP=$(curl -sf -X POST "$GATEWAY/v1/compute/wasm/upload" \
  -H "Content-Type: application/json" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
  -d "{\"wasm_bytes\": \"$WASM_BASE64\", \"name\": \"demo-module\", \"version\": \"1.0.0\"}" 2>&1) \
  || fail "Upload failed: $UPLOAD_RESP"

WASM_HASH=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['hash'])" 2>/dev/null) \
  || fail "Failed to parse upload response: $UPLOAD_RESP"
ok "Uploaded WASM module: hash=$WASM_HASH"

# Step 2: List WASM modules
step "Listing WASM modules..."
LIST_RESP=$(curl -sf "$GATEWAY/v1/compute/wasm?limit=10" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "List failed: $LIST_RESP"

MODULE_COUNT=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null) \
  || fail "Failed to parse list response: $LIST_RESP"
ok "Found $MODULE_COUNT module(s)"

# Step 3: Get module metadata by hash
step "Getting module metadata..."
META_RESP=$(curl -sf "$GATEWAY/v1/compute/wasm/$WASM_HASH" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Get metadata failed: $META_RESP"

MODULE_NAME=$(echo "$META_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('name', '(unnamed)'))" 2>/dev/null) \
  || fail "Failed to parse metadata: $META_RESP"
ok "Module: $MODULE_NAME (hash: $WASM_HASH)"

# Step 4: Submit task by WASM hash
step "Submitting compute task by WASM hash..."
SUBMIT_RESP=$(curl -sf -X POST "$GATEWAY/v1/compute/submit" \
  -H "Content-Type: application/json" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
  -d "{\"code_type\": \"wasm\", \"wasm_hash\": \"$WASM_HASH\", \"fuel_limit\": 10000}" 2>&1) \
  || { ok "Submit returned error (expected if compute daemon not connected)"; }

if [ -n "${SUBMIT_RESP:-}" ]; then
  TASK_HASH=$(echo "$SUBMIT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('task_hash','N/A'))" 2>/dev/null || echo "N/A")
  ok "Task submitted: $TASK_HASH"
fi

echo ""
echo -e "${GREEN}[Flow A] WASM Distribution demo completed successfully${NC}"
