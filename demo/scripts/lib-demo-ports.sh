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

export BLUE GREEN YELLOW RED NC

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
export DEMO_DEFAULT_SCOPES="ledger:read,ledger:write,coop:read,coop:write,governance:read,governance:write,payments:read,payments:write,federation:read,federation:write"

# HTTP status code from the last demo_curl call
export DEMO_LAST_HTTP_CODE=""

# Internal: PID array for port-forwards started by demo_ports_up
_DEMO_PF_PIDS=()

# Internal: file-based token cache directory (survives subshell boundaries)
_DEMO_TOKEN_CACHE_DIR="${TMPDIR:-/tmp}/icn-demo-tokens-$$"
mkdir -p "$_DEMO_TOKEN_CACHE_DIR"

# ---------------------------------------------------------------------------
# Narration helpers
# ---------------------------------------------------------------------------

# narrate: print a section header / step announcement
narrate() { echo -e "\n${BLUE}▶${NC} $*\n"; }

# result: print a success line
result()  { echo -e "  ${GREEN}✓${NC} $*"; }

# aside: print an informational line
aside()   { echo -e "  ${YELLOW}→${NC} $*"; }

# warn: print a warning line (non-fatal)
warn()    { echo -e "  ${YELLOW}⚠${NC} $*"; }

# fail: print a failure line
fail()    { echo -e "  ${RED}✗${NC} $*"; }

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

  trap demo_ports_down EXIT INT TERM

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
}

# ---------------------------------------------------------------------------
# demo_wait_ready [max_wait_seconds]
# Poll all 4 gateways until they return HTTP 200 on GET /v1/health.
# Default timeout: 30 seconds. Returns non-zero if any gateway is still down.
# ---------------------------------------------------------------------------
demo_wait_ready() {
  local max_wait="${1:-30}"
  local deadline=$(( $(date +%s) + max_wait ))

  # Track which coops are ready
  local -A ready=()
  local -A urls=(
    [brightworks]="$BRIGHTWORKS_URL"
    [rivercity]="$RIVERCITY_URL"
    [harbor]="$HARBOR_URL"
    [fingerlakes]="$FINGERLAKES_URL"
  )

  aside "Waiting up to ${max_wait}s for all 4 gateways..."

  while [ $(date +%s) -lt "$deadline" ]; do
    for coop in brightworks rivercity harbor fingerlakes; do
      if [ "${ready[$coop]:-}" = "1" ]; then
        continue
      fi
      local url="${urls[$coop]}"
      local code
      code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 2 \
        "${url}/v1/health" 2>/dev/null; true)
      if [ "$code" = "200" ]; then
        ready[$coop]="1"
        result "$coop gateway ready (${url})"
      fi
    done

    # Check if all 4 are ready
    if [ ${#ready[@]} -eq 4 ]; then
      return 0
    fi

    sleep 1
  done

  # Report which ones failed
  for coop in brightworks rivercity harbor fingerlakes; do
    if [ "${ready[$coop]:-}" != "1" ]; then
      fail "$coop gateway did not become ready within ${max_wait}s"
    fi
  done
  return 1
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

# ---------------------------------------------------------------------------
# demo_get_token <coop_name>
# Fetch a JWT auth token for the given coop (alpha/beta/gamma/delta).
#
# Strategy: run icnctl auth token inside the pod via kubectl exec.
# icnctl is at /usr/local/bin/icnctl in all 4 pods (confirmed in Task 1).
# Passphrase is read dynamically from K8s Secret at call time.
# Token is cached in _DEMO_TOKEN_{COOP} after first successful fetch.
#
# Returns the token via stdout.
# Also sets DEMO_TOKEN_ALPHA / _BETA / _GAMMA / _DELTA.
# ---------------------------------------------------------------------------
demo_get_token() {
  local coop="${1:-}"
  if [ -z "$coop" ]; then
    fail "demo_get_token: coop name required (alpha/beta/gamma/delta)"
    return 1
  fi

  # File-based cache: survives subshell $(...) calls unlike shell variables
  local cache_file="${_DEMO_TOKEN_CACHE_DIR}/token-${coop}"
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
    2>/dev/null | grep -oE 'eyJ[A-Za-z0-9_.-]+' | head -1 || true)

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
  local url="${1:-}"
  local method="${2:-GET}"
  local body="${3:-}"
  local token="${4:-${DEMO_TOKEN:-}}"

  if [ -z "$url" ]; then
    fail "demo_curl: url required"
    return 1
  fi

  local curl_args=(-s -o /tmp/demo_curl_body.tmp -w "%{http_code}")
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

  cat /tmp/demo_curl_body.tmp 2>/dev/null || true
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
