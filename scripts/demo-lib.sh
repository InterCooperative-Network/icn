#!/usr/bin/env bash
# ICN Demo Library — shared helpers for demo scripts
# Source this file: . "$(dirname "$0")/demo-lib.sh"

set -euo pipefail

GATEWAY="${ICN_GATEWAY:-http://localhost:8000}"
TOKEN="${ICN_TOKEN:-}"
AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# --- State ---
_LAST_CMD=""
_LAST_BODY=""
_SECTION_RESULTS=()
_SECTION_START=0
_CURRENT_SECTION=""

# --- Output helpers ---
step()  { echo -e "${CYAN}  ▸ $1${NC}"; }
ok()    { echo -e "${GREEN}  ✓ $1${NC}"; }
fail()  { echo -e "${RED}  ✗ $1${NC}"; return 1; }
skip()  { echo -e "${YELLOW}  ⊘ $1${NC}"; }
header(){ echo -e "\n${BOLD}━━━ $1 ━━━${NC}"; }

# --- Trap: print context on failure ---
_on_error() {
  local exit_code=$?
  echo -e "${RED}  ✗ Command failed (exit $exit_code)${NC}"
  if [ -n "$_LAST_CMD" ]; then
    echo -e "${DIM}    cmd: $_LAST_CMD${NC}"
  fi
  if [ -n "$_LAST_BODY" ]; then
    echo -e "${DIM}    response: $(echo "$_LAST_BODY" | head -c 500)${NC}"
  fi
}
trap '_on_error' ERR

# --- HTTP helpers ---
# All return the response body and set _LAST_BODY. Non-2xx → return 1.
http_get() {
  local url="$1"
  _LAST_CMD="GET $url"
  _LAST_BODY=$(curl -sf --max-time 10 "$url" ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) || {
    _LAST_BODY="${_LAST_BODY:-<no response>}"
    return 1
  }
  echo "$_LAST_BODY"
}

http_post() {
  local url="$1"
  local data="${2:-}"
  _LAST_CMD="POST $url"
  if [ -n "$data" ]; then
    _LAST_BODY=$(curl -sf --max-time 10 -X POST "$url" \
      -H "Content-Type: application/json" \
      ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
      -d "$data" 2>&1) || { _LAST_BODY="${_LAST_BODY:-<no response>}"; return 1; }
  else
    _LAST_BODY=$(curl -sf --max-time 10 -X POST "$url" \
      ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) || { _LAST_BODY="${_LAST_BODY:-<no response>}"; return 1; }
  fi
  echo "$_LAST_BODY"
}

http_put() {
  local url="$1"
  local data="${2:-}"
  _LAST_CMD="PUT $url"
  _LAST_BODY=$(curl -sf --max-time 10 -X PUT "$url" \
    -H "Content-Type: application/json" \
    ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
    -d "$data" 2>&1) || { _LAST_BODY="${_LAST_BODY:-<no response>}"; return 1; }
  echo "$_LAST_BODY"
}

http_delete() {
  local url="$1"
  _LAST_CMD="DELETE $url"
  _LAST_BODY=$(curl -sf --max-time 10 -X DELETE "$url" \
    ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) || { _LAST_BODY="${_LAST_BODY:-<no response>}"; return 1; }
  echo "$_LAST_BODY"
}

http_status_code() {
  local method="$1"
  local url="$2"
  curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -X "$method" "$url" ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>/dev/null
}

# --- JSON helpers (require jq) ---
jq_field() {
  local json="$1"
  local field="$2"
  echo "$json" | jq -r "$field" 2>/dev/null
}

expect_field() {
  local json="$1"
  local field="$2"
  local label="${3:-$field}"
  local val
  val=$(jq_field "$json" "$field")
  if [ -z "$val" ] || [ "$val" = "null" ]; then
    fail "Missing expected field $label"
  fi
  echo "$val"
}

# --- Section tracking ---
section_start() {
  _CURRENT_SECTION="$1"
  _SECTION_START=$(date +%s%N 2>/dev/null || date +%s)
  header "$1"
}

section_end() {
  local status="$1" # pass, fail, skip
  local now
  now=$(date +%s%N 2>/dev/null || date +%s)
  local duration_ms=$(( (now - _SECTION_START) / 1000000 ))
  _SECTION_RESULTS+=("${_CURRENT_SECTION}|${status}|${duration_ms}")
}

# --- Deterministic IDs ---
demo_id() {
  local prefix="$1"
  echo "${prefix}-$(date +%s)-${RANDOM}"
}

# --- Summary ---
print_summary() {
  echo ""
  echo -e "${BOLD}━━━ Summary ━━━${NC}"
  local passed=0 failed=0 skipped=0
  for entry in "${_SECTION_RESULTS[@]}"; do
    IFS='|' read -r name status duration_ms <<< "$entry"
    case "$status" in
      pass)    echo -e "  ${GREEN}✓${NC} $name ${DIM}(${duration_ms}ms)${NC}"; passed=$((passed+1)) ;;
      fail)    echo -e "  ${RED}✗${NC} $name ${DIM}(${duration_ms}ms)${NC}"; failed=$((failed+1)) ;;
      skip)    echo -e "  ${YELLOW}⊘${NC} $name"; skipped=$((skipped+1)) ;;
    esac
  done
  echo ""
  local total=$((passed + failed + skipped))
  echo -e "  ${passed} passed, ${failed} failed, ${skipped} skipped (${total} total)"

  if [ "$failed" -gt 0 ]; then
    return 1
  fi
}

print_summary_json() {
  echo -n '{"sections":['
  local first=true
  for entry in "${_SECTION_RESULTS[@]}"; do
    IFS='|' read -r name status duration_ms <<< "$entry"
    if [ "$first" = true ]; then first=false; else echo -n ','; fi
    echo -n "{\"name\":\"$name\",\"status\":\"$status\",\"duration_ms\":$duration_ms}"
  done
  echo ']}'
}
