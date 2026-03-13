# A Federation in Motion — Self-Serve Quickstart

You don't need to know how ICN works to run this demo. Start here.

---

## What You're Looking At

Four cooperative organizations, each running a live node:

| Coop | Type | The question it answers |
|------|------|------------------------|
| **BrightWorks Cooperative** | Worker coop | Why did this member get this patronage allocation? |
| **River City Tool Library** | Community coop | How does a shared-resource coop track member contributions? |
| **Harbor Homes Cooperative** | Housing coop | Did the vote that authorized this $12,000 spend actually happen? |
| **Finger Lakes CDN** | Intermediate org | Can a regional federation support member coops without owning them? |

These are four separate ICN nodes, running on a four-node Kubernetes cluster. Each has its own governance, its own ledger, and its own identity. They coordinate without a central server.

The demo shows four flows, each answering a different cooperative problem. You can run them in order or pick the one relevant to your audience.

---

## Prerequisites

You need:
- `kubectl` configured against the K3s cluster (`kubectl get pods -A` should show ICN pods)
- `curl` and `python3` (standard on most Unix systems)
- About 5 minutes per flow

You do NOT need: Rust, cargo, npm, a local ICN build, or knowledge of how the protocol works.

---

## Step 0 (first time only): Verify the cluster ratifies the scripts

Before going live with a real audience, run the rehearsal probe from icn-dev (10.8.30.45):

```bash
ssh ubuntu@10.8.30.45
cd /path/to/icn
./demo/scripts/reseed-federation-demo.sh      # seed known state first
./demo/scripts/rehearsal-probe.sh             # run the five-unknown probe
```

Expected output ends with:
```
✓ CONFIRMED   5 / 5
```

If any probe shows `MISMATCH`, read the note next to it — each one says exactly how to adapt the flow script. Run reseed again after probing to clean up probe proposals.

You only need to do this once per cluster rebuild or ICN binary update.

---

## Step 1: Reset to canonical state

Always start here. This ensures the demo is in a known, clean state:

```bash
cd /path/to/icn
./demo/scripts/reseed-federation-demo.sh
```

Expected output ends with:
```
=== Reseed summary ===
✓ Seeded: N
· Skipped: M (already in place)
✓ Failed: 0
```

If you see `Failed: N > 0`, check that the cluster is healthy:
```bash
kubectl get pods -A | grep icn-
```

All four ICN pods should show `1/1 Running`.

---

## Step 2: Run a flow

Pick the flow that matches your audience:

```bash
# Flow 1: Harbor Homes — governance and action traceability (~5 min)
./demo/scripts/flow-1-governance.sh

# Flow 2: BrightWorks — patronage and contribution legibility (~5 min)
./demo/scripts/flow-2-patronage.sh

# Flow 3: River City + BrightWorks — federation agreement (~7 min)
./demo/scripts/flow-3-federation.sh

# Flow 4: Finger Lakes CDN — audit view across all coops (~5 min, optional)
./demo/scripts/flow-4-reporting.sh
```

For the full demo, run them in order: 1, 2, 3, (optionally 4).

---

## What the Output Means

Each script uses colored output:

- **White text**: Narrator line — what's happening, who's doing it
- **Green ✓ lines**: Something succeeded
- **Yellow · lines**: Notes and context for the presenter
- **Yellow ⚠ lines**: Expected gaps or deployment constraints
- **Red ✗ lines**: Something failed that needs attention

### The expected yellow notes

Some endpoints return 403 or 404 in the current cluster. This is expected — not broken. The scripts narrate what the response would show in a fully configured deployment. Look for:

```
· Treasury API is not yet reachable in this deployment (scope 'treasury:read'
· is not in ALLOWED_SCOPES — this is a deployment gap, not a design gap)
```

This means the treasury API works — but the auth scope that grants access to it hasn't been added to the demo token yet. The governance flows work fine without it.

```
· Proof endpoint returned 404 — signing key not configured in this pod
```

The governance proof endpoint (GovernanceReceipt) exists in the code. The pod's signing key isn't configured in this deployment. The governance record itself is still present and auditable.

---

## What Each Flow Shows

### Flow 1 — Governance Legitimacy (Harbor Homes)

**The cooperator question:** "Did the thing we voted on actually happen, and can we prove it?"

The script:
1. Creates a governance domain for Harbor Homes' capital reserve
2. Creates a proposal: authorize a $12,000 roof repair
3. Opens it for voting
4. Casts a vote
5. Shows the vote tally
6. Closes the proposal — result is final
7. Attempts to retrieve a cryptographic governance proof
8. Shows the full provenance: proposal → vote → decision → authorized action

What you'll see at the end:
```
================================================================
 FLOW 1A COMPLETE
 Governance legitimacy and action traceability demonstrated.
...
```

The proposal ID in the output is the permanent on-chain reference. Any Harbor Homes member can query it by ID.

### Flow 2 — Patronage and Value (BrightWorks)

**The cooperator question:** "Why did this member get this distribution?"

The script:
1. Introduces BrightWorks' Q1 figures (524 total labor hours, 3,840 credits to distribute)
2. Finds the pre-seeded Q1 patronage proposal
3. Shows the allocation table: each member's hours, formula, and resulting credits
4. Opens the proposal for member ratification vote
5. Casts the vote
6. Closes the proposal
7. Attempts a ledger settlement — posts credits with the governance decision ID as provenance
8. Shows the member's ledger position

The allocation formula is: `credits = (member_hours / 524) × 3,840`

Every member can verify their allocation from the proposal — no trust in the treasurer required.

### Flow 3 — Federation Coordination (River City + BrightWorks)

**The cooperator question:** "How do independent co-ops work together without creating another bureaucracy?"

The script:
1. Connects to all three relevant nodes (River City, BrightWorks, Finger Lakes CDN)
2. Shows current federation status from each node's perspective
3. River City holds an internal governance vote: authorize equipment access
4. BrightWorks holds an independent governance vote: authorize maintenance contribution
5. Finger Lakes CDN registers both coops and issues vouches
6. Creates a clearing agreement to track the value exchange
7. Queries governance records from all three nodes simultaneously

Key moment to watch: **Step 9** — three separate queries to three separate URLs, each returning the same agreement from a different perspective. No central server holds the authoritative record.

### Flow 4 — Institutional Reporting (Finger Lakes CDN)

**The institutional question:** "Can this produce trustworthy reporting without adding massive admin overhead?"

The script:
1. Authenticates against all four nodes
2. Queries Harbor Homes' governance records for capital decisions
3. Queries BrightWorks' governance records for patronage ratification
4. Queries River City's governance records for federation agreements
5. Queries BrightWorks' ledger history for allocation provenance
6. Shows Finger Lakes CDN's federation-level view
7. Assembles a grant report from the queried evidence

---

## The Architecture in One Paragraph

ICN is a P2P coordination substrate. Each cooperative runs its own node (`icnd` daemon). The nodes gossip with each other but each maintains its own authoritative state. There is no central database. Governance is actor-based: proposals go through GovernanceActor, which manages domains (governance contexts), proposals, votes, and tallies. Ledger balances are double-entry with a Merkle-DAG for provenance. The federation layer handles cross-coop trust and value clearing. The gateway (port 8080) is a REST API that the demo scripts call via `curl`. Auth is DID-based challenge-response, handled transparently by `icnctl` inside each pod.

---

## Troubleshooting

### "demo_wait_ready: timed out"

A gateway didn't come up. Check pods:
```bash
kubectl get pods -A | grep icn-
```

If a pod is `0/1 Running` or `CrashLoopBackOff`:
```bash
kubectl logs -n icn-coop-gamma $(kubectl get pod -n icn-coop-gamma -o name | head -1) --tail=20
```

If the pod is fine but port 8080 isn't bound, the gateway patch may need reapplying. See `deploy/k8s/multi-node/gateway-patch.yaml`.

### "Could not obtain token"

`demo_get_token` runs `icnctl auth token` inside the pod. If that fails:
```bash
kubectl exec -n icn-coop-gamma \
  $(kubectl get pod -n icn-coop-gamma -o name | head -1) \
  -- icnctl auth token \
  --coop-id harbor-homes-cooperative \
  --scopes "governance:write,governance:read"
```

If `icnctl` is not found in the pod, the container image needs to be rebuilt with icnctl included.

### "Proposal creation failed"

Most proposal failures are `422 Unprocessable Entity` — the domain ID doesn't exist yet. The domain is created by `reseed-federation-demo.sh`. Re-run reseed first.

### "Reseed shows Failed: N"

Each failure has an explanation printed above the summary. Common causes:
- Pod not ready (wait 30s, try again)
- Domain already exists and name-uniqueness check failed (usually harmless — see the `aside` lines)
- Token fetch failed (check `kubectl exec` above)

### "I ran the flow but nothing seems to have happened"

The demo cluster uses in-memory state. If a pod restarted since the last reseed, all prior state (proposals, domains, coop records) was wiped. Run reseed again:
```bash
./demo/scripts/reseed-federation-demo.sh
```

---

## Resetting After a Demo

The demo is fully resettable:
```bash
./demo/scripts/reseed-federation-demo.sh
```

This:
- Closes any stale open proposals (can't delete them, so closes them)
- Re-creates the canonical coop records and governance domains
- Re-seeds the Q1 patronage proposal for Flow 2
- Leaves Harbor Homes and River City/BrightWorks clean for live flow runs

It's safe to run multiple times. It's idempotent.

---

## File Map

```
demo/
├── scripts/
│   ├── reseed-federation-demo.sh   # Reset to canonical state (run first)
│   ├── lib-demo-ports.sh           # Shared port-forward + auth library
│   ├── lib-demo-ports.sh.test      # Smoke tests for the library
│   ├── rehearsal-probe.sh          # Pre-demo sanity probe (run BEFORE first live demo)
│   ├── flow-1-governance.sh        # Harbor Homes roof repair (governance)
│   ├── flow-2-patronage.sh         # BrightWorks Q1 patronage (value legibility)
│   ├── flow-3-federation.sh        # River City + BrightWorks (federation)
│   └── flow-4-reporting.sh         # Finger Lakes CDN audit view (optional)
├── data/
│   ├── brightworks-members.json    # BrightWorks member roster + labor hours
│   ├── rivercity-members.json      # River City member roster + contribution hours
│   ├── harborhomes-members.json    # Harbor Homes member roster + capital reserve
│   ├── fingerlakes-members.json    # Finger Lakes CDN staff + member coops
│   ├── federation-proposals.json   # Canonical proposal definitions for all flows
│   └── federation-history.json     # Historical cross-coop transactions (narrative)
└── docs/
    ├── FEDERATION_RUNBOOK.md       # Full presenter runbook (speaking notes, fallback)
    ├── SELF_SERVE.md               # This file
    ├── api-map.md                  # Complete gateway API map
    └── gateway-secrets.md          # JWT secret topology and regeneration
```

---

## What's Not Shown in This Demo

**PR #1327 (ExecutionReceiptGate):** When this merges, Flow 1 upgrades from "governance and action are visible and linked" to "execution is cryptographically bound to the approved governance decision." The proof endpoint will return a signed GovernanceReceipt. Until then, the governance record is the audit trail.

**Pilot UI:** The demo is terminal-first. There's a web UI at http://10.8.30.40:30030 that shows coops, proposals, and balances. It's useful as a visual anchor for non-technical audiences.

**gRPC:** The nodes also expose a gRPC interface (ports 30651/30658/30649/30655). The demo uses the HTTP gateway exclusively.

**Mobile SDK:** A React Native SDK exists. Not in scope for this demo.

---

## For the Presenter

The full speaking notes, duration variants (5 / 12 / 20 minutes), fallback protocol, and audience-specific framing are in:

```
demo/docs/FEDERATION_RUNBOOK.md
```

Read that before going live.
