---
Status: normative
Canonical: no
Last Reviewed: 2026-10-03
---

# N4-A — relying-party evaluation of delegated device authority

**Implements:** first bounded tranche of [icn#2599](https://github.com/InterCooperative-Network/icn/issues/2599) (N4) ·
**Parent ladder:** [icn#2694](https://github.com/InterCooperative-Network/icn/issues/2694) §7, rung 5 of 12 ·
**Code:** `icn/crates/icn-identity/src/device_authority.rs`, `icn/crates/icn-identity/src/authority_log/derive.rs` (`derive_prefix`) ·
**Tests:** `icn/crates/icn-identity/tests/device_authority.rs` ·
**Reference:** `icn/crates/icn-identity/tests/reference/device_act_reference.py`

This document is the **contract** for N4-A. It is written so that an implementation in another
language can reproduce the canonical act bytes in §8 from this text alone, and so that a relying
party in any language can state exactly what it has checked when it accepts a device-signed act.

> **Truth status.** Library, specification and tests only. Nothing here is wired into `icnd`, the
> gateway, `icnctl`, an SDK or any deployment. It makes no claim of institutional recognition,
> membership, standing, session authorization, replication, durability, recovery or production
> readiness. Fixture identities only; no real person's identity, keys or records are involved.

---

## 1. The question this answers

> At a log position the **relying party** has chosen, does this device Principal hold a grant
> from this human Subject that covers this capability — and did that device sign this act?

Two questks, two answers, never one function:

| Check | Proves | Mechanism |
|---|---|---|
| **Authorship** | key `K` signed these bytes | Ed25519 `verify_strict` over the canonical act (§6) |
| **Authorization** | `K` held a covering grant from Subject `S` at position `E` | the N1 fold over the retained prefix through `E` (§5) |

`IDENTITY_SEMANTICS.md` §4: *named ≠ signed ≠ authorized*. A verifying signature never establishes
authorization; a derived grant never establishes that any particular bytes were signed.

### 1.1 What N4-A is not

It is **not** the N4 lifecycle. Device enrollment ceremonies, rotation, replacement, lost-versus-
stolen handling, revocation *effectiveness* under partition, and legacy `multi_device.rs`
disposition remain owned by #2599 and are not advanced by this document. N4-A is the **relying-
party evaluation primitive** those later tranches, and the #2694 vote path (N5-A), consume.

It introduces **no new authority event**. N1's `Authorize` and `Revoke` bodies are the only facts;
this layer reads them.

---

## 2. What a verified act proves — and what it does not

A successful [`verify_device_act`] proves exactly:

> Under the N1 facts this verifier has retained for Subject `S`, folded through position `E`
> which the verifier itself chose, the chain was live and gap-free through `E`; device Principal
> `K` held a grant in force at `E` carrying capability `C`; `K` is not `S`'s establishment
> authority at `E`; and `K` produced a valid signature over this exact act, which names `S`, `K`,
> `C` and `E`.

It does **not** prove: that `K` is authorized *now* (a later `revoke` is outside the prefix by
construction); that any other replica would reach `E` (that is N3, #2598); that the act has not
been presented before (replay slots belong to the action family, §7.3); that `S` is recognized
by any institution (GEN-B / N5); that `E` is the *right* position for the relying party's purpose
(§7.1 states the rule, the relying party applies it); or anything about a session, token or
transport.

**There is no registrar, directory, clock, network lookup or randomness.** Same retained facts,
same act, same `E` ⇒ same verdict, on every replica.

---

## 3. Primitives

Reused from N1 (`icn/crates/icn-identity/src/authority_log/`), not redefined:

```text
LP(x)  := u32be(len(x)) || x          -- length-prefixed byte string
b32(x) := 32 raw bytes, no prefix
P(k)   := u8(0x01) || b32(k)          -- a tagged Ed25519 principal, exactly as in an N1 body
u8/u16be/u64be                        -- big-endian, fixed width
H(x)   := SHA-256(x)
```

The principal tag is load-bearing: a `SubjectId` placed in the device slot is a **decode error**,
not a 32-byte blob (`IDENTITY_SEMANTICS.md` I2).

## 4. Constants (frozen for v1)

| Name | Value |
|---|---|
| `DEVICE_ACT_DOMAIN` | ASCII `icn.n4.device-act` (17 bytes) |
| `DEVICE_ACT_VERSION` | `1` (`u16be`) |
| `MAX_DEVICE_ACT_PAYLOAD` | `65536` bytes |
| capability tags | `Sign 0x01`, `Encrypt 0x02`, `Present 0x03`, `Recover 0x04` — N1's, never renumbered |
| position bound | `1 ..= MAX_POSITION` (N1's `1_048_576`) |

`DEVICE_ACT_DOMAIN` is length-prefixed and pairwise distinct from every N1 separator
(`icn.authority-log`, `.sig`, `.commit`, `.kdf`) and every GEN separator, so act bytes can never
be reinterpreted as an authority body, a signature preimage, a commitment, a KDF input or a GEN
reference — and none of those can be presented as an act.

---

## 5. Prefix derivation — `derive_prefix`

```text
derive_prefix(σ, S, E) := derive(σ, { b ∈ Bodies_S(σ) : position(b) ≤ E })
```

The existing N1 fold, applied to the retained prefix. **Nothing about the fold changes**: the
candidate set at position `p` is drawn only from bodies at exactly `p`, `supersede`/`resolve` remain
the single decision point, a fork at or below `E` still halts, and no body above `E` can alter
what the fold concludes at or below it. Consequently:

```text
derive_prefix(σ, S, E)            == derive(σ, S)     whenever E ≥ frontier(derive(σ, S)) − 1
derive_prefix(σ, S, MAX_POSITION) == derive(σ, S)
```

### 5.1 Why it is needed

N1's `Revoke` **removes** the grant from the derived state; there is no `revoked_at`. The frontier
view therefore answers "is `K` authorized at the frontier" only. Two things the N4 invariants
require are unanswerable from it:

- **non-retroactive revocation** — an act `K` signed at position 2, when `K` was revoked at 3,
  must still verify at `E = 2` (#2599 invariant 9; #2694 §7 "later revocation does not erase
  historical authorship");
- **a relying-party-pinned position** — a governance process that pins `E` must get the state
  *as of* `E`, not as of whatever the verifier happens to have retained since.

`derive_prefix` makes both answerable without touching N1's semantics or wire format.

### 5.2 Reading the result

| `derive_prefix(σ, S, E)` | Meaning for evaluation at `E` |
|---|---|
| `Unknown` | no inception retained — refuse |
| `Halted { disputed_at ≤ E }` | fork inside the prefix — refuse |
| `Live { frontier ≤ E }` | gap: the prefix does not reach `E` — refuse |
| `Live { state, frontier = E + 1 }` | clean prefix through `E`; `state` is the authority state **as of `E`** |

A clean prefix means exactly one authorized candidate at every position `1..=E`.

---

## 6. Canonical device act — `DeviceActV1`

```text
device_act_v1 :=
      LP(DEVICE_ACT_DOMAIN)
   || u16be(DEVICE_ACT_VERSION)
   || b32(subject)                       -- the SubjectId the device acts FOR
   || P(device)                          -- the device Principal that signs
   || u64be(evaluation_position)         -- the position the signer asks to be evaluated at
   || u8(capability_tag)                 -- the capability exercised
   || LP(payload)                        -- opaque application bytes, ≤ MAX_DEVICE_ACT_PAYLOAD

act_id    := H(device_act_v1)
signature := Ed25519-sign(device_key, device_act_v1)
```

Strict decode: wrong domain, wrong version, a non-principal in the device slot, position `0` or
above the bound, an oversized payload, or trailing bytes are each errors.

### 6.1 Field semantics

- **`subject`** — named, never a signer. A Subject has no key. The relying party supplies the
  Subject it is evaluating *for*, and refuses an act that names another (§7, step 1).
- **`device`** — an ordinary Principal. Device-ness is in the grant, not the identifier.
- **`evaluation_position`** — **advisory until a relying party agrees.** It is bound so that an
  act cannot be re-evaluated at a position its signer never saw, but it confers nothing: the
  verifier refuses unless it equals the position the relying party supplies (§7, step 2). This is
  how N4 invariant 6, *the evaluation position is not signer-chosen*, is met while still binding
  the position into the signature.
- **`capability`** — one capability per act. An act that needs two is two acts.
- **`payload`** — opaque. An action family that signs through this envelope places its **own
  domain-separated canonical bytes** here; N4-A never parses them. `act_id` is witness-independent
  (it hashes the act, not the signature) and is the identity a replay slot should key by.

### 6.2 Signing

`sign_device_act(act, key)` refuses a key whose Principal is not `act.device`. A client cannot
produce an act it could not use; a hand-assembled envelope with a foreign signature is refused at
verification instead (§7, step 3).

---

## 7. Verification — `verify_device_act`

Given a `SignedDeviceAct`, and independently supplied `(subject, store, E)` from the relying party:

1. Require `act.subject == subject`. *(ActSubjectMismatch)*
2. Require `act.evaluation_position == E`. *(EvaluationPositionMismatch)*
3. Verify the signature under `act.device` over `act.canonical_bytes()`, `verify_strict`.
   State-independent, exactly as N1 admission is. *(BadSignature)*
4. `evaluate_device_authority(subject, store, act.device, act.capability, E)`:
   1. require `1 ≤ E ≤ MAX_POSITION`; *(PositionOutOfRange)*
   2. `derive_prefix(subject, store, E)`; refuse `Unknown` *(SubjectUnknown)* and `Halted`
      *(AuthorityHalted{disputed_at})*;
   3. require `frontier > E`; *(PrefixIncomplete{frontier, required: E})*
   4. require `device ∉ state.authority`; *(DeviceIsEstablishmentAuthority)*
   5. require a grant for `device` in `state.devices`; *(DeviceNotAuthorized)*
   6. require `grant.in_force_at(E)` — `E ≥ granted_at` and inside the validity span if any;
      *(GrantNotInForce{granted_at, validity})*
   7. require `capability ∈ grant.capabilities`. *(CapabilityNotGranted)*

Every failure is a refusal. There is no partial success and no "verified except". Refusal variants
carry positions and spans only — never payload bytes.

Success returns `DeviceAuthorityEvidence { subject, device, capability, evaluation_position,
grant, generation }` — **a grant, not a bool** (`IDENTITY_SEMANTICS.md` I5).

### 7.1 Choosing `E` — the relying party's rule

`E` is **the last position the relying party relies on**: the position of the last authority fact
it has itself observed, retained and admitted for this Subject. It is a position, never a
timestamp, and it is never read from the act. Concretely:

| Relying-party situation | `E` |
|---|---|
| a governance process that pinned a prefix when it froze its electorate (#2694 §6) | the pinned position, the same for every ballot in that process |
| a verifier holding a store whose `derive` reports `Live { frontier }` | `frontier − 1` |
| a verifier that has not retained position `E` the act names | **refuse**; do not fetch, guess or fall back to the frontier |

`E` chosen as `frontier − 1` of a store that has not seen a revoke is **prospective only**: it
evaluates the authority the verifier knows about. That is the honest semantics of class-2 acts
(§7.2), not a defect of this primitive.

### 7.2 Act classes (`HUMAN_IDENTITY_ARCHITECTURE.md` §9.3)

| Class | How N4-A applies |
|---|---|
| **1 — deferred-decision** (votes, nominations, anything tallied at a decision point) | the decision pins `E`; every act in the process is evaluated at that one `E`; a device revoked after `E` still counts, a device revoked at or before `E` never does |
| **2 — immediately settled** (ledger entries, irreversible effects) | the acceptor evaluates at its own retained `frontier − 1` once, at admission; **no validity bound exists** and R5.1 remains unmet for this class, exactly as HIA states — N4-A does not pretend otherwise |
| **3 — authority events** | not acts; the log orders them itself |

### 7.3 Deliberately outside the verifier

- **Replay.** `act_id` is provided; the one-ballot rule of #2694 §9 (`slot = (process_hash,
  SubjectId)`) and any other replay policy belong to the action family that owns the slot.
- **Attenuation enforcement.** N1 records a capability set and an optional position span; it has
  no caveat, amount or resource field. N4-A checks exactly what N1 recorded. Richer scope is a
  contract change for #2599, not something to improvise here.
- **Session coupling.** A session proves a request may enter a handler now; the act proves
  durable authorship. They are never bound together (#2694 §8).

### 7.4 The establishment-authority rule

N1 permits a Subject's own generation-0 authority key to be named in an `Authorize` event. N4-A
refuses to treat an act signed by the current establishment authority as a *device* act, for the
reason GEN-A refuses it at genesis (`GEN_SUBJECT_CONTEXT_GENESIS.md` §6.3): a credential that is
also the log-writer key has no bounded scope, so a `{Sign}` grant to it is illusory. This is an
**Alpha-profile** rule stated for N4-A and GEN-A together; it is not a claim about every future
profile.

---

## 8. Test vectors

Inputs, stated in full:

| Input | Value |
|---|---|
| `subject` | `subject[i] = 0x20 + i` for i in 0..32 → `2021…3e3f` |
| device seed | `seed[i] = 0x40 + i` → `4041…5e5f` |
| `evaluation_position` | `3` |
| `capability` | `Sign` (`0x01`) |
| `payload` | ASCII `katie:open-workspace` (20 bytes) |

Outputs (hex):

| Value | Bytes |
|---|---|
| device pubkey | `2543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d` |
| `device_act_v1` (121 B) | `0000001169636e2e6e342e6465766963652d6163740001202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f012543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d000000000000000301000000146b617469653a6f70656e2d776f726b7370616365` |
| **`act_id`** | `52df522d5e353e8d2a70f03abd7edb4e8c2770da40b2d395c46d35ff874ef14a` |
| signature (RFC 8032, deterministic) | `0e12ebbe…fe3306` — verified by the reference, not part of identity |

These were produced by an **independent reference written from this document** (Python, sharing
no code, library or constant with `icn-identity`) and pinned as literals in
`tests/device_authority.rs`. The reference is self-checking:

```bash
python3 icn/crates/icn-identity/tests/reference/device_act_reference.py
```

It re-derives the bytes and `act_id` from this document, reads the `EXPECT_*` literals out of the
Rust test purely as a comparison target, verifies the Rust-produced signature over its own bytes,
and exits non-zero on any disagreement.

### 8.1 Behaviour pinned by the fixture scenario

One Subject `S`; devices `A` (authorized at 1) and `B` (authorized at 2); `A` revoked at 3.

- `A`'s act at `E = 3` → `Refused(DeviceNotAuthorized)`; `B`'s act at `E = 3` → evidence with
  `granted_at = 2`, `generation = 0`.
- `S`'s identifier, establishment authority, generation and next commitment are **identical**
  before and after the revoke; `A` is absent from, and `B` present in, the post-revoke state; `B`
  is not in the authority set; the device bytes are not the Subject bytes.
- `A`'s act at `E = 2` → accepted (authorized then); the same bytes at `E = 3` → refused.
- act claims `E = 2`, relying party evaluates at `3` → `EvaluationPositionMismatch`.
- `E = 4` with frontier `4` → `PrefixIncomplete{frontier: 4, required: 4}`; a missing position 2
  with `E = 3` → `PrefixIncomplete{frontier: 2, required: 3}`.
- two authorized candidates at 2 → `AuthorityHalted{disputed_at: 2}` at `E = 2`, accepted at `E = 1`.
- act names another Subject → `ActSubjectMismatch`; foreign signature → `BadSignature`; mutated
  payload → `BadSignature`; capability not in grant → `CapabilityNotGranted`; grant with span
  `[1, 2]` → accepted at 2, `GrantNotInForce` at 3; never-authorized device → `DeviceNotAuthorized`;
  the authority key as device → `DeviceIsEstablishmentAuthority`; unknown Subject →
  `SubjectUnknown`; position `0` and `> MAX_POSITION` refused at construction and evaluation.
- a rotation inside the prefix is honoured: a device authorized by generation 1 evaluates with
  `generation = 1`; a device authorized before the rotation survives it (N1: rotate preserves).
- every verdict is identical across all `4!` ingest orders of the four facts.
- `derive_prefix` through the bound, and through any position at or past the frontier, equals
  `derive`; through `0` yields the inception-only state; it hides a later fork and reveals an
  earlier one.
- act bytes are not an admissible N1 body and an N1 body is not an act; the module reads no clock
  and no randomness.

---

## 9. The external-consumer boundary

This section answers #2599's "exact verifier algorithm" for the bounded tranche, and records the
boundary decision a downstream deployment needs so that it does not build a parallel identity
model.

### 9.1 What a verifier needs

| Input | Source | Mutable? |
|---|---|---|
| the Subject's retained N1 facts: canonical bodies + witnesses | the verifier's own admitted store (local, or rehydrated through the N1-D fact store when #2800 lands) | append-only; the verifier admits each fact through `admissible_bytes` itself |
| the act: `device_act_v1` bytes + 64-byte signature | the client | immutable once signed |
| `(subject, E)` | the relying party's own context | chosen by the relying party, never by the client |

Nothing else. No directory, no resolver, no token, no clock.

### 9.2 Where the boundary sits

**The boundary is a stateless verifier over retained canonical facts, not a query to a
service.** A downstream system obtains ICN truth by holding the facts and running this verifier;
it does not obtain it by asking a gateway "is this device authorized?" and trusting the answer. A
gateway may *host* a verifier for a client that cannot run one, but the gateway is then a relying
party making its own evaluation at its own `E`, and the thing it returns is `DeviceAuthorityEvidence`
plus the facts it evaluated — reproducible, not an oracle verdict. **The gateway is not the owner
of this truth.** This keeps the trust surface where it is: exposing the verifier expands no
authority, because the verifier can only refuse or return a grant that already exists in the log.

The first consumable surface, in order of smallness, is therefore:

1. **this library API** (`icn_identity::device_authority`) — exists;
2. a strict, deterministic **container** for *(facts, act)* so a non-Rust client can carry them —
   **built: N4-B**, `N4B_PORTABLE_EVIDENCE_BUNDLE.md` (`evidence_bundle.rs`,
   `icn.n4.evidence-bundle`): every retained body for `S` at `≤ E` with every witness, grouped by
   body in the N1-D record layout (`event_id || signature → canonical body`), re-admitted through
   `AuthorityStore::ingest`; GEN-A's rule holds: serde/JSON bytes are never the cryptographic
   identity of anything;
3. an `icnctl` verb that consumes that container and prints the evidence or the refusal —
   **built: `icnctl device-authority verify|inspect`** (N4-B §9); local, stateless, no daemon;
4. only after that, a gateway route — and only as a hosted relying party, never as an authority.

Items 2 and 3 landed as N4-B (converged on one canonical container on 2026-10-04; N4-B §11).
Item 4 remains deliberately unbuilt. The ceremony that puts facts
in a device's hands in the first place is N4-C (`N4C_DEVICE_ENROLLMENT_REQUEST.md`).

### 9.3 Downstream contract (what a deployment MAY rely on, MUST NOT reimplement)

A downstream deployment (for example the network-ops Home work) **MAY** rely on:

- the N4-A verdict as the sole answer to "is this device acting for this Subject with this
  capability at this position"; and on `DeviceAuthorityEvidence` as the thing to log, display or
  gate on;
- `act_id` as a stable, witness-independent identity for an act;
- the refusal classes in §7 as the complete vocabulary for *why* an act was refused, with no
  payload leakage;
- the rule that a revoked device is refused at every `E` at or after the revoke, and that an
  earlier `E` is unaffected.

It **MUST NOT**:

- reimplement signature domains, canonical encodings, the N1 fold, grant semantics or refusal
  logic in Python, shell, JavaScript or deployment YAML;
- treat a session, SSH key, OS account, RDP login, VM, Pi, phone or any node key as the Subject,
  or derive a `SubjectId` from any of them;
- hold or derive a `ContinuityRoot` on infrastructure, or mint a Subject on a person's behalf;
- cache a verdict across positions, choose `E` from the act, or fall back to the frontier when the
  act's `E` is not retained;
- promote fixture identities, keys or vectors into anything real.

### 9.4 What is still open for the boundary

How a relying party learns *which* position a process pinned (G1-A, #2694 §6); and
replication of facts between replicas (N3, #2598). None of these changes the verifier.

---

## 10. Deliberate scope boundaries

Not in this slice, by design: a transport container; gateway routes; `icnctl` exposure; SDK
bindings; persistence (N1-D, #2800, is a sibling, not a dependency — the verifier takes an
`AuthorityStore` however it was filled); replication; recovery; session or JWT coupling; the N4
enrollment, rotation, replacement and lost/stolen lifecycle; disposition of legacy
`multi_device.rs` and the dormant `/v1/devices` route (#2588, #2590); any new context kind for
personal or household use (GEN, #2602); any production wiring.

`DeviceActV1` has **no serde impl and no wire encoding beyond its canonical bytes.** That is
intentional and mirrors GEN-A: the canonical bytes *are* the act.
