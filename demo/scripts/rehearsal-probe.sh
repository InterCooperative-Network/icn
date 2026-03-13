#!/usr/bin/env bash
# rehearsal-probe.sh — Live rehearsal probe for the five pre-demo unknowns.
#
# Run this from icn-dev (10.8.30.45) BEFORE running any public demo.
# It answers exactly five questions and exits with a summary.
#
# Usage:
#   ./demo/scripts/rehearsal-probe.sh
#
# Outputs:
#   CONFIRMED  — the system behaved as the scripts expect
#   MISMATCH   — the system behavior differs; notes say how to adapt
#   UNCLEAR    — probe ran but result is ambiguous (check raw output)
#   FAILED     — the probe itself failed (pod error, gateway down, etc.)

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

# ── Counters and observed values for operator table ──────────────────────────
_CONFIRMED=0
_MISMATCH=0
_UNCLEAR=0
_FAILED=0

# Per-probe verdict + observed value — both filled in as probes run; printed in operator table
R1_STATUS="not-run" ; R1_OBSERVED_STATE="not-run"
R2_STATUS="not-run" ; R2_OBSERVED_KEYS="not-run"
R3_STATUS="not-run" ; R3_OBSERVED="not-run"
R4_STATUS="not-run" ; R4_OBSERVED="not-run"
R5a_STATUS="not-run"; R5b_STATUS="not-run"; R5_OBSERVED="not-run"

_probe_result() {
  local status="$1" label="$2" note="$3"
  case "$status" in
    CONFIRMED) _CONFIRMED=$(( _CONFIRMED + 1 )); echo -e "  ${GREEN}✓ CONFIRMED${NC}  $label"; ;;
    MISMATCH)  _MISMATCH=$((_MISMATCH   + 1 )); echo -e "  ${RED}✗ MISMATCH${NC}   $label"; ;;
    UNCLEAR)   _UNCLEAR=$(( _UNCLEAR    + 1 )); echo -e "  ${YELLOW}? UNCLEAR${NC}    $label"; ;;
    FAILED)    _FAILED=$(( _FAILED      + 1 )); echo -e "  ${RED}✗ FAILED${NC}     $label"; ;;
  esac
  [ -z "$note" ] || echo "             $note"
}

# ── Temp file (mirrors _do_curl pattern in flow scripts) ─────────────────────
_RESP_FILE="$(mktemp /tmp/rehearsal-probe-XXXXXX)"
trap 'rm -f "$_RESP_FILE"' EXIT

_do_probe_curl() {
  local url="$1" method="${2:-GET}" body="${3:-}" token="${4:-}"
  local args=(-s -o "$_RESP_FILE" -w "%{http_code}" -X "$method"
              -H "Content-Type: application/json")
  [ -n "$token" ] && args+=(-H "Authorization: Bearer $token")
  [ -n "$body"  ] && args+=(-d "$body")
  PROBE_HTTP_CODE=$(curl "${args[@]}" "$url" 2>/dev/null || echo "000")
}

_field() {
  # Extract named field from JSON in $_RESP_FILE.
  # Handles top-level fields and one-level enum variants {"State": {"field": val}}
  local name="$1"
  python3 -c "
import sys, json
try:
    d = json.load(open('$_RESP_FILE'))
    if '$name' in d:
        print(d['$name'])
    else:
        for v in d.values():
            if isinstance(v, dict) and '$name' in v:
                print(v['$name'])
                break
except Exception as e:
    sys.exit(0)
" 2>/dev/null || true
}

_extract_proposal_state() {
  # Extract proposal state from JSON in $_RESP_FILE.
  # Handles both:
  #   {"state": "Accepted"}                  — plain string field
  #   {"Accepted": {"for_votes": 3, ...}}    — Rust enum variant as top-level key
  #   {"state": {"Accepted": {}}}            — nested enum under "state" key
  # Outputs the state word (Accepted/Rejected/Open/Draft/NoQuorum) or empty string.
  python3 -c "
import json, sys
KNOWN_STATES = {'Accepted', 'Rejected', 'Open', 'Draft', 'NoQuorum', 'Closed', 'Pending'}
try:
    d = json.load(open('$_RESP_FILE'))
    # Case 1: plain string field named 'state'
    if 'state' in d and isinstance(d['state'], str):
        print(d['state'])
        sys.exit(0)
    # Case 2: Rust enum variant IS the top-level key
    for k in d:
        if k in KNOWN_STATES:
            print(k)
            sys.exit(0)
    # Case 3: state field holds an enum object {'Accepted': {}}
    if 'state' in d and isinstance(d['state'], dict):
        for k in d['state']:
            if k in KNOWN_STATES:
                print(k)
                sys.exit(0)
    # Case 4: nested under 'proposal' wrapper
    if 'proposal' in d and isinstance(d['proposal'], dict):
        p = d['proposal']
        if 'state' in p and isinstance(p['state'], str):
            print(p['state'])
            sys.exit(0)
        for k in p:
            if k in KNOWN_STATES:
                print(k)
                sys.exit(0)
except Exception:
    pass
" 2>/dev/null || true
}

# ── Infrastructure: bring up port-forwards ────────────────────────────────────
echo ""
echo "================================================================"
echo " ICN Federation Demo — Pre-Rehearsal Probe"
echo " Five unknowns. Concrete answers."
echo "================================================================"
echo ""

narrate "Starting port-forwards..."
demo_ports_up
demo_wait_ready 45

narrate "Fetching Harbor Homes token (gamma — governance flows)..."
HARBOR_TOKEN="$(demo_get_token gamma)"
if [ -z "$HARBOR_TOKEN" ]; then
  echo ""
  fail "Could not get Harbor Homes token. Check pod: kubectl get pod -n icn-coop-gamma"
  exit 1
fi
result "Harbor Homes token obtained"

# ── R1: Quorum with a single DID ─────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " R1: Does 1/1 single-DID vote satisfy quorum?"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

HARBOR_DOMAIN_ID="harborhomes-governance"   # reseed creates this exact ID (no hyphen between harbor and homes)
R1_TITLE="R1-PROBE-QUORUM-$(date +%s)"

# Create a proposal
_do_probe_curl "${HARBOR_URL}/v1/gov/proposals" POST \
  "{\"domain_id\":\"${HARBOR_DOMAIN_ID}\",\"title\":\"${R1_TITLE}\",\"description\":\"Rehearsal probe — quorum test\",\"payload\":{\"type\":\"text\",\"body\":\"Rehearsal probe body — testing quorum with single DID.\"}}" \
  "$HARBOR_TOKEN"

if [[ ! "$PROBE_HTTP_CODE" =~ ^2 ]]; then
  echo "  Raw response: $(cat "$_RESP_FILE")"
  R1_STATUS=FAILED; R1_OBSERVED_STATE="create-${PROBE_HTTP_CODE}"
  _probe_result FAILED "R1" "Proposal creation returned HTTP $PROBE_HTTP_CODE — domain '$HARBOR_DOMAIN_ID' may not exist (run reseed first)"
else
  R1_PROPOSAL_ID="$(_field id)"
  result "Proposal created: $R1_PROPOSAL_ID"

  # Open it
  _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R1_PROPOSAL_ID}/open" POST "{}" "$HARBOR_TOKEN"
  if [[ ! "$PROBE_HTTP_CODE" =~ ^2 ]]; then
    R1_STATUS=FAILED; R1_OBSERVED_STATE="open-${PROBE_HTTP_CODE}"
    _probe_result FAILED "R1" "Open returned HTTP $PROBE_HTTP_CODE"
  else
    result "Proposal opened"

    # Cast 1 vote (the only DID available)
    _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R1_PROPOSAL_ID}/vote" POST \
      '{"choice":"for","comment":"Rehearsal probe — single-DID quorum test"}' \
      "$HARBOR_TOKEN"
    if [[ ! "$PROBE_HTTP_CODE" =~ ^2 ]]; then
      R1_STATUS=FAILED; R1_OBSERVED_STATE="vote-${PROBE_HTTP_CODE}"
      _probe_result FAILED "R1" "Vote returned HTTP $PROBE_HTTP_CODE; body: $(cat "$_RESP_FILE")"
    else
      result "Vote cast (HTTP $PROBE_HTTP_CODE)"

      # Close it
      _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R1_PROPOSAL_ID}/close" POST "" "$HARBOR_TOKEN"
      R1_CLOSE_CODE="$PROBE_HTTP_CODE"
      R1_CLOSE_BODY="$(cat "$_RESP_FILE")"

      if [[ ! "$R1_CLOSE_CODE" =~ ^2 ]]; then
        R1_STATUS=FAILED; R1_OBSERVED_STATE="close-${R1_CLOSE_CODE}"
        _probe_result FAILED "R1" "Close returned HTTP $R1_CLOSE_CODE; body: $R1_CLOSE_BODY"
      else
        # Check final state — use _extract_proposal_state, which handles both
        # {"state": "Accepted"} and {"Accepted": {...}} (Rust enum-variant top-level key)
        _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R1_PROPOSAL_ID}" GET "" "$HARBOR_TOKEN"
        R1_STATE="$(_extract_proposal_state)"
        echo "  Final proposal JSON (top-level keys and state field):"
        python3 -c "
import json, sys
d = json.load(open('$_RESP_FILE'))
print('  top-level keys:', list(d.keys()))
if 'state' in d:
    print('  state field:', json.dumps(d['state']))
" 2>/dev/null || cat "$_RESP_FILE" | head -5
        echo "  Extracted state:  ${R1_STATE:-<could not extract — inspect JSON above>}"
        R1_OBSERVED_STATE="${R1_STATE:-unknown}"

        if [ "$R1_STATE" = "Accepted" ]; then
          R1_STATUS=CONFIRMED
          _probe_result CONFIRMED "R1: Single-DID vote accepted (quorum passes 1/1)" ""
        elif [ -z "$R1_STATE" ]; then
          R1_STATUS=UNCLEAR
          _probe_result UNCLEAR "R1: Could not extract state — JSON shape not recognized" \
            "Check JSON keys above. Add the actual state field name to _extract_proposal_state."
        else
          R1_STATUS=MISMATCH
          _probe_result MISMATCH "R1: Final state is '${R1_STATE}', not 'Accepted'" \
            "Engine may require >1 voter or 60% threshold fails with 1/1. Flow-1/2 will show non-Accepted branches."
        fi
      fi
    fi
  fi
fi

# ── R2: Tally response field names ────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " R2: What are the tally endpoint field names?"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Create a fresh proposal for tally probing (R1 proposal is already closed)
R2_TITLE="R2-PROBE-TALLY-$(date +%s)"
_do_probe_curl "${HARBOR_URL}/v1/gov/proposals" POST \
  "{\"domain_id\":\"${HARBOR_DOMAIN_ID}\",\"title\":\"${R2_TITLE}\",\"description\":\"Rehearsal probe — tally field names\",\"payload\":{\"type\":\"text\",\"body\":\"Rehearsal probe body — testing tally field names.\"}}" \
  "$HARBOR_TOKEN"

if [[ ! "$PROBE_HTTP_CODE" =~ ^2 ]]; then
  R2_STATUS=FAILED; R2_OBSERVED_KEYS="create-${PROBE_HTTP_CODE}"
  _probe_result FAILED "R2" "Proposal creation returned HTTP $PROBE_HTTP_CODE"
else
  R2_PROPOSAL_ID="$(_field id)"

  _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R2_PROPOSAL_ID}/open" POST "{}" "$HARBOR_TOKEN"
  _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R2_PROPOSAL_ID}/vote" POST \
    '{"choice":"for","comment":"R2 tally probe"}' "$HARBOR_TOKEN"

  _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R2_PROPOSAL_ID}/tally" GET "" "$HARBOR_TOKEN"
  R2_HTTP="$PROBE_HTTP_CODE"
  R2_RAW="$(cat "$_RESP_FILE")"

  echo "  Tally endpoint:   ${HARBOR_URL}/v1/gov/proposals/${R2_PROPOSAL_ID}/tally"
  echo "  HTTP status:      $R2_HTTP"
  echo "  Raw response:"
  echo "$R2_RAW" | python3 -m json.tool 2>/dev/null || echo "  $R2_RAW"

  if [[ "$R2_HTTP" =~ ^2 ]]; then
    # Flatten one level of enum wrapping, print all keys, record exact for operator table
    R2_OBSERVED_KEYS="$(python3 -c "
import json, sys
d = json.loads(open('$_RESP_FILE').read())
flat = {}
for k, v in d.items():
    if isinstance(v, dict):
        flat.update(v)
    else:
        flat[k] = v
print(' '.join(sorted(flat.keys())))
" 2>/dev/null || echo "<parse failed>")"

    echo ""
    echo "  Tally flat keys: ${R2_OBSERVED_KEYS:-<could not parse>}"
    echo ""

    # Bootstrap heuristic: look for any recognizable "for" count field.
    # AFTER the first successful rehearsal, replace this grep with the exact observed
    # field names from R2_OBSERVED_KEYS — collapse from "probably right" to "pinned".
    # e.g. change the grep to: grep -q "^for_votes " (exact match, exact field)
    if echo "$R2_OBSERVED_KEYS" | grep -qi "for_votes\|for_count\|votes_for\|for$"; then
      R2_STATUS="CONFIRMED"
      _probe_result CONFIRMED "R2: Tally has a 'for' vote count field" \
        ">>> Pin: replace this grep with exact fields from R2_OBSERVED_KEYS above <<<"
    else
      R2_STATUS="MISMATCH"
      _probe_result MISMATCH "R2: No recognizable 'for' count field in tally response" \
        "Flow-1 tally extraction needs updating. Actual field names in R2_OBSERVED_KEYS above."
    fi

    # Close the R2 proposal to keep state clean
    _do_probe_curl "${HARBOR_URL}/v1/gov/proposals/${R2_PROPOSAL_ID}/close" POST "" "$HARBOR_TOKEN" || true
  else
    R2_OBSERVED_KEYS="endpoint-${R2_HTTP}"
    R2_STATUS="UNCLEAR"
    _probe_result UNCLEAR "R2: Tally returned HTTP $R2_HTTP — endpoint may not exist yet" \
      "Check if /v1/gov/proposals/{id}/tally is implemented in this build"
  fi
fi

# ── R3: Ledger payment payload ────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " R3: Does POST /v1/ledger/{coop}/payment accept 'decision_id'?"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  NOTE: Deployed binary (2bc85f83, 2026-02-23) uses /payment and 'currency' field."
echo "  Route renamed to /settle + 'unit' in source commit 439a8a2c (2026-03-01)."
echo ""

BRIGHTWORKS_TOKEN="$(demo_get_token alpha 2>/dev/null)" || BRIGHTWORKS_TOKEN=""
if [ -z "$BRIGHTWORKS_TOKEN" ]; then
  R3_STATUS=FAILED; R3_OBSERVED="no-token"
  _probe_result FAILED "R3" "Could not get BrightWorks token"
else
  result "BrightWorks token obtained"

  # Deployed API: RecordTransferRequest = {from, to, amount, currency, memo}
  # decision_id is NOT in the struct. Serde ignores unknown fields by default.
  # Use real DIDs (alpha→gamma) to reach actual validation (self-payment rejected).
  # If 2xx: transfer succeeded, decision_id silently ignored (can keep as no-op in flow-2)
  # If 400 mentioning decision_id: deny_unknown_fields IS set — field explicitly rejected
  # If 400 for self-payment/other: field was ignored, other validation failed
  _do_probe_curl "${BRIGHTWORKS_URL}/v1/ledger/brightworks-cooperative/payment" POST \
    "{\"from\":\"did:icn:zHdQuwTTniwcV4TT1ZcfXsVP7dCojE5czv7vUghmNSmgB\",\"to\":\"did:icn:zyWqWVqGERfRvUz4LVGd4coCZDuNhnufxRpNVTR1BBA7\",\"amount\":1,\"currency\":\"hours\",\"memo\":\"rehearsal-probe-r3\",\"decision_id\":\"00000000-0000-0000-0000-000000000000\"}" \
    "$BRIGHTWORKS_TOKEN"
  R3_HTTP="$PROBE_HTTP_CODE"
  R3_RAW="$(cat "$_RESP_FILE")"

  echo "  Payment endpoint: ${BRIGHTWORKS_URL}/v1/ledger/brightworks-cooperative/payment"
  echo "  HTTP status:      $R3_HTTP"
  echo "  Raw response (always printed — read it):"
  echo "$R3_RAW" | python3 -m json.tool 2>/dev/null || echo "  $R3_RAW"

  # Classify based on BOTH code AND body content.
  R3_BODY_MENTIONS_FIELD=""
  R3_BODY_MENTIONS_AUTH=""
  echo "$R3_RAW" | grep -qi "decision_id\|unknown field\|unrecognized\|invalid field" && R3_BODY_MENTIONS_FIELD=1 || true
  echo "$R3_RAW" | grep -qi "unauthorized\|forbidden\|invalid token\|scope" && R3_BODY_MENTIONS_AUTH=1 || true

  if [[ "$R3_HTTP" =~ ^2 ]]; then
    R3_STATUS=CONFIRMED; R3_OBSERVED="2xx (decision_id silently ignored by Serde)"
    _probe_result CONFIRMED "R3: Payment accepted (2xx) — decision_id silently ignored, not in schema" \
      "Flow-2 payment call works. decision_id is dropped. Can keep or remove."
  elif [ "$R3_HTTP" = "422" ]; then
    R3_STATUS=CONFIRMED; R3_OBSERVED="recognized (422 semantic validation)"
    _probe_result CONFIRMED "R3: Payment returned 422 — field recognized, fake IDs failed semantic validation" \
      "Expected. decision_id field exists in deployed schema."
  elif [ "$R3_HTTP" = "404" ]; then
    # 404 = route not matched (actix http.route=default in logs)
    R3_STATUS=MISMATCH; R3_OBSERVED="404-wrong-route"
    _probe_result MISMATCH "R3: 404 — /payment route not matched. Check binary version." \
      "Binary may predate /payment route or route was registered differently."
  elif [ "$R3_HTTP" = "400" ]; then
    if [ -n "$R3_BODY_MENTIONS_AUTH" ]; then
      R3_STATUS=UNCLEAR; R3_OBSERVED="400-auth (scope issue)"
      _probe_result UNCLEAR "R3: 400 with auth language — scope or token issue, not field question" \
        "Re-run after confirming BRIGHTWORKS_TOKEN has ledger:write scope"
    elif [ -n "$R3_BODY_MENTIONS_FIELD" ]; then
      R3_STATUS=MISMATCH; R3_OBSERVED="rejected (400 unknown field)"
      _probe_result MISMATCH "R3: 400 body explicitly rejects decision_id — field not in schema" \
        "Remove decision_id from flow-2 payment payload."
    else
      R3_STATUS=UNCLEAR; R3_OBSERVED="400-ambiguous"
      _probe_result UNCLEAR "R3: 400 but body doesn't mention decision_id or auth — reason ambiguous" \
        "Body printed above. Classify manually and update flow-2 accordingly."
    fi
  elif [ "$R3_HTTP" = "000" ] || [ "$R3_HTTP" = "503" ] || [ "$R3_HTTP" = "502" ]; then
    R3_STATUS=FAILED; R3_OBSERVED="unreachable-${R3_HTTP}"
    _probe_result FAILED "R3" "Payment endpoint unreachable (HTTP $R3_HTTP)"
  else
    R3_STATUS=UNCLEAR; R3_OBSERVED="unexpected-${R3_HTTP}"
    _probe_result UNCLEAR "R3: Unexpected HTTP $R3_HTTP — body printed above" ""
  fi
fi

# ── R4: Unicode in proposal titles via bash JSON interpolation ─────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " R4: Does ↔ in a proposal title survive bash → curl → API?"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

RIVERCITY_TOKEN="$(demo_get_token beta 2>/dev/null)" || RIVERCITY_TOKEN=""
if [ -z "$RIVERCITY_TOKEN" ]; then
  R4_STATUS=FAILED; R4_OBSERVED="no-token"
  _probe_result FAILED "R4" "Could not get River City token"
else
  result "River City token obtained"
  RIVERCITY_DOMAIN_ID="rivercity-governance"
  # The actual title pattern used in flow-3
  R4_TAG="$(date +%s | tail -c 6)"
  R4_TITLE="Equipment Access ↔ Maintenance ${R4_TAG}"

  _do_probe_curl "${RIVERCITY_URL}/v1/gov/proposals" POST \
    "{\"domain_id\":\"${RIVERCITY_DOMAIN_ID}\",\"title\":\"${R4_TITLE}\",\"description\":\"Rehearsal probe — Unicode title\",\"payload\":{\"type\":\"text\",\"body\":\"Rehearsal probe body — testing Unicode in title.\"}}" \
    "$RIVERCITY_TOKEN"
  R4_HTTP="$PROBE_HTTP_CODE"
  R4_RAW="$(cat "$_RESP_FILE")"

  echo "  HTTP status:      $R4_HTTP"

  if [[ "$R4_HTTP" =~ ^2 ]]; then
    # Retrieve the proposal and check the title came back intact
    R4_ID="$(_field id)"
    _do_probe_curl "${RIVERCITY_URL}/v1/gov/proposals/${R4_ID}" GET "" "$RIVERCITY_TOKEN"
    R4_STORED_TITLE="$(python3 -c "
import json
d = json.load(open('$_RESP_FILE'))
print(d.get('title', d.get('proposal', {}).get('title', '<not found>')))
" 2>/dev/null || echo "<parse failed>")"

    echo "  Sent title:       $R4_TITLE"
    echo "  Retrieved title:  $R4_STORED_TITLE"

    if echo "$R4_STORED_TITLE" | grep -q "↔"; then
      R4_STATUS=CONFIRMED; R4_OBSERVED="preserved"
      _probe_result CONFIRMED "R4: ↔ survived round-trip through bash JSON interpolation" ""
    else
      R4_STATUS=MISMATCH; R4_OBSERVED="garbled (received: ${R4_STORED_TITLE})"
      _probe_result MISMATCH "R4: ↔ did not appear in stored title" \
        "Flow-3 proposal titles that use ↔ will arrive garbled. Use ASCII alternative: <->"
    fi

    # Close probe proposal
    _do_probe_curl "${RIVERCITY_URL}/v1/gov/proposals/${R4_ID}/open" POST "{}" "$RIVERCITY_TOKEN" || true
    _do_probe_curl "${RIVERCITY_URL}/v1/gov/proposals/${R4_ID}/close" POST "" "$RIVERCITY_TOKEN" || true
  elif [[ "$R4_HTTP" =~ ^4 ]]; then
    echo "  Raw response:"
    echo "$R4_RAW" | python3 -m json.tool 2>/dev/null || echo "  $R4_RAW"
    R4_STATUS=FAILED; R4_OBSERVED="create-${R4_HTTP}"
    _probe_result FAILED "R4" "Proposal creation returned $R4_HTTP — domain '$RIVERCITY_DOMAIN_ID' may not exist"
  else
    R4_STATUS=FAILED; R4_OBSERVED="unreachable-${R4_HTTP}"
    _probe_result FAILED "R4" "HTTP $R4_HTTP — endpoint unreachable"
  fi
fi

# ── R5: Federation status shape when module is uninitialized ──────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " R5: What does GET /v1/federation/status return on each node?"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

FINGERLAKES_TOKEN="$(demo_get_token delta 2>/dev/null)" || FINGERLAKES_TOKEN=""

declare -A R5_NAMES=([alpha]="BrightWorks" [beta]="River City" [gamma]="Harbor Homes" [delta]="Finger Lakes CDN")
declare -A R5_URLS=([alpha]="$BRIGHTWORKS_URL" [beta]="$RIVERCITY_URL" [gamma]="$HARBOR_URL" [delta]="$FINGERLAKES_URL")
declare -A R5_TOKENS=([alpha]="${BRIGHTWORKS_TOKEN:-}" [beta]="${RIVERCITY_TOKEN:-}" [gamma]="$HARBOR_TOKEN" [delta]="${FINGERLAKES_TOKEN:-}")

# Two separate observations for R5:
# R5a — does the endpoint exist? (404 = endpoint missing; 2xx or 401/403 = endpoint present)
# R5b — if endpoint exists, does any node show initialized federation state?
# These are NOT the same question. Consistent shape ≠ meaningful federation state.
R5_ENDPOINTS_EXIST=0     # count of nodes where endpoint returned non-404
R5_ANY_INITIALIZED=0    # set if any node returns fields suggesting active federation
R5_404_COUNT=0
R5_OBSERVED_SUMMARY=""

for node in alpha beta gamma delta; do
  coop_name="${R5_NAMES[$node]}"
  coop_url="${R5_URLS[$node]}"
  coop_token="${R5_TOKENS[$node]:-}"

  echo "  --- ${coop_name} (${coop_url}) ---"

  if [ -z "$coop_token" ]; then
    echo "  (no token — skipping)"
    R5_OBSERVED_SUMMARY="${R5_OBSERVED_SUMMARY} ${node}:no-token"
    continue
  fi

  _do_probe_curl "${coop_url}/v1/federation/status" GET "" "$coop_token"
  R5_HTTP="$PROBE_HTTP_CODE"

  if [[ "$R5_HTTP" =~ ^2 ]]; then
    R5_ENDPOINTS_EXIST=$(( R5_ENDPOINTS_EXIST + 1 ))
    # Show raw response — presenter reads it
    echo "  HTTP: $R5_HTTP"
    python3 -m json.tool < "$_RESP_FILE" 2>/dev/null || cat "$_RESP_FILE" | head -10
    # Check whether response indicates active federation (has members, treaties, peers, etc.)
    IS_INITIALIZED="$(python3 -c "
import json, sys
INIT_SIGNALS = {'members', 'treaties', 'peers', 'federation_id', 'participants', 'agreements', 'active'}
d = json.load(open('$_RESP_FILE'))
flat_keys = set(d.keys())
if isinstance(d, dict):
    for v in d.values():
        if isinstance(v, dict):
            flat_keys.update(v.keys())
hit = flat_keys & INIT_SIGNALS
print('yes:' + ','.join(sorted(hit)) if hit else 'no')
" 2>/dev/null || echo "unknown")"
    echo "  Initialized signals: $IS_INITIALIZED"
    if [[ "$IS_INITIALIZED" == yes:* ]]; then
      R5_ANY_INITIALIZED=1
    fi
    R5_OBSERVED_SUMMARY="${R5_OBSERVED_SUMMARY} ${node}:2xx(${IS_INITIALIZED})"
  elif [ "$R5_HTTP" = "404" ]; then
    R5_404_COUNT=$(( R5_404_COUNT + 1 ))
    echo "  HTTP: 404 — endpoint absent or federation module not initialized"
    R5_OBSERVED_SUMMARY="${R5_OBSERVED_SUMMARY} ${node}:404"
  elif [ "$R5_HTTP" = "401" ] || [ "$R5_HTTP" = "403" ]; then
    R5_ENDPOINTS_EXIST=$(( R5_ENDPOINTS_EXIST + 1 ))
    echo "  HTTP: $R5_HTTP — endpoint exists, authorization issue (scope may not cover federation:read)"
    R5_OBSERVED_SUMMARY="${R5_OBSERVED_SUMMARY} ${node}:${R5_HTTP}-auth"
  else
    echo "  HTTP: $R5_HTTP — $(cat "$_RESP_FILE" | head -c 200)"
    R5_OBSERVED_SUMMARY="${R5_OBSERVED_SUMMARY} ${node}:${R5_HTTP}"
  fi
  echo ""
done

echo "  Observation A — endpoint existence: ${R5_ENDPOINTS_EXIST}/4 nodes answered (non-404)"
echo "  Observation B — federation initialized: $([ "$R5_ANY_INITIALIZED" = "1" ] && echo "YES (at least one node)" || echo "NO")"
echo ""

if [ "$R5_ENDPOINTS_EXIST" = "0" ]; then
  # All 404 — endpoint absent or module not initialized anywhere
  R5a_STATUS=CONFIRMED; R5b_STATUS=CONFIRMED; R5_OBSERVED="all-404"
  _probe_result CONFIRMED "R5a: Endpoint absent on all nodes (all 404) — flow-3 aside is correct" ""
  _probe_result CONFIRMED "R5b: No initialized federation state — flow-3 narration handles this" ""
elif [ "$R5_ANY_INITIALIZED" = "1" ]; then
  R5a_STATUS=CONFIRMED; R5b_STATUS=CONFIRMED; R5_OBSERVED="exists+initialized"
  _probe_result CONFIRMED "R5a: Endpoint exists ($R5_ENDPOINTS_EXIST/4 nodes)" ""
  _probe_result CONFIRMED "R5b: Federation module shows initialized state on at least one node" \
    "Flow-3 step 2 aside should be updated to reflect actual initialized fields"
else
  # Endpoint exists but no initialization signals found
  R5a_STATUS=CONFIRMED; R5b_STATUS=UNCLEAR; R5_OBSERVED="exists+not-initialized"
  _probe_result CONFIRMED "R5a: Endpoint exists ($R5_ENDPOINTS_EXIST/4 nodes)" ""
  _probe_result UNCLEAR "R5b: Endpoint returns 2xx but no initialization signals recognized" \
    "Flow-3 aside is still defensible. Check raw output above for actual field names."
fi

# ── Operator table ────────────────────────────────────────────────────────────
echo ""
echo "================================================================"
echo " REHEARSAL PROBE — GO/NO-GO OPERATOR TABLE"
echo "================================================================"
echo ""
printf "  %-5s  %-11s  %-32s  %s\n" "Probe" "Status" "Observed" "Meaning"
printf "  %-5s  %-11s  %-32s  %s\n" "-----" "-----------" "--------------------------------" "----------------------------------------"
printf "  %-5s  %-11s  %-32s  %s\n" "R1"  "${R1_STATUS}"  "${R1_OBSERVED_STATE}"              "Single-DID quorum → final state"
printf "  %-5s  %-11s  %-32s  %s\n" "R2"  "${R2_STATUS}"  "${R2_OBSERVED_KEYS:-not-run}"      "Tally field names (pin flow-1 grep after)"
printf "  %-5s  %-11s  %-32s  %s\n" "R3"  "${R3_STATUS}"  "${R3_OBSERVED}"                    "Payment (/payment+currency): decision_id silently dropped?"
printf "  %-5s  %-11s  %-32s  %s\n" "R4"  "${R4_STATUS}"  "${R4_OBSERVED}"                    "Unicode ↔ survives bash → API"
printf "  %-5s  %-11s  %-32s  %s\n" "R5a" "${R5a_STATUS}" "${R5_OBSERVED}"                    "Federation endpoint: present?"
printf "  %-5s  %-11s  %-32s  %s\n" "R5b" "${R5b_STATUS}" "(see R5 above)"                    "Federation module: initialized?"
echo ""
echo -e "  ${GREEN}✓ CONFIRMED${NC}   $_CONFIRMED"
echo -e "  ${YELLOW}? UNCLEAR${NC}     $_UNCLEAR"
echo -e "  ${RED}✗ MISMATCH${NC}    $_MISMATCH"
echo -e "  ${RED}✗ FAILED${NC}      $_FAILED"
echo ""

if [ "$_MISMATCH" -gt 0 ] || [ "$_FAILED" -gt 0 ]; then
  echo "  VERDICT: NOT READY — address MISMATCH and FAILED items before demoing."
  echo "  Each has a note above on how to adapt. MISMATCHes are script fixes, not blockers."
elif [ "$_UNCLEAR" -gt 0 ]; then
  echo "  VERDICT: REVIEW REQUIRED — read UNCLEAR items and judge manually."
  echo "  If the raw output is consistent with flow script assumptions, proceed."
else
  echo "  VERDICT: GO — all probes confirmed. Scripts are ready for live demo."
fi
echo ""
echo "  After probing: run reseed-federation-demo.sh to clear probe proposals."
echo ""
