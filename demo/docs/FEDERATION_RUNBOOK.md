# A Federation in Motion — Presenter Runbook

**Demo system:** ICN federation demo — 4-node K3s cluster
**Date:** 2026-03-07
**Audience variants:** Cooperators, funders, cooperative developers, technical collaborators
**Flows:** 1A (governance), 2 (patronage), 3 (federation), 4 (reporting, optional)

---

## Before You Read Anything Else

The terminal is stage machinery. The story is the subject.

Every flow opens with a cooperator question — a real problem that real cooperatives face. Keep that question visible. The code is evidence. The cooperative pain is the subject.

---

## Pre-Flight Checklist

Run this before anyone is in the room:

```bash
# 1. Confirm cluster is healthy
kubectl get pods -A | grep icn-

# 2. Confirm gateways are bound (should show LISTEN on 8080)
for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
  pod=$(kubectl get pods -n $ns -o name | head -1)
  kubectl exec -n $ns $pod -- ss -tlnp 2>/dev/null | grep 8080 || echo "$ns: 8080 NOT bound"
done

# 3. Reseed to canonical state
./demo/scripts/reseed-federation-demo.sh

# 4. Confirm reseed succeeded (should show "Seeded: N, Skipped: M, Failed: 0")

# 5. Test ports open cleanly
kubectl port-forward -n icn-coop-gamma svc/icn-gamma 18083:8080 &
sleep 1
curl -s http://localhost:18083/v1/health
kill %1
```

If any pod shows `0/1 Running` or the port-forward fails: do NOT proceed to live demo. Use the prerecorded fallback.

---

## Terminal Setup

Two terminals side by side:

- **Left (script):** Where you run the flow scripts. Font size 20+. Light background if the room has bad monitors.
- **Right (browser or second terminal):** Open to the Pilot UI at http://10.8.30.40:30030 (or have it showing previous run output if the UI isn't responsive).

Set your prompt to something minimal (`PS1='$ '`) — the default Zsh prompt with git branch status adds visual noise.

---

## The One-Sentence Frame

Before running anything, say this once:

> "Cooperatives are supposed to be more democratic than firms. But in practice, they often inherit the same problems: unclear decisions, disconnected records, admin overhead, weak federation tooling, and no easy way to prove what happened across organizations. ICN is built to solve that coordination gap — starting with the problems cooperatives actually have."

Then go straight to the flow.

---

## Duration Variants

### 5 minutes — "The single decision" (Flow 1A only)

**Use when:** You have a single slot at a conference, a five-minute lightning talk, or a skeptical cooperator who wants to see one concrete thing.

**What to run:**
```bash
./demo/scripts/flow-1-governance.sh
```

**Speaking notes:**
- Open with the Harbor Homes inspection report (Step 1 output). Read the quote. "This is a $12,000 decision made by a volunteer board with 12 members. The question is: did the vote that authorized this actually happen, and can we prove it?"
- Don't narrate every curl. Let the output speak.
- At Step 7 (proposal closed), pause: "This decision just became part of Harbor Homes' permanent governance record. Any member can query it — not just the board."
- End: "That's one coop, one decision, one proof. The system generalizes."

**Skip:** Steps 8, 9, 10 can be condensed into the summary paragraph.

---

### 12 minutes — "Governance and value" (Flows 1 + 2)

**Use when:** You have a worker coop or housing coop audience with 10-15 minutes.

**What to run:**
```bash
./demo/scripts/flow-1-governance.sh
# Pause for 30 seconds of commentary
./demo/scripts/flow-2-patronage.sh
```

**Transition between flows:**
> "That was Harbor Homes — a housing coop proving a board decision. Now let's look at BrightWorks, a worker cooperative. Their question is different: not 'did the vote happen?' but 'why did this worker get this allocation?'"

**Speaking notes for Flow 2:**
- Table at Step 3 is the demo's clearest visual. Point to it. "Every member can re-derive every number in this table from two public inputs: their labor hours and the total surplus."
- At Step 5: "The vote doesn't decide the amounts — the formula does. The vote ratifies that the formula was applied correctly."
- At Step 11 (receipt chain): "This is what makes patronage legible, not just a number on a statement. The chain connects the balance to the vote that authorized it."

---

### 20 minutes — "A federation in motion" (Flows 1 + 2 + 3 + optional 4)

**Use when:** Full demo slot, mixed audience, or a funder briefing.

**What to run:**
```bash
./demo/scripts/flow-1-governance.sh
./demo/scripts/flow-2-patronage.sh
./demo/scripts/flow-3-federation.sh
# Optional:
./demo/scripts/flow-4-reporting.sh
```

**Pacing:**
- Flow 1: 6 min
- Flow 2: 6 min
- Flow 3: 6 min
- Flow 4: 5 min (if included)
- Buffer / Q&A: 3 min

**Transitions:**

*After Flow 1 → Flow 2:*
> "One coop, one decision. Now let's widen the frame: how does a cooperative track and justify value allocation when the question is 'was this fair?'"

*After Flow 2 → Flow 3:*
> "Two coops, operating completely independently. River City has equipment. BrightWorks has maintenance workers. They have complementary needs but no shared infrastructure. The question: can they work together without one becoming dependent on the other?"

*After Flow 3 → Flow 4:*
> "That was the coops' view. Let's switch to the intermediary's view. Finger Lakes CDN needs to produce a grant report. The funder wants evidence of accountable governance. Without ICN, Amara Diallo is on the phone asking three coops to email her spreadsheets."

---

## Per-Flow Speaking Notes

### Flow 1 — Governance Legitimacy (Harbor Homes)

**Cooperator question:** "Did the thing we voted on actually happen, and can we prove it?"

**Opening (before running):**
> "Harbor Homes Cooperative manages 48 housing units. Their board is volunteers. There's no full-time admin. A building inspector just flagged a $12,000 emergency roof repair — and the question isn't just 'should we fix it?' It's 'if we vote yes, how do we know the expenditure actually followed the vote?'"

**At Step 3 (proposal created):**
> "The proposal is live. Every member can read the inspection report, the contractor quote, the reserve balance — before voting. No board chair summarizing it. The record is the record."

**At Step 5 (vote cast):**
> "In a full Harbor Homes deployment, each of the 12 voting members would hold their own DID and cast an independent vote. The demo cluster has one seeded identity — but the mechanism is the same."

**At Step 7 (proposal closed):**
> "Accepted. And here's what that means: the governance decision is now permanent. Harbor Homes' treasury staff have authorization to pay the contractor. The proposal ID links the authorization back to the vote. Any member can verify the chain."

**Flow 1A / 1B distinction (always say this):**
> "What we've shown is the governance and the authorized action, linked and visible. What's being finalized — in PR #1327 — is the final enforcement layer: the execution receipt that cryptographically binds the spend to the approval. Until that merges, what we have is 'visible and auditable.' After it merges, we have 'machine-verifiable.'"

---

### Flow 2 — Patronage and Value (BrightWorks)

**Cooperator question:** "Why did this member get this distribution?"

**Opening (before running):**
> "Worker cooperatives distribute surplus as patronage — your contribution determines your share. The political problem is: how does a member know the allocation was fair? How do they verify it without trusting the treasurer blindly?"

**At Step 3 (allocation table):**
> "This table is the answer. The formula is in the proposal. The labor hours are in the record. Any member who thinks they got the wrong number can re-derive it themselves from two public inputs. No black box."

**At Step 5 (vote to ratify):**
> "Notice what's happening here: the vote doesn't set the amounts. The formula does. The vote is the members saying: 'yes, the formula was applied correctly.' That's the difference between a legitimate allocation and one that just got announced."

**At Step 11 (receipt chain, if available):**
> "This is what distinguishes 'we paid this person' from 'we can prove why.' The receipt links the balance to the decision that authorized it."

**Audience skins:**
- Tool library: "Replace 'labor hours' with 'volunteer hours' — same mechanism."
- Food coop: "Replace 'surplus credits' with 'patronage dividends' — same mechanism."
- Federation: "Replace 'one coop' with 'shared program participation across member orgs' — same mechanism."

---

### Flow 3 — Federation (River City + BrightWorks)

**Cooperator question:** "How do independent co-ops work together without creating another bureaucracy?"

**Opening (before running):**
> "Two Rochester cooperatives, zero shared infrastructure. River City has metalworking equipment sitting idle during off-peak hours. BrightWorks makes sustainable building materials and needs occasional metalworking access. They have complementary needs but no way to formalize the relationship without creating a new administrative entity."

**At Step 3 (River City governance vote):**
> "River City just voted on its own node, with its own members, using its own governance domain. Finger Lakes CDN didn't vote. BrightWorks didn't vote. This is River City's autonomous decision."

**At Step 4 (BrightWorks governance vote):**
> "Now BrightWorks does the same — independently, on their own node. Two separate API calls to two separate URLs. Two separate governance decisions."

**At Step 9 (three-node query):**
> "Watch this: three separate queries to three separate nodes. River City's record is queryable at River City's address. BrightWorks' record is queryable at BrightWorks' address. Finger Lakes CDN has a coordination view — but not control. If Finger Lakes CDN disappears tomorrow, the two coops' records remain."

**Closing:**
> "This is what 'autonomous coordination' means. The federation adds value without adding dependency. Coops can leave; their governance records stay. No central entity can be captured because there isn't one."

---

### Flow 4 — Reporting (Finger Lakes CDN) [Optional]

**Institutional question:** "Can this produce trustworthy reporting without adding massive admin overhead?"

**Opening (before running):**
> "Amara Diallo at Finger Lakes CDN needs to submit a grant report. The foundation wants evidence of accountable governance across member coops. Traditionally, this means emails, spreadsheets, follow-up calls. With ICN, it means queries."

**At Step 9 (grant report table):**
> "Every line in this report was generated by querying member coop nodes directly. No spreadsheet. No email. No coop was asked to grant Finger Lakes CDN administrative access. The records are just... there."

**On the Flow 1B gap:**
> "The items marked 'scope gap' in the output are deployment constraints in this cluster, not design gaps. The receipt chain and cryptographic proof endpoints exist in the code — they need their signing keys configured. That's also what PR #1327 completes."

---

## Fallback Protocol

### If port-forward fails to start

```bash
# Kill any stuck forwards
pkill -f "kubectl port-forward" || true
sleep 2
# Retry
./demo/scripts/flow-1-governance.sh
```

If it still fails: switch to prerecorded terminal output. The audience doesn't need to see it live to understand the concept. Narrate what they're seeing.

### If a gateway returns 502/503

The pod probably restarted and lost its in-memory state. Run reseed and try again:

```bash
./demo/scripts/reseed-federation-demo.sh
# Then restart the flow
```

If the pod itself is crashing:
```bash
kubectl get pods -n icn-coop-gamma
kubectl logs -n icn-coop-gamma <pod-name> --tail=20
```

Do NOT try to fix a crashing pod during a live presentation. Use the fallback output.

### If `demo_wait_ready` times out

The pod may be running but the gateway hasn't started yet. Check:
```bash
kubectl exec -n icn-coop-gamma <pod> -- ss -tlnp | grep 8080
```

If 8080 is not bound: the gateway patch may have rolled back. Re-apply:
```bash
cd /home/ubuntu/projects/icn
kubectl patch deployment icn-gamma -n icn-coop-gamma --type=strategic --patch '{
  "spec":{"template":{"spec":{"containers":[{
    "name":"icnd",
    "args":["--config","/etc/icn/icn.toml","--gateway-enable","--gateway-bind","0.0.0.0:8080"],
    "env":[{"name":"ICN_GATEWAY_JWT_SECRET","valueFrom":{"secretKeyRef":{"name":"icn-gamma-secrets","key":"jwt-secret"}}}]
  }]}}}}'
```

See `deploy/k8s/multi-node/gateway-patch.yaml` for all four coops.

### If a proposal is in an unexpected state

The demo creates proposals with run-tag suffixes, so repeated runs don't collide. If you see an unexpected state from a previous run:

```bash
./demo/scripts/reseed-federation-demo.sh
```

Reseed will close any stale open proposals and re-seed the canonical ones.

### If icnctl token fetch fails

```bash
# Check the pod is running
kubectl get pod -n icn-coop-gamma

# Try manual token fetch
kubectl exec -n icn-coop-gamma \
  $(kubectl get pod -n icn-coop-gamma -o name | head -1) \
  -- icnctl auth token \
  --coop-id harbor-homes-cooperative \
  --scopes "governance:write,governance:read"
```

If icnctl is missing from the pod: the image was rebuilt without it. The demo cannot proceed on live cluster without icnctl.

---

## Mid-Demo Reset

If something goes wrong and you want to restart cleanly mid-presentation:

```bash
# Kill any stuck port-forwards
pkill -f "kubectl port-forward" || true

# Reseed canonical state
./demo/scripts/reseed-federation-demo.sh

# Restart the flow
./demo/scripts/flow-1-governance.sh
```

Tell the audience: "Let me reset to the clean state — this is exactly why a reseed script exists."

---

## Audience-Specific Framing

### For cooperators (housing, worker, food coop)

Lead with the pain first. The tech is the last thing you mention.

> "Harbor Homes has 48 units and a volunteer board. They don't have a blockchain problem. They have a coordination problem — decisions that should be traceable, aren't. ICN solves that."

### For funders and grant reviewers

Lead with accountability and scale.

> "If you fund ten cooperatives, you need evidence that your funding drove accountable decisions — not just their word for it. ICN lets you query governance records directly. No spreadsheets. No follow-up emails. The records are on-chain."

### For cooperative developers and ecosystem builders

Lead with the federation story.

> "You're trying to support member coops without becoming their central authority. Every coordination tool you build risks creating the dependency you're trying to avoid. ICN's federation layer is designed so that your visibility and their autonomy aren't in conflict."

### For technical collaborators

Lead with architecture, but be honest.

> "Four nodes, separate governance, gossip sync, clearing for cross-coop value. 2,000+ tests passing. The receipt-gated enforcement layer is in review — PR #1327. What you're seeing in Flow 1A is the current demonstrable scope. Flow 1B shows what that PR completes."

---

## Honest Scope Language

**Before PR #1327 merges (Flow 1A):**
> "The governance and the action are linked and visible — the final enforcement-proof layer is being finalized."

**After PR #1327 merges (Flow 1B):**
> "This proposal was approved through cooperative governance. The resulting action is not merely recorded after the fact — it is bound through the execution receipt path, so the system can prove the action corresponds to approved governance."

**On "is this production-ready?":**
> "We're in pilot phase with real cooperatives. The core infrastructure has 2,000+ tests passing. We're building out the presentation and handoff layer now, and hardening the final proof linkage."

**Do not say:**
- "Cryptographic proof" for the current state (Flow 1A). That claim requires #1327.
- "Blockchain" at any point. ICN is not a blockchain.
- "This is live in production" until the pilot cooperatives have been formally onboarded.

---

## Node Reference Card

| Coop | Node | Port | DID (first 40 chars) |
|------|------|------|----------------------|
| BrightWorks | icn-alpha | 18081 | did:icn:zHdQuwTTniwcV4TT1ZcfXsV... |
| River City | icn-beta | 18082 | did:icn:zDMiXkUafnaRfeA8tdPCYiK... |
| Harbor Homes | icn-gamma | 18083 | did:icn:zyWqWVqGERfRvUz4LVGd4co... |
| Finger Lakes CDN | icn-delta | 18084 | did:icn:zE5E8bz7XrJGr6WozTbUNfS... |

DIDs are fixed in pod keystore secrets. If pods are rebuilt, DIDs change — update `reseed-federation-demo.sh` and the flow scripts.

---

## Quick Command Reference

```bash
# Reset to canonical state
./demo/scripts/reseed-federation-demo.sh

# Run individual flows
./demo/scripts/flow-1-governance.sh   # Harbor Homes roof repair
./demo/scripts/flow-2-patronage.sh    # BrightWorks Q1 patronage
./demo/scripts/flow-3-federation.sh   # River City + BrightWorks equipment
./demo/scripts/flow-4-reporting.sh    # Finger Lakes CDN audit view (optional)

# Library test (port-forward, auth, curl smoke test)
./demo/scripts/lib-demo-ports.sh.test

# Check cluster
kubectl get pods -A | grep icn-
kubectl top pods -A  # if metrics-server is installed
```

---

## Known Deployment Constraints

| Constraint | Impact | Workaround |
|---|---|---|
| `treasury:read` not in ALLOWED_SCOPES | Flow 1 Step 10, Flow 2 Step 8 show narrated fallback | None required — addressed in presenter notes |
| Proof endpoint 404 (signing key not configured) | Flow 1 Step 8 shows fallback | Flow 1B (PR #1327) addresses this |
| `ledger:write` scope limitation | Flow 2 settlement may be HTTP 403 | Narrate what the settlement would show |
| Federation clearing scope | Flow 3 clearing creation may be HTTP 403 | Narrate the clearing schema |
| In-memory state | Pod restart wipes all governance state | Re-run reseed-federation-demo.sh |

None of these prevent a successful demonstration. All flow scripts have graceful fallback output for each constraint.

---

## Success Criteria

The demo is presentation-ready when all of these are true:

- [ ] A cooperator can say: "I know exactly why this would help us."
- [ ] A funder can say: "I can see how this supports accountability and scaling."
- [ ] A technical collaborator can say: "I understand the architecture and why multi-node matters."
- [ ] The core flow (Flow 1A) runs twice in a row without surprises.
- [ ] The reseed script returns "Failed: 0."
- [ ] Flow 1 narration is honest about what is demonstrated vs. what #1327 completes.
- [ ] A handoff user can reproduce the happy path without needing the presenter in the room.
