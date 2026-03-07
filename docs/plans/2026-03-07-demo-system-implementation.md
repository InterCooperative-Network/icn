# ICN Federation Demo System — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a canonical "A Federation in Motion" demo system with four cooperative personas, three modular flows, presenter runbook, and self-serve handoff path.

**Architecture:** Four live K3s coop nodes (alpha/beta/gamma/delta) are reseeded with canonical personas and story data. Bash flow scripts use `kubectl port-forward` to access each coop's gateway (ClusterIP-only, no NodePort on port 8080). A shared port library handles setup/teardown so each flow script stays focused on narration.

**Tech Stack:** Bash, kubectl, curl, icnctl CLI, K3s cluster at 10.8.30.40, coop gateways behind ClusterIP services

**Design doc:** `docs/plans/2026-03-07-demo-system-design.md`

---

## Cluster Topology Reference

| Persona | Node | K8s Namespace | ClusterIP | Local port (port-forward) |
|---|---|---|---|---|
| BrightWorks Cooperative (worker) | icn-alpha | icn-coop-alpha | 10.43.97.80:8080 | 18081 |
| River City Tool Library (resource) | icn-beta | icn-coop-beta | 10.43.194.7:8080 | 18082 |
| Harbor Homes Cooperative (housing) | icn-gamma | icn-coop-gamma | 10.43.12.70:8080 | 18083 |
| Finger Lakes CDN (intermediate org) | icn-delta | icn-coop-delta | 10.43.110.197:8080 | 18084 |

Use ports 18081–18084 to avoid collision with the main icn-daemon NodePort (30080).

---

## Before You Start

```bash
# Confirm cluster is healthy
kubectl get pods -A | grep -E "icn-(alpha|beta|gamma|delta)" | grep -v Succeeded

# Confirm you can reach the main gateway
curl -s http://10.8.30.40:30080/v1/health | python3 -m json.tool

# Confirm kubectl works
kubectl get nodes
```

---

## Task 1: Discover Gateway API Shape

Before writing flow scripts, map what endpoints actually exist.

**Files:**
- Create: `demo/docs/api-map.md`

**Step 1: Port-forward to alpha and probe the API**

```bash
kubectl port-forward -n icn-coop-alpha svc/icn-alpha 18081:8080 &
PF=$!
sleep 2
curl -s http://localhost:18081/v1/health | python3 -m json.tool
```

Expected: JSON with `status: "ok"` or similar.

**Step 2: Discover available routes**

```bash
# Try OpenAPI spec
curl -s http://localhost:18081/v1/openapi.json 2>/dev/null | python3 -m json.tool | grep '"/"' | head -40

# Or enumerate known endpoints
for ep in /v1/health /v1/coops /v1/governance/proposals /v1/ledger/balance /v1/members; do
  code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:18081$ep)
  echo "$code $ep"
done

kill $PF
```

**Step 3: Write api-map.md**

Document each endpoint that returns 200 or 401 (exists but needs auth). Format:

```markdown
# Gateway API Map (as of YYYY-MM-DD)

| Endpoint | Method | Auth | Notes |
|---|---|---|---|
| /v1/health | GET | No | Always returns 200 |
| /v1/coops | GET | Yes | List all coops |
| ... | | | |
```

**Step 4: Kill port-forward and commit**

```bash
kill $PF 2>/dev/null || true
git add demo/docs/api-map.md
git commit -m "docs(demo): add gateway API map for flow scripting"
```

---

## Task 2: Create Shared Port-Forward Library

All flow scripts need the same port-forward setup/teardown. Build it once.

**Files:**
- Create: `demo/scripts/lib-demo-ports.sh`

**Step 1: Write the library**

```bash
cat > /home/ubuntu/projects/icn/demo/scripts/lib-demo-ports.sh << 'EOF'
#!/usr/bin/env bash
# Shared port-forward management for federation demo scripts.
# Source this file; do not execute directly.
#
# Usage:
#   source demo/scripts/lib-demo-ports.sh
#   demo_ports_up           # start all 4 port-forwards
#   demo_ports_down         # kill all port-forwards
#   demo_wait_ready         # block until all 4 gateways respond

BRIGHTWORKS_PORT=18081   # icn-coop-alpha
RIVERCITY_PORT=18082     # icn-coop-beta
         HARBOR_PORT=18083     # icn-coop-gamma
FINGERLAKES_PORT=18084   # icn-coop-delta

BRIGHTWORKS_URL="http://localhost:${BRIGHTWORKS_PORT}"
RIVERCITY_URL="http://localhost:${RIVERCITY_PORT}"
HARBOR_URL="http://localhost:${HARBOR_PORT}"
FINGERLAKES_URL="http://localhost:${FINGERLAKES_PORT}"

_PF_PIDS=()

_pf_start() {
  local ns="$1" svc="$2" local_port="$3" remote_port="$4"
  kubectl port-forward -n "$ns" "svc/$svc" "${local_port}:${remote_port}" \
    >/tmp/pf-${local_port}.log 2>&1 &
  _PF_PIDS+=($!)
}

demo_ports_up() {
  echo "Starting port-forwards..."
  _pf_start icn-coop-alpha icn-alpha "${BRIGHTWORKS_PORT}" 8080
  _pf_start icn-coop-beta  icn-beta  "${RIVERCITY_PORT}"  8080
  _pf_start icn-coop-gamma icn-gamma "${HARBOR_PORT}"     8080
  _pf_start icn-coop-delta icn-delta "${FINGERLAKES_PORT}" 8080
  trap demo_ports_down EXIT INT TERM
}

demo_ports_down() {
  for pid in "${_PF_PIDS[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  _PF_PIDS=()
}

demo_wait_ready() {
  local urls=("$BRIGHTWORKS_URL" "$RIVERCITY_URL" "$HARBOR_URL" "$FINGERLAKES_URL")
  local names=("BrightWorks" "River City Tool Library" "Harbor Homes" "Finger Lakes CDN")
  local max_wait=30
  local waited=0

  for i in "${!urls[@]}"; do
    local url="${urls[$i]}" name="${names[$i]}"
    echo -n "  Waiting for ${name}..."
    while ! curl -fsS "${url}/v1/health" >/dev/null 2>&1; do
      sleep 1
      waited=$((waited+1))
      if [ "$waited" -ge "$max_wait" ]; then
        echo " TIMEOUT"
        return 1
      fi
    done
    echo " ready"
  done
}

# Authenticated curl helper
# Usage: demo_curl <url> <method> [body] [token]
demo_curl() {
  local url="$1" method="$2" body="${3:-}" token="${4:-${DEMO_TOKEN:-}}"
  local args=(-sS -X "$method" -H "Accept: application/json")
  [ -n "$token" ] && args+=(-H "Authorization: Bearer $token")
  [ -n "$body" ] && args+=(-H "Content-Type: application/json" -d "$body")
  curl "${args[@]}" "$url"
}
EOF
```

**Step 2: Verify syntax**

```bash
bash -n /home/ubuntu/projects/icn/demo/scripts/lib-demo-ports.sh
echo $?  # expect 0
```

**Step 3: Commit**

```bash
git add demo/scripts/lib-demo-ports.sh
git commit -m "feat(demo): add shared port-forward library for federation flows"
```

---

## Task 3: Create Canonical Seed Data

Four sets of fixture files — one per coop. These define the stable world the demo always starts from.

**Files:**
- Create: `demo/data/brightworks-members.json`
- Create: `demo/data/rivercity-members.json`
- Create: `demo/data/harborhomes-members.json`
- Create: `demo/data/fingerlakes-members.json`
- Create: `demo/data/federation-proposals.json`
- Create: `demo/data/federation-history.json`

**Step 1: Write BrightWorks member data**

```json
{
  "organization": "BrightWorks Cooperative",
  "type": "worker",
  "coop_id": "brightworks-cooperative",
  "members": [
    { "name": "Yusuf Okafor",    "role": "Board Secretary",  "labor_hours": 120 },
    { "name": "Camille Tran",    "role": "Member-Worker",    "labor_hours": 98  },
    { "name": "Devraj Singh",    "role": "Member-Worker",    "labor_hours": 115 },
    { "name": "Rosa Mendez",     "role": "Treasurer",        "labor_hours": 87  },
    { "name": "Anton Kowalski",  "role": "Member-Worker",    "labor_hours": 104 }
  ]
}
```

Save to `demo/data/brightworks-members.json`.

**Step 2: Write River City Tool Library member data**

```json
{
  "organization": "River City Tool Library",
  "type": "community",
  "coop_id": "river-city-tool-library",
  "members": [
    { "name": "Priya Sharma",    "role": "Coordinator",      "contribution_hours": 45 },
    { "name": "Marcus Webb",     "role": "Member",           "contribution_hours": 32 },
    { "name": "Fatima Al-Amin",  "role": "Member",           "contribution_hours": 28 },
    { "name": "Lucas Ferreira",  "role": "Member",           "contribution_hours": 19 },
    { "name": "Aiko Nakamura",   "role": "Maintenance Lead", "contribution_hours": 51 }
  ]
}
```

Save to `demo/data/rivercity-members.json`.

**Step 3: Write Harbor Homes member data**

```json
{
  "organization": "Harbor Homes Cooperative",
  "type": "housing",
  "coop_id": "harbor-homes-cooperative",
  "members": [
    { "name": "Delphine Moreau",   "role": "Board Chair",   "units": 1 },
    { "name": "James Okafor",      "role": "Board Member",  "units": 1 },
    { "name": "Sunita Kapoor",     "role": "Treasurer",     "units": 1 },
    { "name": "Patrick Brennan",   "role": "Member",        "units": 2 },
    { "name": "Mei-Ling Chen",     "role": "Secretary",     "units": 1 }
  ]
}
```

Save to `demo/data/harborhomes-members.json`.

**Step 4: Write Finger Lakes CDN member data**

```json
{
  "organization": "Finger Lakes Cooperative Development Network",
  "type": "intermediate",
  "coop_id": "fingerlakes-cdn",
  "members": [
    { "name": "Amara Diallo",      "role": "Executive Director" },
    { "name": "Thomas Kowalczyk",  "role": "Program Officer"    },
    { "name": "Rebecca Nwosu",     "role": "Federation Liaison" }
  ]
}
```

Save to `demo/data/fingerlakes-members.json`.

**Step 5: Write canonical proposals fixture**

```json
{
  "proposals": [
    {
      "coop": "harbor-homes-cooperative",
      "title": "Authorize roof repair — Building A",
      "description": "The inspection report dated 2026-02-15 identified water intrusion at the Building A flat roof. This proposal authorizes up to $12,000 from the capital reserve fund for repairs. Work will be performed by Harbor Roofing LLC per quote #HH-2026-14.",
      "type": "treasury_spend",
      "amount": 12000,
      "unit": "usd",
      "status": "pending",
      "demo_role": "flow1_subject"
    },
    {
      "coop": "brightworks-cooperative",
      "title": "Q1 patronage distribution",
      "description": "Distribute Q1 surplus proportionally to member labor hours. Total distributable: 3,840 credits across 524 labor hours.",
      "type": "patronage_distribution",
      "status": "approved",
      "demo_role": "flow2_subject"
    },
    {
      "coop": "fingerlakes-cdn",
      "title": "River City Tool Library — BrightWorks equipment access agreement",
      "description": "Authorize a 90-day equipment-sharing agreement. River City provides access to metalworking tools; BrightWorks contributes 20 maintenance hours per quarter.",
      "type": "federation_agreement",
      "status": "pending",
      "demo_role": "flow3_subject"
    }
  ]
}
```

Save to `demo/data/federation-proposals.json`.

**Step 6: Write transaction history fixture**

Create `demo/data/federation-history.json` with 5–8 realistic cross-coop transactions showing prior coordination between the coops (equipment sharing, maintenance hours, support contributions).

```json
{
  "transactions": [
    {
      "from": "BrightWorks Cooperative",
      "to": "River City Tool Library",
      "type": "maintenance_contribution",
      "amount": 8,
      "unit": "hours",
      "description": "Quarterly tool maintenance — lathe and drill press",
      "date": "2026-01-15T10:00:00Z"
    },
    {
      "from": "River City Tool Library",
      "to": "BrightWorks Cooperative",
      "type": "equipment_access",
      "amount": 40,
      "unit": "commons_credits",
      "description": "Equipment access credits — Q4 2025",
      "date": "2026-01-20T09:00:00Z"
    },
    {
      "from": "Finger Lakes CDN",
      "to": "Harbor Homes Cooperative",
      "type": "technical_assistance",
      "amount": 4,
      "unit": "hours",
      "description": "Governance setup support — new treasurer onboarding",
      "date": "2026-02-03T14:00:00Z"
    }
  ]
}
```

**Step 7: Commit**

```bash
git add demo/data/brightworks-members.json demo/data/rivercity-members.json \
        demo/data/harborhomes-members.json demo/data/fingerlakes-members.json \
        demo/data/federation-proposals.json demo/data/federation-history.json
git commit -m "feat(demo): add canonical seed data for four federation personas"
```

---

## Task 4: Create `reseed-federation-demo.sh`

Reset all four nodes to canonical demo state. This is the "clear the deck" command.

**Files:**
- Create: `demo/scripts/reseed-federation-demo.sh`

**Step 1: Understand what seeding requires**

Before writing the script, verify what API calls actually create proposals and load members. Run:

```bash
# From Task 1 API map — adapt these to real endpoints
source demo/scripts/lib-demo-ports.sh
demo_ports_up
demo_wait_ready

# Try creating a proposal on Harbor Homes (Flow 1 subject)
# Replace /v1/governance/proposals with actual endpoint from api-map.md
TOKEN="<get from kubectl exec or icnctl>"
curl -X POST http://localhost:18083/v1/governance/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title":"test","description":"test","type":"text"}' | python3 -m json.tool
```

Document the actual endpoint in `demo/docs/api-map.md` before proceeding.

**Step 2: Write `reseed-federation-demo.sh`**

Template — fill in actual endpoints discovered in Step 1:

```bash
#!/usr/bin/env bash
# Reset all four federation demo nodes to canonical state.
# Usage: ./demo/scripts/reseed-federation-demo.sh

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'

echo "=== ICN Federation Demo — Reseed ==="
echo

demo_ports_up
demo_wait_ready

# Helper: get a demo token for a coop
# Adjust to actual token endpoint from api-map.md
get_token() {
  local url="$1" coop_id="$2"
  # TODO: replace with actual token generation call
  # e.g.: curl -sS -X POST "$url/v1/auth/token" -d "{\"coop_id\":\"$coop_id\",...}"
  echo "PLACEHOLDER_TOKEN"
}

seed_coop() {
  local name="$1" url="$2" coop_id="$3" data_file="$4"
  echo -n "  Seeding ${name}..."
  local token; token=$(get_token "$url" "$coop_id")
  # Load member display names (API endpoint TBD from api-map.md)
  # curl -sS -X PUT "$url/v1/coops/$coop_id/members" \
  #   -H "Authorization: Bearer $token" \
  #   -H "Content-Type: application/json" \
  #   --data-binary "@${SCRIPT_DIR}/../data/${data_file}"
  echo -e " ${GREEN}done${NC}"
}

seed_coop "BrightWorks Cooperative"                   "$BRIGHTWORKS_URL" "brightworks-cooperative"      "brightworks-members.json"
seed_coop "River City Tool Library"                   "$RIVERCITY_URL"   "river-city-tool-library"      "rivercity-members.json"
seed_coop "Harbor Homes Cooperative"                  "$HARBOR_URL"      "harbor-homes-cooperative"     "harborhomes-members.json"
seed_coop "Finger Lakes Cooperative Development Network" "$FINGERLAKES_URL" "fingerlakes-cdn"           "fingerlakes-members.json"

echo
echo -e "${GREEN}Federation demo world reseeded.${NC}"
echo "Run flows:"
echo "  ./demo/scripts/flow-1-governance.sh"
echo "  ./demo/scripts/flow-2-patronage.sh"
echo "  ./demo/scripts/flow-3-federation.sh"
```

**Step 3: Make executable and test**

```bash
chmod +x /home/ubuntu/projects/icn/demo/scripts/reseed-federation-demo.sh
./demo/scripts/reseed-federation-demo.sh
# Expected: all four coops report "done", no errors
```

**Step 4: Commit**

```bash
git add demo/scripts/reseed-federation-demo.sh
git commit -m "feat(demo): add reseed-federation-demo.sh for canonical world reset"
```

---

## Task 5: Flow 1 — Governance Legitimacy (Flow 1A)

Harbor Homes votes on a roof repair, the treasury action follows, provenance is shown.

**Files:**
- Create: `demo/scripts/flow-1-governance.sh`

**Step 1: Write Flow 1A script**

```bash
#!/usr/bin/env bash
# Flow 1: Governance Legitimacy and Action Traceability
# Narrative: Harbor Homes Cooperative — roof repair authorization
#
# What this demonstrates (Flow 1A — current scope):
#   - Proposal creation
#   - Member voting
#   - Approval result
#   - Treasury action initiation
#   - Provenance context
#
# What this does NOT yet claim (Flow 1B — pending PR #1327):
#   - ExecutionReceiptGate enforcement
#   - Machine-verifiable governance-to-execution binding

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

BLUE='\033[0;34m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'

narrate() { echo -e "\n${BLUE}▶${NC} $*\n"; }
result()  { echo -e "  ${GREEN}✓${NC} $*"; }
aside()   { echo -e "  ${YELLOW}→${NC} $*"; }

demo_ports_up
demo_wait_ready

# --- Act ---

narrate "Harbor Homes Cooperative — Roof Repair Authorization"
echo "  The board has received an inspection report:"
echo "  water intrusion detected at Building A flat roof."
echo "  A proposal to authorize \$12,000 from the capital reserve is pending."
echo

narrate "Step 1: Show the pending proposal"
PROPOSAL=$(demo_curl "$HARBOR_URL/v1/governance/proposals?status=pending" GET "" "$HARBOR_TOKEN")
echo "$PROPOSAL" | python3 -m json.tool
result "Proposal visible to all members"
aside "Anyone in the coop can see this — not just the board"

narrate "Step 2: Members vote"
# Vote yes as Delphine Moreau (Board Chair)
demo_curl "$HARBOR_URL/v1/governance/proposals/${PROPOSAL_ID}/votes" POST \
  '{"vote":"yes","member":"did:icn:delphine"}' "$HARBOR_TOKEN" | python3 -m json.tool
# Vote yes as James Okafor
demo_curl "$HARBOR_URL/v1/governance/proposals/${PROPOSAL_ID}/votes" POST \
  '{"vote":"yes","member":"did:icn:james"}' "$HARBOR_TOKEN" | python3 -m json.tool
result "Votes recorded — 2 yes, quorum reached"
aside "Delphine Moreau, James Okafor — names visible, not raw DIDs"

narrate "Step 3: Proposal passes — treasury action authorized"
APPROVAL=$(demo_curl "$HARBOR_URL/v1/governance/proposals/${PROPOSAL_ID}" GET "" "$HARBOR_TOKEN")
echo "$APPROVAL" | python3 -m json.tool
result "Status: approved"

narrate "Step 4: Show provenance"
PROVENANCE=$(demo_curl "$HARBOR_URL/v1/ledger/provenance?ref=proposal:${PROPOSAL_ID}" GET "" "$HARBOR_TOKEN")
echo "$PROVENANCE" | python3 -m json.tool
result "Provenance chain links proposal to authorized allocation"

echo
echo "================================================================"
echo " FLOW 1A COMPLETE: Governance leading to authorized action"
echo " with provenance context."
echo
echo " NOTE: The final receipt-gated execution proof is being"
echo " finalized (PR #1327 — ExecutionReceiptGate). Once merged,"
echo " the system will provide machine-verifiable binding between"
echo " the approved vote and the resulting execution."
echo "================================================================"
echo
```

Save to `demo/scripts/flow-1-governance.sh`.

**Step 2: Replace placeholder API calls with real endpoints from api-map.md**

This is the critical step. Do not proceed until the curl calls return real data.

For each call:
1. Run it manually against `$HARBOR_URL`
2. Verify the response shape
3. Update the script to extract the right fields (proposal ID, status, etc.)

**Step 3: Test the full flow end-to-end**

```bash
chmod +x demo/scripts/flow-1-governance.sh
./demo/scripts/flow-1-governance.sh 2>&1 | tee /tmp/flow1-output.txt
grep -E "(COMPLETE|ERROR|✓)" /tmp/flow1-output.txt
```

Expected: all `✓` lines, no errors, ends with `FLOW 1A COMPLETE`.

**Step 4: Commit**

```bash
git add demo/scripts/flow-1-governance.sh
git commit -m "feat(demo): add flow-1-governance.sh (Flow 1A — Harbor Homes roof repair)"
```

---

## Task 6: Flow 2 — Patronage and Contribution Legibility

BrightWorks distributes Q1 patronage. Any member can see why allocations look the way they do.

**Files:**
- Create: `demo/scripts/flow-2-patronage.sh`

**Step 1: Write the script**

```bash
#!/usr/bin/env bash
# Flow 2: Patronage, Contribution, and Value Legibility
# Narrative: BrightWorks Cooperative — Q1 patronage distribution
#
# What this demonstrates:
#   - Commons credit / contribution accounting
#   - Provenance-backed balances
#   - PatronageTracker logic
#   - Member-visible explanation of allocations

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

BLUE='\033[0;34m'; GREEN='\033[0;32m'; NC='\033[0m'

narrate() { echo -e "\n${BLUE}▶${NC} $*\n"; }
result()  { echo -e "  ${GREEN}✓${NC} $*"; }
aside()   { echo -e "  → $*"; }

demo_ports_up
demo_wait_ready

narrate "BrightWorks Cooperative — Q1 Patronage Distribution"
echo "  Q1 is closing. 3,840 credits are distributable."
echo "  The question every member asks: 'Why did I get this amount?'"
echo

narrate "Step 1: Show contribution ledger"
# Endpoint TBD from api-map.md
CONTRIBUTIONS=$(demo_curl "$BRIGHTWORKS_URL/v1/ledger/contributions" GET "" "$BRIGHTWORKS_TOKEN")
echo "$CONTRIBUTIONS" | python3 -m json.tool
result "Labor hours per member visible to the whole coop"
aside "Yusuf: 120h, Devraj: 115h, Camille: 98h, Anton: 104h, Rosa: 87h"

narrate "Step 2: Show patronage calculation (PatronageTracker)"
PATRONAGE=$(demo_curl "$BRIGHTWORKS_URL/v1/ledger/patronage?period=2026-Q1" GET "" "$BRIGHTWORKS_TOKEN")
echo "$PATRONAGE" | python3 -m json.tool
result "Allocation = (member hours / total hours) * distributable surplus"
aside "Each member can verify the math. No black box."

narrate "Step 3: Show provenance-backed balance for one member"
BALANCE=$(demo_curl "$BRIGHTWORKS_URL/v1/ledger/balance?member=did:icn:yusuf" GET "" "$BRIGHTWORKS_TOKEN")
echo "$BALANCE" | python3 -m json.tool
result "Balance includes provenance reference to the patronage receipt"

echo
echo "================================================================"
echo " FLOW 2 COMPLETE: Patronage and contribution legibility"
echo " Any member can inspect why they received what they received."
echo "================================================================"
echo
```

**Step 2: Fill in real API calls**

Same process as Task 5 Step 2. Verify against `$BRIGHTWORKS_URL`.

**Step 3: Test**

```bash
chmod +x demo/scripts/flow-2-patronage.sh
./demo/scripts/flow-2-patronage.sh 2>&1 | grep -E "(COMPLETE|ERROR|✓)"
```

**Step 4: Commit**

```bash
git add demo/scripts/flow-2-patronage.sh
git commit -m "feat(demo): add flow-2-patronage.sh (BrightWorks Q1 patronage distribution)"
```

---

## Task 7: Flow 3 — Federation Coordination

River City shares equipment with BrightWorks. Finger Lakes CDN facilitates. Cross-coop commons credits settle.

**Files:**
- Create: `demo/scripts/flow-3-federation.sh`

**Step 1: Write the script**

```bash
#!/usr/bin/env bash
# Flow 3: Federation Coordination Across Autonomous Coops
# Narrative: River City Tool Library + BrightWorks equipment access agreement,
#            facilitated by Finger Lakes CDN
#
# What this demonstrates:
#   - Multiple live nodes, separate identities
#   - Cross-coop proposal and agreement
#   - Commons credit settlement across orgs
#   - Federation without centralization

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

BLUE='\033[0;34m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'

narrate() { echo -e "\n${BLUE}▶${NC} $*\n"; }
result()  { echo -e "  ${GREEN}✓${NC} $*"; }
aside()   { echo -e "  ${YELLOW}→${NC} $*"; }

demo_ports_up
demo_wait_ready

narrate "A Federation in Motion"
echo "  Three separate cooperatives. Three separate nodes."
echo "  No central authority. Each controls its own data."
echo "  Today: River City and BrightWorks are forming an equipment-sharing agreement."
echo

narrate "Step 1: Show each coop's identity — they are independent"
for url_var in BRIGHTWORKS_URL RIVERCITY_URL FINGERLAKES_URL; do
  url="${!url_var}"
  info=$(demo_curl "$url/v1/health" GET)
  echo "  $url_var: $(echo "$info" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("coop_id","unknown"))')"
done
result "Three separate nodes — separate governance, separate ledgers"

narrate "Step 2: Finger Lakes CDN proposes a federation agreement"
# CDN initiates on behalf of the two coops (federation proposal pattern)
AGREEMENT=$(demo_curl "$FINGERLAKES_URL/v1/federation/agreements" POST \
  '{"parties":["river-city-tool-library","brightworks-cooperative"],
    "terms":"River City provides metalworking equipment access; BrightWorks contributes 20 maintenance hours/quarter",
    "duration_days":90}' "$FINGERLAKES_TOKEN")
echo "$AGREEMENT" | python3 -m json.tool
result "Agreement proposed — each party must accept independently"
aside "Finger Lakes CDN facilitates. It does not control."

narrate "Step 3: Both coops accept"
demo_curl "$RIVERCITY_URL/v1/federation/agreements/${AGREEMENT_ID}/accept" POST \
  '{}' "$RIVERCITY_TOKEN" | python3 -m json.tool
demo_curl "$BRIGHTWORKS_URL/v1/federation/agreements/${AGREEMENT_ID}/accept" POST \
  '{}' "$BRIGHTWORKS_TOKEN" | python3 -m json.tool
result "Agreement active — both parties accepted independently"

narrate "Step 4: BrightWorks contributes maintenance hours — commons credits settle"
SETTLEMENT=$(demo_curl "$BRIGHTWORKS_URL/v1/ledger/commons-credit" POST \
  '{"recipient":"river-city-tool-library",
    "amount":8,
    "unit":"hours",
    "description":"Q1 maintenance contribution — lathe and drill press",
    "agreement_ref":"'"${AGREEMENT_ID}"'"}' "$BRIGHTWORKS_TOKEN")
echo "$SETTLEMENT" | python3 -m json.tool
result "Settlement recorded — cross-coop, provenance-backed"

narrate "Step 5: Show the cross-coop settlement at Finger Lakes CDN"
FEDERATION_VIEW=$(demo_curl "$FINGERLAKES_URL/v1/federation/settlements?agreement=${AGREEMENT_ID}" GET "" "$FINGERLAKES_TOKEN")
echo "$FEDERATION_VIEW" | python3 -m json.tool
result "CDN can see settlement status across the federation"
aside "Read-only visibility. No ability to override either coop's ledger."

echo
echo "================================================================"
echo " FLOW 3 COMPLETE: Federation coordination across autonomous coops"
echo " Independent coops coordinated without a central authority."
echo "================================================================"
echo
```

**Step 2: Fill in real API calls**

Pay special attention to federation/agreement endpoints — these may be different. Verify from api-map.md and update.

**Step 3: Test**

```bash
chmod +x demo/scripts/flow-3-federation.sh
./demo/scripts/flow-3-federation.sh 2>&1 | grep -E "(COMPLETE|ERROR|✓)"
```

**Step 4: Commit**

```bash
git add demo/scripts/flow-3-federation.sh
git commit -m "feat(demo): add flow-3-federation.sh (River City + BrightWorks equipment agreement)"
```

---

## Task 8: Flow 4 — Federation Reporting (Optional Capstone)

Finger Lakes CDN shows cross-federation visibility to a funder, without controlling any coop.

**Files:**
- Create: `demo/scripts/flow-4-reporting.sh`

This is a lighter script — mostly read-only queries against the CDN node showing governance evidence and settlement summaries.

```bash
#!/usr/bin/env bash
# Flow 4: Federation Oversight and Institutional Reporting (optional capstone)
# Audience: funders, grant reviewers, ecosystem orgs

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

BLUE='\033[0;34m'; GREEN='\033[0;32m'; NC='\033[0m'

narrate() { echo -e "\n${BLUE}▶${NC} $*\n"; }
result()  { echo -e "  ${GREEN}✓${NC} $*"; }

demo_ports_up
demo_wait_ready

narrate "Finger Lakes CDN — Federation Grant Report"
echo "  A funder asks: 'Show us that governance and spending are linked.'"
echo "  The CDN can produce this without having control over any member coop."
echo

# Show governance activity across federation
narrate "Step 1: Governance activity summary"
demo_curl "$FINGERLAKES_URL/v1/federation/governance-summary" GET "" "$FINGERLAKES_TOKEN" | python3 -m json.tool
result "Proposals, votes, approvals — across all member coops"

# Show settlements
narrate "Step 2: Cross-coop settlement summary"
demo_curl "$FINGERLAKES_URL/v1/federation/settlements" GET "" "$FINGERLAKES_TOKEN" | python3 -m json.tool
result "Settlement records with provenance — exportable for grant reporting"

echo
echo "================================================================"
echo " FLOW 4 COMPLETE: Institutional trust without centralization"
echo " The CDN can report. It cannot command."
echo "================================================================"
```

Follow same steps: fill in real endpoints, test, commit.

```bash
git add demo/scripts/flow-4-reporting.sh
git commit -m "feat(demo): add flow-4-reporting.sh (Finger Lakes CDN federation audit view)"
```

---

## Task 9: Presenter Runbook

**Files:**
- Modify: `docs/demo/DEMO_SCRIPT.md` (replace existing with federation version)

**Step 1: Read the existing file first**

```bash
wc -l /home/ubuntu/projects/icn/docs/demo/DEMO_SCRIPT.md
```

**Step 2: Replace with federation runbook**

The new runbook should cover:

```markdown
# ICN Federation Demo — Presenter Runbook

**Versions:** 5 min (lightning) / 12 min (standard) / 20 min (full)

## Pre-Demo Setup (2 min)
./demo/scripts/reseed-federation-demo.sh
# Verify: all four coops show "ready"

## Opening (all versions)
"Cooperatives are supposed to be more democratic than firms.
 But in practice, they often inherit the same old problems..."

## 5-Minute Version
- Opening (1 min)
- Flow 1A: roof repair governance + provenance (3 min)
- Closing: "This is running on four separate nodes, each
  coop controls its own data" (1 min)

## 12-Minute Version
- Opening (1 min)
- Flow 1A (4 min)
- Flow 2: patronage legibility (4 min)
- Flow 3: federation clip (2 min)
- Closing (1 min)

## 20-Minute Version
- Opening (2 min)
- Flow 1A (5 min)
- Flow 2 (5 min)
- Flow 3 (5 min)
- Flow 4 optional (2 min)
- Closing / Q&A setup (1 min)

## Audience Pivots
[section per audience type with 1 sentence reframe]

## Fallback Plan
[prerecorded video link / static screenshots]

## Reset
./demo/scripts/reseed-federation-demo.sh
```

Write the full version of each section (not just this skeleton).

**Step 3: Commit**

```bash
git add docs/demo/DEMO_SCRIPT.md
git commit -m "docs(demo): replace demo script with federation runbook (5/12/20 min variants)"
```

---

## Task 10: Self-Serve Quickstart

**Files:**
- Create: `demo/SELF_SERVE.md`

This is the handoff document. A stranger should be able to run the demo from this file alone.

Sections:
1. **What this demonstrates** (3 sentences)
2. **Prerequisites** (kubectl, cluster access)
3. **Start** (`./demo/scripts/reseed-federation-demo.sh`)
4. **Run the flows** (one command each, expected output description)
5. **What you're seeing** (one paragraph per flow)
6. **Troubleshooting** (port-forward stalls, token errors, reseed)

```bash
git add demo/SELF_SERVE.md
git commit -m "docs(demo): add self-serve quickstart (handoff-survivable)"
```

---

## Task 11: Flow 1B Upgrade (Gated on PR #1327)

**Do not start this task until PR #1327 is merged to main.**

**Files:**
- Modify: `demo/scripts/flow-1-governance.sh`
- Modify: `docs/demo/DEMO_SCRIPT.md` (upgrade proof language)

**Step 1: Confirm #1327 is merged**

```bash
gh pr view 1327 --json state,mergedAt | python3 -m json.tool
# Expected: state: "MERGED"
```

**Step 2: Add ExecutionReceiptGate call to flow-1 script**

After the treasury action step, add:

```bash
narrate "Step 5: Execution receipt — governance-to-execution proof"
RECEIPT=$(demo_curl "$HARBOR_URL/v1/execution/receipts?proposal=${PROPOSAL_ID}" GET "" "$HARBOR_TOKEN")
echo "$RECEIPT" | python3 -m json.tool
result "Execution receipt bound to approved governance event"
aside "This action could not have run without the approved proposal"
```

**Step 3: Update narrative footer in the script**

Remove the `NOTE: The final receipt-gated execution proof is being finalized...` block.
Replace with:

```
================================================================
 FLOW 1B COMPLETE: Governance to execution proof
 The treasury action is cryptographically bound to the approved
 governance event via ExecutionReceiptGate.
================================================================
```

**Step 4: Update presenter narration in DEMO_SCRIPT.md**

Find the Flow 1 section and update to the stronger claim:

> This proposal was approved through cooperative governance.
> The resulting action is not merely recorded after the fact —
> it is bound through the execution receipt path, so the system
> can prove the action corresponds to approved governance.

**Step 5: Test**

```bash
./demo/scripts/flow-1-governance.sh 2>&1 | grep -E "(COMPLETE|ERROR|✓|receipt)"
```

Expected: `FLOW 1B COMPLETE` in output, receipt data in results.

**Step 6: Commit**

```bash
git add demo/scripts/flow-1-governance.sh docs/demo/DEMO_SCRIPT.md
git commit -m "feat(demo): upgrade Flow 1 to 1B — ExecutionReceiptGate proof (PR #1327)"
```

---

## Verification Checklist

Run these before calling the demo presentation-ready:

```bash
# 1. Reseed works cleanly
./demo/scripts/reseed-federation-demo.sh
echo "Exit: $?"

# 2. Flow 1 runs without errors
./demo/scripts/flow-1-governance.sh 2>&1 | tail -5

# 3. Flow 2 runs without errors
./demo/scripts/flow-2-patronage.sh 2>&1 | tail -5

# 4. Flow 3 runs without errors
./demo/scripts/flow-3-federation.sh 2>&1 | tail -5

# 5. Run all three in sequence (simulates full 20-min demo)
./demo/scripts/reseed-federation-demo.sh && \
./demo/scripts/flow-1-governance.sh && \
./demo/scripts/flow-2-patronage.sh && \
./demo/scripts/flow-3-federation.sh
echo "Full run exit: $?"

# 6. Reseed and run again (determinism check)
./demo/scripts/reseed-federation-demo.sh && \
./demo/scripts/flow-1-governance.sh 2>&1 | grep COMPLETE
```

All checks must pass before the demo is considered presentation-ready.

---

## What to Watch Out For

**Port-forward instability:** kubectl port-forward can drop under load. If a flow dies mid-way, `demo_ports_down && demo_ports_up && demo_wait_ready` resets cleanly. Add this as a fallback step in the runbook.

**Token expiry:** JWTs expire. The reseed script should regenerate tokens as part of setup. Don't hardcode tokens in scripts.

**Non-deterministic IDs:** If proposal IDs are UUIDs generated fresh each run, the flow scripts must dynamically extract them from API responses. Do not hardcode IDs.

**API endpoint drift:** If the gateway API changes, update `demo/docs/api-map.md` first, then update the flow scripts. Keep the map as the source of truth.
