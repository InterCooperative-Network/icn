# ICN Governance Demo — Walkthrough

**Date:** 2026-03-25
**Verified:** All 19 steps pass, exit code 0.
**Script:** `demo/scripts/demo-governance.py`
**Runtime:** ~8 seconds against a local `icnd` process

---

## What the Demo Proves

Three people — Alice, Bob, and Carol — form a cooperative from scratch and make their first democratic decision. No admin account. No corporation in the middle. No one has special permanent authority.

Alice is elected temporary coordinator to bootstrap the coop structure. She uses that role to create the cooperative, set up the governance domain, and add Bob and Carol as members. Once the structure exists, her coordinator role is just another member token. Any member can propose. Any member can vote. Any member can close a vote.

The first democratic act is ratifying the cooperative's own charter — the members adopt their own rules, together, before any substantive decisions are made. Then Bob (not the coordinator) proposes $12,000 for community kitchen equipment. All three vote for it. Carol (not the coordinator, not the proposer) closes the vote.

When the vote closes, the ICN node generates a cryptographic receipt: a `GovernanceProofV2` that bundles the canonical decision hash, the vote tally, and a node attestation signed with the node's Ed25519 key. The receipt can be re-verified by any party at any time. It is anchored to the vote record, not to any central authority.

That's the claim: **a cooperative can self-govern, and the governance record is cryptographically verifiable, without a trusted third party**.

---

## Prerequisites

- **Rust 1.88.0** (pinned — do not upgrade)
- **Python 3.8+** with the `cryptography` package
- **Port 8080** free

```bash
pip install cryptography
ss -tlnp | grep :8080    # must return nothing
```

First-time build (from the `icn/` monorepo root):

```bash
cd icn && cargo build -p icnd   # ~2 minutes first time, ~10s incremental
```

---

## Quick Start

Three commands:

```bash
bash demo/scripts/start-demo.sh /tmp/icn-demo
python3 demo/scripts/demo-governance.py http://localhost:8080
pkill -f icnd
```

For a live-presentation experience with colored output and phase pauses:

```bash
bash demo/scripts/present-governance.sh
```

---

## Step-by-Step Walkthrough

### Phase 1: Founding Assembly (Steps 1–6)

Alice, Bob, and Carol are bootstrapped as equal founding members. Alice holds a temporary coordinator token to create the infrastructure. That role doesn't persist.

---

**Step 1 — Generate member identities**

No API call. Each member generates a fresh Ed25519 keypair in memory. Their DID is derived directly from the public key bytes, base58-encoded with the `did:icn:z` prefix.

```
Alice: did:icn:z<base58-pubkey>
Bob:   did:icn:z<base58-pubkey>
Carol: did:icn:z<base58-pubkey>
```

*What this proves:* Identity is self-sovereign. No registration with a central authority. No username/password. The key is the identity.

---

**Step 2 — Alice authenticates as coordinator**

```
POST /v1/auth/challenge  { "did": alice.did }
POST /v1/auth/verify     { "did": alice.did, "signature": hex(sign(nonce)), "coop_id": "finger-lakes-food", "scopes": [...] }
```

The gateway issues a challenge nonce. Alice signs it with her Ed25519 private key and returns the hex signature. On verification, the gateway derives Alice's expected public key from her DID, verifies the signature, and issues a scoped JWT.

*What this proves:* Authentication is DID-native challenge-response. The gateway never sees Alice's private key. Possession of the key proves identity.

Alice's scopes: `coop:read`, `coop:write`, `coop:admin`, `governance:read`, `governance:write`, `treasury:read`, `treasury:write`.

---

**Step 3 — Create the cooperative**

```
POST /v1/coops  { "id": "finger-lakes-food", "name": "Finger Lakes Food Co-op" }
Authorization: Bearer <alice-token>
```

*What this proves:* A cooperative entity exists in the system. `finger-lakes-food` is the canonical cooperative ID. Alice's admin scope authorizes creation.

---

**Step 4 — Create governance domain**

```
POST /v1/gov/domains  {
  "id": "coop:finger-lakes-food",
  "name": "Finger Lakes Food Co-op Governance",
  "profile": "cooperative_default",
  "quorum_percent": 50,
  "approval_percent": 51,
  "voting_period_days": 7,
  "members": [alice.did]
}
```

*What this proves:* Governance rules are explicit and stored: 50% quorum, simple majority (51%), 7-day voting windows. Alice is the initial member; Bob and Carol will be added next.

---

**Step 5 — Add Bob to the cooperative and governance domain**

```
POST /v1/coops/finger-lakes-food/members  { "did": bob.did, "role": "participant", "display_name": "Bob" }
POST /v1/gov/domains/coop:finger-lakes-food/members  { "did": bob.did, "weight": 1.0 }
```

*What this proves:* Bob is a full coop member with voting weight 1.0. Equal to Alice.

---

**Step 6 — Add Carol to the cooperative and governance domain**

```
POST /v1/coops/finger-lakes-food/members  { "did": carol.did, "role": "participant", "display_name": "Carol" }
POST /v1/gov/domains/coop:finger-lakes-food/members  { "did": carol.did, "weight": 1.0 }
```

*What this proves:* Carol is a full coop member with voting weight 1.0. Equal to Alice and Bob.

✓ Phase 1 complete: Finger Lakes Food Co-op — 3 members, governance domain active.

---

### Phase 2: Charter Ratification (Steps 7–12)

The first democratic act. Members don't inherit rules from above — they vote to adopt their own.

---

**Step 7 — Alice submits the founding charter for ratification**

```
POST /v1/gov/proposals  {
  "domain_id": "coop:finger-lakes-food",
  "title": "Ratify the Finger Lakes Food Co-op Founding Charter",
  "description": "...",
  "payload": {
    "type": "charter",
    "charter_id": "finger-lakes-food-coop",
    "charter_yaml": "..."
  }
}
```

The charter payload includes the full CCL (Cooperative Contract Language) YAML: governance bodies (`general_assembly`, `stewardship_circle`), decision thresholds (`ordinary` = simple majority, `constitutional` = two-thirds), delegation rules, credit limits, and surplus allocation (20% reserve, 80% patronage).

*What this proves:* Cooperative constitutions are first-class data in ICN — stored, governed, and enforced, not just text files.

---

**Step 8 — Open charter for ratification vote**

```
POST /v1/gov/proposals/{charter_id}/open  { "voting_period_seconds": 3600 }
```

*What this proves:* Proposals are not live until explicitly opened. The voting window starts from this call.

---

**Step 9 — Alice votes to ratify the charter**

```
POST /v1/gov/proposals/{charter_id}/vote  { "choice": "for" }
Authorization: Bearer <alice-token>
```

*Alice: FOR*

---

**Step 10 — Bob authenticates and votes to ratify**

Bob runs the same DID challenge-response as Alice (Step 2), receiving scopes `governance:read`, `governance:write`.

```
POST /v1/gov/proposals/{charter_id}/vote  { "choice": "for" }
Authorization: Bearer <bob-token>
```

*Bob: FOR*

---

**Step 11 — Carol authenticates and votes to ratify**

```
POST /v1/gov/proposals/{charter_id}/vote  { "choice": "for" }
Authorization: Bearer <carol-token>
```

*Carol: FOR*

---

**Step 12 — Alice closes the charter ratification**

```
POST /v1/gov/proposals/{charter_id}/close  {}
Authorization: Bearer <alice-token>
```

*What this proves:* 3/3 unanimous ratification. The cooperative's rules are now in effect.

✓ Phase 2 complete: Charter ratified 3/3.

---

### Phase 3: Democratic Decision (Steps 13–17)

Bob proposes a budget item. He's not the coordinator. He doesn't need permission to propose.

---

**Step 13 — Bob proposes community kitchen equipment**

```
POST /v1/gov/proposals  {
  "domain_id": "coop:finger-lakes-food",
  "title": "Approve $12,000 for community kitchen equipment",
  "description": "Purchase commercial-grade equipment for our shared community kitchen:
    convection oven ($4,000), industrial mixer ($3,000),
    prep tables and storage ($2,500), safety and small tools ($2,500).
    This investment serves all 47 member households.",
  "payload": {
    "type": "budget",
    "amount": 12000,
    "currency": "USD",
    "purpose": "Community Kitchen Equipment"
  }
}
Authorization: Bearer <bob-token>
```

*What this proves:* Any member can create a proposal. Bob does not need Alice's permission or coordinator status.

---

**Step 14 — Bob opens the proposal for voting**

```
POST /v1/gov/proposals/{proposal_id}/open  { "voting_period_seconds": 3600 }
Authorization: Bearer <bob-token>
```

*What this proves:* The proposer controls when their proposal goes live — also without coordinator involvement.

---

**Step 15 — Alice votes FOR the kitchen equipment**

```
POST /v1/gov/proposals/{proposal_id}/vote  { "choice": "for" }
Authorization: Bearer <alice-token>
```

*Alice: FOR*

---

**Step 16 — Bob votes FOR**

```
POST /v1/gov/proposals/{proposal_id}/vote  { "choice": "for" }
Authorization: Bearer <bob-token>
```

*Bob: FOR*

---

**Step 17 — Carol votes FOR**

```
POST /v1/gov/proposals/{proposal_id}/vote  { "choice": "for" }
Authorization: Bearer <carol-token>
```

*Carol: FOR*

✓ Phase 3 complete: 3 votes FOR, 0 against.

---

### Phase 4: Verification (Steps 18–19)

Carol closes the vote — not Alice, not Bob. Authority rotates naturally.

---

**Step 18 — Carol closes and tallies the vote**

```
POST /v1/gov/proposals/{proposal_id}/close  {}
Authorization: Bearer <carol-token>
```

After closing, the script retrieves the current proposal state and vote tally:

```
GET /v1/gov/proposals/{proposal_id}
GET /v1/gov/proposals/{proposal_id}/tally
```

Tally response:

```json
{
  "for_votes": 3,
  "against_votes": 0,
  "abstain_votes": 0,
  "total_votes": 3
}
```

*What this proves:* The vote is counted and recorded. 3/3 for, 0 against. Quorum reached (100% > 50%). Approval threshold met (100% > 51%). Outcome: **ACCEPTED**.

---

**Step 19 — Generate cryptographic proof**

```
GET /v1/gov/proposals/{proposal_id}/proof
Authorization: Bearer <carol-token>
```

The gateway verifies the stored proof before serving it: checks the receipt binding hash, confirms at least one attestation is present, verifies each attestation's `decision_hash` matches the receipt, and validates the Ed25519 signature against the signer's DID.

Proof response structure:

```json
{
  "receipt": {
    "proposal_id": "<uuid>",
    "domain_id": "coop:finger-lakes-food",
    "outcome": "accepted",
    "vote_tally": {
      "for_votes": 3,
      "against_votes": 0,
      "abstain_votes": 0
    },
    "vote_hash": [241, 16, 39, 185, ...],
    "decision_hash": [...]
  },
  "attestations": [
    {
      "decision_hash": [...],
      "signer_did": "did:icn:z<node-pubkey>",
      "timestamp": <unix-seconds>,
      "signature": [...]
    }
  ]
}
```

*What this proves:*

- **`vote_hash`** — A BLAKE3 Merkle root over all sorted (voter DID, choice, weight) tuples. Order-independent: the same votes in any order produce the same hash.
- **`decision_hash`** — A canonical BLAKE3 hash of the receipt fields (proposal ID, domain, outcome, tally, vote_hash). Deterministic across nodes: two independent nodes processing the same vote record produce identical `decision_hash` values.
- **`attestation.signature`** — An Ed25519 signature over the canonical attestation payload (domain tag + decision_hash + signer DID + timestamp), signed by the ICN node's keystore key.

The receipt is self-verifiable. Anyone with the node's public key (derivable from its DID) can check the signature. No trust in the node is required — possession of the data is sufficient.

---

## Verified Output

Actual terminal output from the verified run (2026-03-25):

```
============================================================
  ICN Cooperative Governance Demo
============================================================
  Gateway: http://localhost:8080

  ✓ Gateway healthy (version ...)

  ...

  Step 18. Carol closes and tallies the vote
  ✓ Vote closed by Carol
  ✓ Proposal state: ACCEPTED
  ✓ Vote tally retrieved
    {
      "for_votes": 3,
      "against_votes": 0,
      "abstain_votes": 0,
      "total_votes": 3
    }

  Step 19. Generate cryptographic proof
  ✓ Governance proof retrieved
    { "receipt": { ... "vote_hash": [241, 16, 39, 185, ...] ... }, "attestations": [...] }

============================================================
  Demo Complete
============================================================

  Cooperative:  Finger Lakes Food Co-op
  Domain:       coop:finger-lakes-food
  Members:      Alice, Bob, Carol
  Charter:      Ratified 3/3
  Proposal:     "Approve $12,000 for community kitchen equipment"
  Tally:        3 for / 0 against
  Outcome:      ACCEPTED
  Receipt:      f11027b9...
```

The receipt hash (`f11027b9...`) is the `vote_hash` from the proof receipt, hex-encoded. It will differ on each run because Alice, Bob, and Carol each get freshly generated keypairs. The hash is a function of their DIDs and their votes — any change to either invalidates it.

---

## What Makes This Cooperative-First

**The coordinator is temporary.** Alice holds coordinator scopes to bootstrap the structure. Once Bob and Carol are added, she has no special authority over proposals or votes. Steps 13–19 proceed entirely without her initiating them — Bob proposes, Bob opens the vote, Carol closes it.

**Any member can propose.** Step 13 is Bob creating a proposal using his own governance JWT. No coordinator approval gate. No special role check. The governance domain rules (quorum, threshold, voting period) are the only constraints.

**Any member can close a vote.** Step 18 is Carol closing the vote. The governance system doesn't require the proposer or coordinator to close — any authenticated member with `governance:write` scope can do it. Authority is not concentrated.

**All members have equal weight.** Bob and Carol are added with `"weight": 1.0`, the same as Alice. The tally is `for_votes: 3` — three equal votes, not three weighted ones.

**The proof is not signed by a central authority.** The attestation is signed by the ICN node (the gateway daemon), which the cooperative itself runs. The signer's DID is in the proof, derivable to a public key, verifiable locally. A third party verifying the receipt doesn't need to trust the coop or the node — they verify the math.

---

## Running It Yourself

```bash
# 1. Build
cd icn && cargo build -p icnd

# 2. Start gateway and run demo
bash demo/scripts/start-demo.sh /tmp/icn-demo
python3 demo/scripts/demo-governance.py http://localhost:8080

# 3. Cleanup
pkill -f icnd && rm -rf /tmp/icn-demo
```

For interactive presenter mode (colored, paused between phases):

```bash
bash demo/scripts/present-governance.sh
```

For browser UI (after gateway is running):

```
http://localhost:8080/static/demo.html
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Gateway not reachable` | Gateway not started or wrong port | `pgrep -f icnd` → `cat /tmp/icn-demo/gateway.log \| tail -20` |
| `sled lock error` | Previous gateway didn't clean up | `pkill -9 -f icnd` then use a fresh data dir |
| `JWT secret not configured` | Running manually without env vars | See `RUNBOOK.md` for full manual invocation |
| `ModuleNotFoundError: cryptography` | Python package missing | `pip install cryptography` |
| `Proof endpoint 404` | Node keystore not unlocked at startup | Check logs for `GovernanceProof signing key unavailable` |
| Build `SIGSEGV` | Incremental compilation cache corruption | `cargo clean && cargo build -p icnd` |

---

## What to Look at Next

**K3s five-flow federation demo** — Four real cooperatives running on the homelab K3s cluster (BrightWorks Collective, River City Tool Library, Harbor Homes, Finger Lakes CDN). Covers governance, patronage settlement, inter-coop federation treaties, regulatory reporting, and distributed compute with trust-gated task admission. See the K3s section in `demo/RUNBOOK.md`.

**Browser demo** — `http://localhost:8080/static/demo.html` after starting the gateway. Four "Run Phase N" buttons. Suitable for screen-share demos. Shows coordinator role labeled as "Temporary" and equal voting weights explicitly.

**Architecture** — `docs/ARCHITECTURE.md` covers the constraint enforcement model (apps translate meaning, kernel enforces blindly), the actor-based runtime, and the trust graph → PolicyOracle → ConstraintSet flow that underpins everything the demo exercises.

**Proof structure** — `icn/crates/icn-governance/src/proof.rs` is the canonical source for `GovernanceProof`, `GovernanceDecisionReceipt`, `GovernanceDecisionAttestation`, and `GovernanceProofV2`. The hash construction and verification logic is documented inline.
