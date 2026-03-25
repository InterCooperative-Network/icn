---
description: Validate demo flow readiness — check cluster health, pod state, P2P mesh, and all four demo flows
allowed-tools: Read, mcp__icn-dev__icn_icn_status, mcp__icn-dev__icn_kubectl, mcp__icn-dev__icn_ssh
---

Run a comprehensive demo readiness check across all four ICN demo flows.

**Step 1: Cluster Health**

Use `icn_icn_status` to get a quick overview, then check each area:

```
kubectl get nodes -o wide
kubectl get pods -A
df -h / /var/lib/rancher  (check disk, Rust artifacts fill this)
```

Flag any:
- Node not Ready
- Pod in CrashLoopBackOff or Error
- Disk > 80% used

**Step 2: Daemon Health**

```
curl -s http://10.8.30.40:8080/health | python3 -m json.tool
```

Expected: `{"status": "healthy"}` or similar. Flag any unhealthy components.

**Step 3: P2P Mesh (Demo Flow 3 & 4 prerequisite)**

Check that nodes are advertising real IPs, not 0.0.0.0:

```
kubectl logs -n icn-alpha deployment/icn-node --tail=50 | grep -i "advertis\|addr\|peer\|0.0.0.0"
kubectl logs -n icn-beta deployment/icn-node --tail=50 | grep -i "advertis\|addr\|peer"
```

Also check peer count:
```
kubectl exec -n icn-alpha deployment/icn-node -- icnctl net peers 2>/dev/null || echo "icnctl not available in pod"
```

**Step 4: Service Discovery (PR #1381 dependency)**

Restart one pod and check if it re-joins the mesh:
```
kubectl rollout restart deployment/icn-node -n icn-beta
sleep 15
kubectl get pods -n icn-beta
# Then check peer list again
```

**Step 5: Demo Flow 1A — Governance**

```
icnctl proposal create --title "Test Proposal" --body "Demo check" --coop alpha
# note the proposal ID
icnctl vote --proposal <id> --choice yes --member member1
icnctl proposal status --id <id>
```

Expected: proposal transitions to Passed status.

**Step 6: Demo Flow 1B — Governance + Cryptographic Provenance**

Check if signing key is configured:
```
kubectl get secret icn-signing-key -n icn-alpha 2>/dev/null || echo "MISSING: signing key secret not found"
```

If missing: Flag as BLOCKED. Flow 1B cannot run without this.

**Step 7: Demo Flow 2 — Patronage Distribution**

```
icnctl patronage distribute --coop alpha --period current
icnctl account position --member member1 --coop alpha
```

Expected: member positions updated.

**Step 8: Demo Flow 3 — Federation (inter-coop)**

```
# Try to send a message from alpha to beta
icnctl federation ping --from alpha --to beta 2>/dev/null || echo "Federation not operational"
```

**Step 9: ops/mcp State**

```
ssh icn-dev "cd ~/projects/icn/ops/mcp && git status --short"
```

Flag if uncommitted changes (blocks session tracking and decision audit trail).

**Output format:**
```
## Demo Readiness Report — <date>

### Cluster Health: PASS / FAIL
- Nodes: X/3 Ready
- Pods: X/Y Running
- Disk: X% used

### Daemon Health: PASS / FAIL

### P2P Mesh: PASS / FAIL / UNKNOWN
- Address advertisement: OK / BLOCKED (0.0.0.0)
- Peer discovery: X peers visible

### Flow 1A (Governance): PASS / FAIL
### Flow 1B (Governance + Provenance): PASS / BLOCKED
- Blocker: ...

### Flow 2 (Patronage): PASS / FAIL
### Flow 3 (Federation): PASS / FAIL / BLOCKED
- Blocker: ...
### Flow 4 (Reporting): PASS / BLOCKED (depends on Flow 3)

### ops/mcp: CLEAN / DIRTY
- Uncommitted: X files

### Summary
- Flows ready for demo: X/4
- Critical blockers: ...
- Recommended next actions: ...
```
