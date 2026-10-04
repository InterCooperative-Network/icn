---
Status: normative
Canonical: no
Last Reviewed: 2026-10-04
---

# N4-B — the portable evidence bundle, and the stateless `icnctl` relying party

**Implements:** `N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.2 items 2 and 3 — the container for
*(facts, act)* and the verb that consumes it ·
**Parent ladder:** [icn#2694](https://github.com/InterCooperative-Network/icn/issues/2694) §7 ·
[icn#2599](https://github.com/InterCooperative-Network/icn/issues/2599) (N4) ·
**Companion to:** `IDENTITY_SEMANTICS.md` (the canonical contract), `HOME_RUNTIME_IDENTITY_PROFILE.md`
(who consumes this and why), `N4C_DEVICE_ENROLLMENT_REQUEST.md` (how a device comes to hold facts) ·
**Code:** `icn/crates/icn-identity/src/evidence_bundle.rs`, `icn/bins/icnctl/src/device_authority.rs` ·
**Tests:** `icn/crates/icn-identity/tests/evidence_bundle.rs`,
`icn/bins/icnctl/tests/device_authority_bundle_test.rs` ·
**Reference:** `icn/crates/icn-identity/tests/reference/evidence_bundle_reference.py`

This document is the **contract** for the N4-B portable evidence bundle. It is written so that an
implementation in another language can reproduce the canonical bytes in §8 from this text, the N1
canonical-encoding contract it cites, and the N4-A act contract — and so that a stateless consumer
can state exactly what it has checked at each of the three steps between receiving bytes and
accepting a device-signed act.

> **Truth status.** Normative for the bundle bytes and for the verb's contract. **LIB-TESTED** for
> the container (24 tests observed on icn-dev, plus an independent Python reference that
> reproduces the vector and applies its own strict decoder) and **CLI-REACHABLE** for the verb
> (5 end-to-end tests of the built binary). Not PRODUCTION: no runtime, route, daemon or
> deployment calls either. The bundle changes nothing in N1 or N4-A. It is not a network protocol,
> not N3 replication, not a persistence format or durability guarantee, not a session format, not
> an enrollment or recovery protocol, not a Home or deployment format, and not an `icnd`, gateway
> or SDK surface. It confers no institutional recognition, membership, standing or session
> authority. It makes no claim that Katie's Pi or any real device is enrolled. **Fixture identities
> only** in every test; no real person's identity, keys or records are involved.

> **One format.** This is the single canonical N4-B container, the result of converging two
> independently developed implementations of the same boundary on 2026-10-04 (§11). The superseded
> `DeviceAuthorityBundleV1` (`icn.n4.device-authority-bundle`) no longer exists in the tree and has
> no compatibility decoder.

---

## 1. The question this answers

> How does a holder of N1 facts hand a **stateless** verifier exactly the evidence that the existing
> N1 + N4-A semantics need to evaluate one signed device act, so that the verifier rebuilds the same
> `AuthorityStore` and reaches the same verdict — without `icnd`, a network, a directory or a clock?

The answer is a strict, deterministic byte container. It is **framing over canonical facts**. It
adds no fact kind, no selector, no ordering rule the fold could read, and no verdict.

### 1.1 The architectural invariant

**The container transports evidence. It does not interpret authority.** The canonical N1 admission
path (`admissible`) and derivation path (`derive_prefix`, consumed by `verify_device_act`) remain
the only authority semantics. The flow is, and may only be:

```text
portable bytes
    ↓ EvidenceBundle::decode        -- strict, fail-closed, structural (§5)
N1 canonical bodies + witnesses
    ↓ EvidenceBundle::admit         -- every (body, witness) through AuthorityStore::ingest (§6)
AuthorityStore
    ↓ verify_device_act(act, S, store, E)   -- the existing N4-A verifier (§7)
DeviceAuthorityEvidence | refusal
```

No step may be skipped or merged. In particular, decoding a bundle **cannot** cause a fact to bypass
canonical admission: `AuthorityStore` has no ingest path other than `ingest`, which calls
`admissible`, and the bundle type has no method that builds a store any other way.

---

## 2. Three claims, kept separate

A consumer that accepts an act after processing a bundle has made **three** independent checks. They
prove different things and none implies the next.

| Step | Succeeds when | Proves | Does **not** prove |
|---|---|---|---|
| **decode** (§5) | the bytes are one canonical bundle | the framing is well-formed; each carried body is a strict canonical N1 body; each stated `event_id` is that body's digest; each body belongs to the bundle's Subject at position `≤ E`; each body has `≥ 1` witness; the act is a strict canonical device act naming the bundle's Subject and `E` | that any witness signature verifies; that any fact was ever authorized; that the prefix is complete or unforked |
| **admit** (§6) | every `(body, witness)` passes N1 admission | *some key signed each body* — the inline signer — exactly as a live replica would conclude | that the signer had authority; that the set is complete; anything about the act |
| **verify** (§7) | the existing N4-A verifier returns evidence | under **the facts the bundler supplied**, folded through `E`, the chain was live and gap-free through `E`, the device held a covering grant, was not the establishment authority, and signed this exact act | that the bundler supplied *all* facts that exist for `S` through `E` (see §3.3); that the device is authorized *now* (that is N4-E, `N4E_CURRENT_ADMISSION.md`); anything N4-A §2 lists as outside its claim |

A bundle that decodes and admits but whose act is **refused** is a correct bundle. Refusal is a
verdict, and transporting the evidence for a refusal is the container doing its job.

---

## 3. Scope: which facts a bundle carries

A bundle is scoped to one Subject `S` and one evaluation position `E`, both named by the enclosed act
(N4-A §6.1: the act binds the position its signer claims; the relying party must agree, §7).

### 3.1 The rule

> A bundle for `(S, E)` carries **every retained N1 body `b` with `subject(b) = S` and
> `position(b) ≤ E`**, together with **every retained witness `(event_id(b), σ)` over each such
> body**, and nothing else.

### 3.2 Why exactly that set

This is not a heuristic. N4-A §5 defines

```text
derive_prefix(S, store, E) := derive(S, { b ∈ Bodies_store(S) : position(b) ≤ E })
```

so the fold's **only** input is that body set. A verifier that admits exactly these facts folds
exactly what the bundler would have folded. Each consequence below was checked against N1's actual
candidate rule (`candidate_is_authorized`: subject, position, `prev_digest`, authorization by the
state as of the parent) rather than assumed:

- **Facts above `E` are never required** to prove authority at `E`. The fold filters them out before
  it runs, the candidate set at position `p` is drawn only from bodies at exactly `p`, and no body
  above `E` can change what is concluded at or below it. A later revocation therefore need not — and
  (§3.4) *cannot* — be carried into a historical proof. This is N4-A's non-retroactive-revocation
  invariant, honoured by construction.
- **Facts at or below `E` that the fold would never select are carried.** Unauthorized spam,
  orphaned branches and superseded candidates are all in `Bodies(S)`. Deciding that one of them is
  unselectable would require the bundler to run the fold, which is interpretation. So the bundler
  does not decide; it carries.
- **A fork at or below `E` is carried as a fork.** Both candidates satisfy the rule; neither is
  chosen. The verifier reports `AuthorityHalted { disputed_at }` exactly as it would on the original
  store.
- **A gap at or below `E` is carried as a gap.** There is nothing to carry at the missing position;
  bodies above it that name an unretained parent are carried (they are `≤ E`) and remain
  unselectable; the verifier reports `PrefixIncomplete { frontier, required: E }`.
- **An unknown Subject is carried as an empty fact list.** A bundler holding no inception for `S`
  produces a bundle with zero facts. It decodes, admits to an empty store, and the **verifier**
  refuses `SubjectUnknown`. The decoder does not convert that into an error of its own.
- **Every witness over a carried body is carried.** Several witnesses over one body are a re-signing,
  not duplicity (N1 §9.2.1); dropping to one would mean *choosing* a signature.

### 3.3 What the bundler's selection does and does not prove

A bundle proves what the verifier's third claim says: a verdict **under the facts supplied**. It
does not prove the bundler supplied every fact that exists. A bundler can omit a fact at or below
`E` (for example one candidate of a fork, or the revocation at `E` itself) and the verifier cannot
detect the omission from the bundle alone — but it cannot be *deceived* by it either: withholding
the fact at `E` leaves the prefix incomplete and the verifier fails closed (`PrefixIncomplete`),
it does not conclude the device is still authorized. That is N1's O-N7 (late fork discovery) and
the N3 replication question (#2598), neither of which a transport format can close. What the
container **does** guarantee is that the bundler cannot *add* a fact outside scope (§3.4), cannot
substitute a body for the one its witnesses are over (§5), and cannot make the verifier evaluate at
a position other than the one the act binds (§5, act agreement).

### 3.4 Strictness at the boundary

The decoder **refuses** a body whose subject is not `S` (`FactSubjectMismatch`) or whose position is
above `E` (`FactOutsideScope`). A bundle that carries one is not a prefix bundle for `(S, E)`. This
is what makes "the same semantic evidence yields the same bytes" a property of the format rather
than a convention of the encoder, and it is what prevents a revocation after `E` from being smuggled
into a proof about `E` in either direction.

---

## 4. Canonical bytes

### 4.1 Primitives

Reused from N1 (`authority_log/encoding.rs`), not redefined:

```text
LP(x)  := u32be(len(x)) || x          -- length-prefixed byte string
b32(x) := 32 raw bytes, no prefix
b64(x) := 64 raw bytes, no prefix
u8 / u16be / u32be / u64be            -- big-endian, fixed width
H(x)   := SHA-256(x)
```

### 4.2 Constants (frozen for v1)

| Name | Value |
|---|---|
| `BUNDLE_DOMAIN` | ASCII `icn.n4.evidence-bundle` (22 bytes) |
| `BUNDLE_VERSION` | `1` (`u16be`) |
| position bound | `1 ..= MAX_POSITION` (N1's `1_048_576`), applied to `E` |

`BUNDLE_DOMAIN` is length-prefixed and pairwise distinct from every N1 separator
(`icn.authority-log`, `.sig`, `.commit`, `.kdf`), the N4-A act separator (`icn.n4.device-act`), the
N4-C request separator (`icn.n4.enrollment-request`), the N4-D binding separator
(`icn.n4.channel-binding`) and every GEN separator. Bundle bytes can never be presented as, or
mistaken for, any of those, and none of those can be presented as a bundle.

### 4.3 Layout

```text
evidence_bundle_v1 :=
      LP(BUNDLE_DOMAIN)
   || u16be(BUNDLE_VERSION)
   || b32(subject)                        -- S; MUST equal act.subject
   || u64be(evaluation_position)          -- E; MUST equal act.evaluation_position
   || u32be(fact_count)                   -- may be 0
   || fact * fact_count                   -- strictly ascending by event_id
   || LP(device_act_v1)                   -- the act's canonical bytes, verbatim (N4-A §6)
   || b64(act_signature)                  -- Ed25519 over device_act_v1, verbatim

fact :=
      b32(event_id)                       -- H(canonical_body), stated, not inferred
   || LP(canonical_body)                  -- the N1 canonical body bytes, verbatim (N1 §1)
   || u32be(witness_count)                -- MUST be ≥ 1
   || b64(signature) * witness_count      -- strictly ascending bytewise; no duplicates

bundle_id := H(evidence_bundle_v1)
```

### 4.4 Relationship to the N1-D durable record

N1-D (#2800) persists one durable fact as `key = event_id (32) || signature (64)`,
`value = canonical_bytes(body)`. A `fact` above is that record **grouped by body**: one body, its
stated digest, and every signature over it. The grouping is the only difference, and it exists so
that one body has one encoding in the container (a flat record per witness would repeat the body
bytes and open the question of what to do if two records for one `event_id` carried different body
bytes). The digest is carried explicitly for the same reason N1-D carries it in the key: a
substituted or corrupted body is caught as a **disagreement** between what is stated and what is
carried, rather than silently re-identified.

Nothing of a storage backend is serialized. The record layout is a *concept* shared with N1-D; the
bundle is transport framing over canonical facts and would be unchanged if N1-D never existed.

### 4.5 Ordering, and why it carries no meaning

Facts are ordered by `event_id`; witnesses by signature bytes. These are the only canonical
identifiers the durable layer has, and both are digests or signatures — values that bear no relation
to log position, arrival order or preference. In the §8 vector the inception body (position 0) sorts
**third**. A decoder rebuilds set-valued state (`BTreeMap<EventId, _>`, `BTreeSet<WitnessSignature>`);
the fold orders bodies by position itself. **Container order is never authority order.** Ingest
order, backend iteration order, map iteration order, filesystem order and process restarts cannot
change the bytes, because the bytes are a function of the fact *set* only.

No clock, no randomness, no "latest", no lexical branch selection, no repair-on-read.

### 4.6 Bounds

**No new limit is introduced.** Every count and length is bounded by the input itself through N1's
`Reader`: a declared element count that the remaining bytes could not hold is refused
(`CountOverflow`) before any allocation, and a declared length that overruns the input is refused
(`UnexpectedEnd`). The act payload is bounded by N4-A's `MAX_DEVICE_ACT_PAYLOAD` (65 536 bytes) inside
`DeviceActV1::decode`, and `E` by N1's `MAX_POSITION`. Memory consumed by a decode is therefore
proportional to the bytes the caller chose to read. The number of facts a legitimate Subject can
have through `E` is bounded by the chain, but N1 deliberately leaves adversarial storage pressure
open (O-N8); a cap invented here would be a storage-policy decision this layer does not own. (The
superseded implementation carried such a cap, `MAX_BUNDLE_FACTS`; §11 records why it was removed.)

---

## 5. Strict decode — `EvidenceBundle::decode`

Fail-closed on every defect. In order of detection:

| # | Check | Refusal |
|---|---|---|
| 1 | `LP(BUNDLE_DOMAIN)` present and exact | `Framing(BadDomain)` |
| 2 | version `= BUNDLE_VERSION` | `Framing(UnsupportedVersion)` |
| 3 | `1 ≤ E ≤ MAX_POSITION` | `PositionOutOfRange(E)` |
| 4 | `fact_count` fits the remaining input | `Framing(CountOverflow)` |
| 5 | each fact: stated `event_id` strictly greater than the previous | `FactOrder` |
| 6 | each fact: `LP(canonical_body)` fits; body is a strict N1 body (N1's own decoder, including its trailing-byte check) | `Framing(UnexpectedEnd)` · `Body { source: CodecError }` |
| 7 | each fact: `H(canonical_body) = event_id` | `EventIdMismatch { stated, computed }` |
| 8 | each fact: `subject(body) = S` | `FactSubjectMismatch` |
| 9 | each fact: `position(body) ≤ E` | `FactOutsideScope { position, through: E }` |
| 10 | each fact: `witness_count ≥ 1` and fits the input | `NoWitness` · `Framing(CountOverflow)` |
| 11 | each witness strictly greater than the previous | `WitnessOrder` |
| 12 | `LP(device_act_v1)` fits; act is a strict N4-A act (N4-A's own decoder: domain, version, principal tag, position bound, payload bound, trailing bytes) | `Act(DeviceActError)` |
| 13 | 64 signature bytes present; **input fully consumed** | `Framing(UnexpectedEnd)` · `Framing(TrailingBytes)` |
| 14 | `act.subject = S` | `ActSubjectMismatch` |
| 15 | `act.evaluation_position = E` | `ActPositionMismatch { act, bundle }` |

Any ambiguity that would give one logical bundle two byte identities — a reordered fact, a duplicate
fact, a reordered or duplicate witness, trailing bytes — is a refusal, not a normalization. Every
32-byte and 64-byte field is typed by position in the layout and, for principals inside bodies and
the act, by N1's principal tag; no blob is accepted "because it is the right length".

Decode performs **no** signature verification and **no** derivation.

### 5.1 Inspection — `EvidenceBundle::summarize`

A decoded bundle can be *described*: for each carried fact in canonical order, its index, N1 kind
tag, Subject, position, stated `event_id` and witness count. `summarize` admits nothing and
evaluates nothing. A bundle whose act would be refused summarizes exactly as one whose act would be
accepted. It exists for the `inspect` verb (§9) and for logging; its output is never a verdict.

---

## 6. Re-admission — `EvidenceBundle::admit`

For every carried fact and every witness over it, in canonical order:

```text
AuthorityStore::ingest(SignedAuthorityEvent { body, signature })
```

This is N1's gate, unchanged: `admissible(body, signature)` — canonical round-trip, position bound,
`verify_strict` under the body's inline signer over `LP(SIGNATURE_DOMAIN) || canonical_body`. A
witness that does not verify is refused as `Inadmissible { event_id, source: AdmissionError }`.

`admit` **fails closed on the first refusal**. A partially admitted store is a different Subject
history than the one bundled, and returning one would let a single bad witness silently change a
derived authority state. There is no "skip the bad one" path.

Because `AuthorityBody`'s equality, ordering and hashing are defined over `canonical_bytes()`, the
store rebuilt by `admit` is **equal** to the store the bundle was assembled from, restricted to the
scope — not merely equivalent. The tests assert `==`.

---

## 7. Verification

Not defined here. The consumer calls the existing N4-A verifier:

```text
verify_device_act(bundle.act(), subject, &bundle.admit()?, position)
```

where `subject` and `position` are **the relying party's own**. Every rule of N4-A §7 applies
unchanged, including that `E` is the position the relying party evaluates at. The bundle's header
`(S, E)` was required to agree with the act at decode (§5, rows 14–15); the relying party's `(S, E)`
is then checked by N4-A itself: a disagreement is `ActSubjectMismatch` or
`EvaluationPositionMismatch`, a refusal and never a fallback. For a stateless consumer that chooses
to rely on the bundler's selection (§3.3), its own `(S, E)` **is** the bundle's. A consumer that
holds its own facts should ingest the bundle's facts into its own store (the join is set union;
order is irrelevant) and evaluate at its own `E` — and if the question is "is this act valid
*now*?", that `E` is the one N4-E computes from its own retained clean tip (`N4E_CURRENT_ADMISSION.md`),
never one the device chose.

### 7.1 The thin composition — `verify_evidence_bundle`

```text
verify_evidence_bundle(bundle, subject, position)
    := verify_device_act(bundle.act(), subject, &bundle.admit()?, position)

verify_evidence_bundle_bytes(bytes, subject, position)
    := verify_evidence_bundle(&EvidenceBundle::decode(bytes)?, subject, position)
```

These are API convenience and nothing more. They make **no** decision of their own: no scope check
beyond what decode already enforced, no selection, no fallback, no position taken from the bytes.
`EvidenceVerifyError` keeps the two sources apart — `Bundle(BundleError)` for a decode or admission
refusal, `Act(DeviceActVerifyError)` for the N4-A verdict — so a consumer can say which of the
three claims in §2 failed. The tests pin that the wrapper's result equals the direct call's, verdict
for verdict.

---

## 8. Test vector

Inputs, stated in full. These are the `authority_log_support::subject(0x31, 4)` fixture and two
stranger device keys, written out as raw data so no test helper is needed to reproduce them.

| Input | Value |
|---|---|
| Subject root secret | `[0x31; 32]` |
| Subject context nonce | `[0x94; 32]` (`0x31 ^ 0xa5`) |
| establishment plan | 4 generations, all `Rotate` |
| device A seed | `[0x41; 32]` |
| device B seed | `[0x42; 32]` |
| facts | inception @0; `Authorize(A, {Sign}, no span)` @1 by `A_0`; `Authorize(B, {Sign}, no span)` @2; `Revoke(A)` @3 — each with the one RFC 8032 witness by `A_0` |
| act | `DeviceActV1 { subject: S, device: B, capability: Sign, evaluation_position: 3, payload: ASCII "fixture:open-workspace" }`, signed by B |

Derived (hex):

| Value | Bytes |
|---|---|
| `A_0` public key | `216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a` |
| device A public key | `db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d` |
| device B public key | `2152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12` |
| **`SubjectId`** `= H(inception)` | `a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d` |
| fact order (ascending `event_id`) | `1bd0cea4…` (authorize A, pos 1) · `9791575d…` (authorize B, pos 2) · `a0068f2e…` (**inception, pos 0**) · `b22bf1bc…` (revoke A, pos 3) |
| `device_act_v1` (123 B) | `0000001169636e2e6e342e6465766963652d6163740001a0068f2e…db12 0000000000000003 01 00000016 666978747572653a6f70656e2d776f726b7370616365` |
| `evidence_bundle_v1` | 1 335 bytes; pinned in full as `EXPECT_BUNDLE_BYTES_HEX` in `tests/evidence_bundle.rs` |
| **`bundle_id`** | `009fb4ce160121684041689c82bc4d01563dc6e852d5c9390c8aaa31a658deaf` |

Every one of these values is reproduced by an **independent reference written from this document,
the N1 contract and the N4-A contract** (Python, sharing no code, library or constant with
`icn-identity`), which derives the N1 bodies, the witnesses, the act, the container and its id from
the seeds above, applies its own strict decoder to its own bytes, and then reads the `EXPECT_*`
literals out of the Rust test purely as a comparison target:

```bash
python3 icn/crates/icn-identity/tests/reference/evidence_bundle_reference.py
```

It exits non-zero on any disagreement, and additionally demonstrates from the prose that a trailing
byte, a truncation, a reordered fact, a stated digest that is not the body's, a fact above `E` and
an act whose position disagrees with the bundle are each refused.

### 8.1 Behaviour pinned by the test suite

- byte-exact round trip; `decode(encode(b)) == b`; `admit` rebuilds a store **equal** to the source;
- identical bytes and `bundle_id` under all `4!` ingest orders of the four facts and under every
  split-and-join of each order;
- two valid witnesses over one body survive as two witnesses of one body; the rebuilt store has
  4 bodies and 5 witnesses; witness arrival order does not change the bytes; the verifier is unmoved;
- a stated `event_id` that is not the body digest, a body swapped under another fact's digest, and a
  body with one flipped nonce byte are each refused `EventIdMismatch`;
- a body with a wrong version, an unknown kind, an internal trailing byte, or act bytes in a body
  slot is refused with N1's own `CodecError`;
- an N1 body in the act slot, an act at position 0, a `SubjectId` in the act's device slot and a
  truncated act are refused with N4-A's own `DeviceActError`;
- every strict prefix of the bytes is refused; one trailing byte is `TrailingBytes(1)`;
- a `u32::MAX` fact count, a `u32::MAX` witness count and an overrunning body length are refused
  before allocation; `E = 0` and `E = MAX_POSITION + 1` are refused; a 65 537-byte act payload is
  refused by N4-A's bound;
- reordered facts, a duplicate fact, descending witnesses, a duplicate witness and an empty witness
  list are each refused;
- a fact for another Subject, a header Subject disagreeing with the act, and a header position
  disagreeing with the act are each refused;
- a flipped witness bit and a stranger's signature over the right body decode cleanly and are refused
  at **admission** with `AdmissionError::SignatureInvalid`;
- the verifier reaches the **same** result after transport as on the original store for: B accepted
  at 3; A refused `DeviceNotAuthorized` at 3; an unknown Subject refused `SubjectUnknown`; a fork at 2
  refused `AuthorityHalted { disputed_at: 2 }` with both candidates carried; a gap at 2 refused
  `PrefixIncomplete { frontier: 2, required: 3 }` with the orphan carried;
- A at `E = 2` verifies from a 3-fact bundle that does not contain the position-3 revoke; splicing the
  revoke in is refused `FactOutsideScope`; A at `E = 3` is refused because that bundle carries it;
- for every byte index and two bit masks, the mutated bytes have a different `bundle_id` and either
  fail to decode, fail to admit, or are refused by the verifier — **no single-byte mutation yields
  evidence**;
- bundle bytes are not an N1 body and not an act, and neither is a bundle; the six domain separators
  are pairwise distinct;
- `evidence_bundle.rs` references no clock, no randomness, no hash map, and calls none of
  `supersede`, `resolve`, `derive`, `derive_prefix`.

### 8.2 Behaviour ported from the superseded suite

Pinned by `DeviceAuthorityBundleV1`'s tests and not by the donor's; restated over this container:

- a bundler that drops the revoke at `E = 3` and still claims `E = 3` yields
  `PrefixIncomplete { frontier: 3, required: 3 }`, never a still-authorized A (withholding cannot
  extend authority);
- `verify_evidence_bundle` and `verify_evidence_bundle_bytes` return exactly the direct verifier's
  evidence or refusal, and surface an inadmissible witness as `Bundle(Inadmissible)` before any
  verdict;
- a relying party that supplies another Subject or another position is refused by N4-A
  (`ActSubjectMismatch`, `EvaluationPositionMismatch`), not by the container and not by a fallback;
- `BUNDLE_DOMAIN` is distinct from the N4-C, N4-D and GEN separators;
- `summarize` lists the four facts in canonical order with the right kinds, positions and witness
  counts for a bundle whose act would be refused.

---

## 9. `icnctl device-authority`

Stateless. Reads the bundle file and nothing else: no data directory is opened, no daemon or
network is contacted, no N2-A gate applies (nothing persistent is touched). stdout is one JSON
document; the exit code is the verdict class. The verb is a mechanical consumer of §5–§7:

```text
bytes -> EvidenceBundle::decode -> admit (every pair through AuthorityStore::ingest)
      -> verify_device_act at the --subject and --position the caller states
      -> one JSON document; exit code = verdict class
```

```text
icnctl device-authority verify  --bundle FILE --subject HEX64 --position N
icnctl device-authority inspect --bundle FILE
```

| exit | `verdict` | when |
|---|---|---|
| 0 | `authorized` | the act is covered; the document carries the N4-A evidence |
| 1 | `refused` | all facts admissible and N4-A refused (`class` = one of the N4-A refusal names: `ActSubjectMismatch`, `EvaluationPositionMismatch`, `BadSignature`, `PositionOutOfRange`, `SubjectUnknown`, `AuthorityHalted`, `PrefixIncomplete`, `DeviceIsEstablishmentAuthority`, `DeviceNotAuthorized`, `GrantNotInForce`, `CapabilityNotGranted`), **or** one fact was inadmissible (`class` = `InadmissibleFact`, with the fact's `event_id`) |
| 2 | `malformed` | not a strict canonical bundle, unreadable file, or an unusable `--subject` |

`authorized` document:

```json
{
  "verdict": "authorized",
  "subject": "<hex32>", "device": "<hex32>", "device_did": "did:icn:…",
  "capability": "Sign", "evaluation_position": 3,
  "grant": { "capabilities": ["Sign", "Present"], "validity": null, "granted_at": 2 },
  "generation": 0,
  "act_id": "<hex32>", "bundle_id": "<hex32>", "fact_count": 4
}
```

`inspect` prints the bundle's own scope (`subject`, `evaluation_position`), each fact (`index`,
`kind_tag`, `subject`, `position`, `event_id`, `witness_count`) and the act (`subject`, `device`,
`device_did`, `capability`, `evaluation_position`, `payload_len`, `act_id`) and **decides
nothing**: a bundle whose act would be refused inspects with exit 0.

The verb MUST NOT, and does not: choose `E`; fetch missing facts; query `icnd`; repair malformed
evidence; normalize a fork; invent freshness or currentness; fall back on any failure.
`--subject` and `--position` are mandatory and are never read from the bundle. A caller that takes
them from the bundle has made itself the signer's relying party, which is the one thing N4
invariant 6 forbids. (A caller that wants "valid *now*" computes its position with N4-E over its own
facts and passes that.)

---

## 10. What a downstream consumer MAY rely on / MUST NOT do

Everything in `N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.3, plus:

**MAY:** carry a bundle over any transport; store a bundle as the record of what was evaluated;
treat `bundle_id` as the identity of *what was carried* (it is witness-dependent, unlike `act_id`);
run `icnctl device-authority verify` as its relying party and gate on exit code and `class`;
re-verify at a later `position` once it has retained more facts, by assembling a new bundle.

**MUST NOT:** take `subject` or `position` from the bundle; cache a verdict across positions;
treat `inspect` output as a verdict; treat exit 1 with `InadmissibleFact` as "some facts were
fine"; reimplement the framing, the admission rules or the fold in another language (the Python
file is an *audit*, not a client); put anything in a bundle that is not an N1 fact or the act;
reintroduce a second container format or a compatibility decoder for the superseded one.

---

## 11. Convergence record (2026-10-04)

Two implementations of this boundary were developed independently from the same base
(`eee1dca63`): `DeviceAuthorityBundleV1` (`icn.n4.device-authority-bundle`, this branch at
`b5df9d93b`, with the `icnctl` verb) and `EvidenceBundle` (`icn.n4.evidence-bundle`,
`task/n4a-portable-evidence-bundle` at `8962f17a5`/`ae0a0e1ee`, with the independent reference).
The requirement was one canonical byte representation. They were compared directly against
`IDENTITY_SEMANTICS.md`, N4-A, the N1 encoding/admission/store/fold and #2599/#2694, and
`EvidenceBundle` was selected. The differences, and why each resolved the way it did:

| Dimension | `DeviceAuthorityBundleV1` (removed) | `EvidenceBundle` (canonical) | Why it mattered |
|---|---|---|---|
| Evidence scope | any `(body, witness)` pair for any Subject at any position; pruning left to the verifier | exactly `{ b ∈ Bodies(S) : position(b) ≤ E }` with every witness; `FactSubjectMismatch` / `FactOutsideScope` at decode | §3.2: the scope is the `derive_prefix` input set; enforcing it makes "same evidence, same bytes" a property of the format and keeps a fact above `E` out of a proof about `E` |
| Header | none | `b32(S) ‖ u64be(E)`, agreeing with the act | the bundle names what it is evidence *for*; a consumer can state its scope before admitting anything |
| Body/witness representation | flat record per witness: `event_id ‖ σ ‖ LP(body)`; a re-signed body repeated its bytes | one body once with all its witnesses, `≥ 1` | §4.4: one encoding per body; no two records for one digest carrying different bytes |
| Ordering / duplicates | strict by `(event_id, σ)`; duplicates refused | strict by `event_id`, witnesses strict bytewise; duplicates refused | equivalent strictness; the grouped form is the one the header and scope rule need |
| `event_id` stated and checked | yes | yes | equivalent |
| Bound | `MAX_BUNDLE_FACTS = 2^16`, invented | none beyond `Reader::count`, N4-A's payload bound and N1's `MAX_POSITION` | §4.6: traces to no governing contract; a cap is a storage-policy decision this layer does not own |
| Verification wrapper | `verify_bundle` = `store_from_bundle` → `verify_device_act` | `verify_evidence_bundle` = `admit` → `verify_device_act` (ported) | pure composition in both; kept as a thin API, documented as such, not as a layer |
| Reference | zero-fact framing vector + the act vector | full 4-fact vector derived from seeds, plus an independent strict decoder | the stronger cross-implementation evidence |
| CLI | `icnctl device-authority verify|inspect` | none | ported, retargeted mechanically (§9) |

Disposition: `device_authority_bundle.rs`, its test file, its Python reference, the domain
`icn.n4.device-authority-bundle`, `MAX_BUNDLE_FACTS`, `store_from_bundle`, `verify_bundle` and
`BundleVerifyError` are removed. `verify_bundle`'s composition survives as `verify_evidence_bundle`;
its `summarize` survives on `EvidenceBundle`; its tests that the donor suite did not already cover
are ported (§8.2); its `icnctl` verb is retargeted. N4-C, the two-device lifecycle acceptance and
N4-D assemble their bundles from a store instead of from a fact list; nothing they assert changed.
N4-E never depended on the container. No compatibility decoding was added: neither format had
deployed or persisted bytes. The standalone portable-evidence branch is superseded by this one.

---

## 12. What changed in N1 and N4-A

**Nothing semantic.** The only edit outside the module is two `pub(crate)` helpers, `Writer::b64`
and `Reader::b64`, in `authority_log/encoding.rs` (identical on both branches), so a 64-byte
signature is framed with N1's own reader and writer rather than a second implementation of
fixed-width reads. No wire format, no constant, no admission rule, no fold rule and no verifier
rule changed. `DeviceActV1` still has no serde and no wire encoding beyond its canonical bytes; the
bundle carries those bytes verbatim.

---

## 13. What remains open

- **Where a client gets the facts.** This document assumes the bundler holds them. The N1-D fact
  store (#2800) is how a replica persists them; replication between replicas is N3 (#2598); how a
  *device* first receives its Subject's facts is part of the N4 enrollment ceremony (#2599,
  `N4C_DEVICE_ENROLLMENT_REQUEST.md` step 4, unbuilt).
- **Which position a relying party should pin** for a deferred (class 1) decision: G1-A, #2694 §6.
  For a class-2 act admitted now, N4-E computes it from the relying party's own facts.
- **Late fork discovery and omitted facts** (§3.3): O-N7 and N3, not this container.
- A gateway route as a hosted relying party (N4-A §9.2 item 4) — deliberately unbuilt.

---

## 14. Deliberate scope boundaries

Not in this slice, by design: `icnd` wiring; a gateway route; SDK bindings; persistence or any
coupling to N1-D's trait (N1-D is a sibling whose record *concept* is shared, §4.4); N3 replication;
recovery; rotation or replacement lifecycle; GEN subject-context work; Home enrollment transport; Pi
or network-ops changes; governance voting; institution or member recognition; any real identity
material.

The format has one version. A future version is a new `BUNDLE_VERSION` with its own decoder branch;
v1 bytes never change meaning.
