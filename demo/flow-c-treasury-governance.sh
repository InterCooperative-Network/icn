#!/usr/bin/env bash
# Flow C Treasury Governance Demo
# Sequence: balance -> proposeSpend -> vote -> getProof

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

BASE_URL="${ICN_BASE_URL:-${ICN_GATEWAY_URL:-http://127.0.0.1:7845}}"
TOKEN="${ICN_TOKEN:-}"
COOP_ID="${ICN_COOP_ID:-demo-coop}"
RECIPIENT_DID="${ICN_RECIPIENT_DID:-did:icn:zBobDemoRecipient123456789}"
AMOUNT="${ICN_SPEND_AMOUNT:-100}"
CURRENCY="${ICN_SPEND_CURRENCY:-credits}"
MEMO="${ICN_SPEND_MEMO:-Flow C demo treasury spend}"

if [ -z "$TOKEN" ]; then
  echo "ERROR: ICN_TOKEN is required"
  echo "Example: ICN_TOKEN=<jwt> ICN_COOP_ID=<coop> bash demo/flow-c-treasury-governance.sh"
  exit 1
fi

AUTH_HEADER="Authorization: Bearer $TOKEN"

request() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local tmp_body
  tmp_body="$(mktemp)"
  local code
  local curl_rc=0
  if [ -n "$body" ]; then
    code=$(curl -sS -o "$tmp_body" -w "%{http_code}" -X "$method" \
      "$BASE_URL$path" \
      -H "$AUTH_HEADER" \
      -H "Content-Type: application/json" \
      -d "$body") || curl_rc=$?
  else
    code=$(curl -sS -o "$tmp_body" -w "%{http_code}" -X "$method" \
      "$BASE_URL$path" \
      -H "$AUTH_HEADER") || curl_rc=$?
  fi
  local resp
  resp="$(cat "$tmp_body")"
  rm -f "$tmp_body"
  if [ "$curl_rc" -ne 0 ]; then
    code="000"
    if [ -z "$resp" ]; then
      resp="curl failed with exit code $curl_rc"
    fi
  fi
  printf '%s\n%s\n' "$code" "$resp"
}

require_2xx() {
  local code="$1"
  local context="$2"
  local body="$3"
  if [ "$code" -lt 200 ] || [ "$code" -ge 300 ]; then
    echo "ERROR: $context failed (HTTP $code)"
    echo "Response: $body"
    exit 1
  fi
}

require_non_2xx() {
  local code="$1"
  local context="$2"
  local body="$3"
  if [ "$code" -ge 200 ] && [ "$code" -lt 300 ]; then
    echo "ERROR: $context unexpectedly succeeded (HTTP $code)"
    echo "Response: $body"
    exit 1
  fi
}

echo "FLOW_C_START"
echo "Flow C demo against $BASE_URL (coop: $COOP_ID)"

# 0) health
health_out="$(request GET /health)"
health_code="$(printf '%s' "$health_out" | sed -n '1p')"
health_body="$(printf '%s' "$health_out" | sed -n '2,$p')"
require_2xx "$health_code" "Health check" "$health_body"

echo "HEALTH_OK"

# 1) balance
balance_out="$(request GET "/v1/coops/${COOP_ID}/treasury/balance")"
balance_code="$(printf '%s' "$balance_out" | sed -n '1p')"
balance_body="$(printf '%s' "$balance_out" | sed -n '2,$p')"
require_2xx "$balance_code" "Treasury balance" "$balance_body"

# 2) propose
propose_payload="$(cat <<JSON
{"amount":$AMOUNT,"recipient":"$RECIPIENT_DID","memo":"$MEMO","currency":"$CURRENCY"}
JSON
)"

propose_out="$(request POST "/v1/coops/${COOP_ID}/proposals" "$propose_payload")"
propose_code="$(printf '%s' "$propose_out" | sed -n '1p')"
propose_body="$(printf '%s' "$propose_out" | sed -n '2,$p')"
require_2xx "$propose_code" "Propose treasury spend" "$propose_body"

proposal_id="$(python3 - <<'PY' "$propose_body"
import json,sys
obj=json.loads(sys.argv[1])
print(obj.get("proposal_id", ""))
PY
)"
used_nonce="$(python3 - <<'PY' "$propose_body"
import json,sys
obj=json.loads(sys.argv[1])
v=obj.get("nonce")
print("" if v is None else v)
PY
)"

if [ -z "$proposal_id" ]; then
  echo "ERROR: Propose response missing proposal_id"
  echo "Response: $propose_body"
  exit 1
fi

echo "FLOW_C_PROPOSAL_ID=$proposal_id"

# 3) vote
vote_payload='{"choice":"for"}'
vote_out="$(request POST "/v1/proposals/${proposal_id}/vote" "$vote_payload")"
vote_code="$(printf '%s' "$vote_out" | sed -n '1p')"
vote_body="$(printf '%s' "$vote_out" | sed -n '2,$p')"
require_2xx "$vote_code" "Cast vote" "$vote_body"

# 4) proof
proof_out="$(request GET "/v1/proposals/${proposal_id}/proof")"
proof_code="$(printf '%s' "$proof_out" | sed -n '1p')"
proof_body="$(printf '%s' "$proof_out" | sed -n '2,$p')"
require_2xx "$proof_code" "Get proposal proof" "$proof_body"

decision_hash="$(python3 - <<'PY' "$proof_body"
import json,sys
obj=json.loads(sys.argv[1])
receipt=obj.get("receipt", {})
dh=receipt.get("decision_hash")
if isinstance(dh, list):
    print(bytes(int(x) & 0xff for x in dh).hex())
elif isinstance(dh, str):
    print(dh)
else:
    print("")
PY
)"
attestation_count="$(python3 - <<'PY' "$proof_body"
import json,sys
obj=json.loads(sys.argv[1])
att=obj.get("attestations")
print(len(att) if isinstance(att, list) else 0)
PY
)"

if [ -z "$decision_hash" ]; then
  echo "ERROR: Proof response missing receipt.decision_hash"
  echo "Response: $proof_body"
  exit 1
fi

echo "FLOW_C_DECISION_HASH=$decision_hash"
echo "FLOW_C_PROOF_ATTESTATIONS=$attestation_count"

# Adversarial check A: replay submission with same expected nonce (if nonce provided)
if [ -z "$used_nonce" ]; then
  echo "ERROR: Propose response missing nonce; cannot run adversarial nonce checks"
  echo "Response: $propose_body"
  exit 1
fi

replay_payload="$(cat <<JSON
{"amount":$AMOUNT,"recipient":"$RECIPIENT_DID","memo":"$MEMO","currency":"$CURRENCY","expected_nonce":$used_nonce}
JSON
)"
replay_out="$(request POST "/v1/coops/${COOP_ID}/proposals" "$replay_payload")"
replay_code="$(printf '%s' "$replay_out" | sed -n '1p')"
replay_body="$(printf '%s' "$replay_out" | sed -n '2,$p')"
require_non_2xx "$replay_code" "Replay propose check" "$replay_body"
echo "FLOW_C_REPLAY_REJECTED=$replay_code"

# Adversarial check B: bad nonce should fail
bad_nonce=$((used_nonce + 999))
bad_payload="$(cat <<JSON
{"amount":$AMOUNT,"recipient":"$RECIPIENT_DID","memo":"$MEMO","currency":"$CURRENCY","expected_nonce":$bad_nonce}
JSON
)"
bad_out="$(request POST "/v1/coops/${COOP_ID}/proposals" "$bad_payload")"
bad_code="$(printf '%s' "$bad_out" | sed -n '1p')"
bad_body="$(printf '%s' "$bad_out" | sed -n '2,$p')"
require_non_2xx "$bad_code" "Bad nonce check" "$bad_body"
echo "FLOW_C_BAD_NONCE_REJECTED=$bad_code"

echo "FLOW_C_COMPLETE"
