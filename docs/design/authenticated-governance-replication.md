# Authenticated governance replication (#2469)

**Status:** design + slice 1 (unwired primitive).
**Derived against:** `origin/main` = `c378232132139235e2a8cc305792eaee2571267a`.
**Owns:** the durable replacement for the #2470 containment.
**Refs:** #2469, #2441, #2470 (`bc291305`), #2471, #2480, #2510, #2520, #2535, #2544.

Everything in §1 was re-derived from source at `c3782321`. Where an older campaign note
disagrees, the source wins and the disagreement is called out.

**Revision 2** corrects three mechanisms from revision 1 that were unsafe under
eventually-consistent delivery: the sequence gate, first-writer-wins collision
arbitration, and whole-config authority binding. Revision 1's `prev` hash-chain field is
also removed. See §10, §5.2, §5.3.

---

## 1. Verified current behavior — source-to-sink trace

### 1.1 Outbound: operator action → wire

| # | Step | Location |
|---|---|---|
| 1 | Operator issues a `GovernanceCommand` (RPC/HTTP) | `icn-rpc/src/handler/governance.rs`, `apps/governance/src/http/handlers.rs` |
| 2 | `GovernanceActor` **persists to `GovernanceStateStore` first** | `apps/governance/src/actor.rs` command arms |
| 3 | Actor builds a `GovernanceMessage`, serializes with `to_bytes()` = **`serde_json::to_vec`** | `icn-governance/src/message.rs:417` |
| 4 | `gossip.publish(topic, data, author = own_did)`; ACL checked **for the local DID only**; `hash = Self::hash_data(&data)` | `icn-gossip/src/gossip.rs:911`, `:919` |
| 5 | `store_entry(entry)` → fires notification callbacks (including governance's own loopback copy) | `icn-gossip/src/gossip.rs:968` |
| 6 | Outbound send wraps the `GossipMessage` in a **`SignedEnvelope`** — `PayloadType::Gossip`, Ed25519 by the node keypair, **durable monotonic sequence**, fail-closed if the sequence cannot be persisted (#2510) | `icn-core/src/supervisor/init_send_callback.rs:163` |
| 7 | Sent as `MessagePayload::Signed` | `icn-net/src/protocol.rs:628` |

**Honest nodes already sign every outbound gossip message.**

### 1.2 Inbound: wire → state

| # | Step | Location |
|---|---|---|
| 8a | `MessagePayload::Signed` → `ctx.handle_signed()` → `envelope.verify(max_age_secs)`: Ed25519 over `canonical_encoding()` (`sequence ‖ timestamp ‖ payload_type ‖ payload`), age check, replay guard | `icn-net/src/actor/connection.rs:857`, `handlers/signed.rs:360`, `replay_guard.rs` |
| 8b | **`MessagePayload::Gossip` (raw, unsigned) is also accepted** and forwarded to the same external handler | `icn-net/src/actor/connection.rs:822` |
| 9a | Signed path → `gossip.handle_message(&envelope.from, msg)` — `from` **is** authenticated | `icn-core/src/supervisor/init_network.rs:225–255` |
| 9b | Unsigned path → `gossip.handle_message(&net_msg.from, msg)` — `from` is **self-declared** | `icn-core/src/supervisor/init_network.rs:59–71` |
| 10 | `handle_message_inner`: policy-oracle check on `sender` (`Domain::trust()`, `ActionKind::Read`). A coarse trust gate, **not** an authorship check | `icn-gossip/src/protocol.rs:46–68` |
| 11 | `handle_response` — **ignores `sender` entirely**, stores the entry verbatim | `icn-gossip/src/handlers/push.rs:109–121` |
| 12 | `store_entry`: dedups on the **sender-supplied `entry.hash`**; hash is **never recomputed**; no topic ACL; no signature — `GossipEntry` has no signature field | `icn-gossip/src/gossip.rs:968`, `types.rs:116–144` |
| 13 | Fires `EntryNotificationCallback = Fn(String, GossipEntry, Did)`. **No sender. No origin discriminator.** | `icn-gossip/src/gossip.rs:38` |
| 14 | Governance callback: topic filter → `from_bytes` → `observe_replicated_governance_message` → **`debug!` only. No state mutation.** | `apps/governance/src/actor.rs:1277–1303`, `:3841` |

### 1.3 The verified facts that constrain the design

1. **`GossipEntry` carries no signature.** `author: Did` is attacker-chosen.
2. **The entry hash is never recomputed on receipt.** `hash_data` has exactly one caller,
   the publish path (`gossip.rs:919`). Dedup keys on attacker-supplied `entry.hash`.
3. **The transport peer *is* authenticated** — Hello binds DID → live TLS cert
   (`handlers/hello.rs:44–108`), fails closed with no peer certificate.
4. **That identity is discarded at the gossip seam.**
5. **The unsigned gossip door is open** (`connection.rs:822`).
6. **`SignedEnvelope` authenticates the *hop*, not the *author*.** `handle_response`
   (`push.rs:109`) stores a received entry verbatim; `handle_request_missing`
   (`pull.rs:461`) re-serves `entry.clone()` onward under the *relay's own* envelope
   signature. `entry.author` crosses every hop unauthenticated. **Any design requiring
   `envelope.from == entry.author` accepts only single-hop delivery and destroys relay
   and anti-entropy convergence.**
7. **The DID *is* the public key.** `Did::to_verifying_key()` (`icn-identity/src/lib.rs:255`).
   Verification needs no resolution, no DID document, no network round-trip.
8. **`GovernanceDomain` has no owner, creator, or admin field** (`domain.rs:33–50`).
   `GovernanceDomainId::generate()` is a random UUID; the domain declares its own
   membership.
9. **`MembershipSource::TrustThreshold` is node-subjective** (`resolver.rs:71–75`).
10. **`save_domain` / `save_proposal` / `save_vote` are LWW upserts** (`state_store.rs:32,43,54`).
11. **`VoteCast.signature` is dead** (only producer `actor.rs:2090` passes `None`);
    `GovernanceDomainSeedManifest` (`bootstrap.rs:114`) has zero production consumers.

### 1.4 New in revision 2 — identity and key structure

12. **All governance identifiers are caller-chosen strings.**
    `ProposalId::generate()` / `GovernanceDomainId::generate()` / `DelegationId::generate()`
    are random UUIDv4, and each type also exposes `new(impl Into<String>)` accepting an
    arbitrary string (`proposal.rs:243–255`, `domain.rs:9–21`, `delegation.rs:69–81`).
    **No identifier is bound to its creator.**
13. **Exactly one storage key is author-bound.** From `state_store.rs:192–247`:

    | Key helper | Shape | Binds principal? |
    |---|---|---|
    | `vote_key(proposal_id, voter)` | `gov:vote:{pid}:{voter}` | **Yes — the voter DID** |
    | `proposal_key(id)` | `gov:proposal:{id}` | No |
    | `domain_key(id)` | `gov:domain:{id}` | No |
    | `delegation_key(id)` | `gov:delegation:{id}` | No |
    | `proof_key(id)` / `close_intent_key(id)` | `…:{pid}` | No |

14. **The pre-containment mutation set was exactly 8 variants** (from the `bc291305`
    diff): `DomainCreated`, `DomainUpdated`, `ProposalCreated`, `ProposalOpened`,
    `VoteCast`, `ProposalClosed`, `DelegationCreated`, `DelegationRevoked`.
    **`Comment*`, `Deliberation*` and `Reaction*` were never persisted by the replication
    ingress** — `GovernanceStateStore` has no comment key space at all. Revision 1 listed
    them as restoration candidates; that was wrong, and replicating them would be a no-op.
15. **`CastVote` overwrites unconditionally** (`actor.rs:2072–2087`) — no "already voted"
    check. A voter can change their vote, so the vote key is **not** write-once.
16. **`GovernanceConfig` = `{ profile, membership, params, emergency }`** (`config.rs:25–37`).
    `params` (quorum, approval thresholds) and `emergency` govern how state is *evaluated*,
    not who may *author*. **`cooperative_default()` uses `MembershipConfig::trust_threshold(0.3)`**
    (`config.rs:81`) — i.e. the default profile is **not** supported by v1 (§5.5).

### 1.5 The authority-root finding

- **Domain-anchored variants** reference a domain directly or via `proposal_id`.
  Authority *could* be membership in that domain — if the receiver already holds it.
- **Domain-defining variants** (`DomainCreated`, `DomainUpdated`): per fact 8, **no
  authority root exists in the data model.** A signature proves only "some keypair said so."

---

## 2. Threat model

Attacker: can open a QUIC connection, complete Hello (so holds *some* valid DID — DIDs are
free to mint), and send arbitrary frames. Does not hold any victim's private key. May be a
legitimate federation member acting outside its authority.

| # | Attack | Status on `c3782321` |
|---|---|---|
| T1 | Forge `entry.author` as a victim DID and publish governance state | **Blocked** by containment; would otherwise fully succeed |
| T2 | Publish raw unsigned `MessagePayload::Gossip` | **Open** (`connection.rs:822`) |
| T3 | Supply `entry.hash` unrelated to content → poison dedup, pre-claim a hash to suppress a legitimate entry | **Open** — hash never recomputed |
| T4 | Replay a legitimately signed op into a *different* domain | Open under a signature that omits domain binding |
| T5 | Replay a superseded op | Open without a replay identity |
| T6 | `ProposalOpened` force-reset on a terminal proposal, then `ProposalClosed` — **two-message finalization bypass** | **Blocked**; must stay blocked *regardless of authentication* |
| T7 | Collide a `GovernanceDomainId` / `ProposalId` / `DelegationId` and overwrite via LWW | **Open** in the data model (facts 12–13); masked by containment |
| T8 | Member of domain A authors an op affecting domain B | Open without explicit per-domain authority |
| T9 | Relay tampers with a relayed entry's contents | **Open** — no content binding survives a hop |
| T10 | Two honest nodes disagree on `TrustThreshold` membership → divergent verdicts | **Latent**; resolved by the O1 decision (§5.5) |
| T11 | **Delivery-order divergence** — two honest nodes apply the same op set in different orders and reach different final state | **Latent**; the subject of §10 |
| T12 | **Sequence-gap denial** — an attacker (or ordinary loss) causes an op to be discarded permanently because a higher sequence arrived first | Would be **introduced** by revision 1's design; corrected in §10 |

Out of scope per #2469: TLS redesign, governance consensus, the `GovernanceProofV2`
attestation-signer authority model, generic gossip ACL redesign.

---

## 3. Required invariants

- **I1 Content binding.** Signature over the exact op bytes, verified before any state read
  the op could influence.
- **I2 Author binding.** Verifies under `author.to_verifying_key()`; `author` is the acting
  principal — not `entry.author`, not `envelope.from`.
- **I3 Domain binding.** The signed bytes include the affected `GovernanceDomainId`.
- **I4 Authority.** `author` held a specific authority over that specific domain.
- **I5 Origin vs relay.** Relay is untrusted; only the origin signature counts.
- **I6 Replay identity.** Applying the same op twice is a no-op.
- **I7 Lifecycle monotonicity.** Terminal proposals never regress, *independently of
  authentication*.
- **I8 Conflict safety.** Colliding IDs must not permit silent overwrite.
- **I9 No circular bootstrap.** Verification never fetches authority material over the
  channel carrying the op.
- **I10 Deterministic verdict.** Two honest nodes with the same durable state reach the same
  accept/reject decision.
- **I11 Positive convergence.** A legitimate op from another node **is applied**.
- **I12 Order-independent convergence (new).** Two honest nodes that receive the same set of
  ops in *any* order reach the same final state. No op is permanently discarded because of
  arrival order.

---

## 4. Design alternatives

### A. Signed `GossipEntry` (generic content envelope)
*Against:* changes the shared gossip wire format for every subsystem at once; requires every
entry producer to hold a signing key; delivers **no** authority (I4). The "generic gossip
security" broadening #2469 warns against.

### B. Transport rebinding + sender-authored-entry requirement
**Disqualified.** Per fact 6, requiring `envelope.from == entry.author` accepts only
single-hop delivery, breaking relay and anti-entropy (violates I5, I11). Closing the
unsigned door (T2) remains worth doing separately under #2471/#2480.

### C. Governance-specific signed replication envelope — **recommended**
Carried inside `entry.data`, which gossip already treats as opaque. Minimal blast radius;
carries domain binding (I3) and authority context (I4), which A and B structurally cannot;
survives relay because the signature is *inside* the replicated content (I5).

### D. Typed `ReplicationPolicy` at the shared topic-registration seam (#2441)
The name **already exists** at `icn-kernel-api/src/state.rs:29` as a durability/placement
policy — #2441's proposal collides head-on. Substantively, the three #2441 classes have
different authority roots; a shared seam would be a framework built before any instance is
proven. **Prove the governance instance first.**

### E. Reuse `SignedEnvelope`
Hop authenticator (fact 6), no domain binding, no authority model. **Reuse the pattern, not
the type** — specifically its length-prefixed `canonical_encoding()` and its
sign-the-transmitted-bytes discipline (§5.4).

---

## 5. Recommended minimal architecture

### 5.1 The v1 envelope

```rust
pub struct SignedGovernanceOp {
    pub version: u16,                     // GOV_OP_V1; unknown → reject
    pub author: Did,                      // acting principal; key via to_verifying_key()
    pub domain_id: GovernanceDomainId,    // explicit, never inferred from the payload
    pub authority: ReplicationAuthority,  // StaticMembership { membership_hash }
    pub seq: u64,                         // per-(author, domain); COMPARATOR, never a gate
    pub op_kind: GovernanceOpKind,        // signed routing discriminator
    pub op_bytes: Vec<u8>,                // transmitted verbatim; signature covers these bytes
    pub signature: Vec<u8>,               // Ed25519 over canonical_body()
}

pub enum ReplicationAuthority {
    StaticMembership { membership_hash: [u8; 32] },
}
```

`op_id = SHA-256(canonical_body())` — **derived, never carried**, so it cannot disagree with
content.

Removed from revision 1: `prev` (hash chaining — see §10) and `domain_config_hash`
(over-binding — see §5.3). No timestamp field: wall-clock orders nothing here, so carrying
one would only invite it to be used.

### 5.2 Canonical encoding

`canonical_body()` is an explicit, hand-rolled, length-prefixed concatenation in the style of
`SignedEnvelope::canonical_encoding` (`icn-net/src/envelope.rs:403`):

```
"ICN-GOV-OP-v1\0"                     domain separator (fixed, unambiguous)
u16_be(version)
lp(author.as_str())                   lp(x) = u32_be(len) ‖ bytes
lp(domain_id.0)
u8(authority discriminant) ‖ authority payload
u64_be(seq)
u8(op_kind discriminant)
lp(op_bytes)
```

Every variable-length field is length-prefixed, so no field boundary is ambiguous and no
concatenation collision is reachable.

### 5.3 Why `op_bytes` is opaque, and why that is *not* "signing serde_json"

`op_bytes` are signed and transmitted **verbatim**: the receiver verifies the signature over
exactly the bytes it received, and decodes those same bytes. There is no re-serialization
step anywhere in the pipeline, so serde_json's determinism is **never relied upon**. This is
strictly stronger than canonicalizing `GovernanceMessage`, and it is the discipline
`SignedEnvelope` already uses (`payload: Vec<u8>` + `payload_type`).

Verbatim transport is guaranteed by verified fact 6: `entry.data` is opaque to gossip and
relays re-serve `entry.clone()` byte-for-byte.

The alternative — hand-rolling a canonical encoder for ~20 `GovernanceMessage` variants over
deeply nested types (`Proposal`, `GovernanceDomain`, `TallySnapshot`, `GovernanceProofV2`) —
would be a large, permanently-maintained surface where **a newly added field is silently
excluded from the signature**. That is a worse vulnerability class than the one it avoids.

*Accepted consequence:* JSON is malleable, so two byte strings can decode to the same
message and yield different `op_id`s. Not exploitable — an attacker cannot re-sign a
re-encoding — and the §10 comparator resolves any resulting duplicate deterministically.

### 5.4 The authority snapshot — membership only, not whole config

Revision 1 bound `domain_config_hash`. That conflates two different questions:

- **who may author an operation** → depends only on the member set;
- **the rules under which the resulting state is evaluated** → `params`, `emergency`,
  `profile`.

Binding the whole config means an unrelated quorum-threshold edit invalidates every
in-flight signed vote — a liveness failure with **no** security benefit, because a quorum
change never grants anyone authorship.

v1 therefore binds exactly the authority-relevant snapshot:

```
membership_hash = SHA-256(
    "ICN-GOV-STATIC-MEMBERSHIP-v1\0"
  ‖ lp(domain_id.0)
  ‖ u32_be(member_count)
  ‖ lp(member[0]) ‖ lp(member[1]) ‖ …      // sorted bytewise, deduplicated
)
```

Sorted and deduplicated so the hash is a function of the *set*, not of list order.
`domain_id` is inside the hash so a membership snapshot cannot be lifted between domains
even if two domains happen to share a member set.

The `ReplicationAuthority` enum exists so a v2 basis (witness set, `AuthorityGrant` from
`icn-governance/src/authority.rs`, charter binding) can be added as a new variant that a v1
verifier rejects explicitly rather than misreads. One variant today; no abstract framework.

### 5.5 O1 resolved — StaticList only, and what that excludes

**Decision: v1 supports only domains whose `MembershipSource` is `StaticList`.**

`TrustThreshold` resolves against each node's local trust graph (fact 9), so two honest
nodes can legitimately compute different membership and reach different verdicts on the
same op — permanent divergence (T10, violating I10). It **remains fail-closed for remote
state application** until ICN has a separately designed deterministic membership-snapshot /
witness protocol.

**Compatibility limitation, stated plainly:**

- `GovernanceConfig::cooperative_default()` uses `trust_threshold(0.3)` (`config.rs:81`).
  **Domains built from the default cooperative profile are NOT eligible for authenticated
  replication in v1**, and no part of #2469 attempts to make that configuration safe.
- A `TrustThreshold` domain behaves exactly as it does under the #2470 containment today:
  replicated governance ops targeting it are **not applied**. This is a no-op change for
  those domains, not a regression.
- Eligibility is decided by the **receiver's local** copy of the domain. A node that holds a
  `StaticList` domain applies eligible ops; a node whose copy is `TrustThreshold` does not.
  Operators must therefore not mix membership sources for the same logical institution
  across nodes — a mixed deployment converges only on the `StaticList` side.

### 5.6 Verification order at ingress (fail-closed, in this order)

1. Decode the envelope framing; reject unknown magic or `version`.
2. Reject `op_kind` outside the v1 restorable set (§7) — **before** decoding `op_bytes`, so
   an unsupported or oversized payload is never parsed.
3. Recover `author.to_verifying_key()`; verify `signature` over `canonical_body()`. **No
   lookup, no network.** → I1, I2
4. Decode `op_bytes`; reject if the decoded variant disagrees with `op_kind`.
5. Reject if the op's internal principal disagrees with `author` (`vote.voter`).
6. Recompute `entry.hash` from `entry.data`; reject mismatch. → T3
7. Resolve `domain_id` in **local durable state**. Absent → **quarantine; never fetch**. → I9
8. **Resolve the op's state anchor locally and confirm it belongs to `domain_id`.** For
   `VoteCast`: load `vote.proposal_id`; absent → quarantine; reject unless
   `proposal.domain_id == envelope.domain_id`. Without this a member of domain D could
   vote into domain E's tally by declaring `domain_id = D` — the signature, the membership
   check and the domain binding would all pass, because none of them looks at what the
   *proposal* belongs to. → I3, T8
9. Reject unless the local domain's `MembershipSource` is `StaticList`. → §5.5
10. Recompute `membership_hash` from the local member set; reject mismatch. → I10
11. Reject unless `author` ∈ the local member set. → I4
12. Reject if `op_id` is already in the durable applied-set. → I6
13. Lifecycle: reject terminal-state regressions **unconditionally**. → I7, T6
14. Apply under the §10 comparator, not LWW.

Note the ordering discipline: the allow-list gate (2) precedes the fallible decode (4), so
the gate stays reachable and its counters mean something under attack.

---

## 6. Wire-format implications

- **Gossip wire format: unchanged.** `entry.data` is already opaque bytes.
- **Governance topic payload: changed.** `entry.data` becomes a `SignedGovernanceOp` frame
  instead of a bare `GovernanceMessage`.
- Frames are distinguished by a leading **magic + version**, never by trial decoding (a bare
  `GovernanceMessage` is JSON; trial decoding is fragile and attacker-steerable).
- The `entry.hash` recomputation (step 6) is a **generic gossip** change, separable, and
  fixes T3 for all topics independently of this envelope.

## 7. First safe replication variant set

Applying facts 12–15 to the 8 pre-containment mutation variants:

| Variant | Storage key | Cross-author collision | Authority root | v1 |
|---|---|---|---|---|
| **`VoteCast`** | `gov:vote:{pid}:{voter}` — **author-bound** | **Impossible** (§8) | membership + `vote.voter == author` | **RESTORE** |
| `ProposalCreated` | `gov:proposal:{id}` | **Yes** — any member may pick another's `ProposalId` | membership | contain (§8) |
| `DelegationCreated` | `gov:delegation:{id}` | **Yes** — same | membership | contain (§8) |
| `DelegationRevoked` | mutates existing `gov:delegation:{id}` | n/a | `revoked_by == delegator` in local state | contain — safe but inert (§7.1) |
| `ProposalOpened` | mutates `gov:proposal:{id}` | n/a | **undecided** (§7.2) | contain |
| `ProposalClosed` | mutates `gov:proposal:{id}` + proof | n/a | **undecided** (§7.2) | contain |
| `DomainCreated` / `DomainUpdated` | `gov:domain:{id}` | Yes | **none exists** (fact 8) | contain |

**v1 restores exactly one variant: `VoteCast`.**

### 7.1 Why `DelegationRevoked` is excluded despite being safe

Revocation is monotonic, idempotent, order-independent, and authorized by a principal named
in state the receiver already holds — it is genuinely safe. But it can only act on a
delegation that exists locally, and `DelegationCreated` is contained, so on a remote node it
is a guaranteed no-op. Shipping a capability with no reachable effect adds verification
surface for nothing. It becomes useful the moment `DelegationCreated` is restorable.

### 7.2 Why `ProposalOpened` / `ProposalClosed` / `ProposalCancelled` are excluded

Membership is not the right authority for choosing an outcome. `ProposalClosed` carries
`outcome` and `tally` as **attacker-supplied fields**; accepting it on membership alone lets
any single member declare any result, which is strictly worse than the current containment.
The correct authority is the tally/state transition itself — which entangles with the
`GovernanceProofV2` signer-authority model that #2469 lists as a non-goal. `ProposalOpened`
is the force-reset half of the T6 finalization bypass and must not return before the
lifecycle guard exists.

### 7.3 Honest scope of what v1 delivers

A replicated `VoteCast` requires the referenced proposal to be present locally, so that step
5's domain check can confirm the vote belongs to `domain_id`. Since `ProposalCreated` stays
contained, **v1 restores vote convergence only for proposals a node already holds** (created
locally or provisioned out of band). That is a narrow capability. It is nonetheless the
right first slice: it is the smallest change that exercises the entire chain — sign → gossip
→ relay → verify → authority → apply → converge — which is exactly the positive-convergence
evidence #2469 demands, and it establishes the pattern without inventing unsafe semantics
for the harder variants.

## 8. Collision semantics — corrected

Revision 1 proposed first-writer-wins. **That is arrival-order dependent and non-convergent**
(T11): node A sees Alice's object first, node B sees Bob's colliding object first, and the
two permanently disagree on the owner. It is withdrawn.

Since no identifier is bound to its creator (fact 12), the options for a colliding
`ProposalId` / `DelegationId` are:

| Rule | Deterministic? | Safe? |
|---|---|---|
| First-writer-wins | **No** — arrival-order | — |
| Deterministic tiebreak (e.g. highest `op_id` wins) | Yes | **No** — lets any member displace another's proposal content by grinding `op_id`; votes keyed by `(pid, voter)` survive and silently re-attach to the substituted proposal |
| Reject on existing-with-different-author | No — arrival-order | — |
| **Derived identifiers** — `ProposalId = H(proposer ‖ domain ‖ nonce ‖ content)` | Yes | Yes — collision becomes impossible by construction |

Derived identifiers are the correct fix, and they are **out of scope for #2469**: identifier
construction reaches HTTP routes, RPC, storage keys, receipts, proofs and the test suite.

**Per the standing instruction, the affected mutation class stays contained rather than
receiving invented conflict semantics.** `ProposalCreated`, `DelegationCreated`,
`DomainCreated` and `DomainUpdated` remain refused. A follow-up issue should give governance
objects creator-derived identifiers; that unlocks them.

**`VoteCast` needs none of this.** `vote_key(proposal_id, voter)` already embeds the voter
DID, and step 5 requires `vote.voter == author`, which step 3 authenticated. Alice can write
only `gov:vote:{pid}:{alice}`. **Cross-author collision on a vote key is impossible by
construction** — the one place the existing data model already does the right thing.

## 9. Key rotation / revocation

Because the DID *is* the key (fact 7), an op signed by DID *D* verifies forever — so rotation
and revocation are invisible at the signature layer.

`DidDocument` exists (`icn-identity/src/multi_device.rs:19`) with a `DidDocumentCache`
(`sync.rs:68`), and `revocation.rs` / `revocation_store.rs` exist, but the supervisor wires
revocation only for **RPC tokens** (`init_rpc.rs:57–63`), not identities.

**v1 treats rotation and revocation as *membership* changes, not signature-layer changes.**
A removed or rotated-out member fails the step 10 membership check even though the signature
still verifies. This keeps the signature layer lookup-free (I9) and puts revocation where
authority already lives. Because `membership_hash` is bound into the signature, an op
authored against a stale member set is rejected at step 9 — membership edits invalidate
in-flight ops by construction, which is the desired fail-closed direction.

Accepting an op signed by a key valid *at authoring time* but rotated since requires a
trusted time source and resolvable key history. **Deferred to its own issue.**

## 10. Replay / order / conflict semantics — corrected

Revision 1 proposed rejecting `seq <= last_seen`. **That is unsafe** and is withdrawn.

### 10.1 The failure it would have caused

Alice authors seq=10 (vote on proposal P) and seq=11 (vote on proposal Q). Node B receives 11
then 10. A watermark gate rejects 10 as stale, so **B permanently loses Alice's vote on P**
while A has it — T12, violating I12. Gossip guarantees no delivery order, and anti-entropy
can deliver an op arbitrarily late.

### 10.2 The five candidate mechanisms

| Mechanism | Verdict |
|---|---|
| **`op_id`-only durable dedup** | **Necessary and sufficient for replay.** Order-independent; never discards an unseen op. Does not by itself resolve same-key conflict. |
| `seq` + sliding replay window | ❌ A window is a bounded watermark. Gossip delivery delay is unbounded — anti-entropy may deliver hours later — so any window drops legitimate ops. |
| `seq` + gap quarantine + contiguous-prefix watermark | ❌ The most plausible-looking option and the trap: it requires contiguity, so a **permanently dropped op wedges that author's entire stream forever**, and the quarantine grows unboundedly under a trivial attack (author high sequence numbers, never send the gaps). |
| Hash chaining (`prev`) | ❌ Same wedge as above — a missing link blocks every descendant — and it forces an author's ops to be strictly serial, which they are not. This is why `prev` is removed from the envelope. |
| Drop `seq` from acceptance entirely; rely on state/lifecycle/CAS | ✅ Correct for *acceptance*. Insufficient alone for same-key conflict where state is non-monotonic — and vote state **is** non-monotonic (For ↔ Against, fact 15). |

### 10.3 The chosen design

**`seq` has zero role in acceptance. It is only a comparator, scoped to a key.**

- **Replay gate — the only acceptance gate.** A durable set of applied `op_id`s. Reject iff
  `op_id` was already applied. Order-independent; an op arriving late is applied normally.
  No watermark, no contiguity requirement, no gap quarantine. Gaps are normal and permanent.
- **Same-key conflict — deterministic comparator.** When a key already holds a value, apply
  the incoming op iff `(incoming.seq, incoming.op_id) > (stored.seq, stored.op_id)`
  lexicographically. `seq` carries the author's intent order (a later vote beats an earlier
  one); `op_id` breaks ties deterministically.

This converges under any delivery order. Worked through the §10.1 scenario, and through
Alice changing her vote (seq=10 For, seq=11 Against):

| | Node A (10 then 11) | Node B (11 then 10) |
|---|---|---|
| first op | apply 10 → For | apply 11 → Against |
| second op | 11 > 10 → apply → **Against** | 10 < 11 → no-op → **Against** |

Both converge to Against, and **op 10 is never *rejected* — it is accepted and then loses
the comparator**, which is the distinction that makes §10.1 safe.

`seq` must be per-`(author, domain)` monotonic at the origin. The durable monotonic counter
pattern from #2510 (`init_send_callback.rs`, fail-closed on persistence failure) is the
precedent to reuse when slice 3 wires emission.

- **Cross-author ordering is deliberately not provided.** That is consensus, which #2469
  excludes. Safety without it comes from I7 (lifecycle monotonicity) and §8 (no cross-author
  key collisions in the restorable set).
- **Wall-clock fields order nothing.** `entry.timestamp`, `vote.timestamp` and
  `updated_at`-style LWW are attacker-supplied and are not authorities.

## 11. Relationship to #2441

Same defect *class*, different authority roots.

- #2441's proposed `ReplicationPolicy` **name-collides** with `icn-kernel-api/src/state.rs:29`.
  Whichever issue moves first should rename.
- Governance can close without #2441, and vice versa.
- **Do not build the shared seam yet.** The pieces most likely to generalize are the
  quarantine store, the canonical-encoding helper and the `op_id` dedup set — **not** the
  authority model, which is genuinely per-class.

## 12. RED tests required before implementation

Exercised through production wiring (`init_governance_actor` + `GossipActor::handle_message`),
extending `apps/governance/tests/fp02_governance_replication_containment.rs`. Slice 1's own
unit tests are listed separately in §13.

**Content / author / domain binding**
1. Forged `author`, no valid signature → rejected before any state write.
2. Valid signature, one byte of `op_bytes` mutated → rejected.
3. Valid envelope re-wrapped under a different `entry.author` → still applied on its own
   merits (proves `entry.author` is *not* an authority) **and** rejected if the inner
   signature fails.
4. `entry.hash` not matching `entry.data` → rejected.
5. Envelope validly signed for domain A, replayed naming domain B → rejected.

**Authority**
6. Valid signature by a non-member of the target domain → rejected.
7. `vote.voter` ≠ envelope `author` → rejected.
7b. **Cross-domain tally injection:** a member of domain D signs a `VoteCast` for a
    proposal belonging to domain E, declaring `domain_id = D` → rejected at step 8. Every
    other check passes, so this fails only if the proposal anchor is checked.
8. Unknown `domain_id` → quarantined, not applied, **no network fetch**.
8b. Known `domain_id` but unknown `vote.proposal_id` → quarantined, not applied.
9. Local domain is `TrustThreshold` → **not applied** (§5.5), and this is asserted as
   deliberate, not incidental.
10. `membership_hash` computed against a stale member set → rejected.

**Replay / order — the corrected semantics**
11. Byte-identical replay of an applied op → no second application.
12. **Out-of-order delivery: ops seq=10 (proposal P) and seq=11 (proposal Q) delivered 11
    then 10 → BOTH applied.** The direct regression test for T12; must fail against a
    watermark implementation.
13. Same-key conflict delivered in both orders (10 For / 11 Against, and 11 then 10) → both
    nodes converge to Against.
14. An op with `seq` far below the author's highest seen, on an untouched key → applied.

**Lifecycle**
15. `ProposalOpened` on a terminal proposal → rejected **even when fully authorized** (T6).
16. Reopen→close finalization bypass → outcome unchanged.

**Containment retained**
17. A validly signed, authority-bearing `ProposalCreated` / `DelegationCreated` /
    `DomainCreated` → **still not applied** (§7, §8), asserted as deliberate.

**Positive convergence — the test that makes this a fix rather than another block**
18. A legitimately signed, authority-bearing `VoteCast` from a *second* node, relayed through
    a *third*, **is applied**, and the nodes converge. Must exercise a real relay hop so
    fact 6 is actually covered.

**Migration**
19. An unsigned legacy payload is refused while a signed one is applied.

## 13. Implementation slices, in order

| Slice | Content | Restores state application? |
|---|---|---|
| **1** | `SignedGovernanceOp` type, magic + version, canonical encoding, `membership_hash`, sign/verify, derived `op_id`. **Library only, unwired.** | No |
| 2 | Recompute `entry.hash` on receipt in gossip. Generic, separable. | No |
| 3 | Emit signed envelopes from the governance publish path (durable per-`(author,domain)` seq, #2510 pattern); accept both shapes; apply nothing. | No |
| 4 | Bounded quarantine store + steward release valve. | No |
| 5 | Durable `op_id` applied-set + the §10.3 comparator in the state store. | No (local semantics only) |
| 6 | Lifecycle monotonicity guard, enforced unconditionally. | No |
| 7 | **Verify + authority + apply, lifting containment for `VoteCast` only.** Containment tests updated, not deleted. | **Yes** |

Slices 1–6 restore nothing. Slice 7 is the only one that lifts containment, and does so in
the same change that proves positive convergence (test 18).

**Slice 1 unit tests (this change):**
sign→verify round-trip; tampered `op_bytes`; tampered `domain_id`; tampered `author`;
tampered `seq`; tampered `authority`; tampered `op_kind`; wrong-signer rejection; unknown
version rejected; bad magic rejected; truncated frame rejected; `op_id` determinism and
sensitivity to every signed field; `membership_hash` set-order and duplicate invariance,
domain separation, and sensitivity to member changes; encode→decode round-trip;
`op_kind`/payload disagreement rejected.

## 14. Is the field set stable enough to freeze?

**Yes.** Each previously open question either has a decision or has been shown not to touch
the envelope:

| Question | Resolution | Touches field set? |
|---|---|---|
| O1 membership determinism | StaticList-only (§5.5) | Yes → `ReplicationAuthority::StaticMembership { membership_hash }` |
| Replay under out-of-order delivery | `op_id` gate + `seq` comparator (§10.3) | Yes → keeps `seq`, **removes `prev`** |
| Collision arbitration | Contain the affected classes (§8) | No — no field needed |
| Authority snapshot scope | Membership only, not whole config (§5.4) | Yes → **removes `domain_config_hash`** |
| First restorable variant set | `VoteCast` only (§7) | No — `op_kind` already carries it |
| `ProposalClosed` authority (old O2) | Contained; deferred with the proof-signer model | No |
| Key rotation / revocation | Membership-mediated in v1 (§9) | No |
| Quarantine bounds (old O3) | Slice 4 detail | No |

The remaining unknowns — derived identifiers, tally-rooted lifecycle authority, deterministic
membership witnesses — all resolve into either a **new `ReplicationAuthority` variant** or a
**version bump**, which is precisely what `version` and the one-variant enum exist to absorb.

**Slice 1 is cleared to implement.**
