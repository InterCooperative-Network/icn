# Flow 5 — Presenter Notes: Commons Compute
**Audience**: Tech cooperatives, federation builders, commons-compute advocates (some technical literacy assumed)
**Duration**: ~10 minutes with pauses
**Key message**: The commons pool runs on trust, not accounts. Any cooperative with standing can submit work; the network enforces fairness without a central platform.

---

## Beat: Step 0 — Trust seeding
**Say**: "Before Finger Lakes CDN can submit work to the commons pool, the network checks their standing. We're seeding their trust score — 0.85 out of 1.0. This is their track record of contributions made visible to the system."
**Point to**: The gRPC trust seed output
**If asked "What is a trust score?"**: "It's a measure of how reliably a cooperative has participated in the network — contributed resources, honored agreements, paid it forward. A new member starts at zero and earns trust over time. The score determines what you can do, not whether you can sign up."
**If asked "Why gRPC?"**: "The trust graph lives inside the daemon — gRPC is the direct channel to update it. The HTTP API for trust was added after this deployment was built; we'll see it in production next sprint."

---

## Beat: Step 1 — Scoped authentication
**Say**: "Finger Lakes CDN is authenticating now — but not with a username and password. They're generating a token that says exactly what they're allowed to do: submit compute tasks, check task status, and read their ledger position. Nothing more."
**Point to**: The scope list in the output
**If asked "Who decides the scopes?"**: "The cooperative's own governance policy does. The gateway enforces whatever scopes are configured — it doesn't decide what's appropriate. Governance decides; technology enforces."
**If asked "Can scopes be faked?"**: "No. The token is signed by the node's Ed25519 key. If it says compute:write, the key that signed it had the authority to issue compute:write. The gateway verifies the signature on every request."

---

## Beat: Step 2 — Submit to the commons pool
**Say**: "This is the core moment. Finger Lakes CDN is submitting a route optimization task to the commons pool. Two gates have to pass before it's admitted: does the token have compute:write scope? Does the cooperative have enough trust standing? Both passed — we got HTTP 200."
**Point to**: The task_id and task_hash in the output
**If asked "What is the task hash?"**: "It's a 32-byte fingerprint of the entire submission — code, inputs, submitter identity, timestamp. That hash follows the task through its entire lifecycle. When it completes, the settlement receipt will include this hash as the provenance link: 'this credit earned traces back to this task.'"
**If asked "What is CCL?"**: "Cooperative Contract Language — ICN's own execution language. It's deterministic and fuel-metered, meaning a task can only consume a declared amount of computation. No runaway processes, no surprise bills."
**If asked "Why route optimization?"**: "Finger Lakes CDN coordinates bandwidth routing across the region. Route optimization is real compute work cooperatives do. We're not demoing a toy — we're showing the kind of task a real CDN would offload to the commons pool."

---

## Beat: Step 3 — Ledger position
**Say**: "Here's Finger Lakes CDN's standing in the mutual credit system. Before any task executes, the system checks whether they have the credit line to cover it. This is credit reservation — like a hold on a card, except it belongs to a cooperative and not a bank."
**Point to**: The ledger position response (or the 404 / zero balance message)
**If position is 404 or zero**: "They have no prior activity — this is their first interaction with the commons pool. That's allowed. New members can participate; their credit line grows as they contribute. Trust gates access; credits settle after the work is done."
**If asked "What is mutual credit?"**: "It's a credit system where the cooperative network issues credit based on real contributions — not capital. When Finger Lakes CDN routes traffic for other cooperatives, they earn credits. When they spend compute, they spend credits. No bank, no interest, no extraction."
**If asked "What if they go negative?"**: "The system has a credit floor — configured by governance, not hard-coded. Going below the floor blocks further spending until the balance is restored through contribution. The cooperative's members set the floor through their own governance policy."

---

## Beat: Step 4 — Task status: Pending
**Say**: "The task is Pending — meaning it's been accepted into the pool and is waiting for an executor node to claim it. Think of it like a job board: the task is posted, qualified executors will pick it up. In this cluster, we haven't wired the executor nodes yet — that's next sprint."
**Point to**: The status: pending in the output
**If asked "Why is it stuck pending?"**: "We proved the admission layer — that's what this flow demonstrates. The executor layer is being wired into this cluster in Sprint 28. Right now we're showing that the coordination substrate works: trust gates, scope enforcement, task queuing. The execution layer drops in on top."
**If asked "When will it complete?"**: "Once executor nodes are registered in the cluster, this task — or tasks like it — will transition to Processing and then Completed within seconds. The settlement receipt is generated on completion and anchored to the task hash we just saw."
**If asked "Is there a timeout?"**: "Tasks can be cancelled. See the cancel endpoint in the API. The fuel limit also functions as an execution bound — if an executor would need more than the declared fuel to run the task, it won't claim it."

---

## Beat: Step 5 — Authorization boundary
**Say**: "Now we try to submit a task with a token that only has ledger:read — no compute:write. Watch what happens."
**Point to**: The 4xx response
**If asked "Why does this matter?"**: "Because in a system anyone can connect to, the authorization layer is the entire security story. A member can't exceed their granted capabilities. A rogue node can't submit tasks it wasn't authorized to submit. The cooperative's governance policy is enforced at every request, not just at signup."
**If asked "What if someone steals the token?"**: "Tokens expire — configurable lifetime, default one hour. A stolen token is a time-limited credential. The keystore is Age-encrypted on disk; without the passphrase, no new tokens can be issued. And each token is scoped — a stolen ledger:read token can't submit compute tasks."

---

## Beat: Summary
**Say**: "What we just showed: a cooperative submitted real compute work to a commons pool. The trust graph admitted it. The ledger tracked the credit reservation. The scope system enforced the authorization boundary. All without a central platform, all verifiable by any member. The task is queued — executor wiring completes the story next sprint."
**If asked "What does this cost the cooperative?"**: "Nothing to run their own node. The mutual credit system pays contributors in the network's own credit unit — not dollars. The goal is to make compute a cooperative commons: the more you contribute, the more you can use."
**If asked "How is this different from renting cloud compute?"**: "You own it. The rules are set by the cooperative, not a vendor. There's no profit extraction — the system settles among members. And when you leave, you take your node and your trust history with you."

---

## Trouble Scenarios

**Compute submit returns HTTP 500 "Internal server error"**:
Trust score is 0.0. The trust seed in Step 0 failed or the pod was restarted. Re-run the trust seed command:
```
kubectl exec -n icn-coop-delta deploy/icn-delta -- \
  icnctl --endpoint "[::1]:5655" trust add \
  did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln 0.85 --label "compute-demo"
```
Then retry the submit.

**Token acquisition fails (exit 1)**:
Passphrase is read from K8s Secret `icn-delta-secrets[passphrase]`. If the pod was rebuilt, the passphrase may have changed. Check with:
```
kubectl get secret icn-delta-secrets -n icn-coop-delta -o jsonpath='{.data.passphrase}' | base64 -d | wc -c
```
Should return 32.

**gRPC trust seed fails with "Connection refused"**:
Port 5655 is the gRPC port inside the pod. Confirm it's listening:
```
kubectl exec -n icn-coop-delta deploy/icn-delta -- \
  cat /proc/net/tcp6 | awk '{print $2}' | cut -d: -f2 | \
  while read hex; do printf '%d\n' "0x$hex" 2>/dev/null; done | sort -n | uniq
```
Should include 5655. If not, the daemon may have crashed — check logs:
```
kubectl logs -n icn-coop-delta deploy/icn-delta --tail=50
```

**Authorization step returns 200 instead of 4xx**:
The deployed binary may have looser scope enforcement than the current codebase. Note it for the audience and move on — the admission gate (Step 2) is the higher-priority gate to prove.

---

## Connections to Other Flows

- **Flow 1–3** prove the governance and federation layers. Flow 5 builds on top of that — a cooperative with established trust relationships can use the commons compute pool.
- **Flow 2** shows patronage settlement. The settlement receipt from a completed Flow 5 task would look similar — same receipt chain, same provenance structure.
- **The task hash from Step 2** is the anchor for the full provenance chain: task → execution receipt → credit settlement → audit trail. Flows 1–5 together tell the complete story: govern → federate → compute → settle.
