#!/usr/bin/env bash
# run-all.sh — Boot 3-node devnet and run the full demonstration suite
#
# Usage:
#   bash demo/run-all.sh                 # Boot devnet + run all demos
#   bash demo/run-all.sh --no-boot       # Skip docker compose up (devnet already running)
#   bash demo/run-all.sh --presenter     # Interactive pause between phases
#   bash demo/run-all.sh --no-boot --presenter
#
# What this runs:
#   Phase 1-4: Cooperative Governance Demo on node-a (localhost:8080)
#   Phase 1-4: Cooperative Governance Demo on node-b (localhost:8081)
#   (Demonstrates that both nodes are independently operational)
#
# Requirements:
#   - docker + docker compose (to boot the devnet)
#   - python3 + cryptography package (for demo-governance.py)
#   - Devnet image must be built: cd deploy/devnet && docker compose build

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEVNET_DIR="${REPO_ROOT}/deploy/devnet"
DEMO_SCRIPTS="${SCRIPT_DIR}/scripts"

# ── Parse args ───────────────────────────────────────────────────────────────
BOOT=true
PRESENT_FLAG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-boot)   BOOT=false ;;
    --presenter) PRESENT_FLAG="--presenter" ;;
    --help|-h)
      echo "Usage: $0 [--no-boot] [--presenter]"
      echo ""
      echo "Options:"
      echo "  --no-boot    Skip 'docker compose up' (devnet already running)"
      echo "  --presenter  Interactive pause between demo phases"
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
  shift
done

# ── Colors ───────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  BOLD='\033[1m'
  GREEN='\033[0;32m'
  RED='\033[0;31m'
  YELLOW='\033[1;33m'
  NC='\033[0m'
else
  BOLD=''; GREEN=''; RED=''; YELLOW=''; NC=''
fi

banner() { echo -e "\n${BOLD}=== $* ===${NC}"; }
ok()     { echo -e "  ${GREEN}✓${NC} $*"; }
fail()   { echo -e "  ${RED}✗${NC} $*"; }
warn()   { echo -e "  ${YELLOW}⚠${NC} $*"; }

# ── Pre-flight checks ─────────────────────────────────────────────────────────
banner "Pre-flight"

if ! command -v docker >/dev/null 2>&1; then
  fail "docker not found — install Docker to run the devnet"
  exit 1
fi
ok "docker available"

if ! python3 -c "import cryptography" 2>/dev/null; then
  warn "python3 'cryptography' package not found — installing..."
  pip3 install cryptography --quiet
fi
ok "python3 cryptography available"

# ── Step 1: Boot devnet ───────────────────────────────────────────────────────
if [ "$BOOT" = true ]; then
  banner "Booting 3-node devnet"
  cd "$DEVNET_DIR"
  docker compose up -d
  cd "$REPO_ROOT"
  ok "docker compose up"
else
  banner "Devnet boot skipped (--no-boot)"
fi

# ── Step 2: Wait for all 3 nodes ──────────────────────────────────────────────
banner "Waiting for devnet nodes"
for port in 8080 8081 8082; do
  printf "  Waiting for localhost:%d " "$port"
  retries=0
  until curl -sf "http://localhost:${port}/v1/health" >/dev/null 2>&1; do
    retries=$((retries + 1))
    if [ "$retries" -gt 60 ]; then
      echo ""
      fail "localhost:${port} did not become healthy within 120s"
      echo "  Check logs: cd deploy/devnet && docker compose logs"
      exit 1
    fi
    printf "."
    sleep 2
  done
  echo ""
  ok "localhost:${port} healthy"
done

# ── Step 3: Run demo suite ────────────────────────────────────────────────────
PASS=0
FAIL=0

run_demo() {
  local label="$1"
  local gateway="$2"

  banner "$label"
  set +e
  python3 "${DEMO_SCRIPTS}/demo-governance.py" "$gateway" $PRESENT_FLAG
  local rc=$?
  set -e

  if [ "$rc" -eq 0 ]; then
    ok "$label passed"
    PASS=$((PASS + 1))
  else
    fail "$label failed (exit $rc)"
    FAIL=$((FAIL + 1))
  fi
}

run_demo "Governance Demo — node-a (localhost:8080)" "http://localhost:8080"
run_demo "Governance Demo — node-b (localhost:8081)" "http://localhost:8081"

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
if [ "$FAIL" -eq 0 ]; then
  echo -e "${GREEN}${BOLD}┌──────────────────────────────────────────────────────┐${NC}"
  echo -e "${GREEN}${BOLD}│  All demos passed (${PASS}/${PASS})                            │${NC}"
  echo -e "${GREEN}${BOLD}└──────────────────────────────────────────────────────┘${NC}"
else
  echo -e "${RED}${BOLD}┌──────────────────────────────────────────────────────┐${NC}"
  echo -e "${RED}${BOLD}│  ${FAIL} demo(s) failed, ${PASS} passed                          │${NC}"
  echo -e "${RED}${BOLD}└──────────────────────────────────────────────────────┘${NC}"
fi

echo ""
echo "  Devnet endpoints:"
echo "    Node A (gateway):   http://localhost:8080"
echo "    Node B (gateway):   http://localhost:8081"
echo "    Node C (gateway):   http://localhost:8082"
echo "    Prometheus:         http://localhost:9090"
echo "    Grafana:            http://localhost:3000  (admin / devnet)"
echo ""
echo "  Manage devnet:  cd deploy/devnet && make [up|down|logs|status]"
echo ""

[ "$FAIL" -eq 0 ]
