---
Status: descriptive
Canonical: no
Last Reviewed: 2026-08-15
---

# Human Identity in ICN — a first-principles architecture

What a human identity *should* be in ICN, how cryptographic authority over it
should evolve, what institutions and peers should observe, and how devices act
for humans.

> **Truth status, section by section.** No section is left unclassified; §20 is
> the full per-result index.
>
> | Sections | Status | Meaning |
> |---|---|---|
> | §1 | **summary** | restates results established or recommended below; carries no independent authority |
> | §2 | **normative** | requirements adopted for this analysis, each labelled HARD / STRONG / SOFT. They are *chosen*, not discovered — a reader may reject one, and §7's verdict may move if they do |
> | §3 | **descriptive** | verified at `b26bf681` with file:line citations. Load-bearing rows re-read by hand; §3.0 marks which |
> | **§4** | **ESTABLISHED — derivation over §3** | the decomposition, and the proof that a durable subject identifier cannot be a public key. It follows from §2's requirements plus §3's facts and holds unless one of those is rejected |
> | **§5** | **ESTABLISHED as a diagnosis; REJECTED as a verdict on Model A** | that the six blockers share one cause is a derivation over §3, checkable by re-tracing each row. The consequent rejection of key-derived durable identifiers is a *design decision*, recorded REJECTED in §20 |
> | §6 | **mixed, per claim** | §6.1 is RECOMMENDED on a bounded argument (**not** ESTABLISHED — see the caveat there); §6.2–§6.3 are ESTABLISHED external results; §6.4–§6.5 are summaries |
> | §7 onward | **PROPOSED architecture** | design, not implementation |
>
> No claim here asserts production operation, pilot readiness, or institutional
> adoption. Prose that reads like a fact but is not classified ESTABLISHED is a
> defect in this document.

> **Relationship to [PRINCIPAL_MODEL.md](PRINCIPAL_MODEL.md).** That document's
> §1–§3 (the invariant, the verified current state, the classification of
> historical intent) remain the best evidence map in the repo and are **retained**;
> §3.0 below lists the corrections this pass found in them. Its §4–§16 (the
> proposed four-class taxonomy, member-origin signing via a carried
> `DeviceAuthorization`, and the A0→A→B→D slice program) are **superseded** by
> this document — not because they were careless, but because the open decisions
> they accumulated (O8, O9, O13, O16, O17, O18) turned out to be six symptoms of
> one structural choice, which §5 identifies and §6 removes.

> **Adversarial review, and what it changed.** This document was attacked on ten
> axes before publication (hidden clocks, hidden arrival-order dependence, hidden
> correlation, hidden permanent root, hidden registry, claims-vs-source,
> proposed-as-current, invented protocol, internal contradiction, unmet HARD
> requirements). It found **nine blocking errors**, all of which are corrected
> in place with the correction marked rather than silently removed. The four that
> most changed the result:
> **(1)** pre-rotation constrains rotations only, so a compromised current key can
> still fork a non-establishment event (§6.2);
> **(2)** the impossibility claim was overstated and is downgraded from ESTABLISHED
> to a bounded argument (§6.1);
> **(3)** under the default recovery path, continuity-root compromise is a
> **takeover and is unrecoverable** (§12.2);
> **(4)** the class-1 fallback is a known R5.1 violation, not a safe default (§9.3).
>
> **Round 2** attacked the replicated state model specifically and found **seven
> more blockers**, all corrected:
> **(5)** the durable element must be a canonical **body**, not an event — Ed25519
> admits several valid signatures per message, so an event-keyed set is really a map
> with an undefined value merge (§9.2.1);
> **(6)** `σ` and `prev_digest` must use the **body** digest too, or a re-signing
> mints a second `SubjectId` and a child can name a discarded parent (§9.2.1);
> **(7)** pre-rotation commits to the next *key set*, not the *body*, so rotation
> bodies must be **deterministic** or an honest crash-retry halts the subject
> permanently (§9.2.1);
> **(8)** independent M-of-N guardian signatures give any `M` guardians a **kill
> switch**, making threshold signing a prerequisite rather than an upgrade (§12.2);
> **(9)** a single compromised current key **halts a subject unilaterally**, and
> superseding recovery **orphans the entire suffix** (§9.2.1, §9.2.2);
> **(10)** class-2 settlement is **not convergent under a fork** — `Live(A) → Live(B)`
> leaves permanently different settled state, so this model is not by itself a
> sufficient basis for irreversible settlement (**O-N7**);
> **(11)** the durable set is attacker-writable, so eviction must be
> protocol-specified or convergence-under-attack is vacuous (**O-N8**).
>
> **Round 3** found **two more**:
> **(12)** a position bound does not stop spam at `frontier + 1` from evicting the
> legitimate next event — eviction must be **authority-aware**, and the gap region
> beyond the frontier remains unsolved, so R4.6 and permutation-invariant
> convergence hold only absent adversarial storage pressure (§9.2.1, **O-N8**);
> **(13)** an existing `did:icn:<key>` **cannot serve as a `SubjectId`**, because
> inception admission requires `σ == event_id(b)` — migration must allocate a new
> subject naming that DID as its initial key, plus a membership bridge (§15.1).
>
> §9.6 lists every shortfall the model does not discharge.

**Related, deferred to, not restated:**
[authenticated-governance-replication](../design/authenticated-governance-replication.md)
(#2469 — the convergence doctrine this design must satisfy) ·
[AUTHORITY_SPINE](AUTHORITY_SPINE.md) (attenuation / revocation lifecycle) ·
[MEMBER_STANDING](MEMBER_STANDING.md) (the derived read model) ·
[KERNEL_APP_SEPARATION](KERNEL_APP_SEPARATION.md) (the Meaning Firewall) ·
[passport-keyring-position-receipt](../design/passport-keyring-position-receipt.md)
(vocabulary doctrine)

---

## 1. The executive answer

**A human identity in ICN should not be a globally published identifier at all.**

It should be three separable things, and ICN's current difficulty comes almost
entirely from having collapsed them into one 32-byte string:

| Layer | What it is | Who sees it | Lifetime |
|---|---|---|---|
| **Cryptographic principal** | a key. Exactly what `Did` already is. | whoever verifies a signature | as long as the key is held |
| **Context subject** | the identifier a person is known by *inside one institution* | that institution's members | as long as the relationship |
| **Continuity root** | a private secret the person's own app holds, linking their contexts and driving recovery | **nobody but the person** | lifetime, recoverable |

The person is not any one of these. The person is the human who holds the
continuity root and, through it, organizes a set of context relationships, each
of which is currently exercised by one or more device keys.

Three consequences follow immediately, and they are the substance of this
document:

1. **`Did` should mean exactly one thing — a cryptographic principal (a public
   key).** Every other use of it today (privacy anchor, treasury identifier,
   stable subject across rotation, credential subject) is type reuse that the
   type cannot support, and §3 shows it failing in production code.

2. **Authority to act for a subject evolves in a log the subject alone writes,
   and each institution decides for itself how fresh that log must be.** The
   subject keeps a single-writer key-event log; the institution is the *relying
   party* that holds a copy and sets its own recency bar. Neither owns the other:
   the person cannot force an institution to accept them, and the institution
   cannot rewrite who the person is. #2469 §9 already reached the neighbouring
   conclusion for key rotation ("v1 treats rotation and revocation as *membership*
   changes"); §6 shows what it generalizes to. That generalization **dissolves two**
   blockers (O8, O18), **partly answers two** (O13, O17), **narrows one** (O16), and
   **rejects one as posed** (O9) — see §6.5, which is the authoritative summary.

3. **There is no convergent global device revocation, and there cannot be one
   without paying a price ICN has declined to pay.** Agreeing on a time-varying
   predicate across an asynchronous partition-tolerant network requires a
   synchrony assumption, randomization, or an ordering authority — ICN rejects all
   three (§6.1). The correct move is to stop asking. Revocation must be scoped to a
   *relying party that has an authoritative view* — which is why ICN's one
   revocation mechanism that is designed this way (RPC/gateway token revocation,
   scoped to a single gateway with a durable store and a fail-closed read,
   [AUTHORITY_SPINE](AUTHORITY_SPINE.md) §2) has a coherent story where identity
   revocation does not. *(That is an architectural observation about its shape,
   not a re-verification of its runtime behaviour this pass.)*

The recommended architecture is **Model C** of §7 — *context-scoped identity with
a private continuity root* — and §9 states it precisely.

**What this does not do.** It does not eliminate hard work; it relocates it. The
recommendation depends on a **per-subject, single-writer key-event log** that does
not exist today (§3.7 proves the absence of any ordering substrate). That is one
named, bounded piece of infrastructure whose cost is formally understood — a
single-writer object has **consensus number 1** and is implementable with reliable
broadcast alone (§6.1) — replacing six unbounded open questions about a global
namespace. That trade is the whole argument, and §9.6 states honestly what
remains open.

---

## 2. Normative requirements

Derived before any representation was chosen, from the mission brief, the
[Constitutional Core](../ai/ICN_CONSTITUTIONAL_CORE.md) immutable principles,
#2469's invariants I1–I12, and PRINCIPAL_MODEL §1's five invariant tests.

Labels: **[HARD]** a violation is a defect · **[STRONG]** tradeable only against
another HARD/STRONG, explicitly · **[SOFT]** a goal whose cost may exceed benefit.

### R1 Sovereignty
- **R1.1 [HARD]** Genesis requires no registrar, no network, no permission.
- **R1.2 [HARD]** No institution, node, gateway or operator holds a key that lets
  it author acts as a human. *Custody defines identity.*
- **R1.3 [HARD]** Loss of any infrastructure must not destroy the human's identity
  or their ability to establish identity elsewhere.
- **R1.4 [STRONG]** No single institution may revoke a person's *identity*. It may
  revoke *standing within itself* only.

### R2 Continuity — three properties that must not be conflated
- **R2.1 [HARD] Continuity of subject.** The same human remains the same subject
  across device replacement, key rotation, total device loss, algorithm change.
- **R2.2 [HARD] Continuity of authority.** Which keys may currently act is
  time-varying and must change without changing the subject.
- **R2.3 [HARD] Validity of history.** An act validly authored when made stays
  *verifiable as having been made*, forever, independent of later authority
  changes. (Verifiable ≠ still-effective — see R5.)
- **R2.4 [STRONG]** Subject continuity must not require a permanent master secret
  whose compromise is unrecoverable.

### R3 Multi-device and least privilege
- **R3.1 [HARD]** A device holds its own key and never receives another
  principal's key.
- **R3.2 [HARD]** Delegated authority is a strict subset of the delegator's own
  (attenuation). *Currently violated — #2590.*
- **R3.3 [HARD]** Every security-relevant field of a delegation is
  cryptographically bound to it. *Currently violated — #2588.*
- **R3.4 [STRONG]** Adding or replacing a device must not require the highest
  authority to be online, except where root-only is a deliberate choice.
- **R3.5 [STRONG]** Device classes differ by scope and custody strength, never by
  principal class.

### R4 Determinism and partition tolerance *(inherited from #2469; non-negotiable)*
- **R4.1 [HARD]** No validity decision depends on the receiver's wall clock.
- **R4.2 [HARD]** No validity decision depends on the order in which a receiver
  learned two independent facts. (I12)
- **R4.3 [HARD]** Two honest nodes with the same durable state reach the same
  verdict. (I10)
- **R4.4 [HARD]** Verification never fetches authority material over the channel
  carrying the act. (I9)
- **R4.5 [HARD]** Authority **may** be evaluated against receiver-local replicated
  state **iff** that state is itself authenticated and converges deterministically.
  > *This corrects PRINCIPAL_MODEL §6.2's "carried, not resolved", which
  > over-generalized #2469 §14.1. #2469 resolves against local durable state at
  > ingress steps 7–11 by design; what it forbids is resolving against
  > **unauthenticated** state (§7.0: "the receiver would compare it against
  > commons state that is itself unauthenticated"). The distinction is
  > load-bearing: it is what makes §9 possible at all.*
- **R4.6 [HARD]** A legitimate act is eventually applied; nothing is permanently
  discarded because of arrival order. (I11/I12)

### R5 Revocation — a requirement on *effect*, not on admission
- **R5.1 [HARD]** A compromised device loses the ability to produce **new
  effective** acts within a bounded, stated window, without requiring a global
  total order.
- **R5.2 [HARD]** Revocation semantics is stated **per act class** and never
  differs silently between classes.
- **R5.3 [STRONG]** Revocation does not retroactively invalidate acts whose
  effects have already settled irreversibly. Without rollback semantics that is a
  correctness bug, not a security win (PRINCIPAL_MODEL §6.5 established this).
- **R5.4 [HARD]** "Could not determine whether this was revoked" never resolves to
  "not revoked". (AUTHORITY_SPINE §2)

### R6 Privacy — structural
- **R6.1 [STRONG]** Two institutions cannot correlate a person by default from
  protocol-visible identifiers alone.
- **R6.2 [STRONG]** A person proves standing in a context without revealing other
  contexts.
- **R6.3 [STRONG]** A person may, at their option, prove two personas are one human.
- **R6.4 [STRONG]** Recovery does not require deanonymizing every persona.
- **R6.5 [SOFT]** Network- and device-layer correlation is outside identity's sole
  control, but must not be made worse.
- **R6.6 [HARD]** Sybil resistance must not require a public global person
  identifier. **Uniqueness ≠ identification.**

### R7 Offline and partitioned operation
- **R7.1 [HARD]** Genesis works fully offline. **R7.2 [HARD]** Signing works fully
  offline. **R7.3 [HARD]** Delayed/reordered delivery is safe and converges.
- **R7.4 [STRONG]** Multiple devices may act concurrently while partitioned.

### R8 Authorship and governance
- **R8.1 [HARD]** Infrastructure never manufactures human authorship.
- **R8.2 [HARD]** These five stay distinct: session authentication · content
  authorship · delegated authority · institutional authority · transport identity.
- **R8.3 [HARD]** A receiver establishes authorship from bytes in hand plus
  authenticated local state — never from unauthenticated state.
- **R8.4 [STRONG]** A relay relays without holding any key of the author.

### R9 Recovery
- **R9.1 [HARD]** Recovery is possible from total device loss.
- **R9.2 [HARD]** Recovery is an authenticated, auditable event, never a silent
  substitution.
- **R9.3 [HARD]** Competing recovery attempts resolve deterministically; the
  losing attempt is visible.
- **R9.4 [HARD]** M-of-N counts **distinct authenticated** participants. *(#2591)*
- **R9.5 [STRONG]** Recovery does not create a new permanent single point of
  catastrophic failure.
- **R9.6 [STRONG]** Recovery is contestable by the legitimate subject (delay + veto).

### R10 Usability
- **R10.1 [HARD]** A member never needs to understand DIDs, PKI, Merkle trees,
  quorums or key-event logs to join and participate.
- **R10.2 [HARD]** Phone-only onboarding produces a secure, recoverable identity
  **by default** — not a degraded one to repair later.
- **R10.3 [STRONG]** At most one irreversible user decision at onboarding.

### R11 Infrastructure independence
- **R11.1 [HARD]** Identity works with no node / one personal node / many personal
  nodes / hosted-only. Moving between them changes *hosting*, never identity.

### R12 Evolvability
- **R12.1 [HARD]** Signature algorithm replaceable without "create a new person".
- **R12.2 [HARD]** New device classes without protocol change.
- **R12.3 [STRONG]** A migration has an enforceable end state. *A dual-stack that
  never ends is not a migration* (#2469 / O13).

### R13 Meaning Firewall *(constitutional)*
- **R13.1 [HARD]** The kernel holds keys, identifiers and opaque scopes — never
  `Person`, `member`, `role`, `cooperative`.
  > PRINCIPAL_MODEL §1.1 states this, and then §4 proposes `Person` and `Device`
  > as principal classes in `icn-identity`. That tension is real and §9.5
  > resolves it.

### Invariant tests (PRINCIPAL_MODEL §1, retained verbatim)
T1 replacing a phone must not create a new human · T2 replacing a node must not
create a new cooperative · T3 hosting a cooperative must not make the host the
cooperative · T4 assigning a personal node to a coop must not rewrite the node's
identity · T5 running one institution's workload must not change the node's species.

---

## 3. What ICN implements today

Verified at `b26bf681`. **This section is descriptive.** It does not restate
PRINCIPAL_MODEL §2, which remains accurate on the points it covers; it records
what this pass found *in addition to and in correction of* it.

### 3.0 Corrections to PRINCIPAL_MODEL §2, and new findings

Rows marked **verified** were re-read in source by hand during this pass; rows
marked *(agent-traced)* were produced by a source-tracing pass and spot-checked
rather than fully re-read. **One agent-traced claim was checked and found wrong**
— see F4 — so the *(agent-traced)* rows should be treated as high-confidence but
not yet load-bearing, and re-verified before any of them is used to justify
deleting code.

| # | Finding | Evidence | Status vs PRINCIPAL_MODEL |
|---|---|---|---|
| **F1** | **≈50% of hash-derived DIDs cannot be deserialized.** A SHA-256 digest is a valid compressed Ed25519 point only about half the time; `Did`'s hand-written `Deserialize` routes through `from_str` (`icn-identity/src/lib.rs:181-189`) which calls `VerifyingKey::from_bytes`. So an anchor-derived `Did` **serializes and stores fine and fails to read back ~50% of the time.** Measured 1994/4000 = 49.9% valid over SHA-256 outputs (independent check this session). Corroborated in-tree: `icn-governance/src/mandate.rs:404` and `apps/governance/src/manager.rs:10309` both warn that fixtures must hand-pick seeds that decompress. | verified | **New.** O18 understates this: it is not only "no controllable key", it is a coin-flip serialization defect. |
| **F2** | **`icn-zkp` silently substitutes an all-zero issuer key.** `prover.rs:227-230`: `match attestation.issuer_did.to_verifying_key() { Ok(vk) => *vk.as_bytes(), Err(_) => [0u8; 32] }` with the comment `// Fallback for test/development`, then proceeds to generate a proof. Combined with F1, an anchor-issued attestation produces a proof bound to a key nobody holds instead of failing. | verified verbatim | **New.** |
| **F3** | **The SDIS anchor is documented as *not* key-derived, on purpose.** `anchor.rs:1-13`: *"Unlike traditional DIDs where the identifier is derived from a public key, an SDIS Anchor is derived from a VUI"*; properties listed are **Permanent** (survives rotation), **Recoverable**, **Unique** (sybil resistance), **Privacy-preserving**. | verified verbatim | **New context for O18** — the anchor is a deliberate durable-subject primitive, and `to_did()` is what breaks it. |
| **F4** | **The VUI→anchor derivation *is* wired — and it is fed a VUI derived from the public DID.** `Anchor::from_vui` is called at `icn-commons/src/inner.rs:106`, inside `create_anchor_from_enrollment` (`:76`), which is production-reachable from `icn-gateway/src/api/commons/mod.rs:492`, `icn-gateway/src/api/sdis/simple_enrollment.rs:703` and `icn-core/src/services/sdis_service.rs:643`. But the VUI is `SHA256("gateway-enrollment-vui:" ‖ did)` (`inner.rs:81-87`), with the in-source comment *"Generate pseudo-VUI from DID (in real SDIS, this comes from ceremony)"*. `genesis_random` **is** generated (`:90-92`) but is not a field of `Anchor`, so it is discarded and the anchor cannot be re-derived from a re-presented VUI. | verified by grep + read | **New.** Together with F6 this is the **second independent site** where the VUI is a pure function of the public DID, so no deployed anchor carries the uniqueness or unlinkability the design claims. *(An earlier draft of this row, from a subagent trace, said `from_vui` had no production callers. That was wrong and is corrected here.)* |
| **F5** | **Even in the SDIS enrollment path the anchor is already a label, not a principal.** `simple_enrollment.rs:667`: `// The DID is the ephemeral_did - in SDIS, keys are created on the device`; the token is minted for `&ephemeral_did` (`:782`). The anchor is returned alongside as `anchor_id`. | verified verbatim | **New.** Anchor-derived DIDs carry no authority anywhere. |
| **F6** | **The routed enrollment endpoint bypasses the threshold PRF entirely.** `compute_temporary_vui(did) = SHA256(did.to_string())` (`simple_enrollment.rs:76-80`, used `:606`), a pure function of the *public* DID. This destroys the VUI's own stated properties — `vui.rs:17-19` claims *"Private: VUI reveals nothing about the person"* and *"Unlinkable: Without pepper, can't reverse VUI to identity"*. The file admits it: `:13-16` says **"NO sybil resistance"**. | *(agent-traced)* | **New.** |
| **F7** | **`combine_prf_partials` is XOR-then-hash, not a threshold scheme.** `icn-crypto-pq/src/threshold.rs:103-128`. Different t-subsets XOR to different values, so the VUI is reproducible only from **the exact set used at derivation** — n-of-n in effect, not 3-of-5. The header itself lists *"(Future) FROST threshold signatures"*. | verified verbatim | **New** (PRINCIPAL_MODEL §4.1.1 noted "PRF not signing"; the availability consequence is new). |
| **F8** | **The gateway multi-device subsystem is unreachable in production.** `IdentityManager::get_or_create_document` (`icn-gateway/src/identity_mgr.rs:60`) has **exactly three references repo-wide: the definition and two unit tests** (`:510`, `:515`, `:533`). No route, enrollment path or bootstrap ever creates a `DidDocument`, so `register_device` always returns `NotFound` at `:158`. | verified by grep | **Corrects the gap matrix**, which rates "Device keypair + roster" as **I — implemented**. It is implemented and **not reachable**. |
| **F9** | **`RotationEvent::verify()` and `DidDocumentCache::apply_event` have zero production callers, and `apply_event` never calls `verify()` at all.** `IDENTITY_UPDATES_TOPIC` (`sync.rs:23`) has no publisher and no subscriber. | *(agent-traced, grep-verified)* | Extends PRINCIPAL_MODEL §6.5 row 1: not only unwired, but the apply path would not verify even if wired. |
| **F10** | **The one production rotation signer and the verifier sign different messages.** `icnctl` signs a format string — `format!("{}:add_device:{}:{}:{}", …)` (`bins/icnctl/src/main.rs:6302-6311`) and the revoke analogue (`:6409-6418`) — while `verify()` checks a signature over `icn_encoding::encode(event_with_proof_cleared)` (`multi_device.rs:583,594-601`). **Every rotation event ICN actually produces would be rejected by its own verifier.** | *(agent-traced)* | PRINCIPAL_MODEL noted this for revoke; it holds for **both** kinds. |
| **F11** | **`is_anchor_did` is a tautology** — `self.as_str().starts_with("did:icn:")` (`anchor.rs:203-209`), true of every valid `Did`. | verified verbatim | Confirms "indistinguishable by format" with the stronger statement that the *discriminator function itself* is vacuous. |
| **F12** | **`RotationEvent::signing_message` is bincode over the struct with no domain separator** (`multi_device.rs:594-601`), sharing a signing domain with any bincode payload of the same shape. | *(agent-traced)* | New; folds into O10's obligation. |
| **F13** | **RPC recovery lets a caller name an arbitrary victim, and the *node* signs the trustee attestation.** `handle_recovery_initiate` takes `old_did`, `threshold` and `delay_period` from caller params (`icn-rpc/src/handler/recovery.rs:52-58`); `attest` builds the attestation with `state.own_signer()` — the node's own key (`:138`, `:173-183`). `RecoveryEvent` has no trustee registry, so any attestation counts. Reachable with the `recovery:write` scope. | verified verbatim | **New, and distinct from #2591**, which covers `sync.rs` / `recovery.rs` verification and dedup. |
| **F13a** | **F13 is currently inert.** `RecoveryEvent::finalize()` (`icn-identity/src/recovery.rs:215-225`) only sets a status field and a timestamp; no production code consumes `RecoveryStatus::Finalized` to change any authority, and `sync.rs::apply_event` (the only path that would clear verification methods) has no production callers (F9). **The defect is a latent critical: it becomes a takeover the moment anyone wires application of recovery events.** | verified by grep | Stated precisely so it is neither overstated nor dismissed. |
| **F14** | **Every wall-clock expiry check in the identity stack fails *open*.** `icn_time::current_timestamp_secs()` returns **0** on clock error (`icn-time/src/timestamp.rs:34-42`), so `now > expiry` is false and nothing has expired. | verified verbatim | New; strengthens R4.1's case beyond convergence into fail-open. |
| **F15** | **No pairwise, per-relationship or per-verifier identifier exists anywhere.** Not found workspace-wide: Pedersen commitment, nullifier, linkage secret, re-randomization, BBS+, selective disclosure, scoped DID, HKDF-derived *identifier*. | *(agent-traced)* | Confirms O3 is unstarted. |
| **F16** | **ICN has no working identity-privacy primitive.** `icn-crypto-pq/src/blind.rs:18-20` self-declares *"simplified commit-hash based"* and `unblind` stores the blinding factor **in the clear** (`:153-163`) — no unlinkability, and no caller outside the crate. `icn-zkp`'s AIRs set their transition constraints to zero (`stark/non_revocation.rs:122-123`) — **in fairness, the file argues soundness rests on boundary assertions plus trace commitment instead, and points to issue #506 for algebraic constraints**, so this is a tracked design position, not a hidden hole; but it does mean the transition relation is not algebraically enforced. Alongside it: the accumulator uses a 32-bit test modulus (`accumulator.rs:87-90`); `verify_non_membership` only checks the witness is nonzero (`:254-259`); `is_trusted_issuer` returns true when the trusted list is **empty** (`verifier.rs:78-80`); and `default = []` with `stark` behind a feature flag (`Cargo.toml:57-59`), so the default build can neither prove nor verify. `icn-privacy` is network-metadata privacy (onion routing, topic-name encryption), not identity. | *(agent-traced)* | **New and load-bearing:** any architecture depending on ZK selective disclosure is proposing **new cryptographic work**, not wiring. |
| **F17** | **`AuthorityGrant` and `Mandate` carry no signature field and are not replicated**; membership records carry no signature (`icn-entity/src/membership.rs:23-48`) and merge by **last-write-wins on a wall-clock field** (`icn-entity/src/actor.rs:315-330`); memberships are not gossiped at all — only the parent entity record is announced (`actor.rs:277-279`). | *(agent-traced)* | New; constrains what "authenticated local state" can mean today. |
| **F18** | **The only cryptographic evidence an institution exists is a set of *person* signatures over its charter** — `FounderSignature { did, signature, timestamp }` (`icn-governance/src/charter.rs:388-397`). | *(agent-traced)* | Supports PRINCIPAL_MODEL §4.1's conclusion with a concrete existing primitive. |
| **F19** | **The React Native client has no Person-vs-Device distinction at all.** `wallet.ts:94-120` generates one Ed25519 key per install and the DID is a direct encoding of it (`:106`); the private key is stored **hex and exportable** (`:102`, `:110`). Reinstalling produces a different person. `sdk/typescript` `registerDevice` signs nothing itself — the caller must hand-reproduce the `ICN_ADD_DEVICE:` string. | *(agent-traced)* | Extends §2.9. |
| **F20** | **`/v1/sdis/anchor` add-device and rotate-keys are disabled**, not merely unauthenticated: both handlers take no arguments and unconditionally return `Forbidden`, each carrying a doc-comment reading *"Disabled until this route can verify a signed current-key transition… Public anchor data is not authority to mutate an identity root"* (`api/sdis/anchor.rs:268-278`, `:301-313`). The sibling **reads** — `get_anchor`, `get_rotation_history`, `list_devices` — remain unauthenticated. | verified verbatim | **Live-state note for #2448**, which was verified at `b34cd3f6`: the **write** half of that issue is no longer reachable as described, while the **unauthenticated read** half stands. Reported, not acted on. |

### 3.1 What a `Did` actually is

`pub struct Did(String)` (`icn-identity/src/lib.rs:177`). `from_str` (`:209-244`)
requires `did:icn:` + multibase decode + exactly 32 bytes + a valid Ed25519 point.
`from_public_key` (`:193-196`) always emits base58btc; `from_str` accepts any
multibase, and `PartialEq`/`Hash` are derived over the inner `String` (`:174`) —
so **`Did` equality is string equality, not key equality**.

`new_unchecked` (`:284`) bypasses all of it. It has **one** call site:
`from_anchor_id` (`anchor.rs:194-197`). And `from_anchor_id`'s production use has
almost nothing to do with persons — it is a generic *"mint a `Did` from 32
arbitrary bytes"* hatch: cooperative **treasury** identifiers
(`icn-coop/src/actor.rs:493,788`; `apps/membership/src/coop_core/actor.rs:359,523`,
from 16 bytes zero-padded to 32), a ZKP holder pseudonym
(`icn-zkp/src/prover.rs:257`), and placeholders (`icn-ledger/src/fx.rs:118`,
`icn-gateway/src/api/sdis/qr.rs:163`).

**ESTABLISHED: `Did` is a key identifier, and every non-key use of it is type
reuse the type cannot support.** F1 and F2 are that reuse failing in production
code, not in theory.

### 3.2 Genesis is already sovereign — and that part is right

`KeyPair::generate()` (`:341`) → `Did::from_public_key` (`:191`). No registry, no
network. **R1.1 and R7.1 are already met and must be preserved.**

### 3.3 The device model is built, unreachable, and unverifiable

F8, F9 and F10 together: `multi_device.rs` implements a coherent roster and
rotation model; the gateway path that would use it can never find a document; the
verifier has no callers; and the one production signer signs a different message
than the verifier checks. `can_sign` has zero production callers (confirmed —
`docs/status.toml:32` already records this).

**Consequence for #2588 and #2590.** Both describe real defects in real code. But
the gateway route they sit on is **currently unreachable** (F8), which changes
their *urgency* without changing their *validity*: they are correctness debts to
fix before the path is wired, not live exploited surface. This document does not
propose closing them here; it proposes (§19) that they be fixed as part of
whatever wires device authority, so the wiring cannot ship the defects.

### 3.4 Authority is a bearer token over an HMAC, and one mint path proves nothing

Confirmed against source: `join_via_invite` (`api/invites.rs:245-296`) takes
`req.did` from the request body with form validation only, takes no
`HttpRequest`, and mints a token for it (`:280`) — **#2589 confirmed
line-for-line.** The challenge paths do prove key control, but the signed payload
is the **raw nonce bytes only** (`auth.rs:315`), with no domain separator and no
binding to the requested `coop_id` or `scopes`.

### 3.5 SDIS held two incompatible models of its own anchor, simultaneously

This is the most important historical finding, and it is a genuine unreconciled
split rather than a decision that was made and lost:

- **Anchor as public identifier** — it *becomes* the DID
  (`SDIS_IMPLEMENTATION_PLAN.md:344-345`), is a URL path key
  (`/v1/sdis/anchor/{anchor_id}`), is written to replicated state, is carried in
  cleartext gossip, is typed by users during recovery, and is returned in an HTTP
  response **alongside its own hex** (`api/commons/anchor.rs:67`).
- **Anchor as secret witness** — `Attestation` is *"Private attestation from an
  issuer (kept secret by holder)"* (`icn-zkp/src/types.rs:217`); the membership
  circuit takes the anchor-derived DID as a **private** input
  (`icn-zkp/src/circuit/membership.rs:60-63`); the audit checklist requires
  *"No VUI/anchor logged"* (`SDIS_AUDIT_CHECKLIST.md:286`).

**Anchor-to-anchor correlation across cooperatives is never named as a threat in
any SDIS document.** And `genesis_random`'s only documented rationale —
*"prevents rainbow table attacks on VUIs"* (`anchor.rs:102-104`) — is a
*pre-image* defence, which only makes sense if the anchor is published. So the
honest reading is: **the anchor was intended to be a publishable pseudonym that
hides its inputs, not a private continuity root** — and the ZK layer, built on
the opposite assumption, was never reconciled with it.

`Anchor::to_did` was in the original SDIS commit (`8c434077`, 2025-12-10) but was
specified under *"Task S2.1.2: Implement DID format extension"* with the literal
instruction **"Add backward compatibility"** — an interop bridge to the
pre-existing key-derived `Did`, not a doctrine that a person's public identifier
should be their anchor.

### 3.6 Documented contradictions nobody has adjudicated

The sharpest, which PRINCIPAL_MODEL does not record: **does recovery preserve the
DID?** `design/multi-device-identity-design.md:664` says the DID stays stable;
`design/social-recovery-design.md:26,29` says recovery **creates a new DID** and
installs a `did_mapping:<old_did>` indirection. Those are incompatible, both are
"PARTIALLY CURRENT", and each has code. Also unadjudicated: whether keys are
exportable (`CLIENT_MODEL` says never; MDI ships an encrypted `BackupSeed`), and
whether a person has one DID or many (`CLIENT_MODEL:99` vs PRINCIPAL_MODEL:1144).

### 3.7 There is no ordering substrate. At all.

This is the fact that decides the architecture, so it is stated flatly.

| Substrate | Authenticated | Ordered | Scope | Replicated |
|---|---|---|---|---|
| `SignedGovernanceOp` | yes (Ed25519 over a length-prefixed canonical body) | **no** — `seq` is per-`(author, domain)` and is explicitly *"a comparator for same-key conflicts, never an acceptance gate"* (`replication.rs:291-294`) | per-domain key, no domain-wide order | **no** — emission only; ingress recognises the frame and returns without applying (`apps/governance/src/actor.rs:1368-1371`) |
| Gossip | **no** — `GossipEntry` has `author: Did` and **no signature field** (`types.rs:116-133`) | partial only; the vector clock is node-global, LRU-evicting at 10 000, and never used to order within a topic | topics are global-by-domain (`governance:proposal`, `identity:updates`, `entity:updates`) | yes |
| Ledger journal | yes (per-entry Ed25519) | partial — Merkle-**DAG**, local timestamps | per-**currency** | yes |
| Entity / membership | **no** | LWW on a wall-clock field | global public topic; memberships not gossiped at all | partial |
| Identity rotation | event is signed, but the apply path never verifies it | per-DID `new_version` | per-DID | **no** — topic unwired |
| `Coordination` / Raft | — | — | — | **zero implementors** (`icn-kernel-api/src/coord.rs:124`) |

**ESTABLISHED: ICN has no replicated authenticated ordered log that a governance
operation lands on, and no per-cooperative or per-domain order that a governance
operation and an identity event could share.** The nearest specification of one is
#2469 §5.6, and it is unbuilt.

---

## 4. What "identity" collapses today

Testing the mission's decomposition hypothesis against §3, `Did` is currently
serving as **eight** distinct concepts:

| # | Concept | Where | Must it be a key? |
|---|---|---|---|
| 1 | key identifier | `from_public_key` | **yes — this is what the type is** |
| 2 | durable subject across rotation | `DidDocument.id` (`multi_device.rs:21-22`) | **no — see below** |
| 3 | privacy anchor | `Anchor::to_did` | **no**, and it must not be public |
| 4 | entity identifier | treasury DIDs from 16 padded bytes | **no** |
| 5 | network principal | node DID + Hello cert binding | yes |
| 6 | credential subject | `icn-zkp` holder DID | no — should be per-context |
| 7 | storage / membership key | sled keys, `gov:vote:{pid}:{voter}` | needs canonical bytes, not string-equality |
| 8 | session subject | `claims.sub` | no — a session is not authorship |

**Result of the decomposition test.** Only 1 and 5 genuinely need to be a public
key. Concept 2 is the decisive one:

> **ESTABLISHED: a durable human subject identifier must not be a public key.**
>
> *Proof.* R2.1 requires the subject to survive rotation; R2.2 requires the
> authorizing keys to change; R1.2 says custody defines identity. If the subject
> identifier *is* a key, then either that key can never be retired — contradicting
> R2.2 — or the subject is permanently named by a key nobody controls. ICN
> already exhibits the second horn: `DidDocument.id` is the original key's DID, so
> after `revoke_device("device-1")` **the document is identified by the key it
> just revoked** (`multi_device.rs:21-22`, `:426-447`). ∎

Concepts 2, 3, 6 and 8 must therefore each get their own representation. §9 gives
them one; §10 states what is left for `Did`.

---

## 5. The structural diagnosis: one choice, six blockers

PRINCIPAL_MODEL accumulated O8, O9, O13, O16, O17 and O18 across twenty review
rounds. They are not six problems. They are six consequences of **one** decision:

> **The Person's durable identifier is derived from a key, and authority over it
> must nevertheless evolve.**

Trace each:

| Blocker | Question | Why the choice forces it |
|---|---|---|
| **O17** | How does a first-contact peer get a trustworthy *starting* document? | The identifier commits to a **key**, not to a genesis **event**, so nothing self-certifies the document's origin. |
| **O8** | How does a verifier learn the *current* root after rotation? | `to_verifying_key()` recovers the **genesis** key forever, so the identifier points at the wrong authority the moment it changes. |
| **O16** | Which branch is legitimate when a compromised genesis key forks the chain? | The chain is anchored at a key that **can never be retired**, because retiring it would break the anchor. A permanent root is a permanent forking capability. |
| **O18** | How are anchor-derived Persons modelled? | Anything that is *not* a key cannot be an identifier — so a legitimately non-key subject (F3) had to be smuggled through `new_unchecked`, producing F1 and F2. |
| **O13** | How is `GOV_OP_V1` retired convergently? | `Did` carries no principal type tag and no version position, so no receiver can classify an author or place a cutover. |
| **O9** | How is a device revoked convergently? | An authorization is signed once against an authority that changes, and there is no order relating the two. |

O16 deserves emphasis because it is the deepest: **a key-derived durable
identifier makes its genesis key a permanent master secret.** That directly
violates R2.4 and R9.5, and no amount of chain machinery repairs it — the chain's
anchor *is* the master key.

**REJECTED: the key-derived durable Person identifier** (PRINCIPAL_MODEL §4's
Person row, and Model A of §7). Not because it is inelegant, but because it
mathematically entails a permanent unretireable root, and every one of the six
blockers is downstream of that.

O9 is the one blocker that is **not** dissolved by fixing the identifier, because
it is not a consequence of the choice — it is a consequence of physics. §6.1.

---

## 6. The reframe: three moves, two of them forced

### 6.1 Move 1 — stop asking for global convergent revocation

**Classification: RECOMMENDED, on a bounded formal argument — not ESTABLISHED.**
*(An earlier draft called this "provably unachievable" and classified it
ESTABLISHED. That was an overstatement, corrected in adversarial review; the
correction is recorded here rather than quietly removed.)*

The argument has two steps and one honest caveat.

1. Requiring all peers to agree on whether an act preceded a revocation is
   requiring **atomic broadcast**. Chandra & Toueg, *Unreliable Failure Detectors
   for Reliable Distributed Systems*, JACM 43(2), 1996: *"Consensus and Atomic
   Broadcast are equivalent in asynchronous systems with crash failures… a
   solution for one automatically yields a solution for the other."*
2. **Deterministic** consensus is unsolvable in the **pure asynchronous** model
   with crash faults. Fischer, Lynch & Paterson, JACM 32(2), 1985: *"every
   protocol for this problem has the possibility of nontermination, even with only
   one faulty process."*

> **The caveat, stated because omitting it would be misuse.** FLP is routinely
> circumvented — by randomization (Ben-Or), by partial synchrony (DLS, PBFT), and
> by unreliable failure detectors. Chandra & Toueg is itself a **positive** result:
> it shows ◊W suffices to solve consensus. Citing only its equivalence half while
> suppressing its solution half would be quoting a paper against its own thesis.
>
> So the correct claim is **not** "global convergent revocation is impossible." It
> is: **global convergent revocation requires adopting a synchrony assumption,
> randomization, or an ordering authority — and ICN declines all three.** #2469
> excludes consensus by design; R4.1 forbids clock-based synchrony; and
> `icn-kernel-api::Coordination` is a Raft-shaped trait with **zero implementors**
> (§3.7). That is a *design choice with a cost*, and it is reopenable by paying the
> cost, not a theorem that forecloses it.

⇒ **O9, as posed — "how is a device revoked convergently *at all*" across the
network — has no solution within ICN's stated model.** PRINCIPAL_MODEL was right
that all three candidate shapes fail; it stopped one step short of the reason,
which is that the target is unreachable *given premises ICN has chosen*, rather
than merely unbuilt.

The survey confirms it empirically: **no deployed system achieves decentralized
global convergence on revocation.** Every one either supplies an ordering
authority (Signal Sesame's central server; Matrix's homeserver; WebAuthn's RP
database; BitstringStatusList's issuer endpoint), only *detects* divergence
(Certificate Transparency, CONIKS, KEYTRANS — and draft-ietf-keytrans-architecture-09
§3.3 concedes detection needs *"a connected graph of all users"*, so a partition
defeats it), or deliberately discards the ambiguous case (DIDComm: *"The message
recipient MUST ignore those messages"*). KERI detects duplicity and resolves it
only **per validator** — *"the first seen version of an event is the authoritative
one for that validator"* — so partitioned validators diverge **by design**.

**The escape hatch is formal.** Guerraoui, Kuznetsov, Monti, Pavlovic &
Seredinschi, *The Consensus Number of a Cryptocurrency*, PODC 2019: *"the
consensus number of an asset transfer object is 1… a more general k-shared asset
transfer object where up to k processes can atomically withdraw from the same
account… has consensus number k."*

Consensus number 1 means the object is **wait-free implementable without
consensus** — but be precise about what that buys:

- The primitive it needs is **Byzantine reliable broadcast (BRB)**, not "gossip".
  BRB guarantees non-equivocation *among receivers of a delivered message*; it
  does **not** stop a Byzantine single writer from authoring two conflicting
  events in the first place. That residual is exactly the duplicity case §9.2 has
  to handle explicitly.
- **ICN does not have BRB.** `GossipEntry` carries no signature field at all
  (§3.7), so building the log's replication path is "implement Byzantine reliable
  broadcast", not "add a topic". §19.1 slice N3 is costed accordingly.

> **This gives the design its single most important constraint:**
> **keep the authority log single-writer.** The moment two devices can
> independently author authority changes for the same subject, k > 1 and the
> system is back in consensus territory.

### 6.2 Move 2 — make the identifier commit to an *event*, not to a key

A **self-addressing identifier** — `SAID = H(inception_event)` — is
self-certifying without being a key. Given the identifier and the inception
event, any peer verifies the binding by hashing. This is KERI's construction, and
it is the only one in the survey that discovers current authority **with no
online authority, no clock and no consensus** (by replaying a log anchored at a
self-certifying inception).

Two consequences fall out immediately:

- **O17's *authenticity* half dissolves.** The genesis document *is* the thing the
  identifier commits to, so there is no trusted starting document to obtain. Its
  **acquisition** half does not: a truncated-but-valid prefix verifies perfectly and
  yields a stale authority state, so completeness remains open (§18, O-N5).
- **O16 is narrowed by pre-rotation — for rotations only.** KERI's inception
  commits a *digest* of the next key set; rotation reveals it. A compromised
  **current** key therefore cannot *rotate*, because it does not hold the
  pre-committed next keys.

> **Pre-rotation does not prevent forking in general, and an earlier draft of
> this section wrongly said it did.** *(Corrected in adversarial review.)* It
> constrains **establishment** events only. A compromised current key can still
> author a competing **non-establishment** event — an `authorize` or a `revoke` —
> at the next position, because those are signed by a currently-authorized key and
> commit to nothing held in cold storage.
>
> This matters twice over. First, the fork is real. Second, and worse, a naive
> "every honest holder refuses to advance past the fork" rule turns a key
> compromise into **permanent denial of identity**: the victim can never publish
> the revocation that would remove the attacker, because the log is wedged.
>
> The mitigation is KERI's **superseding recovery**, and it is adopted here:
> *"a rotation event overrides or supersedes an interaction event with the same
> event sequence number"*, after which the witness *"will no longer accept any new
> events of any type into the disputed branch."* So the victim recovers by
> **rotating at the disputed position** using pre-rotated material the attacker
> does not hold. KERI's own honest concession applies unchanged: *"Recovery may
> create an unavoidable race condition but the special rule minimizes the extent of
> that race condition."*
>
> Net: forking a **rotation** requires cold-key compromise; forking a
> **non-establishment** event requires only current-key compromise and is
> *recoverable but racy*. Residual policy is **O16′** (§17), and it is a liveness
> question, not only a selection question.

### 6.3 Move 3 — let the acceptor set recency, and let the subject own the order

Rivest, *Can We Eliminate Certificate Revocation Lists?*, FC'98 — 28 years old and
still the correct architecture here:

> **"Proposition 1: Recency requirements must be set by the acceptor, not the
> certificate issuer. The reason is that the acceptor is the one who is running
> the risk if his decision is wrong."**
> *Corollary 1: "Periodically-issued CRLs are wrong."*
> *Proposition 2: "The signer can (and should) supply all the evidence the
> acceptor needs, including recency information."*

Applied to ICN: an institution is the acceptor. It decides how fresh a member's
authority evidence must be. The member's device supplies that evidence with the
act. Nobody needs a global agreement, because **no global decision is being
made** — each institution decides for itself, about its own state, which is
exactly the scope in which it already has legitimate authority.

This also explains why ICN's one *working* revocation mechanism works: RPC and
gateway token revocation (AUTHORITY_SPINE §2) is scoped to a single relying party
with a durable store and a fail-closed read. It is not an accident; it is the only
shape that can work.

### 6.4 What the external survey teaches, architecture by architecture

Primary specifications, not summaries. **Adopted** means the idea is used in §9;
**rejected** means it was examined and does not fit ICN's premises.

| Architecture | The one thing it teaches | Verdict |
|---|---|---|
| **W3C DID Core** | The four *verification relationships* (`authentication`, `assertionMethod`, `capabilityInvocation`, `capabilityDelegation`) are a clean device least-privilege vocabulary. But its own §9.8 says that positioning a signature in time requires anchoring "*on a blockchain*", and otherwise "*the only safe course is to disallow any consideration of DID state with respect to time*". | **data model adopted; resolution rejected** |
| **did:key** | The correct primitive for *genesis* and the wrong one for a durable subject. Its spec says so: *"they cannot be updated or deactivated"*, *"any change to the cryptographic key… effectively creat[es] a new identity"*, and a section titled **"Long Term Usage is Discouraged."** **This is precisely what ICN's `Did` is being used as today.** | **diagnosis adopted** |
| **did:peer / DIDComm** | Relationship-scoped identifiers with no registry — the direct precedent for per-context subjects. But its rotation answer is *"The message recipient MUST ignore those messages"*: determinism bought with **silent message loss**, which violates R4.6. | **scoping adopted; rotation rejected** |
| **VC 2.0 + BitstringStatusList** | Portable standing is real and worth having (§17, O6). Its revocation is not: status checking mandates an online fetch (`STATUS_RETRIEVAL_ERROR` when unreachable), and its own privacy section concedes correlation below ~131 072 entries — unattainable per cooperative. | **credentials adopted as a layer; status lists rejected** |
| **KERI** | The closest architecture to ICN's premises, and the source of the two mechanisms §9 uses: **self-addressing identifiers** and **pre-rotation**. Also the honest limit: duplicity is *"first seen… for that validator"*, so partitioned validators diverge **by design**. | **mechanisms adopted; the stack (witnesses, watchers, CESR, OOBI) rejected as too heavy for ICN's stage** |
| **Key transparency / CT / CONIKS** | Non-equivocation is *detection*, never a decision. draft-ietf-keytrans-architecture-09 §3.3: detecting a fork needs *"a connected graph of all users"*, else the log "*can attempt to partition users into subsets that do not gossip*". | **rejected** — it yields an alarm, not a verdict |
| **WebAuthn / passkeys** | The best deployed device-holder proof and UX, and a model of correlation resistance: *"Relying Parties are not able to detect any properties, or even the existence, of credentials scoped to other Relying Parties."* Revocation is purely the RP's database. | **adopted as a device-key gate; rejected as a subject identifier** |
| **UCAN / ZCAP-LD** | Attenuating delegation chains are the right shape for R3.2, and UCAN's revocation set is deliberately a **grow-only, order-insensitive CRDT**. But it concedes it does not undo an already-honoured invocation — convergence on the *set*, not on the *effect*. | **attenuation adopted; revocation-as-answer rejected** |
| **FROST (RFC 9591)** | A stable group public key with membership changing beneath it — the shape a threshold-held institution key would need. But *"FROST does not provide robustness"*, DKG is out of scope, and signing needs `t` reachable participants in one session. | **deferred** — see O12 |
| **BBS+ / anonymous credentials** | The only design offering cross-institution unlinkability *and* third-party verifiability. Requires a BLS12-381 pairing stack, is still Internet-Draft, and its revocation story falls back to status lists. | **deferred** — and F16 means ICN could not wire it today regardless |
| **Signal Sesame / Matrix cross-signing** | Two shipping patterns worth stealing: **tombstone-with-bounded-window** (Sesame keeps stale device records until `MAXLATENCY` so delayed messages still decrypt) and **receive-time trust re-evaluation** (Matrix: *"messages sent from non-cross-signed devices… SHOULD NOT be displayed"*). Both assume a central ordering authority ICN does not have. | **patterns adopted (§9.3); central authority rejected** |

**The cross-cutting lesson.** Of every architecture surveyed, exactly one
discovers current authority with **no online authority, no clock and no
consensus** — KERI, by replaying a single-writer log anchored at a self-certifying
inception. That is not a coincidence: it is the only one built from the same
premises ICN's R1/R4/R7 state.

### 6.5 What the three moves buy

**This table is a summary of §18 and must never diverge from it.** *(An earlier
draft marked O13 and O17 "dissolved" here while §18 correctly called both partial;
review caught the mismatch, and readers of the summary alone could have treated
unresolved prerequisites as closed.)*

| Blocker | Disposition | Remaining condition |
|---|---|---|
| **O8** | **DISSOLVED** by 6.2 — current authority is derived by replaying the subject's own log from a self-certifying inception | — |
| **O18** | **DISSOLVED** by 6.2 + §10 — a non-key durable subject becomes expressible, so nothing needs `new_unchecked` | the anchor↔member keying trace in §15.2 is still incomplete |
| **O17** | **PARTIAL** — self-addressing settles **authenticity** of the inception event | **acquisition/distribution is not settled**: a truncated-but-valid prefix verifies and yields stale authority. §18, O-N5 |
| **O16** | **NARROWED**, not defused — pre-rotation covers establishment events only | non-establishment forks remain; recovery is by superseding rotation and is racy. **O16′** |
| **O13** | **PARTIAL** — per-subject declaration beats a global cutover | O13 constrains **receivers**; a subject who never declares keeps v1 alive, which R12.3 [HARD] forbids. Receiver-side retirement **unsolved** |
| **O9** | **REJECTED as posed** by 6.1 — unachievable *within premises ICN has chosen* (not a theorem-level impossibility; see §6.1) | replaced by the per-act-class requirement in §9.3, which itself carries a known R5.1 gap until O-N6 |

---

## 7. Candidate models

Five models, each stated at its strongest. None is a straw man; A is the repo's
own current proposal and D is the position most of the external ecosystem holds.

### Model A — Key-as-Person *(the status quo, plus the A0 program)*

The Person identifier is a self-certifying key-derived `Did`. Authority evolves
through an authenticated identity-document chain anchored at the genesis key the
DID encodes. Device authority travels as a carried, self-contained
`DeviceAuthorization`.

**Strongest case.** Nothing new to build for genesis — it already works offline
(§3.2). Verification needs no resolution, no registry, no network round-trip:
`author.to_verifying_key()` and done, which is exactly why #2469 could freeze a
field set. One identifier everywhere means membership records, storage keys and
the whole existing type surface keep working. The chain's cheap parts already
exist: capability checks, Ed25519 verification, strict `+1` monotonicity.

**Why it fails.** §5: the identifier's binding to the genesis key makes that key a
**permanent, unretireable master secret**, which is a direct R2.4/R9.5 violation
and the root of O16. O8, O17, O13 and O18 follow. It also fails R6.1 outright —
one permanent 32-byte value in every membership record, every `gov:vote:{pid}:{voter}`
key and every gossiped op. **REJECTED.**

### Model B — Stable non-key Person anchor, globally resolvable

A durable Person identifier exists independent of keys (random bytes, or a hash);
keys and devices are authorized *for* it. This is what SDIS's `Anchor` actually is
(F3).

**Strongest case.** It gets R2.1/R2.2 right — subject and authority are genuinely
separate, rotation is natural, and there is no permanent genesis key. It matches
the strongest existing ICN design intent, and `KeyBundle::did()` already carries
the doc-comment *"Returns the anchor's DID, **not a key-based DID**"*.

**Why it fails.** Three ways. (i) **Genesis has no authority root**: nothing stops
a second party claiming an unclaimed anchor, and first-writer-wins in a global
namespace is arrival-order dependent — the exact rule #2469 §8 withdrew as
non-convergent. (ii) **Resolution requires a global authenticated directory**, so
you inherit key-transparency's split-view problem, whose own architecture draft
concedes detection needs *"a connected graph of all users"*. (iii) **Correlation
is worse than Model A**, because a permanent non-rotating anchor cannot even be
changed. **REJECTED** as a *global* identifier; its core idea — a durable non-key
subject — is retained in §9 and made self-certifying and *per-context*.

### Model C — Context-scoped identity with a private continuity root

There is no global Person identifier. The person holds keys and a private
continuity root. Each institution knows them by a distinct, self-certifying
subject identifier whose authority evolves in a log the subject alone writes. The
institution is the relying party.

**Strongest case.** Satisfies R6.1 structurally rather than by policy. Removes the
global namespace and therefore O8/O16/O17 in their global form. Single-writer
authority logs have consensus number 1 (§6.1), so no consensus is required.
Matches the only external designs built on the same premises (did:peer's
relationship-scoped registry; KERI's per-relationship AIDs).

**Costs, stated up front.** Recovery is per-context work, not one act (§12).
Cross-context proof requires an explicit linkage step (§11.3). Sybil resistance
needs a nullifier, and ICN's ZK stack cannot currently provide one (F16). "Who is
this person, globally?" becomes unanswerable — which is usually correct and
occasionally inconvenient.

> **Two costs an earlier draft omitted, both found in adversarial review, and both
> worse than the ones above.**
>
> 1. **The continuity root has a larger blast radius than Model A's genesis key.**
>    It is the person's index of their own subjects *plus* the material that
>    recovers each one, so compromising it is simultaneously takeover of **every
>    context** and **full cross-context deanonymization** — it hands the attacker
>    exactly the linkage map §11 exists to prevent anyone else from building.
>    Model A's rejected genesis key compromises **one** identity. This model
>    concentrates what Model A distributes, and §12.2 path (b) exists because of it.
> 2. **Availability is a per-subject dependency, not zero infrastructure.** A
>    verifier needs the subject's log prefix; when it is missing, *someone
>    reachable must serve it*. Model B was rejected partly for needing one global
>    directory; Model C needs **N per-subject availability paths**. They are
>    materially different — a relying party can serve the logs of its own members,
>    so the dependency is per-relationship rather than global and universal — but
>    "no centralization dependence" is too clean, and the matrix below is corrected.

### Model D — Credential-centric, no person identifier at all

The person holds keys; institutions issue verifiable credentials; every
relationship is a credential. Standing is proven by presentation.

**Strongest case.** Maximum portability — standing provable to a third party that
holds no registry, which is precisely the gap PRINCIPAL_MODEL §7.2 names. Aligns
with the largest external ecosystem (VC-DM 2.0). Institutions already behave this
way: `FounderSignature` (F18) is a credential in all but name.

**Why it fails as the *whole* answer.** (i) A credential subject still needs an
identifier, so pairwise identifiers reappear — D collapses into C plus a
credential layer. (ii) **Credential revocation is the same impossibility again**,
and the ecosystem's answer is BitstringStatusList, which mandates an online fetch
(`STATUS_RETRIEVAL_ERROR` if unreachable) and whose own privacy section concedes
correlation below ~131 072 entries — unattainable per cooperative. (iii) It
supplies **no continuity mechanism**: lose every device and there is nothing that
is still you. **REJECTED as the whole answer; adopted as a layer** (§11.2, §14).

### Model E — Hybrid identity/authority graph

Continuity anchor + authority keys + pairwise identities + recovery authorities +
institutional credentials, all first-class.

**Strongest case.** It can express every requirement, and every component has a
real justification somewhere in §2.

**Why it fails.** Complexity is itself a cost, and here it is disqualifying: five
first-class concepts means five genesis stories, five recovery stories and five
revocation stories, whose interactions are where security bugs live. §3 is a
catalogue of what happens when ICN maintains several parallel identity models at
once — three "steward vouch" authority models that disagree (F6/§3.5), two
incompatible `DidDocument` types, two contradictory definitions of "who may sign".
E institutionalizes that failure mode. **REJECTED** — but note that C *is* E with
four of the five concepts demoted from protocol objects to client-side or
app-layer concerns, which is the whole point.

### Evaluation matrix

Qualitative, with reasons rather than false precision.
**✔** meets · **~** partial / costed · **✘** fails · **n/a** not applicable.

| Criterion | A key-as-person | B global anchor | **C context-scoped** | D credential-only | E hybrid |
|---|---|---|---|---|---|
| Self-sovereignty (R1.1–R1.3) | ✔ | ~ needs directory | **✔** | ✔ | ✔ |
| No centralization dependence | ✔ | ✘ global directory | **~ per-subject log availability** | ~ status lists | ~ |
| Offline genesis (R7.1) | ✔ | ~ claim race | **✔** | ✔ | ✔ |
| Device compromise bounded (R3.2) | ~ | ~ | **~** bounded only while ≥1 key survives (§9.3) | ~ | ~ |
| **Root compromise survivable (R2.4)** | **✘ permanent master** | ✔ | **✘ path (a) — and unrecoverable; ✔ path (b)** | n/a | ~ |
| Total-loss recovery (R9.1 **[HARD]**) | ✘ O14 — chain cannot advance | ~ | **~ admitted shortfall** — (a) fails if the backup is lost; (b) needs threshold **signing** ICN lacks (F7) and which is **absent from §19.1** | ✘ nothing persists | ✔ |
| Multi-device (R3) | ~ roster only | ~ | **✔** | ~ | ✔ |
| Least privilege (R3.2) | ✘ #2590 | ~ | **✔** attenuated | ✔ | ✔ |
| Deterministic verdict (R4.3) | ~ | ✘ split view | **✔** single-writer | ✘ fetch may fail | ~ |
| Partition tolerance (R7.3) | ~ | ✘ | **✔** | ✘ | ~ |
| Historical signature validity (R2.3) | ~ ambiguous after recovery | ~ | **✔** anchored position | ✔ | ~ |
| **Correlation resistance (R6.1)** | **✘** | **✘ worse** | **✔** | ✔ | ✔ |
| Selective disclosure (R6.2) | ✘ | ✘ | ~ needs new crypto (F16) | ✔ | ✔ |
| Governance authorship (R8) | ~ blocked O8/O9 | ~ | **✔** | ~ | ✔ |
| Institutional delegation | ~ | ~ | **✔** | ✔ | ✔ |
| Hosted-node independence (R11) | ✔ | ✔ | **✔** | ✔ | ✔ |
| User experience (R10) | ✔ one identity | ✔ | **~** many contexts, hidden by the app | ~ | ✘ |
| Implementation complexity | ~ | ✘ directory | **~** | ~ | ✘ |
| Protocol complexity | ~ | ✘ | **~** | ~ | ✘ |
| **Migration complexity** | ✔ none | ✘ | **~ low, not zero** — keys and custody are untouched, but every existing DID needs a new `SubjectId` plus a membership bridge (§15.1). *(Was scored "near-zero" before review round 3 showed a DID cannot be a subject identifier.)* | ✘ | ✘ |
| Cryptographic agility (R12.1) | ✘ DID pins the suite | ✔ | **✔** | ✔ | ✔ |
| Auditability (R9.2) | ~ | ~ | **✔** append-only log | ~ | ~ |
| Meaning Firewall (R13.1) | ✘ `Person` in kernel | ✘ | **✔** §9.5 | ✔ | ✘ |

**Selected: Model C** — on the balance of the table, not on a clean sweep. Three
cells above are shortfalls the model does **not** discharge: R2.4 under the
default recovery path, R9.1 [HARD] under both recovery paths, and per-subject log
availability. §9.6 carries them forward; a reader who weighs R9.1 above
correlation resistance should reach a different answer, and that is a legitimate
reading of the same evidence.

---

## 8. Adversarial scenarios

All 25 mission scenarios, walked against **Model C as specified in §9**, with the
discriminating failures of the rival models noted. A scenario that Model C cannot
answer is marked **OPEN** and appears in §17 — none are silently absorbed.

| # | Scenario | Model C outcome | Where A/B/D/E differ |
|---|---|---|---|
| 1 | Genesis key compromised 5 yrs after a legitimate rotation | **Contained.** The identifier commits to the inception *event*, not to a key, and rotation revealed pre-committed next keys. A stolen genesis key can sign nothing current and cannot rotate — it does not hold the pre-rotated material. | **A fails catastrophically** — the genesis key is the chain anchor forever (O16). |
| 2 | Attacker and user each create a same-generation successor | **Split.** Forking a *rotation* requires cold-key compromise. Forking a *non-establishment* event (`authorize`/`revoke`) requires only current-key compromise (§6.2). Duplicity is detected; the victim recovers by **superseding rotation** at the disputed position. Racy, and **OPEN as O16′** — which is a *liveness* question, not only a selection one | A: both branches verify from genesis; a content tiebreak converges *on the attacker*. |
| 3 | Two concurrent recovery attempts | Recovery is per-context (§12). Each institution runs its own delay-and-veto and reaches one outcome. Contexts **can** diverge — the person may recover Coop A while an attacker takes Coop B. That is a real cost, and it is *bounded blast radius* rather than a global race. **Stated, not hidden.** | A/B: one global race decides everything at once. |
| 4 | One trustee submits the same proof M times | Rejected — thresholds count **distinct authenticated** participants (R9.4). Today this fails: `sync.rs:263-269` has neither signature check nor dedup (#2591), and SDIS's `approve_by_steward()` is a bare `+= 1`. | same for all models; a requirement, not a differentiator |
| 5 | Threshold trustees collude | **Not fully mitigated, by design.** Mitigations: per-context recovery means colluding guardians must pass *each* institution's procedure; delay + subject veto (R9.6); recovery is a logged event, never silent (R9.2). **Residual risk stated.** | E claims more; the extra machinery does not remove the trust |
| 6 | Phone stolen while disconnected two weeks | The thief acts **within the device's attenuated scope** until the log advances past the authorization. Bounded by position, not wall-clock — **but only if the subject retains another authorized key.** For a sole-device person the bound is recovery latency, and **R5.1 is not met** (§9.3) | A: unbounded and with no recovery path at all |
| 7 | Revoked phone signs a governance action and delivers it late | **Per act class (§9.3).** Deferred-decision acts (votes): the decision pins a log position, so the act is evaluated against converged state and does not count. Settled acts: revocation is **prospective only**; the act stands and the bound is scope, not retroaction. | **No model solves this globally — §6.1 proves it impossible.** A/B claim to and cannot |
| 8 | User loses every device | Recovery from the continuity root, per context (§12) — **if the backup survived**. Lost devices *and* backup is answered only by guardians, which need threshold signing ICN lacks (F7). **R9.1 [HARD] is not fully met** | A: **impossible today** — `RotationEvent::verify` needs an existing non-revoked key (O14). D: nothing persists |
| 9 | Replacing a ten-year-old phone | Ordinary device authorization + revocation in the subject's log. Invariant test **T1 passes**. | all pass |
| 10 | Signature algorithm deprecated | Rotate to a new suite; the identifier is a hash of an event, not of a key, so it is unchanged (R12.1). | **A fails** — the DID *is* an Ed25519 key |
| 11 | Gateway compromised | It relays; it holds no subject key and cannot author (R8.1/R8.4). It **can** withhold or delay — unmitigated and stated. | all models that separate relay from authorship |
| 12 | Personal node compromised | Same as 11. The node is a distinct principal; T4 passes. | — |
| 13 | Malicious hosting provider | Cannot become the institution: institutional acts are person-signed mandate chains, not host-key signatures (§14). **T3 passes.** Can deny service — stated. | — |
| 14 | Institution impersonates a member | Cannot: the institution holds no key that derives the member's subject, and acts carry a device signature the institution cannot produce. | today this **fails** — #2589 mints a token for an unproven subject |
| 15 | Member wants Coop A and Coop B unable to correlate them | **Satisfied by construction** — distinct subject identifiers, distinct per-context device keys (§11.1). | **A and B fail** — one identifier everywhere |
| 16 | Member wants to prove two personas are one human | Sign a linkage statement with both subjects' current keys (§11.3). Cheap and consent-gated. | A: trivial but always-on, i.e. not consent-gated |
| 17 | Federation needs a delegated mandate without learning unnecessary identity | The delegating coop attests "this subject is our delegate"; the federation learns the federation-scoped subject only. Note the coop **does** learn both — necessarily, since it created the delegation. **Correlation follows the relationship, not the identifier.** | — |
| 18 | User has no personal node | Unaffected — identity is client-side; a gateway relays. **R11.1.** | all pass |
| 19 | User runs three personal nodes | Unaffected — nodes are separate principals. **T4/T5 pass.** | all pass |
| 20 | User signs while partitioned | Works: signing is offline (R7.2); the act references a log position the device already holds. | — |
| 21 | Device authorization arrives before/after a root rotation, in different orders | Both are events in **one single-writer log** with `+1` monotonicity, so their relative order is fixed by the log, not by arrival. Out-of-order delivery **buffers**, never rejects (R4.6). | **A cannot place them at all** — this is exactly O8's collapse into O9 |
| 22 | Revocation and a member act arrive in opposite orders | Admission is monotone and order-independent; **effect** is computed from converged state at a pinned position (§9.3). Two honest nodes agree. | A/B: arrival-order divergence — PRINCIPAL_MODEL §6.5 refutes both shapes |
| 23 | An old receipt must stay verifiable after recovery | **Yes.** The act names the log position at which its signing key was authorized; the log is append-only and hash-chained, so "K was authorized at position N" is a permanent fact. R2.3. | A: recovery replaces the root and history becomes ambiguous |
| 24 | Migrating an SDIS anchor identity | Near-free. F5 (the anchor was never the acting principal) and F20 (mutating routes disabled) mean **no deployed anchor carries authority**; F4 shows the VUI feeding it is `SHA256(did)`, so none carries uniqueness either. §15.2. | A: O18 is listed as **critical** precisely because it assumed otherwise |
| 25 | An existing key-derived DID holder must not "become a new person" | **Their key, custody and signing ability are untouched** — no ceremony, no re-enrollment. A **new `SubjectId` is allocated** whose inception names their existing `Did` as initial authorized key; the identifier changes, the person does not, and membership rows keyed by `Did` need a documented bridge (§15.1). | B/D/E require a full cutover |

**Scenarios Model C does not fully answer, carried to §17:** 2 (residual duplicity
policy), 3 (cross-context divergence under concurrent recovery), 5 (guardian
collusion), 7 (irreducible — §6.1), 11 (relay withholding).

---

## 9. The recommended architecture

**Classification: RECOMMENDED.** It is the strongest model after comparison, not a
forced conclusion. §17 lists what would reopen it.

### 9.1 Four objects, each with exactly one job

| Object | Definition | Layer | Public? |
|---|---|---|---|
| **Principal** | a keypair. Its public key **is** a `Did`. | kernel | yes, to whoever verifies a signature |
| **Subject** | `SubjectId = H(inception_event)` — a **self-addressing identifier**, not a key. One per context. | identity | to that context only |
| **Authority log** | a per-subject, **single-writer**, hash-chained, `+1`-monotone log of authority events, with **pre-rotation** | identity | replicated to that subject's relying parties |
| **Continuity root** | a private client-side secret: the person's index of their own subjects, plus the pre-rotation material that lets them recover each one | **client only — never published, never sent** | **no** |

The person is not an object in the protocol. That is the point. `Person` is an
app-layer word for *the human who holds a continuity root*, and no kernel type
needs to know it exists (R13.1).

**Why the subject is a hash of an event, not a key.** §4 proved a durable subject
must not be a key; §6.2 supplies the alternative. Given `SubjectId` and the
inception event, any peer verifies the binding by hashing — self-certifying with
no registrar, no resolver and no trusted starting document. That answers **O17's
authenticity half only** — completeness of the prefix you were handed is a separate,
still-open question (§18).

### 9.2 How authority evolves

The inception event declares the subject's initial authorized keys **and commits
to a digest of the next key set** (pre-rotation). Each later event is signed by a
key authorized in the immediately preceding state and advances the position by
exactly one.

```
inception  { keys: [K0], next: H(K1set), policy }        position 0
   │        SubjectId = H(this event)          ← self-certifying, no registry
   ▼
authorize  { device: D1, scopes, valid_for }             position 1
   ▼
rotate     { reveal K1set, next: H(K2set) }              position 2
   ▼
revoke     { device: D1 }                                position 3
```

Four properties, and each does specific work:

- **Single-writer ⇒ consensus number 1** (§6.1), given Byzantine reliable
  broadcast underneath — which ICN does not have yet (§6.1, §19.1 N3).
- **Pre-rotation** means a compromised *current* key cannot **rotate**. It does
  **not** prevent that key forking a non-establishment event — see §6.2. Scenario 1
  contained; scenario 2 narrowed, not eliminated.
- **`+1` monotonicity** makes out-of-order delivery **buffer**, never reject
  (R4.6) — precisely the gap-fill obligation PRINCIPAL_MODEL's A0 identified, now
  with a reason it is safe.
- **Duplicity is detected, and recovered from by superseding rotation** (§6.2).

> **Do not let two devices write this log.** The instant two devices can
> independently author authority events for one subject, k > 1 and consensus is
> required again (§6.1). Concurrency here is a **correctness boundary**, not a
> feature request.

### 9.2.1 The replicated state model

*Three successive drafts got this wrong, and all three errors are recorded because
each is instructive. **(a)** The first called the merge "a monotone join, i.e. a
CRDT" while also saying holders "refuse to advance past the fork" — which cannot
both hold. **(b)** The second proposed `(longest verified prefix, set of duplicity
positions)`; review supplied the counterexample — if A advances through `X@n` and B
through `Y@n`, recording the duplicity needs either branch selection (no authority)
or prefix truncation (non-monotone). **(c)** The third stored whole events in a set;
review pointed out that Ed25519 admits more than one valid signature per message, so
that is really a keyed map with an undefined value merge — and any rule for picking
a signature is a content-based tiebreak that breaks commutativity. The lesson each
time: **a verified prefix is a conclusion, and a signature is not part of identity.***

#### Digests, stated once and used everywhere

```
event_id     = H(canonical_body)          ← signatures are OUTSIDE every digest
SubjectId σ  = event_id(inception body)
prev_digest  = event_id(parent body)
```

All three must use the **body** digest. If `σ` or `prev_digest` hashed the whole
event, a hedged or retrying signer re-signing an identical body would mint a second
`SubjectId` — or a child would name a parent variant that dedup discards, wedging
the chain permanently. That is precisely the false duplicity the body rule exists to
remove, relocated one layer down.

#### Durable state — two grow-only sets, whose elements are bodies

For subject σ:

```
Bodies(σ)   = { b : ∃ s. admissible(b, s) ∧ b.subject = σ }     -- canonical bodies
Witness(σ)  = { (event_id(b), s) : admissible(b, s) }           -- signatures, never compared
```

`admissible(b, s)` is **state-independent** — decidable from the bytes alone:

1. canonical encoding parses; version and domain separator are as specified;
2. `s` verifies under `b.signer_key`, carried inline *(this proves only "some key
   signed this body" — authority is a derived question)*;
3. for an inception body, `σ == event_id(b)`; otherwise `b.subject == σ`;
4. `b.position ≤ MAX_POSITION`, an **absolute protocol constant** — not
   `local_frontier + C`, which would make admissibility depend on local state and
   break the join.

- **Partial order:** componentwise `⊆`. **Join:** componentwise `∪`.
- **Associative, commutative, idempotent:** inherited from set union. ∎
- **Receipt order cannot change durable state** — union discards order.
- **Monotone:** nothing is removed by the join.

**Why bodies and not events.** Ed25519 permits several valid signatures over one
message. With whole events as elements, `(id, s₁)` and `(id, s₂)` are two elements at
one position — manufacturing duplicity that halts the subject — and collapsing them
requires choosing a signature, which is a content tiebreak and destroys
commutativity and idempotence. With bodies as elements, re-signings collapse
automatically and `Witness` simply accumulates; nothing ever compares two signatures.

**Representation of the awkward cases falls out for free:**

| Case | Representation |
|---|---|
| **gap** | `Bodies(σ)` contains no body at that position. No sentinel |
| **duplicity** | two **distinct bodies** at one position. No flag |
| **invalid** (fails `admissible`) | never enters — and because the test is state-independent, every replica agrees |
| **re-signed identical body** | one element, several witnesses. **Not** duplicity |
| **unauthorized but well-formed** | *is* in `Bodies(σ)`, and is never selected by the derived view. **Spam cannot manufacture duplicity** |

State-independence is what makes the filter commute with union —
`filter(A) ∪ filter(B) = filter(A ∪ B)` — and it is why the bound in rule 4 must be
an absolute constant.

> **Admission is cheap, so the durable set is attacker-writable, and "retention
> affects liveness not safety" was too strong.** Anyone can mint unlimited
> admissible bodies for any public σ by signing arbitrary bytes with a fresh key.
> They can never enter `C` (they are unauthorized), so they cannot fork anything —
> but they are a storage and bandwidth attack, which forces eviction, and eviction
> is a per-replica function. **Under adversarial shaping, "convergence over the
> eventual retained set" is close to vacuous.**
>
> Two consequences, both stated rather than absorbed: the eviction rule must be
> **protocol-specified rather than implementation-chosen**, so that two honest
> replicas under the same attack retain the same set; and per-signer admission cost
> belongs in the replication layer (N3), where it may be state-dependent without
> touching the join.
>
> **Eviction must be authority-aware, or spam evicts legitimate authority events.**
> *(Raised in review round 3 and correct.)* A position bound alone does not help:
> an attacker floods `frontier + 1` with self-signed bodies that correctly name the
> verified prefix, and since `admissible` is state-independent it cannot tell them
> from the legitimate next event. A naive bounded store then discards the real one,
> and anti-entropy cannot restore it while the cap stays full — so receipt order
> would decide which replicas ever derive it.
>
> The distinguishing predicate exists, just not in `admissible`: **`authorized` in
> the derived view separates them perfectly**, because spam is signed by keys the
> log never authorized. Eviction may therefore consult derived state — it is a
> lossy local operation, not part of the join — and the rule is **retain authorized
> bodies before unauthorized ones**, which makes spam at or below the frontier
> harmless.
>
> **The residual is the gap region and it is not solved here.** Beyond the frontier,
> with intervening positions missing, `authorized` cannot yet be evaluated, so
> far-future spam is indistinguishable from legitimate future events. Retention
> should prefer proximity to the frontier and a carried chain-inclusion witness
> makes the far-future case decidable — but until that rule is specified,
> **R4.6 and permutation-invariant convergence hold only absent adversarial storage
> pressure**, and this document does not claim otherwise. Recorded as **O-N8**
> (which now covers admission *and* compaction), with **O-N5** for the topology.

#### Derived view — a pure function of `Bodies(σ)`

```
derive(Bodies(σ)):
    b₀ ← the unique b with σ = event_id(b)          # unique by construction
    if none: return Unknown
    state ← apply(b₀);  p ← 1
    loop:
        C ← { b ∈ Bodies(σ) : b.position = p
                            ∧ b.prev_digest = event_id(chosen(p−1))
                            ∧ authorized(b, state) }
        C ← supersede(C)
        match |C|:
            0 → return Live(state, frontier = p)
            1 → state ← apply(state, c);  p ← p+1
            _ → return Halted(state, disputed_at = p, candidates = C)
```

**Inception cannot fork:** two different inception bodies hash to two different `σ`,
so they are two different subjects.

**The recursion is well-founded:** `derive` advances one position at a time and only
after `|C| = 1`, so `chosen(p−1)` is defined whenever `C` at `p` is computed; if
`p−1` halts, `derive` has already returned.

**The derived view may regress; the durable state may not.** That is a conclusion
changing on new information, and it is convergent — every replica holding the same
`Bodies(σ)` computes the same verdict.

**`derive` is pure only if authority principals are keys.** Every principal named
inside an event — device, guardian, recovery authority — must be a raw `Did`,
verifiable from bytes. If a guardian were named by `SubjectId`, `authorized` would
depend on *another* subject's log, which may itself be gapped or halted, and mutual
guardianship would make the recursion non-well-founded. **Normative for N1.**

#### `supersede` — the one selection rule, and the constraints that make it safe

```
supersede(C) = if C contains any establishment event
               then { b ∈ C : b is an establishment event }
               else C
```

Establishment events (rotations, recovery) supersede non-establishment events
(authorize, revoke) at the same position — KERI's superseding recovery, and the
authenticated selector O16 needed. It selects by **authority class**, never by
content: no lowest-hash, no first-seen, no longest-chain. Where it cannot select on
authority it **halts**.

Three constraints are load-bearing. Violating any one turns the selector into an
attack:

1. **Establishment eligibility is defined by a pre-rotation pre-image reveal — never
   by a capability flag, and no delegation may mint it.** If `recover` were
   establishment because the signer holds a `Recover` capability, an attacker with a
   compromised *current* device would author an establishment event and **win
   `supersede` against the legitimate holder's rotation** — authority-based selection
   becoming attacker-based selection. F13/F13a in §19.3 is exactly such a route in
   today's code. This also explicitly bars §9.5's generic attenuating delegation from
   delegating establishment authority. Same class of error as #2590, one layer up.

2. **Rotation bodies must be deterministic.** Pre-rotation commits to
   `H(K_next_set)`, *not* to the rotation body — so the holder of `K_next` can
   author `rotate{reveal K_next, next: H(K_a)}` and `rotate{reveal K_next, next:
   H(K_b)}`, both authorized, different bodies ⇒ `|C| = 2` ⇒ **permanent halt with no
   attacker present** (a device that crashes before persisting its new commitment and
   retries with fresh randomness kills the subject). The commitment scheme must
   therefore make the whole rotation body **re-derivable** — e.g. `K_{p+1} =
   KDF(root ‖ p+1)` with every other field determined — so an honest retry collapses
   to one `event_id`. **Normative for §19.4 item 2.**

3. **Threshold recovery must produce one body, not one body per quorum.** With `M`-of-`N`
   guardians signing independently, two different quorums yield two different
   authorized establishment bodies at `p` ⇒ permanent halt. So **any `M` guardians
   hold a kill switch, not merely a takeover** — and even with no adversary, a
   phrase-holder rotating at `p` while a guardian quorum recovers at `p` halts the
   subject. Path (b) therefore requires a **single group public key producing one
   signature over one body** (FROST-class), which makes **N7 a hard prerequisite of
   guardian recovery, not an upgrade** (§12.2, §19.1).

#### What this does and does not give

| Question | Answer |
|---|---|
| Can later events become usable after a fork? | **Yes, and only** via a superseding establishment event at the disputed position |
| Does any rule silently select a branch? | **No.** `supersede` is explicit and authority-based; every other `\|C\| ≥ 2` halts |
| Does BRB prevent equivocation? | **No**, and nothing here assumes it. BRB gives non-equivocating *delivery* among receivers; a Byzantine writer can still author two conflicting signed bodies — which is why `derive` must handle `\|C\| ≥ 2` |
| Can one compromised current key halt a subject unilaterally? | **Yes.** It authors `authorize{D₁}@p` and `authorize{D₂}@p`, both authorized ⇒ `Halted(p)` with **no victim action**. Current-key compromise is an instant DoS whose only cure is a cold rotation, and if device compromise persists each revealed generation is captured in turn, burning one pre-rotation generation per round. **O16′** |
| What does superseding recovery cost? | **It orphans the suffix.** Rotating at a disputed `p` changes `chosen(p)`, so every honest later body whose `prev_digest` names the old `chosen(p)` can never enter `C` — events at `p+1 … frontier` are lost and must be re-authored. Recovery destroys legitimate later authority events, and a fork deep in the log is far more expensive than one at the frontier |
| Are class-2 settled acts convergent under a fork? | **No — and this is a settlement-safety problem, not a liveness one.** See below |

> **Class-2 settlement is not convergent under a fork, and the earlier framing
> understated it.** The regression is not only `Live → Halted`; it is
> `Live(branch A) → Live(branch B)`. Replica R1 receives only the attacker's
> `authorize@5`, derives `Live(frontier 6)`, and finalizes a class-2 ledger act
> citing position 5. R2 receives the victim's `rotate@5` first and rejects the same
> act. After merge both hold an identical set and derive an identical verdict — and
> hold **permanently different ledger state**, because R5.3 says settled effects are
> never unwound. That is the arrival-order divergence #2469 §8 withdrew, reintroduced
> through class 2.
>
> An earlier draft hid this by saying "acts at positions `< p` remain evaluable", as
> if `p` were known at admission. **It is not — `p` is discovered afterwards.**
>
> There is no fix inside this model. Reducing exposure means admitting class-2 acts
> only against positions deep enough to be beyond challenge, which is a **finality
> depth** — a concept ICN does not have, and one whose local evaluation reintroduces
> the same divergence in weaker form. Recorded as **O-N7**, and **O16′ is
> reclassified from a liveness question to a settlement-safety question**. Until it
> is answered, **this authority model is not by itself a sufficient basis for
> irreversible settlement**, and §9.3 class 2 must be read with that limit attached |

### 9.2.2 The model against adversarial delivery orders

Every case reasoned through against §9.2.1. The requirement in each: **the same
durable joined state and the same derived verdict for all honest replicas given
the same eventual event set.**

| # | Case | Durable state | Derived verdict | Converges? |
|---|---|---|---|---|
| 1 | `n` arrives before `n−1` | `{eₙ}` — union, no ordering | advance halts at the gap; `eₙ` sits in `S` unused. On `eₙ₋₁` arriving, advances through `n` | ✔ |
| 2 | `n−1` before `n` | same set as case 1 | same as case 1 | ✔ — 1 and 2 reach identical state |
| 3 | valid `X@n`, then conflicting valid `Y@n` | `{…, X, Y}` | `Halted(n, {X,Y})` | ✔ |
| 4 | `Y@n` then `X@n` | identical set to case 3 | identical — `derive` is a function of the set | ✔ |
| 5 | replicas hold `X@n` and `Y@n` separately, merge later | `S_A ∪ S_B` | both reach `Halted(n)` after merge; before merge each is legitimately *behind* | ✔ |
| 6 | identical event repeatedly | union is idempotent; keyed on `event_id = H(body)`, so re-signings collapse to one element | unchanged | ✔ *(only because of the body-digest rule above)* |
| 7 | invalid event at `n` before the valid one | fails `admissible` ⇒ never enters `S`. If merely **unauthorized**, it enters `S` but never enters `C` | the valid event is unaffected — **spam cannot block or fork** | ✔ |
| 8 | `rotate@n` without the pre-committed material | enters `S` (it is well-formed and self-signed) | fails `authorized`, so never enters `C`; ignored | ✔ |
| 9 | compromised current key forks a non-establishment event at `n` | both bodies in `Bodies(σ)` | **A single compromised key needs no victim to fork.** It authors `authorize{D₁}@n` *and* `authorize{D₂}@n` — both authorized, distinct bodies ⇒ `Halted(n)` **unilaterally**. (If it authors only one, `derive` advances on it: a stolen-key action, not a fork.) | ✔ converges — on a halted subject. Current-key compromise is an **instant DoS**; cure is a cold rotation |
| 10 | legitimate rotation recovers from case 9 | attacker's `authorize@n` and victim's `rotate@n` both present | `supersede` filters to the establishment event ⇒ `\|C\| = 1` ⇒ advance, rotating the attacker out | ✔ **defined and deterministic — but it orphans the suffix.** Changing `chosen(n)` strands every honest body at `> n` whose `prev_digest` names the old `chosen(n)`; those must be re-authored (§9.2.1) |
| 11 | fork **behind** the frontier: bodies at 0–5 and 8–9, fork at 3 | all present | `Halted(3)`. Bodies at 4, 5, 8, 9 are unreachable — `derive` never evaluates past 3. Rotating at 3 recovers the subject and **permanently orphans 4, 5, 8, 9** | ✔ converges; the cost grows with fork depth. Case 10 alone would not have caught this |

**Two operational consequences that are easy to get wrong.**

1. **Recovery must rotate *at the disputed position*, not at the next one.** If the
   victim's own legitimate event also sits at `p`, their view may have shown
   `frontier = p+1` before the fork was known, and the natural instinct is to
   publish at `p+1` — which is **unreachable**, because `derive` halts at `p`. The
   rotation must be authored at `p`, where `supersede` filters out *both*
   non-establishment events — the attacker's **and the victim's own**. The victim
   therefore loses their own event at `p` and must re-issue it at `p+1`. That is a
   real cost of recovery, not a bug.
2. **This assumes the subject notices.** A halted subject stays halted until its
   controller observes the fork and rotates. That is KERI's *"sufficiently
   responsive controller"* assumption inherited wholesale, and it means fork
   **monitoring** is a client obligation, not an optional feature. A person whose
   app never checks is a person whose subject can be frozen indefinitely.

**Does superseding recovery terminate?** Yes. The rotation at `p` replaces the key
set, so the attacker's compromised key is not authorized at `p+1` and cannot fork
again — provided the rotation actually revokes it, which is why a recovery rotation
must replace rather than extend the authorized set.

**Where it does not converge on a *good* outcome, and this is stated rather than
engineered around:** if the attacker holds the **pre-rotation material** as well,
they can author an authorized establishment event at `n`. If the victim also does,
`\|C\| = 2` after `supersede` and the subject **halts permanently** — all replicas
agree, so it is convergent, but the subject is dead rather than recovered. If only
the attacker does, they take over. Both are the backup-compromise case §12.2
records as unrecoverable, and the residual policy is **O16′**.

### 9.3 Revocation, stated per act class — the replacement for O9

Because §6.1 forbids a single global answer, the semantics is stated per class.
**R5.2 requires this table to be explicit; silence would be the defect.**

| Class | Examples | Admission | Effect | Revocation reaches back? |
|---|---|---|---|---|
| **1 — deferred-decision** *(currently degraded — see the O-N6 note below)* | votes, nominations, anything tallied at a decision point | monotone: well-formed **and** authored by a key authorized at the referenced log position. Never depends on revocation state. | computed at the decision point against the log prefix **the decision pins**. Deterministic function of converged state. | **yes, within the open window** — and the cost is small **under this model**, because the vote key would be keyed by the *subject*, so re-casting from a new device overwrites and the person loses nothing. **This is not true today** — see the note below |
| **2 — immediately settled** | ledger entries, receipts, anything with irreversible external effect | authority checked once, at admission, and **finalized** | fixed at admission | **no — prospective only.** Bounded by *scope*, not by retroaction (R5.3). Honest analogue: a stolen card is stopped going forward, not unwound |
| **3 — authority events** | the log's own events | ordered by the log itself | — | n/a — self-ordering |

> **The vote key is keyed by a `Did`, not by a subject — verified.**
> `vote_key(proposal_id, voter: &Did)` (`apps/governance/src/state_store.rs:222-224`)
> takes a **key**, which is concept 7 of §4's conflation table. `save_vote`
> (`:379-384`) is an unconditional `put`, `list_votes` (`:386-`) scans the prefix,
> and `AlreadyVoted` (`icn-governance/src/error.rs:33`) is **declared and never
> constructed**. So today: re-casting under the *same* DID overwrites (#2469 fact
> 15), but a person acting under a *second* DID — which is what a second device
> means today (F19) — writes a **second tallied row**.
>
> That is the sybil surface subject/device separation exists to close, and it is a
> reason to prefer this model rather than evidence for it. *(An earlier draft
> asserted the present-tense claim "votes are keyed by subject"; corrected in
> adversarial review.)*

**Bounding the window without a clock (R5.1).** A device authorization carries
validity expressed in **log positions**, not wall-clock: valid for acts
referencing positions in `[N, N+k]`. The subject periodically advances the log, so
stale authorizations age out **deterministically** — every holder of the log
computes the same answer, with no clock anywhere (R4.1, and it avoids F14's
fail-open entirely). This is Let's Encrypt's short-lived-credential strategy
translated into a counter.

*Cost, stated:* a subject who advances too aggressively can strand an offline
device. **Choosing `k` and the re-authorization cadence is OPEN (§17, O-N1).**

> **This does not bound the case it looks like it bounds, and R5.1 is not fully
> met.** *(Corrected in adversarial review.)* The window closes only because *the
> subject advances the log*. In the scenario the mechanism exists for — the
> attacker holds the person's device — the subject may have **no surviving
> authorized key with which to advance**, so the window does not close at all until
> recovery completes.
>
> The Let's Encrypt analogy is therefore **false in the compromise case** and is
> withdrawn: a certificate lifetime expires autonomously with time, whereas a log
> position advances only if the victim can act.
>
> Honest statement: **R5.1 [HARD] is met for a subject retaining at least one
> authorized key, and is not met for sole-device compromise.** In that case the
> bound is not staleness but *recovery latency* (§12), and sole-device compromise
> should be treated as a recovery problem, not a cadence-tuning problem.

> **Class 1 has a dependency this document does not discharge.** "Evaluated at the
> decision point against the log prefix the decision pins" is deterministic *only
> if the decision point itself is deterministic* — i.e. if closing a proposal is a
> replicated act that fixes the pinned position, rather than a node-local handler
> action. ICN does not have that today: #2469 §7.2 keeps `ProposalClosed`
> **contained** precisely because "membership is not the right authority for
> choosing an outcome" and `outcome`/`tally` are attacker-supplied fields, and the
> correct authority "entangles with the `GovernanceProofV2` signer-authority
> model" that #2469 lists as a non-goal.
>
> So class 1 is **convergent conditional on deterministic proposal closure**, which
> is a governance problem with a known shape and a named owner — not an identity
> problem, and not one this document solves. Recorded as **O-N6** (§17).
>
> **The fallback is not safe, and an earlier draft wrongly said it was.**
> *(Corrected in adversarial review.)* Without a pinned position there are exactly
> two options, and both break a [HARD] requirement:
>
> | Fallback | Breaks |
> |---|---|
> | evaluate revocation against the receiver's local prefix **at arrival** | **R4.2/R4.3** — a node learning vote-then-revocation tallies differently from one learning revocation-then-vote. This is the arrival-order divergence #2469 §8 withdrew |
> | ignore revocation for class 1 entirely (consistent with "never depends on revocation state") | **R5.1** — a stolen device's votes count indefinitely |
>
> **This document picks the second**, because divergence is a correctness failure
> while indefinite exposure is a bounded, *stateable* security cost — and because
> it degrades to exactly today's behaviour rather than introducing a new failure
> mode. It is a **known R5.1 violation held open until O-N6 is answered**, not a
> safe default, and it must be described that way wherever class 1 is claimed.

### 9.4 Governance authorship

```
Person holds continuity root (offline / backup)
   │
   ├── subject S_A in Coop A, with its own authority log
   │        │
   │        └── log authorizes device D (scopes, positions [N, N+k])
   │
   ▼
D signs the governance act, referencing log position N
   │
   ▼
gateway / node RELAYS — holds no key of S_A, asserts no authorship
   │
   ▼
receiver verifies, from bytes in hand + its own authenticated local state:
   1. act signature under D's key                       [bytes]
   2. D authorized at position N, scope admits the act  [local log prefix]
   3. S_A ∈ this domain's members                       [local domain state]
   4. position N within validity                        [local log prefix]
   ── if the local prefix is shorter than N: QUARANTINE, never reject
```

This satisfies #2469's I9 (no circular bootstrap — the log arrives on its own
topic, not on the channel carrying the act) and R4.5 (local state that is
authenticated and converges). It is the honest resolution of PRINCIPAL_MODEL
§7.0.2's custody boundary: **the gateway relays an act it cannot author**, which
is exactly what #2469 says is impossible today.

> **Quarantine must be bounded, and correctness rests on re-delivery — not on
> retention.** "Never reject" invites an obvious attack: reference a log position
> that will never exist and the entry sits in quarantine forever. Rejecting
> unknown signing keys is *not* the fix — a device legitimately authorized at
> position `N+1` will act before slow receivers hold `N+1`, and rejecting it
> would permanently discard a legitimate act, violating R4.6.
>
> The correct shape has **two** bounds, not one:
>
> 1. **Bound the position.** Accept an act into quarantine only for
>    `N ≤ (highest verified position) + C` for a protocol constant `C`. Without
>    this, an attacker cites position 2⁶³ and the receiver can never resolve it —
>    it cannot distinguish "not yet replicated" from "will never exist", and §9.4
>    forbids fetching the log over the act's channel (I9). Alternatively, require
>    the act to carry a hash-chain inclusion witness for `N`; **carrying is not
>    fetching**, so R4.4/I9 permits it.
> 2. **Bound the store.** #2469 slice 4's bounded quarantine with eviction and a
>    steward release valve, where an evicted entry is recovered by **anti-entropy
>    re-delivery** once the prefix catches up.
>
> Convergence therefore depends on re-delivery being reliable over time — a
> property of the replication layer, not of this design. Stating "never reject"
> without both bounds would be an unbounded-memory claim, and the position bound in
> particular was **missing from an earlier draft** (found in adversarial review).
> Folded into **O-N5**.

Note what is *not* claimed: this does not make the production gateway composition
converge on its own. #2469 §7.0.1's second barrier — a receiver cannot
deterministically evaluate standing/suspension because that state is
unauthenticated (F17) — is untouched by anything here, and remains #2441's.

### 9.5 Where the Meaning Firewall falls

PRINCIPAL_MODEL §1.1 forbids `Person` in the kernel and §4 then proposes it as a
principal class. This model has no such tension:

| Layer | Holds | Never holds |
|---|---|---|
| **kernel** (`icn-kernel-api`, `icn-identity` core) | `Did` (a key), signature verification, canonical encodings, a generic **attenuating delegation** between principals | any notion of person, member, device-class or coop |
| **identity** (`icn-identity`) | `SubjectId`, the authority log, pre-rotation | why a subject exists, or what it is a subject *of* |
| **app** (`apps/*`, governance, membership) | `Person`, `member`, `role`, standing, recovery policy, act-class rules | key custody |

A **subject** is meaning-free: it is any entity with an evolving authorized key
set. A person, a treasury and a node are all subjects. That is why this model
satisfies R13.1 where a `Person` principal class cannot.

> **One leak to watch.** §9.1 says the log is "replicated to that subject's
> relying parties", and knowing *which* parties those are is context knowledge
> sitting inside the identity layer. Keep the identity layer's interface to an
> opaque destination set supplied from above, or R13.1 erodes here first. Flagged
> in adversarial review; folded into **O-N5**.

### 9.6 What this does not solve — read this before building on it

1. **The per-subject authority log does not exist.** §3.7. `RotationEvent` has the
   right shape (signed, `+1` monotone) and is unwired, unverified on apply, and
   signed with a different preimage than it verifies (F9, F10). This is real work.
2. **Pre-rotation does not exist** anywhere in ICN. It is the load-bearing new
   cryptographic mechanism, and it is the reason scenarios 1 and 2 are contained.
3. **Selective disclosure does not exist and cannot be wired today.** F16: blind
   signatures are not blind, the accumulator uses a 32-bit test modulus, the
   trusted-issuer check passes on an empty list, and the default build can neither
   prove nor verify. R6.2 and any nullifier-based sybil story are **new
   cryptographic work**, not integration.
4. **#2469 §7.0.1 still contains the production gateway composition.** Nothing
   here lifts it.
5. **Guardian collusion and relay withholding remain unmitigated** (scenarios 5, 11).
6. **Cross-context divergence under concurrent recovery is a real cost** (scenario 3).
7. **The continuity root concentrates what Model A distributes** — its compromise is
   takeover of every context *plus* full deanonymization, and under path (a) it is
   **unrecoverable** (§12.2).
8. **R9.1 [HARD] is not fully met.** Path (a) fails if the backup is lost; path (b)
   needs threshold **signing** ICN does not have (F7) and which appears nowhere in
   §19.1's slice graph.
9. **Per-subject log availability is a real dependency** (§7, Model C costs).
10. **Byzantine reliable broadcast is a prerequisite and does not exist** — gossip
    entries carry no signature at all (§3.7, §6.1).
11. **Class 1 currently degrades to a known R5.1 violation** until O-N6 (§9.3).
12. **Class 2 is not convergent under a fork** — two replicas can hold permanently
    different settled state (**O-N7**). This model is **not** by itself a sufficient
    basis for irreversible settlement.
13. **A single compromised current key halts a subject unilaterally**, and recovery
    **orphans every later event** (§9.2.1). Fork depth is a cost multiplier.
14. **Guardian recovery needs threshold *signing* (N7) to avoid being a kill
    switch** — independent M-of-N signatures make any `M` guardians able to halt the
    subject permanently (§12.2).
15. **The durable set is attacker-writable**; eviction must be protocol-specified and
    authority-aware, and **beyond the frontier the gap region is unsolved** — so
    R4.6 and permutation-invariant convergence hold only absent adversarial storage
    pressure (**O-N8**).
16. **Existing key-derived DIDs need a new `SubjectId` and a documented membership
    bridge** — the DID cannot be the subject identifier (§15.1).

---

## 10. What `Did` should mean

**RECOMMENDED: option B of the mission's list — `Did` names a cryptographic
principal, and nothing else.**

That is what the type already is (§3.1); the change is to stop projecting other
concepts into it, and to make the type enforce what it claims.

| Change | Rationale | Classification |
|---|---|---|
| `Did` = an Ed25519 (or successor) public key. No other construction. | §3.1; F1 and F2 are the current alternative failing in production | RECOMMENDED |
| **Remove `Did::from_anchor_id` and `new_unchecked`** | Their only effect today is to mint values that ~50% of the time cannot be read back (F1) and that a ZK prover silently replaces with an all-zero key (F2) | RECOMMENDED |
| **Canonicalize the encoding** — pin base58btc at parse, or compare by decoded key bytes | `Did` equality is string equality (§3.1), so two encodings of one key are two unequal DIDs. Folds in PRINCIPAL_MODEL O10 | RECOMMENDED |
| Treasury identifiers become `EntityId`-shaped, not `Did`-shaped | A treasury holds no key; `derive_treasury_did` mints a string that would fail `Did::from_str` and a *second, different* one via `from_anchor_id` (`icn-coop/src/actor.rs:493`) — already flagged in `docs/status.toml:77` | RECOMMENDED |
| A durable subject is a `SubjectId`, never a `Did` | §4's proof | ESTABLISHED (the proof), RECOMMENDED (the type) |

**Should a global Person ID exist? No.** Not because privacy outranks
verifiability, but because nothing needs it: authorship is provable against
context-local authenticated state (§9.4), and sybil resistance needs *uniqueness*,
not *identification* (R6.6). Every mechanism whose sole purpose was to make a
global identifier work — global current-root discovery, global fork selection,
global genesis bootstrap — is cost with no corresponding requirement.

---

## 11. Privacy and correlation

### 11.1 What each observer learns

| Observer | Learns | Does not learn |
|---|---|---|
| unrelated network peer | that some key signed something on a topic | which subject, which person, which contexts |
| node operator / gateway | the subjects and device keys it relays for — **including, in the common hosted-only case, several of one person's contexts and therefore their linkage** | the continuity root |
| a cooperative | its own `S_A`, its authorized device keys, its acts | `S_B`; that `S_A` and `S_B` are one human |
| a federation | the federation-scoped subject of a delegate | the delegate's coop-scoped subject, unless the coop discloses it |
| recovery guardian | that they are a guardian for one subject | the person's other contexts (if guardians are per-context) |

**Device keys must be per-context too.** If one device key appears in two
contexts, the identifier separation is defeated. Per-context device keys are a
deterministic derivation from the device's own secret.

> **That derivation is itself a correlation oracle, and an earlier draft called it
> "bookkeeping, not custody burden".** *(Corrected in adversarial review.)* Anyone
> holding the device secret can *recompute* the per-context key for any candidate
> `SubjectId` and so **confirm or deny** membership in any context they can guess.
> Device-secret compromise therefore deanonymizes **every** context, not just the
> one the device was stolen for. A non-derived (independently random, stored) key
> per context removes the oracle at the cost of backup complexity. **OPEN — folded
> into O-N3.**

**Residual correlation, stated honestly.** Four channels, none of them closed:

1. **Within a context** — everyone in Coop A correlates `S_A`'s activity there.
   Unavoidable and correct.
2. **Shared infrastructure** — a gateway relaying two of a person's contexts sees
   both. This is the *default* hosted deployment (R11.1), not an edge case.
3. **Recovery timing** — recovery is a logged event visible to every relying party
   (R9.2). A person recovering after device loss rotates in every context within
   one window, and an observer of two coops' logs correlates on timing. **R6.4 is
   weakened by the very auditability R9.2 requires** — a genuine tension between
   two requirements, not an oversight.
4. **Social knowledge, timing and network metadata** generally.

This is **protocol-level unlinkability against a passive observer of distinct
contexts, not anonymity**, and nothing here should be described as the latter.

### 11.2 Sybil resistance without identification

SDIS's genuine contribution is `VUI` — *"proves uniqueness without revealing the
underlying identity data"*. The correct shape is a **per-context nullifier**
derived from a uniqueness credential: an institution learns "this is one distinct
verified human, and not one it has already admitted" without learning which.

**This cannot be built today (F16),** and saying otherwise would be inventing a
protocol to close a hole. Interim honest posture: uniqueness is a *steward
attestation* against a registry the steward holds — which is what SDIS actually
does — and that registry is a correlation surface. Recorded as **OPEN (O-N2)**.

### 11.3 Proving two personas are one human

The person signs a linkage statement with the current authorized keys of both
subjects, naming both `SubjectId`s and a purpose. Cheap, consent-gated,
verifiable, and revealing nothing until the person chooses. This is the property
Model A gets backwards: there, linkage is *always on* and cannot be withheld.

---

## 12. Recovery

### 12.1 Pre-rotation is the recovery mechanism

The inception event commits to a digest of the next key set. If that next key set
is derived from the **continuity root**, held in backup and not on any device,
then total device loss is recovered by restoring the root, deriving the
pre-committed keys, and rotating.

**This dissolves O14's mechanism — and only its mechanism.** O14 asks how a
total-loss recovery can *advance the chain* when `RotationEvent::verify` requires
an existing non-revoked authorized key (`multi_device.rs:541-552`) and total loss
has none. Pre-rotation answers exactly that: the committed-but-never-used next key
*is* an authorized key, so no separate "threshold-recovery transition" needs to be
bolted beside the chain rule — it **is** the chain rule.

> **What it does not solve, stated plainly.** Pre-rotation presupposes the
> pre-rotation material survived. It converts "I lost my devices" into a solvable
> problem; it does **not** solve "I lost my devices *and* my backup." That case has
> only one answer — guardians (path (b) below) — and a person who chose path (a)
> and lost both has lost the subject. Describing O14 as fully closed would be
> exactly the invented-protocol failure the review discipline forbids.

### 12.2 Two paths; the person chooses

| | (a) Self-recovery | (b) Guardian-gated |
|---|---|---|
| Pre-rotation commits to | keys derived from the continuity root (recovery phrase) | a **threshold** key set held by M-of-N guardians |
| Total-loss recovery | restore phrase → rotate | M distinct guardians co-sign → rotate |
| Root/backup compromise | **takeover, and *unrecoverable*** — see below | insufficient alone |
| Any M guardians can... | — | **halt the subject permanently**, unless (b) uses a single group key — see below |
| Produced offline? | **yes** | **no** — needs an online M-of-N quorum |
| UX cost | one phrase at onboarding | guardian setup + coordination |

> **Path (b) is a kill switch unless it uses a single group key.** With `M`-of-`N`
> guardians signing **independently**, two different quorums produce two different
> authorized establishment bodies at the same position, so `supersede` cannot
> choose and the subject **halts permanently** (§9.2.1, constraint 3). That means
> any `M` colluding guardians can destroy the subject rather than merely capture it
> — a strictly worse outcome than takeover — and it can happen with **no adversary
> at all**, if a phrase-holder rotates at `p` while a guardian quorum recovers at
> `p`. Path (b) therefore requires a **single group public key emitting one
> signature over one body** (FROST-class), which makes **N7 a hard prerequisite of
> guardian recovery rather than an optional upgrade**. An earlier draft's "insufficient
> alone" understated this badly.

> **Path (a) root compromise is not a race the victim can win.** An attacker
> holding the continuity root derives the pre-committed keys, rotates first, **and
> commits a fresh `next` digest the victim does not hold**. From that point the
> victim cannot rotate even though they still know the root — pre-rotation has
> handed the attacker the same one-way advantage it was meant to give the
> legitimate holder. Worse, the attacker can do this silently from a backup copy,
> at leisure, with no need to touch a device.
>
> So pre-rotation makes root compromise **more** decisive, not less. It is an
> excellent defence against *device* compromise and no defence at all against
> *backup* compromise. That asymmetry is the entire argument for path (b), and an
> earlier draft's matrix cell claiming "root compromise survivable ✔ pre-rotation"
> was simply wrong.

**Default is (a), upgradeable to (b).** That answers PRINCIPAL_MODEL's **O1**
("may the root live on the first phone?"): the *operational* keys live on the
phone; the *pre-rotation* material is backed up off-device at onboarding. R10.2 is
met — phone-only onboarding is secure and recoverable by default rather than
degraded — at the cost of exactly one irreversible user decision (R10.3).

### 12.3 Constraints any recovery must satisfy

- **R9.4 — distinct authenticated participants.** Today both threshold paths fail:
  `sync.rs:263-269` neither verifies signatures nor deduplicates; SDIS's
  `approve_by_steward()` is a bare `+= 1`. Fixing this is #2591 plus its SDIS
  sibling, and it is a precondition for (b), not a follow-up.
- **R9.2 — recovery is a logged event.** It is an ordinary event in the subject's
  authority log, visible to every relying party. Never a silent substitution.
- **R9.6 — contestable, and this is a named exception to R4.1/R4.3.** A relying
  party may apply a notice-and-veto window before honouring a recovery rotation.
  It **cannot** be position-denominated: in a takeover the attacker controls log
  advancement and would simply advance past any positional window. So it is a
  **wall-clock** window, and deciding whether to honour a rotation *is* a validity
  decision.
  > **Consequence, stated rather than finessed** *(found in adversarial review)*:
  > two nodes **of the same institution** with identical durable state but skewed
  > clocks can reach different verdicts during the window. That violates R4.1 and
  > R4.3, both [HARD]. It is accepted here as a **bounded, deliberate exception**
  > confined to recovery-rotation acceptance — never to ordinary act admission —
  > because the alternative (no contest window) makes R9.6 unimplementable and
  > hands a stolen backup an uncontested takeover. A deployment that cannot
  > tolerate the exception must set the window to zero and accept that instead.
  > This is the only place in the design where a receiver's clock decides anything.
- **R9.5 and R2.4 — path (a) does not meet them, and that is a deliberate trade.**
  The continuity root in path (a) *is* a permanent secret, and an attacker holding
  it can rotate the subject away. Recovery from that is a race the legitimate
  holder may lose, mitigated only by a relying party's notice-and-veto policy.
  **Path (a) therefore trades R2.4/R9.5 [STRONG] against R10.2 [HARD]** — phone-only
  onboarding that is recoverable by default. Path (b) removes the single-secret
  failure at the cost of guardian UX. The trade is the person's to make, and a
  deployment that silently pins everyone to (a) has made it for them.
- **The recovery↔DID contradiction must be adjudicated** (§3.6). Under this model
  **the subject identifier is unchanged by recovery** — it commits to the
  inception event, which recovery does not rewrite. `social-recovery-design.md`'s
  `did_mapping:<old_did>` indirection becomes unnecessary and should be retired.

---

## 13. Offline and partition semantics

| Operation | Offline? | Why it converges |
|---|---|---|
| Genesis (inception) | **fully** | self-addressing; nothing to contact (R7.1) |
| Signing an act | **fully** | references a log position the device already holds (R7.2) |
| Authorizing a device | **fully** | an event in the subject's own log |
| Recovery rotation, path (a) | **fully** to produce | log event |
| Recovery rotation, path (b) | **no** — needs an online M-of-N guardian quorum, and the R9.6 veto needs a notification channel bound to the subject, which total loss removes and takeover captures | §12.2 |
| Verifying an act | needs the log prefix | shorter prefix ⇒ **quarantine**, never reject (R4.6) |
| Two devices acting concurrently while partitioned | **yes** | acts are unordered; only the *authority log* is single-writer (R7.4) |
| Two devices *authorizing* concurrently | **no — forbidden** | k > 1 ⇒ consensus (§6.1). A correctness boundary |

**No wall clock in *act admission*.** Positions replace timestamps (§9.3), which
also removes F14's fail-open failure mode. **One validity decision does use a
clock** — recovery-rotation acceptance (§12.3) — and it is declared an exception
rather than hidden.

> **Where clocks legitimately remain — and why they are not R4.1 violations.**
> R4.1 forbids a *receiver's* clock from deciding *validity*. Three clock-shaped
> things survive, and none is that:
>
> | Thing | Whose clock | What it decides |
> |---|---|---|
> | "the subject **periodically** advances the log" (§9.3) | the **subject's**, on their own device | when *they* re-authorize. A liveness schedule, not a verdict. Every receiver still decides positionally |
> | a relying party's **notice-and-veto window** before honouring a recovery (§12.3) | the **acceptor's** | **whether to honour a recovery rotation — a genuine validity decision, and a named R4.1/R4.3 exception**, not an exempt case. See §12.3 |
> | quarantine **eviction** (§9.4) | the receiver's | when to *forget* and rely on re-delivery — never whether an act is valid |
>
> The test to apply to any future addition: *if two honest receivers with identical
> durable state could reach different verdicts because their clocks differ, it is an
> R4.1 violation.* The first and third above cannot. **The second can, and is
> therefore declared an exception in §12.3 rather than defended as compliant.**

---

## 14. Institutions and nodes

### 14.1 The pattern generalizes — and that is a result, not an aesthetic

> durable subject ≠ current authority

holds for humans (subject ≠ device keys), for institutions (charter ≠ current
stewards) and for nodes (instance ≠ operator). All three are **subjects** in the
§9.1 sense, which is why the identity layer can stay meaning-free.

### 14.2 Institutions: PRINCIPAL_MODEL §4.1 is upheld, with a caveat

Its conclusion — an institution holds no signing key, because `Did` models custody
and institutions are constituted by governance — **stands, and this model
strengthens the reasoning**: `Did` is *only* a key here, so it is even more clearly
the wrong type for a governed entity.

But the caveat matters. The argument's premise was "an institution's identity
would have to be a key." Under §9.1 that premise is false: an institution could be
a **subject** with an authority log whose authorized keys are its current
stewards, and no single person would "be" the institution. **O12 therefore
reopens on better terms** — the question is no longer "key or mandate chain" but
"does an institution need a self-authenticating statement, given that a subject
log now makes one possible without single-custodian capture?" Recorded in §17.

`FounderSignature` (F18) is the existing primitive to build on: an institution's
inception event, signed by its founding persons, is a natural `EntityId` binding
and resolves PRINCIPAL_MODEL's O11 by construction (the id commits to the event).

### 14.3 Nodes

Node identity is the strongest binding ICN has — the Hello three-fact cert check
(`icn-net/src/handlers/hello.rs:62-97`) is genuinely sound, and is **per
connection, not per peer** (`handlers/mod.rs:76-78`). Nothing here changes it.

Two facts worth carrying forward: `operator_did` is populated as `did.clone(),
did.clone()` (`supervisor/lifecycle.rs:371-375`), so node and operator are the
same principal today; and node key rotation produces a new DID and **is never
persisted** (`keystore.rs:1224-1257` does not call `save_v4`). Under §9.1 a node
becomes a subject whose authority log survives key rotation — which is the
principled fix for both, and which also gives **O2** (does a restored node keep
its DID?) a cleaner answer: the *subject* persists, the *keys* rotate, and
restore-twice is detectable as duplicity rather than indistinguishable from a clone.

---

## 15. Migration

Classification per mechanism: **KEEP · MIGRATE · DEPRECATE · REMOVE ·
COMPATIBILITY-ONLY**.

### 15.1 Key-derived Person DIDs — KEEP the key, allocate a new subject

This is the largest deployed class, and the **key** does not change. An existing
`did:icn:<key>` is already exactly what §10 says a `Did` should be: a
cryptographic principal. Two consequences:

- Nobody "becomes a new person" (scenario 25). No cutover, no re-enrollment, and
  **no key ceremony**: the person keeps signing with the key they already hold.

> **But the existing DID cannot itself serve as the `SubjectId`, and an earlier
> draft said it could.** *(Corrected in review round 3.)* `SubjectId = event_id(inception body)`
> and inception admission checks `σ == event_id(b)`, whereas `did:icn:<key>` encodes
> an Ed25519 public key. Barring an accidental preimage the two cannot coincide, so
> such an inception is **inadmissible** and the derived view stays `Unknown`.
>
> The honest migration is therefore: **allocate a new `SubjectId`, whose inception
> event names the person's existing `Did` as its initial authorized key.** What is
> preserved is what actually matters — the key, custody, and the ability to sign —
> and what changes is the *context identifier*.
>
> That is a smaller cost than a re-enrollment and a larger one than "no event". In
> particular, **membership rows keyed by the member's `Did` need a documented bridge
> to the new `SubjectId`**, which is the same shape as the bridge `icn-commons`
> already maintains for enrollment-DID → anchor (§15.2). Specifying that bridge is a
> migration prerequisite, not a detail.
>
> Rejected alternative: a "legacy inception" rule admitting `σ == did_of(initial_key)`.
> It preserves the identifier but destroys inception uniqueness — two different
> inception bodies could then claim one `σ`, producing duplicity at position 0, which
> §9.2.1 relies on being impossible.

### 15.2 SDIS anchor-derived DIDs — DEPRECATE, at near-zero cost

PRINCIPAL_MODEL rates O18 **critical** on the assumption that anchor-derived
Persons are a live identity class with deployed authority. Anchors *are* created
in production (F4) — but two verified facts show they carry no authority:

- **F5** — in the enrollment path the acting principal is already the **device
  key**: `simple_enrollment.rs:667` (*"The DID is the ephemeral_did — in SDIS, keys
  are created on the device"*), and the token is minted for `ephemeral_did` (`:782`).
  The anchor is returned alongside as `anchor_id`.
- **F20** — the anchor's mutating routes are disabled, returning `Forbidden`.

And **F4** shows the anchors that do exist carry none of the properties the design
claims for them: the VUI is `SHA256("gateway-enrollment-vui:" ‖ did)`, so it is a
pure function of the public DID.

**No deployed anchor-derived DID carries authority, and no deployed anchor carries
a uniqueness or unlinkability property.** Migration is therefore:

> **That trace is now complete — and it comes out clean, for an uncomfortable
> reason.** *(Left open in the first pass; resolved in review round 2.)*
>
> | Step | Finding |
> |---|---|
> | `PersonhoodAnchorStore::put` indexes the **anchor-derived** DID | `personhood_store.rs:202-205` — `let did = anchor.to_did(); … put(&did_index_key(&did), anchor_id)` |
> | `link_did`, which would add the member's real DID to that index | **zero production callers.** The one hit, `icn-net/src/rate_limit.rs:1781`, sits after the `#[cfg(test)]` boundary at `:1406` |
> | `PersonhoodAnchorStore::get_by_did` | reached only by the trait forwarder `get_anchor_by_did` at `personhood_store.rs:599`, and **no production code constructs this store** (`supervisor/lifecycle.rs:1407` passes `None`). *(An earlier draft said "zero production callers", which was imprecise.)* |
| the **live** anchor-by-DID lookup is a **different store** | `icn-commons/src/store.rs:529`, reached from `api/commons/mod.rs:131,469` — and it maintains an explicit bridge, `put_anchor_did_index`, doc-commented *"used when the enrollment DID differs from the anchor's internal DID"* (`store.rs:539-545`), written during enrollment at `icn-commons/src/inner.rs:173` |
> | standing / holder lookups | go through `CommonsStore::get_holder_by_did(holder_did)`, where `holder_did` is the DID passed to `create_holder_from_anchor` — the **member's `ephemeral_did`** in the enrollment path (`simple_enrollment.rs:703`) |
>
> **So there is no live cross-keying mismatch — and for a better reason than the
> first draft gave.** The live path (`icn-commons`) maintains an *explicit bridge
> index* from the enrollment DID to the anchor, precisely because the two differ;
> the path that indexes only the anchor DID (`icn-identity`'s
> `PersonhoodAnchorStore`) is never constructed in production. Standing and
> membership consistently key on the member/device DID.
>
> This does not make the migration risk-free, and the finding should not be read as
> a clean bill of health. It means the hazard is **latent rather than active**: the
> first caller to use `PersonhoodAnchorStore::get_by_did` with a *member* DID gets
> `None`, because only the anchor DID was ever indexed and `link_did` is never
> called. That fails **closed**, which is the safe direction, but it is surprising
> and would read as "this person has no anchor". Removing `to_did()` should
> therefore also remove or repurpose the DID index rather than leave a half-wired
> lookup behind.

| Mechanism | Disposition |
|---|---|
| `Anchor` as a record | **KEEP** — it is a durable subject label, which is a real need |
| `Anchor::to_did` / `Did::from_anchor_id` | **REMOVE** — the source of F1 and F2 |
| anchor id in API responses | **COMPATIBILITY-ONLY**, then removed — it is currently returned twice, as `anchor_id` **and** as `did` (`api/commons/anchor.rs:67`) |
| `is_anchor_did` | **REMOVE** — vacuous (F11) |
| `VUI` / steward network | **KEEP the concept, MIGRATE the wiring** — uniqueness is a real requirement (R6.6); `compute_temporary_vui = SHA256(did)` (F6) must go, and `combine_prf_partials` is not a threshold scheme (F7) |

### 15.3 Everything else

| Mechanism | Disposition | Note |
|---|---|---|
| `DidDocument` (`multi_device.rs`) | **MIGRATE** → authority log | Cheaper than it looks: the gateway path never creates one (F8), so only the `icnctl`/keystore path has state |
| `DidDocument` (`icn-kernel-api`) | **REMOVE** | zero implementors; a parallel unimplemented sketch over `pub type Did = String` |
| `RotationEvent` | **MIGRATE** | right shape; needs a domain-separated canonical preimage (F12) and a signer/verifier that agree (F10) |
| device rosters | **MIGRATE** | fix #2588 (binding) and #2590 (attenuation) **as part of** the migration, never after |
| memberships | **MIGRATE** (was "KEEP, extended") | the bidirectional index (`sled_registry.rs:9-12`) is the right shape, but rows keyed by the member's `Did` need a documented bridge to the new `SubjectId` (§15.1). Signing and replication remain #2441's (F17) |
| governance state | **KEEP** | class 1 becomes cheap **once votes key on the subject**; today `vote_key(proposal_id, voter: &Did)` (`apps/governance/src/state_store.rs:222`) keys on a **`Did`**, so re-casting from a new device writes a second tallied row (§9.3) |
| historical signatures / receipts | **KEEP — permanently verifiable** | "key K was authorized at position N" is an append-only fact (R2.3) |
| gateway challenge auth | **MIGRATE** | add a domain separator and bind `coop_id`/`scopes` into the signed payload (§3.4) |
| invite redemption | **REMOVE the unproven-subject mint** | #2589 |
| React Native wallet | **MIGRATE** | today one key per install *is* the person (F19); needs subject/device separation and off-device pre-rotation backup |
| personal nodes | **KEEP** | §14.3 |
| `GOV_OP_V1` | **DEPRECATE per subject** | see O13 in §18 |

---

## 16. Rejected alternatives

| Rejected | Reason |
|---|---|
| **Key-derived durable Person identifier** (Model A; PRINCIPAL_MODEL §4 Person row) | Entails a permanent, unretireable master key (§5); violates R2.4/R9.5; generates O8/O13/O16/O17/O18 |
| **Globally resolvable non-key anchor** (Model B) | Genesis has no authority root; resolution needs a global directory with an unsolved split-view problem; correlation worse than A |
| **Credential-only, no subject identifier** (Model D) | Collapses into C plus a credential layer; revocation reintroduces an online correlator; no continuity mechanism |
| **Hybrid five-concept graph** (Model E) | Complexity is the cost; §3 documents what ICN's existing parallel identity models already produce |
| **O9 as posed** — a convergent global device-revocation model | Unachievable **within premises ICN has chosen** (§6.1) — a bounded argument, not a theorem. Replaced by §9.3 |
| **Wall-clock validity windows** (`not_after` as an ingress gate) | Non-convergent (#2469); and F14 shows ICN's clock helper fails *open* |
| **`standing_hash` in the envelope** | #2469 §14.1 — a deterministic-looking check over unauthenticated state |
| **Carried-proof-only verification** ("carried, not resolved" as an absolute) | Over-generalizes #2469, which resolves against local durable state by design. Corrected in R4.5 |
| **Institution DID held by threshold signing (A′)** | ICN has a threshold **PRF**, not threshold **signing** (F7); requires FROST-class new work. Not rejected forever — see O12 in §17 |
| **ZK selective disclosure as a near-term mechanism** | F16 — the stack cannot prove or verify in its default build |

---

## 17. Open decisions

Carried forward from PRINCIPAL_MODEL where still live, plus new ones from this
pass. **A correct OPEN is better than an invented answer.**

| # | Question | Why open |
|---|---|---|
| **O-N1** | What are the device-authorization validity span `k` and the re-authorization cadence (§9.3)? | Too small strands offline devices; too large widens the compromise window. Needs empirical UX input, not a derivation. **Does not bound sole-device compromise at all** (§9.3) |
| **O-N2** | How is per-context sybil resistance built, given F16? | A nullifier needs working ZK. Interim is steward attestation against a registry that is itself a correlation surface (§11.2) |
| **O-N3** | Are recovery guardians per-context or shared? And are per-context device keys *derived* or *independently random*? | Shared guardians are better UX and worse privacy (the guardian learns the linkage); per-context is the reverse (R6.4). Derived device keys are a **confirm-or-deny oracle** on context membership for anyone holding the device secret (§11.1); independent keys remove the oracle and complicate backup |
| **O-N4** | Which act classes exist beyond the three in §9.3, and who assigns a new act to a class? | §9.3 is exhaustive today but the taxonomy must be governed, or class assignment becomes an unreviewed security decision |
| **O-N5** | How is the authority log replicated and bounded? | It is per-subject and monotone, so gossip suffices in principle — but topic naming, retention, and a bound on log growth are unspecified, and ICN's topics are global-by-domain today (§3.7). **Topic naming is also a correlation surface** (§11.1) |
| **O-N6** | What makes a governance decision point deterministic, so §9.3 class 1 can pin a log position? | #2469 §7.2 contains `ProposalClosed` because `outcome`/`tally` are attacker-supplied and the real authority entangles with the `GovernanceProofV2` signer model, an explicit #2469 non-goal. **A governance problem, not an identity one** — but class 1 degrades to class 2 until it is answered |
| **O16′** | On detected duplicity, what happens beyond refusal — and what bounds the DoS? | **Reclassified in review round 2 from a liveness question to a settlement-safety one.** A single compromised current key halts a subject unilaterally; recovery orphans the suffix; and if device compromise persists, each revealed pre-rotation generation is captured in turn. KERI concedes *"an unavoidable race condition"* |
| **O-N7** | What makes class-2 (irreversibly settled) acts safe under a fork? | **New in review round 2.** `Live(branch A) → Live(branch B)` leaves two replicas with permanently different ledger state, because settled effects are never unwound (R5.3). Reducing exposure needs a **finality depth**, which ICN does not have and whose local evaluation reintroduces the divergence. **Until answered, this authority model is not by itself a sufficient basis for irreversible settlement** (§9.2.1) |
| **O-N8** | What is the protocol-specified **admission and compaction** rule for the durable event set? | **New in review round 2, widened in round 3.** `admissible` is cheap, so the set is attacker-writable and grow-only; eviction is forced, and if implementation-chosen then "convergence over the eventual retained set" is vacuous under attack. Authority-aware eviction (retain authorized before unauthorized) handles spam at or below the frontier; the **gap region beyond the frontier is unsolved**, so **R4.6 and permutation-invariant convergence hold only absent adversarial storage pressure** (§9.2.1) |
| **O12** | Do institutions need self-authenticating statements — now that §14.2 makes one possible without single-custodian capture? | **Reopened on better terms.** The original rejection assumed institution-identity ⇒ institution-keypair; §9.1 falsifies that premise |
| **O2** | Does a restored node keep its identifier? | §14.3 improves it (subject persists, keys rotate) but does not settle restore-twice detection policy |
| **O5** | Remove or wire `public_did` institutional signing? | Depends on O12 |
| **O6** | Does the membership-credential layer belong in this arc? | Needed for portable standing (Model D's genuine contribution); not needed for §9.4 |
| **O11** | Canonical encoding of a genesis decision before hashing to an `EntityId`? | §14.2 makes this the *same* mechanism as `SubjectId`, so it should be solved once, not twice |
| **O15** | Canonical preimage of the node-claim transcript | Unchanged from PRINCIPAL_MODEL §5.3 |

**Closed by this document:** O1 (§12.2); O7 (§9.3's class table answers "which
operation classes may a device sign"); O8 (§18); O9 (§18 — *rejected as posed*,
which is a disposition, not a solution).

**Decided in principle, mechanism still open:** O3 — pairwise identity is adopted
with costs stated (§11), but the mechanism that would complete it is O-N2, and F16
says it cannot be built today. Calling O3 "closed" would be relabelling.

**NOT closed, contrary to an earlier draft of this list** *(corrected in
adversarial review, which caught it contradicting §18)*: **O10** — still required
and widened; **O13** — partial, receiver-side retirement unsolved; **O14** —
mechanism only, custody remains; **O16** — narrowed to O16′; **O17** — partial,
authenticity ≠ acquisition; **O18** — closed as an identity class, but see the
untraced anchor↔member keying question in §15.2.

---

## 18. Consequences for the recorded open decisions and slices

| Item | Disposition |
|---|---|
| **O8** — current root after rotation | **DISSOLVED.** Replay the subject's log from a self-certifying inception (§6.2, §9.2). No external authority, no clock |
| **O9** — convergent device revocation | **REJECTED AS POSED** — unachievable *within the premises ICN has declined to pay for* (§6.1); **not** a theorem-level impossibility. **REPLACED** by §9.3's per-act-class semantics plus position-bounded validity, which itself carries the O-N6 and O-N7 gaps |
| **O10** — canonical `DeviceAuthorization` bytes | **STILL REQUIRED**, and widened: it now covers the log event preimage (F12: bincode, no domain separator) and the `Did` canonicalization rule (§10) |
| **O13** — retiring `GOV_OP_V1` | **PARTIAL, and an earlier draft had this backwards.** O13 constrains **receivers**, not authors. A subject writing "v1 no longer accepted for me" binds only acts *they* author; any subject who never writes it keeps v1 alive forever — which is precisely the never-ending dual stack **R12.3 [HARD]** forbids. Per-subject declaration is a genuine improvement on a global cutover, and **receiver-side retirement remains unsolved** |
| **O14** — total-loss recovery cannot advance the chain | **MECHANISM DISSOLVED by pre-rotation** (§12.1) — the committed-but-unused next key *is* the missing authorized key. **The custody question remains**: pre-rotation presupposes the backup survived, and "lost devices *and* backup" is answered only by guardians |
| **O16** — fork selection under genesis compromise | **NARROWED, not defused.** Pre-rotation covers *rotations* only; a current-key compromise still forks a non-establishment event (§6.2). Recovery is by **superseding rotation**, which is racy. **O16′** is now a liveness question, not only a selection one |
| **O17** — authenticated genesis document | **PARTIAL.** Self-addressing proves the inception event is *genuine*; it does not prove the prefix you were handed is *complete*. A truncated-but-valid prefix verifies perfectly and yields a stale authority state — the split-view problem Model B was rejected for. KERI covers this with **witnesses and watchers**, which this model does not adopt. **Availability/completeness is open** (§7 Model C costs, O-N5) |
| **O18** — anchor-derived Persons | **DISSOLVED**, at far lower cost than estimated (§15.2, F4/F5/F20) |
| **Slice A0** (identity-document chain) | **REDIRECTED, not deleted.** Becomes the per-subject authority log with self-addressing inception and pre-rotation. Its hard prerequisites change: O17 and O14 are answered by construction; O16 shrinks; and it gains one new requirement — pre-rotation — that did not exist before |
| **Slice A** (device principal + authorization) | **UNBLOCKED in principle**, because O8 and O9 no longer gate it — but its field set now depends on O-N1 (validity span) and O10 (canonical bytes) |
| **Slice B** (device enrolment) | **KEEP, with #2588 + #2590 folded in.** F8 shows the path is unreachable today, so this is a "fix before wiring" job, not a live-incident response |
| **Slice C** (mobile genesis) | **KEEP, widened** — must now also produce a subject and off-device pre-rotation backup (F19, §12.2) |
| **Slice D** (`GOV_OP_V2`) | **UNBLOCKED from O9/O13**, still bounded by #2469 §7.0.1: the receiver cannot evaluate standing/suspension because that state is unauthenticated (F17). That barrier is #2441's, not this document's |
| **Slices E/F/G** (node claim, institution genesis, hosting) | **KEEP.** §14.2 makes F's `EntityId` binding the same mechanism as `SubjectId` — solve once |
| **#2469 slices 4–6** | **Still independent.** Nothing here blocks them |

---

## 19. Revised dependency graph and the next slice

### 19.1 The graph

```
  N1  authority-log primitive          (library only; no wiring)
   │      SubjectId = H(inception) · pre-rotation · +1 monotone
   │      canonical domain-separated preimage  [absorbs O10, F12]
   │
   ├──► N2  narrow `Did`               [F1, F2 — removes from_anchor_id]
   │
   ├──► N3  log replication  ==  BUILD BYZANTINE RELIABLE BROADCAST
   │          gossip entries carry no signature at all (F17/§3.7), so this is
   │          a new primitive, not a new topic.  + bounded quarantine [O-N5]
   │          │
   │          └──► N4  device authorization + attenuation
   │                     [folds #2588, #2590; needs O-N1]
   │                          │
   │                          └──► N5  GOV_OP_V2 member-origin signing
   │                                    [still bounded by #2469 §7.0.1]
   │
   ├──► N6  mobile: subject + device split + pre-rotation backup   [F19]
   │
   └──► N7  threshold SIGNING (FROST-class) — a HARD PREREQUISITE of guardian
              recovery, not an upgrade.  ICN has a threshold PRF, not threshold
              signing (F7).  Without N7, independent M-of-N signatures give any
              M guardians a KILL SWITCH (two quorums => two authorized bodies at
              one position => permanent halt), so path (b) must not ship first.
              Without N7, R9.1 [HARD] is met only by the backup-phrase path.

  independent, and should not wait:
     X1  #2589 invite-mint fix
     X2  #2591 recovery signature + dedup  (+ the SDIS sibling)
     X3  the F2 / F13 findings — see §19.3
```

The old order — `[O18] → A0 → (B ∥ C) → [A blocked on O9; D blocked on O9+O13] → …`
— is **replaced**. O18 no longer precedes anything (§15.2), and A/D are no longer
blocked on O9 because O9 is not a prerequisite; it was an unachievable goal.

### 19.2 The exact next slice — **N1: the authority-log primitive**

Smallest slice that either validates or refutes the architecture, with **no
wiring, no gateway change, no `SignedGovernanceOp` change**.

- **Invariant — stated over §9.2.1's two-layer model, not over a "verified
  prefix".** *(The earlier formulation — permutation-invariance of
  `(longest verified prefix, duplicity set)` — is withdrawn; §9.2.1 explains why it
  is not a join.)* Two obligations, tested separately:
  - **Durable layer.** `S(σ)` is a set of `admissible` events keyed by
    `event_id = H(canonical_body)`; the join is set union; `admissible` is
    **state-independent**, so it commutes with union.
  - **Derived layer.** `derive(S(σ))` is a **pure function** — same set, same
    verdict, regardless of receipt order — halting on `|C| ≥ 2` after `supersede`.
- **Seam.** A new module beside `icn/crates/icn-identity/src/multi_device.rs`. Do
  not modify `multi_device.rs` in this slice.
- **Proof / tests — the two layers are tested separately.**

  *Durable join (properties, ideally `proptest`):*
  1. **commutativity** — `S₁ ⊔ S₂ = S₂ ⊔ S₁`;
  2. **associativity** — `(S₁ ⊔ S₂) ⊔ S₃ = S₁ ⊔ (S₂ ⊔ S₃)`;
  3. **idempotence** — `S ⊔ S = S`, including a **re-signed identical body**, which
     must collapse to one element (the `event_id` rule);
  4. **permutation-invariant durable state** — every delivery permutation of one
     event multiset yields the same `S`;
  5. **`admissible` is state-independent — as a differential property, not a
     signature observation.** Classify one event under many randomly generated
     surrounding contexts and require an identical verdict every time. *(An earlier
     draft said "assert by construction: the function takes only the event"; that is
     unfalsifiable — `fn admissible(&self, e: &Event)` on a struct holding the store
     satisfies it while consulting state.)*

  *Derived view (determinism):*
  6. **deterministic authority view** — `derive` over equal `S` yields equal
     verdicts, across permutations and across independent replica instances;
  7. **gaps buffer** — an out-of-order event is retained and unused, never rejected,
     and never wedges once the gap fills (cases 1–2);
  8. **conflicting same-position events** → `Halted(p)`, identically on every
     replica, in both arrival orders (cases 3–5);
  9. **unauthorized or malformed events cannot fork or block** — spam at position
     `p` never enters `C` (case 7);
  10. **pre-rotation** — a rotation revealing the committed material succeeds; one
      without it is retained but never authorized (cases 8, and scenario 1);
  11. **non-establishment fork detection** — a compromised current key forking an
      `authorize`/`revoke` is detected, **not** prevented by pre-rotation (case 9);
  12. **superseding recovery** — a legitimate `rotate` at the disputed position
      supersedes the forked non-establishment event and advances (case 10). **Two
      authorized establishment events at one position must halt, not choose**;
  13. **no branch selection by content — as two falsifiable properties.**
      (a) `derive`'s verdict is invariant under any **injective relabeling of
      `event_id`s**, which no lowest-hash or ordering-based tiebreak can survive;
      (b) for every generated set with `|C| ≥ 2` after `supersede`, `derive` returns
      `Halted`. *(An earlier draft asserted "no such rule exists anywhere", an
      absence claim over an unbounded space that nothing can fail.)*

  *Cases the first draft of this list could not fail — added after review round 2:*
  14. **re-signed identical body collapses to one element** and does **not** produce
      `Halted`; the witness set grows instead;
  15. **deterministic rotation body** — deriving the same rotation twice from the
      same state yields one `event_id`; a non-deterministic construction that yields
      two authorized rotations at one position is rejected at construction time,
      because at `derive` time it is already an unrecoverable halt;
  16. **digest consistency** — `σ`, `prev_digest` and dedup all use
      `event_id = H(canonical_body)`; a child naming a whole-event digest fails;
  17. **fork behind the frontier** — bodies at 0–5 and 8–9 with a fork at 3 must halt
      at 3, and a superseding rotation at 3 must be shown to **orphan** 4, 5, 8, 9
      (the test asserts the loss, so it cannot be discovered later as a surprise);
  18. **unilateral self-equivocation** — one authorized key emitting two distinct
      bodies at one position halts the subject with no second party involved;
  19. **authority principals are raw `Did`s** — an event naming a `SubjectId` as a
      device or guardian is rejected, so `derive` cannot become mutually recursive.

  *Encoding and hygiene:*
  20. encode/decode round trip over a **versioned, domain-separated,
      length-prefixed** preimage, plus cross-implementation vectors (absorbs O10;
      avoids F12's bare-bincode signing domain);
  21. **no wall clock is read anywhere in the module** — assert in CI, given F14.
- **Security properties.** Compromise of a current key cannot rotate, and can fork
  only non-establishment events — which superseding rotation recovers. Compromise of
  the inception key after a legitimate rotation grants nothing. Compromise of the
  **pre-rotation material** is out of scope of this slice's guarantees (§12.2).
- **Non-goals.** No replication, no gossip topic, no device-authorization format,
  no revocation policy, no `Did` change, no migration.
- **Dependencies.** None. This is why it is first.
- **Migration impact.** None — additive library code.

### 19.3 Findings that warrant issues before any of this

Listed, **not filed** — filing is a separate decision for the maintainer. The
tracking column reflects a GitHub issue search performed at review round 2; it is a
point-in-time result, not a standing guarantee.

| Finding | Why it should not wait | Already tracked? |
|---|---|---|
| **F2** — `icn-zkp/src/prover.rs:227-230` substitutes `[0u8;32]` for a failed issuer key and proceeds | A silent all-zero-key fallback in a prover. Independent of everything here | **UNTRACKED** — no issue matches `icn-zkp`, `prover`, or the fallback |
| **F13/F13a** — RPC recovery lets a caller name an arbitrary victim `old_did` with `threshold: 1`, and the **node** signs the trustee attestation | Latent critical: inert only because nothing applies recovery events (F9) | **UNTRACKED.** #2591 is the nearest and explicitly scopes to `sync.rs`/`recovery.rs` verification and dedup — it does **not** cover the RPC handler path |
| **F6** — `compute_temporary_vui(did) = SHA256(did)` on the routed enrollment path, and the second site at `icn-commons/src/inner.rs:81-87` | Destroys the VUI's stated unlinkability; the file itself says "NO sybil resistance" | **UNTRACKED** — #2468 covers enrollment *retry idempotency*, not VUI derivation |
| **F1** — ~50% of hash-derived DIDs fail Ed25519 deserialization | Data-integrity defect masked by hand-picked test fixtures | **UNTRACKED** |
| **F14** — `current_timestamp_secs()` returns 0 on clock error | Every expiry check in the identity stack fails **open** | **HISTORICALLY TRACKED, NOT COVERED.** #417 (the defect) and #425 (migrate security-critical callers to `try_*`) are both **CLOSED** — yet at `b26bf681` ten files across `icn-identity`/`icn-steward` still call `current_timestamp_secs()` and **zero** call `try_current_timestamp_*`. The identity stack was never migrated |
| **F8** — the gateway multi-device path is unreachable | Not a defect; a **truth correction** — `docs/status.toml` and the PRINCIPAL_MODEL gap matrix rate this capability as implemented | n/a — a truth-plane correction, not a defect |
| **`AlreadyVoted` is declared and never constructed** (`icn-governance/src/error.rs:33`), while `save_vote` is an unconditional put keyed by a `Did` | Not new behaviour (#2469 fact 15 records the overwrite), but the *sybil* consequence — one human with two device DIDs writes two tallied rows — is worth its own decision | **UNTRACKED** as a sybil concern |
| **F20 live-state** — `/v1/sdis/anchor` write routes now return `Forbidden` | Not a new defect | **#2448 is OPEN and partially stale**: verified at `b34cd3f6`, its *write* attack is no longer reachable at `b26bf681`; the *unauthenticated read* half stands |

### 19.4 Entry conditions for the N1 session

N1 is a design-and-test slice, not a wiring task. Before writing code it must
settle three things this document deliberately left open, because each changes
the event field set and freezing the format first would repeat the mistake
PRINCIPAL_MODEL §15.1 identified:

1. **The canonical preimage** — version, domain separator, field order, encoding,
   and the `Did` canonicalization rule (§10). Absorbs O10; must not repeat F12's
   bare-bincode signing domain.
2. **The pre-rotation commitment scheme** — what is committed, and how a rotation
   proves it. This is the load-bearing new mechanism and has no precedent in the
   repo. **It must additionally make the whole rotation body re-derivable** (e.g.
   `K_{p+1} = KDF(root ‖ p+1)` with every other field determined), because
   pre-rotation commits to the *next key set* and not to the *body* — so a
   non-deterministic construction lets one honest signer author two authorized
   rotations at one position and halt the subject permanently, with no attacker
   present (§9.2.1, constraint 2).
3. **The event taxonomy** — inception / rotate / authorize / revoke / recover;
   which are **establishment** events (and so eligible for `supersede`); and which
   may carry a validity span (O-N1). Two invariants are normative, not stylistic:
   **an event is establishment iff `authorized` requires a pre-rotation pre-image
   reveal** — never a capability flag, and no delegation path may mint one, which
   explicitly bars §9.5's generic attenuating delegation from establishment
   authority; and **every authority principal named inside an event is a raw `Did`**,
   never a `SubjectId`, or `derive` stops being a pure function.
4. **The replicated state model must be taken from §9.2.1, not re-invented.** This
   was added after review round 2 found that two successive drafts had specified a
   merge that does not form a join. The N1 session inherits, and must not redesign:
   - durable state = a set of `admissible` events keyed by
     `event_id = H(canonical_body)`, joined by **union**;
   - `admissible` is **state-independent** — it must take the event and nothing else;
   - the derived authority view is a **pure function** of that set;
   - `supersede` (establishment over non-establishment) is the **only** selection
     rule, and `|C| ≥ 2` after it **halts**;
   - **no content-based tiebreak of any kind** — no lowest hash, no first-seen, no
     longest chain.

Everything else in §19.2 is testable once those four are fixed.

---

## 20. Decision classification index

| Result | Class |
|---|---|
| A durable human subject identifier must not be a public key (§4) | **ESTABLISHED** (proof from R1.2/R2.1/R2.2, exhibited in `multi_device.rs:21-22`) |
| `Did` is a key identifier; every non-key use is unsupported type reuse (§3.1) | **ESTABLISHED** (F1, F2, F11) |
| ICN has no replicated authenticated ordered log a governance op lands on (§3.7) | **ESTABLISHED** (source-traced) |
| Global convergent revocation is unachievable **within premises ICN has chosen** (§6.1) | **RECOMMENDED** — a bounded argument, *not* ESTABLISHED. FLP forbids only *deterministic* consensus in *pure asynchrony*, and is circumvented by randomization, partial synchrony and failure detectors; Chandra–Toueg is itself a positive result. *(Downgraded from ESTABLISHED in adversarial review.)* |
| A single-writer authority log needs no consensus, **given Byzantine reliable broadcast** (§6.1) | **ESTABLISHED** (Guerraoui et al., PODC 2019) — with the caveat that BRB does not prevent a Byzantine writer equivocating, which is why §9.2 handles duplicity explicitly, and that **ICN has no BRB today** |
| Recency is the acceptor's decision (§6.3) | **ESTABLISHED** (Rivest, FC'98) |
| No deployed system achieves decentralized global revocation convergence (§6.1) | **ESTABLISHED** (survey of primary specs) |
| Model C — context-scoped identity with a private continuity root (§9) | **RECOMMENDED** |
| Durable state = two grow-only sets whose elements are canonical **bodies** (signatures held separately and never compared), joined by union (§9.2.1) | **ESTABLISHED** — join laws follow from set union once `admissible` is state-independent |
| `admissible` must be state-independent, or the filter does not commute with union (§9.2.1) | **ESTABLISHED** |
| Derived authority view as a pure function; halt on `\|C\| ≥ 2` (§9.2.1) | **RECOMMENDED** |
| `supersede`: establishment over non-establishment, with establishment eligibility defined by pre-committed material and **never** by a capability flag (§9.2.1) | **RECOMMENDED** — the constraint is load-bearing; violating it makes the selector a privilege-escalation path |
| Class-2 settlement is not convergent under a fork (§9.2.1) | **OPEN — O-N7.** Not merely late detection: `Live(A) → Live(B)` leaves permanently different settled state. **This model is not a sufficient basis for irreversible settlement until answered** |
| Rotation bodies must be deterministic; threshold recovery must emit one body (§9.2.1) | **RECOMMENDED, normative for N1** — without them a crash-retry or a second quorum halts the subject with no attacker |
| Authority principals inside events must be raw `Did`s, never `SubjectId`s (§9.2.1) | **RECOMMENDED, normative for N1** — otherwise `derive` is not a pure function |
| Superseding recovery orphans the suffix (§9.2.1) | **ESTABLISHED consequence** — a stated cost of recovery, growing with fork depth |
| N7 (threshold signing) is a hard prerequisite of guardian recovery (§12.2) | **RECOMMENDED** — otherwise any `M` guardians hold a kill switch |
| Fork monitoring is a client obligation ("responsive controller") (§9.2.2) | **RECOMMENDED**, inherited from KERI |
| `Did` narrowed to "cryptographic principal" (§10) | **RECOMMENDED** |
| No global Person identifier (§10) | **RECOMMENDED** |
| Per-act-class revocation semantics (§9.3) | **RECOMMENDED** |
| Pre-rotation as the recovery mechanism (§12.1) | **RECOMMENDED** — solves O14's *mechanism*; makes **backup** compromise more decisive, not less (§12.2) |
| Per-context subjects and per-context device keys (§11.1) | **RECOMMENDED** |
| Institutions hold no signing key (§14.2) | **RECOMMENDED**, with O12 reopened |
| Anchors retained as labels; `to_did` removed (§15.2) | **RECOMMENDED** |
| Key-derived Person DIDs unchanged (§15.1) | **RECOMMENDED** |
| O-N1…O-N6, O16′, O12, O2, O5, O6, O10, O11, O13 (receiver-side), O15, O17 (acquisition) (§17) | **OPEN** |
| R9.1 [HARD] total-loss recovery; R5.1 [HARD] under sole-device compromise and under the class-1 fallback; R4.1/R4.3 during the recovery veto window | **ADMITTED SHORTFALLS** — named in §9.6, not discharged |
| Anchor-derived `Did`s; `new_unchecked`; `is_anchor_did`; kernel-api `DidDocument`; `did_mapping` recovery indirection | **LEGACY** — compatibility only, then removed |
| Model A key-as-person; Model B global anchor; Model D credential-only; Model E hybrid; O9 as posed; wall-clock validity; `standing_hash`; carried-proof-only as an absolute; threshold-held institution DID (for now); near-term ZK selective disclosure | **REJECTED** (§16) |
