# Sprint 11: Federated Governance Wiring — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a federation-scoped proposal on one coop node, have it propagate to all 4 coop nodes via gossip, collect votes from multiple nodes, and verify converged outcomes everywhere.

**Architecture:** The federation gossip transport already exists (`icn-federation`). The governance actor already publishes to `federation:governance` topic when `ProposalScope::Federation`. The only code gap is that `CreateProposalRequest` has no `scope` field, so every proposal defaults to `Local`. We thread `scope` through the gateway → manager → actor → command chain, then deploy NodePorts + CORS to make each coop's gateway accessible externally. Scripts prove it works end-to-end.

**Tech Stack:** Rust (actix-web gateway, governance actor), K8s YAML (NodePort services), Bash (demo scripts)

---

## PR A: Thread `scope` through proposal creation (T1)

### Task 1: Add `ProposalScopeRequest` type to gateway models

**Files:**
- Modify: `icn/crates/icn-gateway/src/models.rs:296-301`

**Step 1: Add the scope request type**

After `CreateProposalRequest` (line 301), add:

```rust
/// Proposal scope — determines gossip routing
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalScopeRequest {
    /// Local to this cooperative (default)
    Local,
    /// Visible across a federation
    Federation {
        federation_id: String,
    },
}
```

**Step 2: Add `scope` field to `CreateProposalRequest`**

Add to the struct at line 296:

```rust
pub struct CreateProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub payload: ProposalPayloadRequest,
    /// Proposal scope. Omit or set to "local" for local-only.
    /// Set to {"type": "federation", "federation_id": "..."} for federation-wide.
    #[serde(default)]
    pub scope: Option<ProposalScopeRequest>,
}
```

**Step 3: Run `cargo check -p icn-gateway`**

Expected: compile errors in `governance_mgr.rs` because `create_proposal` doesn't accept scope yet.

**Step 4: Commit**

```bash
git add icn/crates/icn-gateway/src/models.rs
git commit -m "feat(gateway): add ProposalScopeRequest to CreateProposalRequest"
```

### Task 2: Thread scope through GovernanceManager

**Files:**
- Modify: `icn/crates/icn-gateway/src/governance_mgr.rs:525-540`

**Step 1: Add scope parameter to `GovernanceManager::create_proposal`**

Change signature at line 525:

```rust
pub async fn create_proposal(
    &self,
    proposal_id: ProposalId,
    domain_id: GovernanceDomainId,
    proposer: Did,
    title: String,
    description: String,
    payload: ProposalPayload,
    scope: icn_governance::ProposalScope,
) -> Result<ProposalId> {
```

In actor-backed mode (line 534-540), pass scope:

```rust
if let Some(ref handle) = self.governance_handle {
    let generated_id = handle
        .create_proposal(domain_id, title, description, payload, scope)
        .await?;
    return Ok(generated_id);
}
```

In standalone mode (line 556), apply scope:

```rust
let mut proposal = Proposal::new(domain_id, proposer, title, description, payload);
proposal = proposal.with_scope(scope);
proposal.id = proposal_id.clone();
```

**Step 2: Run `cargo check -p icn-gateway`**

Expected: compile errors because `GovernanceHandle::create_proposal` doesn't accept scope yet, and the call site in `governance.rs` doesn't pass scope.

**Step 3: Commit**

```bash
git add icn/crates/icn-gateway/src/governance_mgr.rs
git commit -m "feat(gateway): thread scope through GovernanceManager::create_proposal"
```

### Task 3: Thread scope through GovernanceActor

**Files:**
- Modify: `icn/apps/governance/src/actor.rs` (lines 121-131, 697-703, 848-858, 1280-1298)

**Step 1: Add scope to `GovernanceCommand::CreateProposal`**

At line 121:

```rust
CreateProposal {
    proposal_id: ProposalId,
    domain_id: GovernanceDomainId,
    title: String,
    description: String,
    payload: ProposalPayload,
    scope: icn_governance::ProposalScope,
},
```

**Step 2: Add scope to `GovernanceHandle::create_proposal`**

At line 697:

```rust
async fn create_proposal(
    &self,
    domain_id: GovernanceDomainId,
    title: String,
    description: String,
    payload: icn_governance::ProposalPayload,
    scope: icn_governance::ProposalScope,
) -> Result<ProposalId> {
```

At line 851 (submit call):

```rust
self.submit(GovernanceCommand::CreateProposal {
    proposal_id: proposal_id.clone(),
    domain_id,
    title,
    description,
    payload,
    scope,
})
.await?;
```

**Step 3: Apply scope in handler**

At line 1289-1298:

```rust
GovernanceCommand::CreateProposal {
    proposal_id,
    domain_id,
    title,
    description,
    payload,
    scope,
} => {
    info!("Creating proposal: {} (scope: {:?})", title, scope);

    let mut proposal = Proposal::new(
        domain_id,
        self.did.clone(),
        title.clone(),
        description,
        payload,
    );
    proposal = proposal.with_scope(scope);
    proposal.id = proposal_id.clone();
    // ... rest unchanged
```

**Step 4: Run `cargo check -p icn-governance-actor`**

Expected: should compile. May need to fix the trait definition if `create_proposal` is in a trait.

**Step 5: Commit**

```bash
git add icn/apps/governance/src/actor.rs
git commit -m "feat(governance): thread scope through GovernanceCommand::CreateProposal"
```

### Task 4: Wire scope in gateway API handler

**Files:**
- Modify: `icn/crates/icn-gateway/src/api/governance.rs:473-488`

**Step 1: Map scope from request and pass to manager**

After the payload conversion (line 471), add scope conversion:

```rust
// Convert scope (default to Local if not specified)
let scope = match &req.scope {
    Some(ProposalScopeRequest::Federation { federation_id }) => {
        icn_governance::ProposalScope::Federation(federation_id.clone())
    }
    Some(ProposalScopeRequest::Local) | None => {
        icn_governance::ProposalScope::Local
    }
};
```

At line 479, pass scope to `create_proposal`:

```rust
let proposal_id = gov_mgr
    .create_proposal(
        suggested_id,
        domain_id,
        proposer_did.clone(),
        req.title.clone(),
        req.description.clone(),
        payload,
        scope,
    )
    .await?;
```

Add the import at the top of the file:

```rust
use crate::models::ProposalScopeRequest;
```

**Step 2: Run full check**

```bash
cd icn/icn && cargo check --workspace
```

Expected: PASS

**Step 3: Run gateway tests**

```bash
cd icn/icn && cargo test -p icn-gateway --lib
```

Expected: existing tests pass (they don't specify scope, so default=Local).

**Step 4: Commit**

```bash
git add icn/crates/icn-gateway/src/api/governance.rs
git commit -m "feat(gateway): map ProposalScopeRequest to ProposalScope in create_proposal"
```

### Task 5: Add scope to proposal response

**Files:**
- Modify: `icn/crates/icn-gateway/src/api/governance.rs` (proposal list/get responses)

**Step 1: Check if proposal responses already include scope**

The `Proposal` struct from `icn-governance` includes `scope: ProposalScope` (with `#[serde(default)]`). If responses serialize the full `Proposal`, scope is already visible. Verify by checking how proposals are returned in list/get endpoints.

If proposals are returned directly as `Proposal` (not a DTO), no change needed — `scope` serializes automatically.

**Step 2: Run `cargo test -p icn-gateway`**

```bash
cd icn/icn && cargo test -p icn-gateway
```

Expected: PASS

**Step 3: Commit (only if changes needed)**

### Task 6: Build, push, and verify

**Step 1: Full workspace check**

```bash
cd icn/icn && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

**Step 2: Build Docker image**

```bash
cd /home/ubuntu/projects/icn && docker build -f deploy/Dockerfile.icnd -t icn:latest .
```

**Step 3: Push to registry**

```bash
docker tag icn:latest 10.8.10.40:30500/icn:latest
docker push 10.8.10.40:30500/icn:latest
```

**Step 4: Create PR**

```bash
git push -u origin feat/proposal-scope
gh pr create --title "feat(gateway): add scope field to proposal creation" --body "..."
```

---

## PR B: Coop Gateway NodePorts + CORS (T2)

### Task 7: Add gateway NodePort to deploy-coop.sh

**Files:**
- Modify: `deploy/k8s/multi-node/scripts/deploy-coop.sh:331-387`

**Step 1: Add gateway NodePort to services section**

After the existing NodePort service (line 387), add a gateway NodePort service. The port needs to be deterministic from coop name. Add a `NODEPORT_GATEWAY` variable:

```bash
# Add after line 71 (NODEPORT_RPC calculation)
NODEPORT_GATEWAY=$((30081 + (BASE_PORT - 7777)))
```

Add to the services YAML (after the existing NodePort service, before the closing `EOF`):

```yaml
---
apiVersion: v1
kind: Service
metadata:
  name: icn-${COOP_NAME}-gateway
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
spec:
  type: NodePort
  selector:
    app: icn
    coop: $COOP_NAME
  ports:
  - name: gateway
    port: 8080
    protocol: TCP
    targetPort: health
    nodePort: ${NODEPORT_GATEWAY}
```

**Step 2: Add CORS env var to deployment template**

In the deployment YAML (line 259-268), add after the HOME env var:

```yaml
        - name: ICN_CORS_ORIGINS
          value: "http://10.8.10.40:30030"
```

**Step 3: Print gateway port in summary**

After line 76 (`echo "  Metrics: $METRICS_PORT"`), add:

```bash
echo "  Gateway: 8080 (NodePort: $NODEPORT_GATEWAY)"
```

**Step 4: Commit**

```bash
git add deploy/k8s/multi-node/scripts/deploy-coop.sh
git commit -m "feat(deploy): add gateway NodePort and CORS to coop deployments"
```

### Task 8: Apply NodePort services to existing coops

**Step 1: Create gateway NodePort for each existing coop**

Run `kubectl apply` for each coop to add gateway NodePort services. The port numbers for our existing coops are:

Check the actual ports first:
```bash
# For each coop, check their QUIC port to derive gateway NodePort
kubectl -n icn-coop-alpha get svc icn-alpha -o jsonpath='{.spec.ports[?(@.name=="quic")].port}'
# Then: NODEPORT_GATEWAY = 30081 + (QUIC_PORT - 7777)
```

For the existing alpha/beta/gamma/delta coops, create gateway NodePort services:

```bash
# For each coop, apply a gateway NodePort service
for COOP in alpha beta gamma delta; do
    NS="icn-coop-${COOP}"
    # Get the existing gateway port
    NODEPORT=$(kubectl -n $NS get svc icn-${COOP}-nodeport -o jsonpath='{.spec.ports[0].nodePort}')
    # Derive gateway nodeport: subtract 30777 base, add 30081 base
    OFFSET=$((NODEPORT - 30777))
    GW_PORT=$((30081 + OFFSET))

    kubectl apply -f - <<EOF
apiVersion: v1
kind: Service
metadata:
  name: icn-${COOP}-gateway
  namespace: $NS
  labels:
    app: icn
    coop: $COOP
spec:
  type: NodePort
  selector:
    app: icn
    coop: $COOP
  ports:
  - name: gateway
    port: 8080
    protocol: TCP
    targetPort: health
    nodePort: $GW_PORT
EOF
    echo "Created gateway NodePort for $COOP at :$GW_PORT"
done
```

**Step 2: Add CORS to existing coop deployments**

For each coop, patch the deployment to add `ICN_CORS_ORIGINS`:

```bash
for COOP in alpha beta gamma delta; do
    NS="icn-coop-${COOP}"
    kubectl -n $NS set env deployment/icn-${COOP} ICN_CORS_ORIGINS="http://10.8.10.40:30030"
done
```

**Step 3: Verify all 4 coop gateways respond**

```bash
NODE_IP=10.8.10.40
for PORT in 30081 30082 30083 30084; do
    echo -n "Port $PORT: "
    curl -s "http://${NODE_IP}:${PORT}/v1/health" | jq -r '.status'
done
```

Expected: all return `ok`

**Step 4: Commit and create PR**

```bash
git push -u origin fix/coop-gateway-nodeports
gh pr create --title "feat(deploy): add gateway NodePorts for coop instances" --body "..."
```

---

## PR C: Federation Init + Demo Scripts (T3, T4, T5, T7)

### Task 9: Create federation-init.sh

**Files:**
- Create: `scripts/federation-init.sh`

**Step 1: Write the init script**

```bash
#!/usr/bin/env bash
# Initialize federation on all coop instances.
# Idempotent: re-running is safe.
set -euo pipefail

NODE_IP="${NODE_IP:-10.8.10.40}"
COOPS=("alpha:30081" "beta:30082" "gamma:30083" "delta:30084")
FED_ID="${FED_ID:-pilot-fed}"

echo "Initializing federation '${FED_ID}' on ${#COOPS[@]} coops..."

for entry in "${COOPS[@]}"; do
    COOP="${entry%%:*}"
    PORT="${entry##*:}"
    URL="http://${NODE_IP}:${PORT}"

    echo -n "  ${COOP} (${URL}): "

    # Get the node's DID
    DID=$(curl -sf "${URL}/v1/health" | jq -r '.version // "unknown"')

    # Check if already initialized
    STATUS=$(curl -sf "${URL}/v1/federation/status" 2>/dev/null || echo '{"initialized":false}')
    INITIALIZED=$(echo "$STATUS" | jq -r '.initialized // false')

    if [ "$INITIALIZED" = "true" ]; then
        echo "already initialized"
        continue
    fi

    # Initialize federation
    RESULT=$(curl -sf -X POST "${URL}/v1/federation/init" \
        -H "Content-Type: application/json" \
        -d "{
            \"coop_id\": \"${COOP}\",
            \"name\": \"${COOP} cooperative\",
            \"federation_id\": \"${FED_ID}\"
        }" 2>&1) || true

    echo "${RESULT}" | jq -r '.status // .error // "done"'
done

echo ""
echo "Federation status:"
for entry in "${COOPS[@]}"; do
    COOP="${entry%%:*}"
    PORT="${entry##*:}"
    echo -n "  ${COOP}: "
    curl -sf "http://${NODE_IP}:${PORT}/v1/federation/status" | jq -c '.' 2>/dev/null || echo "unreachable"
done
```

**Step 2: Make executable and commit**

```bash
chmod +x scripts/federation-init.sh
git add scripts/federation-init.sh
git commit -m "feat(scripts): add federation-init.sh for multi-coop federation setup"
```

### Task 10: Create federated-governance-demo.sh (T7)

**Files:**
- Create: `scripts/federated-governance-demo.sh`

**Step 1: Write the demo script**

This script proves the full federation governance lifecycle. It:
1. Health-checks all 4 coop gateways
2. Obtains JWT tokens for each coop (via `kubectl exec`)
3. Creates a governance domain on alpha
4. Creates a federation-scoped proposal on alpha
5. Verifies the proposal propagated to beta/gamma/delta
6. Opens voting, casts votes from alpha + beta
7. Verifies tally convergence on all coops
8. Closes the proposal, verifies closed state everywhere

Structure should follow `scripts/governance-demo.sh` as the template, but operate across all 4 coops with per-coop auth tokens and endpoints.

Key differences from governance-demo.sh:
- `NODE_IP` and per-coop ports (30081-30084) instead of single :30080
- Per-coop JWT tokens obtained from each coop's pod
- Proposal scope: `{"type": "federation", "federation_id": "pilot-fed"}`
- Propagation verification: poll each coop's proposal list
- Vote from multiple coops using their respective tokens
- Tally convergence check: compare tallies across all coops

**Step 2: Make executable and test**

```bash
chmod +x scripts/federated-governance-demo.sh
./scripts/federated-governance-demo.sh
```

Expected: exits 0, all steps pass

**Step 3: Commit**

```bash
git add scripts/federated-governance-demo.sh
git commit -m "feat(scripts): add federated-governance-demo.sh proof-of-life"
```

### Task 11: Create PR

```bash
git push -u origin feat/federation-demo
gh pr create --title "feat(scripts): federation init + federated governance demo" --body "..."
```

---

## PR D: Pilot UI Federation View (T6)

### Task 12: Add federation indicators to governance tab

**Files:**
- Modify: `web/pilot-ui/app.js` (governance tab section)

**Step 1: Add origin coop badge to proposal cards**

In the governance tab's proposal rendering, check if the proposal has `scope.federation` and display:
- An origin badge: "Created on: alpha" (derive from proposal's proposer DID or scope metadata)
- A federation indicator: small icon or text showing "Federation: pilot-fed"

**Step 2: Add "seen on N coops" indicator**

Add a function that checks the proposal exists on each coop gateway:

```javascript
async function checkProposalPropagation(proposalId) {
    const coops = [
        { name: 'alpha', port: 30081 },
        { name: 'beta', port: 30082 },
        { name: 'gamma', port: 30083 },
        { name: 'delta', port: 30084 },
    ];
    let seen = 0;
    for (const coop of coops) {
        try {
            const res = await fetch(`http://${NODE_IP}:${coop.port}/v1/gov/proposals/${proposalId}`);
            if (res.ok) seen++;
        } catch { /* unreachable */ }
    }
    return { seen, total: coops.length };
}
```

Display as "Seen on 4/4 coops" badge in the proposal detail.

**Step 3: Test in browser**

Navigate to Pilot UI → Governance tab → verify federation badges appear on federation-scoped proposals.

**Step 4: Commit and PR**

```bash
git add web/pilot-ui/app.js
git commit -m "feat(pilot-ui): add federation indicators to governance tab"
git push -u origin feat/pilot-ui-federation
gh pr create --title "feat(pilot-ui): federation view indicators" --body "..."
```

---

## Merge Order

1. **PR A** (scope field) — independent Rust change
2. **PR B** (NodePorts + CORS) — independent K8s/deploy change
3. **PR C** (scripts) — depends on A + B being deployed
4. **PR D** (Pilot UI) — depends on A + B being deployed

A and B can merge in parallel. C and D wait for A + B to be deployed to the cluster.

---

## Acceptance Criteria

1. `POST /v1/gov/proposals` with `"scope": {"type": "federation", "federation_id": "pilot-fed"}` returns 201 on alpha
2. Proposal appears on beta/gamma/delta within 30 seconds
3. Votes from alpha + beta produce identical tallies on all 4 coops
4. Close on alpha → closed state on all 4 coops
5. `federated-governance-demo.sh` exits 0 with clean transcript
6. Pilot UI shows federation badge on federation-scoped proposals

## Key File Map

| File | Change | PR |
|------|--------|-----|
| `icn/crates/icn-gateway/src/models.rs` | Add `ProposalScopeRequest`, scope field | A |
| `icn/crates/icn-gateway/src/api/governance.rs` | Map scope, pass to manager | A |
| `icn/crates/icn-gateway/src/governance_mgr.rs` | Accept scope parameter | A |
| `icn/apps/governance/src/actor.rs` | Scope in command, handle, handler | A |
| `deploy/k8s/multi-node/scripts/deploy-coop.sh` | Gateway NodePort + CORS | B |
| `scripts/federation-init.sh` | New: init federation on coops | C |
| `scripts/federated-governance-demo.sh` | New: end-to-end proof script | C |
| `web/pilot-ui/app.js` | Federation indicators | D |

## Risks

| Risk | Mitigation |
|------|------------|
| Gossip not connecting between coops | Check mDNS discovery, verify coops can see each other with `/v1/federation/status` |
| Proposals don't propagate | Check governance actor logs for federation publish attempts, verify gossip topic subscription |
| Port conflicts (NodePorts overlap) | Ports derived deterministically from coop name hash; verify no collision |
| CORS issues on coop gateways | Same fix as Sprint 10: `ICN_CORS_ORIGINS` env var on each coop deployment |
