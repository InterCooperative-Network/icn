#!/usr/bin/env bash
# lib-demo-ports.sh — Shared port-forward library for ICN federation demo
#
# MUST BE SOURCED, not executed directly:
#   source demo/scripts/lib-demo-ports.sh
#
# Provides:
#   demo_ports_up          — start kubectl port-forwards for all 4 coop gateways
#   demo_ports_down        — kill all port-forwards started by demo_ports_up
#   demo_wait_ready [sec]  — poll until all 4 gateways return HTTP 200 /v1/health
#   demo_get_token <coop>  — fetch JWT via icnctl inside pod (alpha/beta/gamma/delta)
#   demo_curl <url> <method> [body] [token] — authenticated curl wrapper
#   demo_require_2xx <ctx> — assert DEMO_LAST_HTTP_CODE is 2xx or exit 1
#   narrate/result/aside/warn/fail — colored output helpers
#
# Token strategy: icnctl is at /usr/local/bin/icnctl inside each pod.
# Passphrase is read dynamically from K8s Secret icn-{coop}-secrets[passphrase].
# Tokens are cached in _DEMO_TOKEN_{COOP} after first successful fetch.
# No secrets are hardcoded anywhere in this file.

# ---------------------------------------------------------------------------
# Guard: warn if caller doesn't have pipefail/nounset active
# ---------------------------------------------------------------------------
if [[ "$-" != *e* ]]; then
  echo "[lib-demo-ports] WARNING: caller does not have 'set -e' active" >&2
fi

# ---------------------------------------------------------------------------
# Colors (only when stdout is a terminal)
# ---------------------------------------------------------------------------
if [ -t 1 ]; then
  BLUE='\033[0;34m'
  GREEN='\033[0;32m'
  YELLOW='\033[1;33m'
  RED='\033[0;31m'
  NC='\033[0m'
else
  BLUE=''
  GREEN=''
  YELLOW=''
  RED=''
  NC=''
fi

# ---------------------------------------------------------------------------
# Exported configuration variables
# ---------------------------------------------------------------------------

# Local gateway URLs (via kubectl port-forward)
export BRIGHTWORKS_URL="http://localhost:18081"
export RIVERCITY_URL="http://localhost:18082"
export HARBOR_URL="http://localhost:18083"
export FINGERLAKES_URL="http://localhost:18084"

# Cooperative IDs (as registered in the ICN daemon)
export BRIGHTWORKS_COOP_ID="brightworks-cooperative"
export RIVERCITY_COOP_ID="river-city-tool-library"
export HARBOR_COOP_ID="harbor-homes-cooperative"
export FINGERLAKES_COOP_ID="fingerlakes-cdn"

# Kubernetes namespaces
export BRIGHTWORKS_NS="icn-coop-alpha"
export RIVERCITY_NS="icn-coop-beta"
export HARBOR_NS="icn-coop-gamma"
export FINGERLAKES_NS="icn-coop-delta"

# gRPC NodePorts (on K3s control plane 10.8.30.40)
export BRIGHTWORKS_GRPC="10.8.30.40:30651"
export RIVERCITY_GRPC="10.8.30.40:30658"
export HARBOR_GRPC="10.8.30.40:30649"
export FINGERLAKES_GRPC="10.8.30.40:30655"

# Default scopes to request for demo tokens
export DEMO_DEFAULT_SCOPES="ledger:read,ledger:write,coop:read,coop:write,governance:read,governance:write,settlements:read,settlements:write,treasury:read,treasury:write,federation:read,federation:write,federation:admin"

# HTTP status code from the last demo_curl call
export DEMO_LAST_HTTP_CODE=""

# Internal: temp file holding the last demo_curl response body (for _pretty)
_DEMO_LAST_RESP_FILE=""

# ---------------------------------------------------------------------------
# Presenter mode — set by flow scripts that parse their own args
# PRESENTER_MODE = "" (normal) | "present" (keypress) | "narrated" (auto)
# DEMO_BEAT_PAUSE = seconds between beats in narrated mode (default 4)
# PRESENTER_LOG = file path for technical detail output
# ---------------------------------------------------------------------------
export PRESENTER_MODE="${PRESENTER_MODE:-}"
export DEMO_BEAT_PAUSE="${DEMO_BEAT_PAUSE:-4}"
export PRESENTER_LOG="${PRESENTER_LOG:-/tmp/icn-demo-presenter-$$.log}"

# Initialize log file
if [ -n "$PRESENTER_MODE" ]; then
  : > "$PRESENTER_LOG"
fi

# Internal: PID array for port-forwards started by demo_ports_up
_DEMO_PF_PIDS=()

# Internal: file-based token cache directory (survives subshell boundaries)
_DEMO_TOKEN_CACHE_DIR="${TMPDIR:-/tmp}/icn-demo-tokens-$$"
mkdir -p "$_DEMO_TOKEN_CACHE_DIR"

# ---------------------------------------------------------------------------
# Narration helpers
# ---------------------------------------------------------------------------

# _strip_technical: remove HTTP codes and DID noise from a string
# Used by result/warn/narrate in presenter mode to keep output clean.
_strip_technical() {
  sed -e 's/ (HTTP [0-9]\{3\})//g' \
      -e 's/HTTP:[0-9]\{3\}//g'
}

# narrate: print a section header / step announcement
narrate() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo -e "\n${BLUE}▶${NC} $*\n" | _strip_technical
  else
    echo -e "\n${BLUE}▶${NC} $*\n"
  fi
}

# result: print a success line
result() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo -e "  ${GREEN}✓${NC} $*" | _strip_technical
  else
    echo -e "  ${GREEN}✓${NC} $*"
  fi
}

# aside: in normal mode print inline; in presenter mode redirect to log only
aside() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo "  → $*" >> "${PRESENTER_LOG:-/dev/null}"
  else
    echo -e "  ${YELLOW}→${NC} $*"
  fi
}

# warn: print a warning line (non-fatal)
warn() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo -e "  ${YELLOW}⚠${NC} $*" | _strip_technical
  else
    echo -e "  ${YELLOW}⚠${NC} $*"
  fi
}

# fail: print a failure line
fail() { echo -e "  ${RED}✗${NC} $*"; }

# _pretty: pretty-print the last curl response body
# In presenter mode, redirects to the log instead of showing on screen.
_pretty() {
  local src="${_DEMO_LAST_RESP_FILE:-/dev/null}"
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo "--- response body ---" >> "${PRESENTER_LOG:-/dev/null}"
    python3 -m json.tool < "$src" >> "${PRESENTER_LOG:-/dev/null}" 2>/dev/null \
      || cat "$src" >> "${PRESENTER_LOG:-/dev/null}"
  else
    python3 -m json.tool < "$src" 2>/dev/null || cat "$src"
  fi
}

# ---------------------------------------------------------------------------
# _beat [presenter-note]
# Pause point between story beats.
#   --present mode: wait for keypress; ? shows presenter note
#   --narrated mode: sleep DEMO_BEAT_PAUSE seconds
#   normal mode: no-op
# ---------------------------------------------------------------------------
_beat() {
  local note="${1:-}"
  if [ "${PRESENTER_MODE:-}" = "present" ]; then
    echo ""
    if [ -n "$note" ]; then
      printf "  \033[2m[ any key to continue — ? for notes ]\033[0m"
    else
      printf "  \033[2m[ any key to continue ]\033[0m"
    fi
    while IFS= read -r -s -n1 key 2>/dev/null; do
      if [ "$key" = "?" ] && [ -n "$note" ]; then
        echo ""
        echo ""
        echo -e "  ${YELLOW}📋 PRESENTER:${NC} $note"
        echo ""
        printf "  \033[2m[ any key to continue ]\033[0m"
      else
        break
      fi
    done
    echo ""
  elif [ "${PRESENTER_MODE:-}" = "narrated" ]; then
    sleep "${DEMO_BEAT_PAUSE:-4}"
  fi
  # normal mode: no-op
}

# ---------------------------------------------------------------------------
# demo_ports_up
# Start kubectl port-forwards for all 4 coop gateways in background.
# Logs go to /tmp/pf-{port}.log. Registers EXIT/INT/TERM trap.
# ---------------------------------------------------------------------------
demo_ports_up() {
  aside "Starting port-forwards for all 4 coop gateways..."

  kubectl port-forward -n icn-coop-alpha svc/icn-alpha 18081:8080 \
    >/tmp/pf-18081.log 2>&1 &
  _DEMO_PF_PIDS+=($!)

  kubectl port-forward -n icn-coop-beta svc/icn-beta 18082:8080 \
    >/tmp/pf-18082.log 2>&1 &
  _DEMO_PF_PIDS+=($!)

  kubectl port-forward -n icn-coop-gamma svc/icn-gamma 18083:8080 \
    >/tmp/pf-18083.log 2>&1 &
  _DEMO_PF_PIDS+=($!)

  kubectl port-forward -n icn-coop-delta svc/icn-delta 18084:8080 \
    >/tmp/pf-18084.log 2>&1 &
  _DEMO_PF_PIDS+=($!)

  # Give kubectl processes time to bind their ports before demo_wait_ready polls
  sleep 1

  # Chain with any existing EXIT trap so we don't overwrite caller's cleanup
  local _existing_exit_trap
  _existing_exit_trap=$(trap -p EXIT 2>/dev/null | sed "s/trap -- '//;s/' EXIT//")
  if [ -n "$_existing_exit_trap" ]; then
    trap "demo_ports_down; $_existing_exit_trap" EXIT INT TERM
  else
    trap demo_ports_down EXIT INT TERM
  fi

  aside "Port-forward PIDs: ${_DEMO_PF_PIDS[*]:-none}"
}

# ---------------------------------------------------------------------------
# demo_ports_down
# Kill all port-forwards started by demo_ports_up. Silently ignores dead PIDs.
# ---------------------------------------------------------------------------
demo_ports_down() {
  if [ ${#_DEMO_PF_PIDS[@]} -eq 0 ]; then
    return 0
  fi

  aside "Stopping port-forwards (PIDs: ${_DEMO_PF_PIDS[*]})..."
  for pid in "${_DEMO_PF_PIDS[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  # Wait for all to exit so ports are released before returning
  for pid in "${_DEMO_PF_PIDS[@]}"; do
    wait "$pid" 2>/dev/null || true
  done
  _DEMO_PF_PIDS=()
  # Clean token cache
  rm -rf "${_DEMO_TOKEN_CACHE_DIR:-}" 2>/dev/null || true
  # Clean last curl response temp file
  rm -f "${_DEMO_LAST_RESP_FILE:-}" 2>/dev/null || true
  _DEMO_LAST_RESP_FILE=""
}

# ---------------------------------------------------------------------------
# demo_wait_ready [max_wait_seconds]
# Poll all 4 gateways until they return HTTP 200 on GET /v1/health.
# Default timeout: 30 seconds. Returns non-zero if any gateway is still down.
# ---------------------------------------------------------------------------
demo_wait_ready() {
  local max_wait="${1:-30}"
  local deadline=$(( $(date +%s) + max_wait ))

  # Track which coops are ready using parallel indexed arrays (bash 3.2 compatible)
  local -a coop_names=("BrightWorks" "River City Tool Library" "Harbor Homes" "Finger Lakes CDN")
  local -a coop_urls=("$BRIGHTWORKS_URL" "$RIVERCITY_URL" "$HARBOR_URL" "$FINGERLAKES_URL")
  local -a coop_ready=(0 0 0 0)

  aside "Waiting up to ${max_wait}s for all 4 gateways..."

  while [ $(date +%s) -lt "$deadline" ]; do
    for i in 0 1 2 3; do
      if [ "${coop_ready[$i]}" = "1" ]; then
        continue
      fi
      local url="${coop_urls[$i]}"
      local code
      code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 2 \
        "${url}/v1/health" 2>/dev/null; true)
      if [ "$code" = "200" ]; then
        coop_ready[$i]="1"
        result "${coop_names[$i]} gateway ready (${url})"
      fi
    done

    # Check if all 4 are ready
    local all_ready=1
    for i in 0 1 2 3; do
      if [ "${coop_ready[$i]}" != "1" ]; then
        all_ready=0
        break
      fi
    done
    if [ "$all_ready" = "1" ]; then
      return 0
    fi

    sleep 1
  done

  # Report which ones failed
  for i in 0 1 2 3; do
    if [ "${coop_ready[$i]}" != "1" ]; then
      fail "${coop_names[$i]} gateway did not become ready within ${max_wait}s"
    fi
  done
  return 1
}

# ---------------------------------------------------------------------------
# demo_api_preflight
#
# Print the deployed gateway binary SHA/version and report which ledger API
# surface is active. Call this after demo_wait_ready for any flow that
# touches the ledger.
#
# API surface detection uses the git SHA from /v1/health. actix-web scope
# middleware intercepts ALL requests matching /ledger prefix before route
# lookup, so unauthenticated probes return 401 for both existing and
# non-existing routes — making 401-vs-404 unreliable as a detection signal.
# SHA-based lookup is exact and requires no token.
#
# Known SHAs:
#   2bc85f83*  (2026-02-23) → OLD API  /payment + currency + /balance/{did}
#   commit 439a8a2c (2026-03-01) renamed to:
#   any newer SHA             → NEW API  /settle  + unit     + /position/{did}
#
# Exports: DEMO_LEDGER_MODE = "old" | "new" | "unknown"
# ---------------------------------------------------------------------------
demo_api_preflight() {
  local health git_sha build_time version
  health=$(curl -s --connect-timeout 3 "${BRIGHTWORKS_URL}/v1/health" 2>/dev/null || true)
  git_sha=$(echo "$health" | python3 -c \
    "import sys,json; d=json.load(sys.stdin); print(d.get('git_sha','unknown'))" 2>/dev/null \
    || echo "unknown")
  build_time=$(echo "$health" | python3 -c \
    "import sys,json; d=json.load(sys.stdin); print(d.get('build_time','unknown')[:10])" 2>/dev/null \
    || echo "unknown")
  version=$(echo "$health" | python3 -c \
    "import sys,json; d=json.load(sys.stdin); print(d.get('version','unknown'))" 2>/dev/null \
    || echo "unknown")

  # SHA-based API surface detection.
  # Binaries predating commit 439a8a2c (2026-03-01) use old route names.
  # The rename commit changed: /payment→/settle, currency→unit, /balance→/position.
  local ledger_mode_label
  case "$git_sha" in
    2bc85f83*)
      ledger_mode_label="OLD  /payment + currency + /balance/{did}  (sha=${git_sha:0:8})"
      export DEMO_LEDGER_MODE="old"
      ;;
    unknown)
      ledger_mode_label="UNKNOWN — health endpoint unreachable"
      export DEMO_LEDGER_MODE="unknown"
      ;;
    *)
      # Binary is newer than the last known-old SHA.
      # If flow-2 settlement fails, re-run rehearsal-probe.sh to re-pin.
      ledger_mode_label="NEW  /settle + unit + /position/{did}  (sha=${git_sha:0:8})"
      export DEMO_LEDGER_MODE="new"
      ;;
  esac

  echo ""
  echo "  ┌────────────────────────────────────────────────────────────┐"
  echo "  │  GATEWAY PREFLIGHT                                         │"
  printf "  │  binary:  sha=%-45s│\n" "${git_sha} (${build_time})"
  printf "  │  version: %-48s│\n" "${version}"
  printf "  │  ledger:  %-48s│\n" "${ledger_mode_label}"
  echo "  └────────────────────────────────────────────────────────────┘"
  echo ""

  if [ "$DEMO_LEDGER_MODE" = "unknown" ]; then
    warn "Health endpoint unreachable — cannot determine API surface."
    warn "Verify gateway is up, then re-run."
  elif [ "$DEMO_LEDGER_MODE" = "new" ]; then
    warn "Binary is newer than flow-2 was last rehearsed against (sha 2bc85f83)."
    warn "If settlement fails, run: bash scripts/rehearsal-probe.sh"
    warn "then update flow-2 route/field names to match the new API surface."
  fi
}

# ---------------------------------------------------------------------------
# _demo_get_passphrase <namespace> <secret-name>
# Internal helper: read passphrase from K8s Secret. Never echoes the value.
# Sets _DEMO_PASSPHRASE variable.
# ---------------------------------------------------------------------------
_demo_get_passphrase() {
  local ns="$1"
  local secret="$2"
  _DEMO_PASSPHRASE=$(kubectl get secret "$secret" -n "$ns" \
    -o jsonpath='{.data.passphrase}' 2>/dev/null | base64 -d)
  if [ -z "${_DEMO_PASSPHRASE:-}" ]; then
    fail "Could not read passphrase from secret $secret in $ns"
    return 1
  fi
}

# NOTE: Tokens are cached per coop for the duration of the demo session.
# Default ICN JWT lifetime is 1 hour (3600s). If a demo runs longer,
# call: demo_get_token --refresh <coop> to force a fresh token.
# Stale token symptoms: 401 responses from demo_curl calls.

# ---------------------------------------------------------------------------
# demo_get_token [--refresh] <coop_name>
# Fetch a JWT auth token for the given coop (alpha/beta/gamma/delta).
#
# Strategy: run icnctl auth token inside the pod via kubectl exec.
# icnctl is at /usr/local/bin/icnctl in all 4 pods (confirmed in Task 1).
# Passphrase is read dynamically from K8s Secret at call time.
# Token is cached in _DEMO_TOKEN_{COOP} after first successful fetch.
#
# Pass --refresh as first argument to force a fresh token (clears cache).
#
# Returns the token via stdout.
# Also sets DEMO_TOKEN_ALPHA / _BETA / _GAMMA / _DELTA.
# ---------------------------------------------------------------------------
demo_get_token() {
  # Handle optional --refresh flag
  local _refresh=0
  if [ "${1:-}" = "--refresh" ]; then
    _refresh=1
    shift
  fi

  local coop="${1:-}"
  if [ -z "$coop" ]; then
    fail "demo_get_token: coop name required (alpha/beta/gamma/delta)"
    return 1
  fi

  # File-based cache: survives subshell $(...) calls unlike shell variables
  local cache_file="${_DEMO_TOKEN_CACHE_DIR}/token-${coop}"

  # --refresh: delete the cached file to force a fresh fetch
  if [ "$_refresh" = "1" ]; then
    rm -f "$cache_file"
  fi

  if [ -f "$cache_file" ] && [ -s "$cache_file" ]; then
    cat "$cache_file"
    return 0
  fi

  local ns deploy secret coop_id
  case "$coop" in
    alpha)
      ns="icn-coop-alpha"; deploy="icn-alpha"
      secret="icn-alpha-secrets"; coop_id="$BRIGHTWORKS_COOP_ID"
      ;;
    beta)
      ns="icn-coop-beta"; deploy="icn-beta"
      secret="icn-beta-secrets"; coop_id="$RIVERCITY_COOP_ID"
      ;;
    gamma)
      ns="icn-coop-gamma"; deploy="icn-gamma"
      secret="icn-gamma-secrets"; coop_id="$HARBOR_COOP_ID"
      ;;
    delta)
      ns="icn-coop-delta"; deploy="icn-delta"
      secret="icn-delta-secrets"; coop_id="$FINGERLAKES_COOP_ID"
      ;;
    *)
      fail "demo_get_token: unknown coop '$coop' (must be alpha/beta/gamma/delta)"
      return 1
      ;;
  esac

  # Get passphrase from K8s Secret (not hardcoded)
  _demo_get_passphrase "$ns" "$secret" || return 1

  # Fetch token via icnctl inside the pod
  local token
  token=$(kubectl exec -n "$ns" "deploy/$deploy" -- \
    env ICN_PASSPHRASE="$_DEMO_PASSPHRASE" \
    /usr/local/bin/icnctl auth token \
    --coop-id "$coop_id" \
    --scopes "$DEMO_DEFAULT_SCOPES" \
    2>/dev/null | grep -oE 'eyJ[A-Za-z0-9_.-]+\.[A-Za-z0-9_.-]+\.[A-Za-z0-9_.-]+' | head -1 || true)

  # Clear passphrase from memory
  _DEMO_PASSPHRASE=""

  if [ -z "$token" ]; then
    fail "demo_get_token: failed to obtain token for coop '$coop'"
    fail "  Checked: kubectl exec -n $ns deploy/$deploy -- /usr/local/bin/icnctl auth token"
    return 1
  fi

  # Write to file-based cache (readable by subshells on subsequent calls)
  echo "$token" > "$cache_file"

  echo "$token"
}

# ---------------------------------------------------------------------------
# demo_curl <url> <method> [json_body] [token]
# Authenticated curl wrapper.
#   - token: uses $DEMO_TOKEN env var if omitted
#   - Sets DEMO_LAST_HTTP_CODE to the HTTP status code
#   - Returns response body via stdout
# ---------------------------------------------------------------------------
demo_curl() {
  DEMO_LAST_HTTP_CODE=""

  local url="${1:-}"
  local method="${2:-GET}"
  local body="${3:-}"
  local token="${4:-${DEMO_TOKEN:-}}"

  if [ -z "$url" ]; then
    fail "demo_curl: url required"
    return 1
  fi

  local _tmp_body
  _tmp_body=$(mktemp)

  local curl_args=(-s -o "$_tmp_body" -w "%{http_code}")
  curl_args+=(-X "$method")
  curl_args+=(-H "Accept: application/json")

  if [ -n "$token" ]; then
    curl_args+=(-H "Authorization: Bearer $token")
  fi

  if [ -n "$body" ]; then
    curl_args+=(-H "Content-Type: application/json")
    curl_args+=(-d "$body")
  fi

  DEMO_LAST_HTTP_CODE=$(curl "${curl_args[@]}" "$url" 2>/dev/null || echo "000")
  export DEMO_LAST_HTTP_CODE

  # Remove previous response file and expose current one for _pretty
  rm -f "${_DEMO_LAST_RESP_FILE:-}"
  _DEMO_LAST_RESP_FILE="$_tmp_body"

  local _body
  _body=$(cat "$_tmp_body" 2>/dev/null || true)
  echo "$_body"
}

# ---------------------------------------------------------------------------
# demo_require_2xx <context>
# Assert that DEMO_LAST_HTTP_CODE is 2xx. Exit 1 with message on failure.
# ---------------------------------------------------------------------------
demo_require_2xx() {
  local ctx="${1:-unknown}"
  local code="${DEMO_LAST_HTTP_CODE:-000}"

  if [[ "$code" =~ ^2[0-9][0-9]$ ]]; then
    return 0
  fi

  fail "Expected 2xx response at: $ctx"
  fail "  Got HTTP $code"
  exit 1
}
