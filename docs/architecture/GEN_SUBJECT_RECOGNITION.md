---
Status: normative
Canonical: no
Last Reviewed: 2026-11-05
---

# GEN-B — institutional Subject recognition (`SubjectRecognitionV1`)

**Slice:** rung 3 of the [#2694](https://github.com/InterCooperative-Network/icn/issues/2694) semantic
convergence ladder · **Protocol owner:** #2602 (GEN) · **Builds on:**
[GEN-A](GEN_SUBJECT_CONTEXT_GENESIS.md) (#2695) ·
**Code:** `icn/crates/icn-governance/src/subject_recognition.rs` ·
**Vectors:** `icn/crates/icn-governance/tests/subject_recognition.rs` ·
**Reference:** `icn/crates/icn-governance/tests/reference/subject_recognition_reference.py`

This document is the **byte-level contract**. An implementation in another language can reproduce
every value in §9 from this text and the published GEN-A §8 values alone.

---

## 1. The question this answers

> A governance domain wants to record that one specific context-scoped human Subject is
> recognised at standing `Member`, on a stated basis. What is the immutable, deterministic fact?

It answers that and nothing else. It is the second link after GEN-A in the chain
*Subject genesis → delegated device → **institutional recognition** → membership → …* and does
not implement any later link.

## 2. What a recognition proves — and what it does not

A verified `SubjectRecognitionV1` proves exactly this:

> The holder of a GEN-A genesis bundle that verifies for **this exact governance-domain id**
> names a Subject; a recognition of that Subject at `Member` under basis kind *K* with basis hash
> *H* is committed by `recognition_hash`.

It **does not** establish: that any institution actually decided to admit anyone; membership
legitimacy; voting eligibility; economic authority or settlement capacity; that any device is
authorized **now** (GEN-A §2.1: genesis is historical); delegation or session authorization;
custody of, or control over, the Subject's continuity material; or production readiness.

A recognition is not a vote, a permission grant, a session token or an authorization decision.
Later facts may *reference* `recognition_hash`; none is entailed by it.

## 3. Rehearsal-policy boundary (case A, not case B)

Two different recognition questions exist. This slice implements only the first.

| | |
|---|---|
| **A. Fresh Alpha Subject** | A new Subject incepts via GEN-A; a separately governed domain records recognition under a declared rehearsal basis. **Implemented.** |
| **B. Legacy-member migration** | A legacy DID, continuity evidence and institutional records are bridged to a Subject (FACT A / FACT B evidence, #2626). **Not implemented; no field, tag or API here anticipates it.** |

The only basis in v1, `BootstrapRehearsalV1` (`0x01`), is **non-production**. It is bounded to the
semantic rehearsal profile: it states "this Subject's genesis verifies for this domain" and
nothing about who authorised admission. It must not be confused with a production institution's
properly authorized decision. `RecognitionBasisKind::is_production_grade()` returns `false` for
it, and a consumer that must refuse rehearsal recognition for real membership should branch on
that, not on the variant. Later governance / credential / federated bases are separate work and
need a new basis kind and tag, never a reinterpretation of `0x01`.

## 4. Constants (frozen for v1)

| Name | Value |
|---|---|
| `SUBJECT_RECOGNITION_TAG` | ASCII `icn:gov:subject-recognition:v1` (30 bytes) |
| `BOOTSTRAP_REHEARSAL_BASIS_TAG` | ASCII `icn:gov:recognition-basis:bootstrap-rehearsal:v1` (48 bytes) |
| `standing_class` `Member` | `0x01` (`u8`) |
| `recognition_basis_kind` `BootstrapRehearsalV1` | `0x01` (`u8`) |

Hash function: **BLAKE3**, field-wise, per [`CANONICAL_ENCODING.md`](CANONICAL_ENCODING.md) §4.1.3,
implemented by `icn_kernel_api::canonical::CanonicalHasher`. Both tags are in the `icn:gov:`
namespace, are registered in the workspace domain-separation audit
(`icn/crates/icn-kernel-api/tests/domain_separation_audit.rs`), are prefix-free with respect to
each other and to every other registered tag, and do not collide with the GEN-A SHA-256 domains
(`icn.gen.*`, length-first framing, different primitive).

Encodings used below: `LE64(x)` = `u64` little-endian; `STR(x)` = `LE64(len(x)) || x`; `b32(x)` =
32 raw bytes with no prefix; `u8(x)` = one byte.

## 5. Fields

Committed, in this order:

1. `domain_id` — the **exact UTF-8 bytes** of the existing `GovernanceDomainId.0`. No case folding,
   no Unicode normalization (NFC and NFD spellings are different domains), no trimming, no display
   name. Empty is refused.
2. `SubjectId` — 32 raw bytes: the N1 inception event id derived by GEN-A verification. Never a
   device Principal, never a legacy DID string.
3. `standing_class` — `u8`, `0x01` = `Member`. Any other value is refused where it is parsed.
4. `recognition_basis_kind` — `u8`, `0x01` = `BootstrapRehearsalV1`. Any other value is refused
   where it is parsed.
5. `recognition_basis_hash` — 32 bytes, produced by the basis-specific derivation in §6.1.

No timestamp, counter, nonce, JSON, or implicit ordering enters any preimage.

## 6. Derivations

### 6.1 Bootstrap-rehearsal basis hash

```text
basis_preimage :=
      BOOTSTRAP_REHEARSAL_BASIS_TAG
   || STR(domain_id_utf8)
   || b32(SubjectId)
   || b32(subject_context_ref)            -- GEN-A §6.4

recognition_basis_hash := BLAKE3(basis_preimage)
```

The basis commits the domain, the Subject and the GEN-A `subject_context_ref`, so it names *which
context-bound inception* it rests on. It therefore cannot be replayed for another domain, another
Subject, or another incarnation of the same context, and it is independent of the device grant
(replacing or revoking a device does not change it — `subject_context_ref` excludes the device).

### 6.2 Recognition hash

```text
recognition_preimage :=
      SUBJECT_RECOGNITION_TAG
   || STR(domain_id_utf8)
   || b32(SubjectId)
   || u8(standing_class)
   || u8(recognition_basis_kind)
   || b32(recognition_basis_hash)

recognition_hash := BLAKE3(recognition_preimage)
```

Every variable-length field is length-prefixed; every other field has a fixed width. The two
tags make the two hashes domain-separated even over identical field bytes.

## 7. Construction and verification

`SubjectRecognitionV1::recognize_bootstrap_rehearsal(domain, genesis)` takes the GEN-A
`SubjectContextGenesisV1` **bundle**, not the `VerifiedSubjectContext` result. The result type
omits the context id and has public fields, so a value of it cannot prove which domain it was
verified for. The constructor instead:

1. refuses an empty `domain_id`;
2. calls `verify_subject_context_genesis_v1(genesis, GovernanceDomainV1, domain_id_bytes)`, which
   enforces kind, a byte-exact context id, nonce recomputation, N1 admission, the bootstrap
   separation invariant and the derived live authority view (GEN-A §7);
3. takes the Subject **from that verification** — there is no parameter through which a caller
   supplies a Subject, Principal or legacy DID;
4. derives §6.1 and §6.2.

Any refusal is final; there is no partial success. `verify_against_genesis(&self, genesis)`
re-derives the recognition from the recognition's own domain and requires equality in every
committed field (`Mismatch` otherwise). Fields are private; the type has no serde or wire form in
v1, so no non-canonical byte representation can become its identity.

Refusals: `EmptyDomain`, `Genesis(GenesisVerifyError)` (wrong domain, bad version, tampered salt,
bad witness, …), `UnsupportedStandingClass(u8)`, `UnsupportedBasisKind(u8)`, `Mismatch`.

## 8. Privacy, correlation and compatibility

- Recognition adds **no** global Person identifier and no cross-context link. It is scoped to one
  `(domain, SubjectId)` pair; the SubjectId is already context-scoped by GEN-A.
- It reveals nothing a holder of the GEN-A bundle did not already have (the domain and the
  Subject); `recognition_hash` is a commitment, not a new identifier for a person.
- It confers no custody of continuity material and no control over the Subject.
- A new standing class, basis kind, or any field change requires a **new tag version** (Rule 2 of
  CANONICAL_ENCODING §4.1.3); the bytes under `…:v1` never change. `0x01` is never reinterpreted.
- Unchanged by this slice: N1 wire formats, GEN-A hashes, every existing canonical domain and
  golden vector.

## 9. Test vectors

Inputs: domain `coop.example.governance`; Subject and `subject_context_ref` are the published GEN-A
§8 values.

| Input | Value |
|---|---|
| `domain_id` | `coop.example.governance` (23 ASCII bytes) |
| `SubjectId` | `3b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a` |
| `subject_context_ref` | `5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86` |

| Output | Bytes |
|---|---|
| `basis_preimage` (143 B) | `69636e3a676f763a7265636f676e6974696f6e2d62617369733a626f6f7473747261702d72656865617273616c3a76311700000000000000636f6f702e6578616d706c652e676f7665726e616e63653b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86` |
| **`recognition_basis_hash`** | `327aab543a13a4bda082ff2e921919b53e4b95b6c5ca4db965e5cde0a6dd252e` |
| `recognition_preimage` (127 B) | `69636e3a676f763a7375626a6563742d7265636f676e6974696f6e3a76311700000000000000636f6f702e6578616d706c652e676f7665726e616e63653b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a0101327aab543a13a4bda082ff2e921919b53e4b95b6c5ca4db965e5cde0a6dd252e` |
| **`recognition_hash`** | `916ce2f5420ef322d570f07d76e57abce8085fde05cb66a46b2a0d2485b3139f` |

Second vector (same Subject and context ref, domain `coop.example.other`, a pure hash-layout
vector — no genesis exists for that pairing): basis hash
`a9676bd71551d9d26bcaf4a20e3ba79b902b6c0658c4af390087243726fe8b75`, recognition hash
`5e7bc9db8ef7246acd3e3d569ce14d885ee9e2a2bd255515bae8b06383d2c916`.

The values were produced by `subject_recognition_reference.py`, written from this document with a
minimal single-chunk BLAKE3, sharing no code with Rust. Because the two preimages are 143 B and
127 B (three and two 64-byte blocks), the script self-checks its BLAKE3 against the published
empty-input hash **and** ten multi-block digests (lengths 1–1024, including 63/64/65, 127/128/129
and 143) before deriving anything; those digests agree with the Rust `blake3` crate at every
length 0–1024. The script reads the `EXPECT_*` literals in the Rust test only as a comparison target and
exits non-zero on disagreement:

```bash
python3 -I icn/crates/icn-governance/tests/reference/subject_recognition_reference.py
```

**What the vectors validate:** the tags, field order, framing and widths of §6, and that the Rust
`CanonicalHasher` composition equals an independent byte-level derivation. The Rust test also
rebuilds both preimages by hand and hashes them with the raw BLAKE3 crate. **What they do not
validate:** GEN-A itself (pinned by its own vectors), any institutional decision, membership
legitimacy, or any later slice.

## 10. Negative cases covered

Wrong domain; empty domain; case, whitespace, NUL, trailing-delimiter and NFC/NFD alias spellings;
another Subject's genesis; a genesis for another domain; unsupported genesis version; mutated salt;
unsupported standing and basis tags; mutation of domain, Subject and basis hash; device
independence (a different initial device yields the identical recognition); a device Principal's
bytes forced into a Subject; a second context incarnation (different salt) giving a different Subject that does not cross-verify; tag prefix-freeness
and separation; rehearsal basis is never production-grade. Length-prefix framing is pinned by the hand-built preimage test and the
independent reference, not by a dedicated negative test. Type-level: a Subject is not a
`PrincipalKey`, `SubjectRecognitionV1` has private fields and no Subject parameter, and the basis
hash type has no public constructor.

## 11. Deliberate scope boundaries

Not in this slice: legacy-member migration (#2626); domain actor-model firewall; membership roster;
`VotingProcessSnapshotV1`; proposal commitment; vote or decision records; settlement; persistence,
gateway routes, wire/serde representation, session claims; production recognition bases.

The next rung of #2694 consumes `recognition_hash` per eligible Subject in the frozen process
snapshot.
