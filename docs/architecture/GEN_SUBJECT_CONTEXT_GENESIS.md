---
Status: normative
Canonical: no
Last Reviewed: 2026-09-15
---

# GEN-A — context-scoped Subject genesis (`SubjectContextGenesisV1`)

**Implements:** [icn#2695](https://github.com/InterCooperative-Network/icn/issues/2695) ·
**Parent:** #2694 semantic convergence, slice 1 of 12 ·
**Code:** `icn/crates/icn-identity/src/subject_context.rs` ·
**Vectors:** `icn/crates/icn-identity/tests/gen_subject_context.rs`

This document is the **byte-level contract**. It is written so that an implementation in another
language can reproduce every value in §8 from this text alone.

---

## 1. The question this answers

> What does it mean for a human Subject to exist **as this Subject, in this ICN context**, before
> any institution recognizes, enrols, authorizes or governs them?

GEN-A is an **outer binding protocol over N1**. It derives the 32 opaque bytes N1 already accepts
as its `ContextNonce`, hands them to N1 unchanged, and defines two witness-independent reference
hashes that later semantic facts can name. **No N1 wire format, canonical body, signature
preimage or digest rule is altered or extended.**

## 2. What a verified genesis proves — and what it does not

A verified `SubjectContextGenesisV1` proves exactly this:

> A fresh N1 Subject was incepted for one explicitly named governance-domain context, with one
> narrow initial device authorization to a principal **distinct from the Subject's establishment
> authority**, under existing N1 authority-log semantics, without any registrar.

It **does not** establish, imply or contribute to: institutional recognition; membership or
standing; governance participation or voting rights; economic authority or settlement capacity;
delegation or session authorization; device authority beyond the single `{Sign, Present}` grant
it carries; guardian recovery; replication, finality or durability; production readiness.

Each of those is a separate, later fact. A later fact may **reference** `subject_context_ref`;
none of them is entailed by it.

### 2.1 Genesis is a historical fact, not a current-authority statement

Verification folds a **fresh** `AuthorityStore` containing only the two genesis events. That is
deliberate — genesis is a closed historical fact — but it has a consequence a relying party must
not get wrong:

> A verified bundle says the device **was** authorized at position 1. It does **not** say the
> device is authorized **now**.

Any later `Revoke` or `Rotate` event is invisible to this verifier, because it is not in the
bundle. A caller that needs current device authority MUST consult the Subject's live authority
log via N1's `derive`, and MUST NOT substitute a verified genesis bundle for that check.

There is **no registrar**. Nothing in this protocol consults a service, directory or database
row. A Subject is established by the person's own client from material the person holds. A
governance domain is named as a *context*, never as an issuer or an owner: the institution does
not create the Subject, and the Subject exists whether or not any institution ever recognizes it.

## 3. Primitives

Reused from N1 (`icn/crates/icn-identity/src/authority_log/`), not redefined:

```text
LP(x)  := u32be(len(x)) || x          -- length-prefixed byte string
b32(x) := 32 raw bytes, no prefix
u8/u16be/u32be/u64be                  -- big-endian, fixed width
H(x)   := SHA-256(x)
```

## 4. Constants (frozen for v1)

| Name | Value |
|---|---|
| `GEN_CONTEXT_DOMAIN` | ASCII `icn.gen.subject-context` (23 bytes) |
| `GEN_CONTEXT_VERSION` | `1` (`u16be`) |
| `SUBJECT_CONTEXT_REF_DOMAIN` | ASCII `icn.gen.subject-context-ref` (27 bytes) |
| `SUBJECT_CONTEXT_REF_VERSION` | `1` (`u16be`) |
| `INITIAL_DEVICE_BINDING_REF_DOMAIN` | ASCII `icn.gen.initial-device-binding-ref` (34 bytes) |
| `INITIAL_DEVICE_BINDING_REF_VERSION` | `1` (`u16be`) |
| `SUBJECT_CONTEXT_GENESIS_VERSION` | `1` (`u16`) |
| context kind `GovernanceDomainV1` | `0x01` (`u8`) |

All three GEN domains are length-prefixed and pairwise distinct from every `authority_log`
separator (`icn.authority-log`, `.sig`, `.commit`, `.kdf`), so a GEN preimage can never be
reinterpreted as an N1 body, signature, commitment or KDF preimage.

## 5. Context

Exactly one context kind is defined in v1:

```text
0x01 = GovernanceDomainV1
```

`context_id` is the **exact UTF-8 byte sequence** of the existing `GovernanceDomainId.0` string.
No case folding, no Unicode normalization, no UUID canonicalization, no display-name
substitution. An **empty `context_id` is rejected** at both construction and verification, even
though the legacy `GovernanceDomainId` newtype does not itself enforce that.

This is not a claim that a governance domain is ICN's final institution or entity identifier. It
is the concrete runtime decision space ICN has today. The kind tag is inside every preimage, so a
later GEN version may define further kinds without disturbing this one.

> **Implementation note.** `icn-identity` is a **kernel**-class crate under
> `scripts/firewall-taxonomy.toml`; `icn-governance`, which owns `GovernanceDomainId`, is a
> **domain** crate that already depends on `icn-identity`. Importing the type would be both a
> dependency cycle and a meaning-firewall violation, so the context id crosses the boundary as
> validated bytes. The byte contract above is unchanged; the governance-side caller passes
> `domain.0`.

### 5.1 `context_salt`

32 bytes drawn from a CSPRNG, once, per (Subject, context). It is **public** — it travels in the
genesis evidence — but it is the *entire* entropy source of the derived nonce. It must be
retained in the person's private subject index and protected recovery material, and it must never
be treated or indexed as a global identifier.

### 5.2 Why the salt is mandatory — the invariant I4b argument

`IDENTITY_SEMANTICS.md` §10 / invariant **I4b** requires a `ContextNonce` to be generated
independently with CSPRNG randomness per context, **prohibits deterministic derivation from a
globally stable public value**, and admits an alternative generation method only with an
*explicit unlinkability argument, stated and reviewed*. This is that argument.

1. The banned construction is `nonce = H(context_id)`. A context id is globally stable and
   public, so every human in one governance domain would publish the **same** nonce in their
   inception body — a free cross-subject grouping handle for anyone who sees two inception
   bodies.
2. `context_salt` is a fresh 32-byte CSPRNG draw per (Subject, context), and is the only varying
   input.
3. With the public prefix fixed, `salt |-> H(prefix || salt)` over a uniform 32-byte salt is
   computationally indistinguishable from a uniform 32-byte draw. Two Subjects in one context
   therefore publish independent, unlinkable nonces — exactly the property the direct-draw rule
   exists to guarantee.
4. Deriving rather than drawing buys one thing a direct draw cannot: a client that retained its
   salt can **reproduce** the nonce after a restart, without storing the nonce separately and
   without weakening (3).
5. An observer holding only an inception body sees the nonce alone. Recovering `context_id` from
   it requires guessing the 32-byte salt, so the nonce still names nothing and leaks no context.

"Never reused" holds in the sense the rule means: a distinct intended Subject or a distinct
context draws a distinct salt and therefore a distinct nonce. Reproducing the *same* nonce for
the *same* intended Subject during recovery is the goal, not a violation.

### 5.3 One Subject per context is a client invariant

There is no registrar, so no protocol oracle can prove a person did not deliberately create a
second Subject with a second salt in the same context. Clients **MUST** refuse a second fresh
genesis for an already-indexed context unless a separately specified recovery or migration flow
authorizes it. **Global uniqueness is not claimed and must not be inferred.**

## 6. Derivations

### 6.1 Context nonce

```text
context_preimage :=
      LP(GEN_CONTEXT_DOMAIN)
   || u16be(GEN_CONTEXT_VERSION)
   || u8(context_kind)
   || LP(context_id_utf8)
   || b32(context_salt)

context_nonce := H(context_preimage)          -- 32 bytes, handed to N1 unchanged
```

### 6.2 N1 inception and the Subject

The nonce is given to an N1 `ContinuityRoot` built with a caller-chosen continuity secret and
establishment plan. N1's own rules then produce the Subject:

```text
SubjectId = event_id(inception body) = H(canonical inception body)
```

GEN-A does not invent an identifier and does not redesign the establishment plan.

**Determinism requires the whole continuity configuration.** The inception body commits
`next_commitment`, which is folded backwards from the plan's horizon. So the retry/recovery
invariant is stronger than "same salt":

```text
same continuity secret + same plan/horizon + same context descriptor + same salt
    -> same context_nonce -> same canonical inception body -> same SubjectId
```

A client that reconstructs a *different* plan has **not** reconstructed the same Subject and must
not claim continuity. The Alpha execution profile must pin the exact plan/horizon it uses, and
protected recovery material must retain enough to rebuild it. N7 still owns the general recovery
protocol.

### 6.3 Initial device authorization

Exactly one N1 `Authorize` event, signed by the generation-0 authority:

```text
subject      = derived SubjectId
position     = 1
prev_digest  = inception event_id
signer       = N1 generation-0 authority Principal
device       = an independently generated device Principal
capabilities = { Sign, Present }          -- exactly; tags 0x01, 0x03
validity     = None
```

`Encrypt` is not granted because nothing in this profile addresses content to the Subject yet.
`Recover` is not granted because an app-layer label named `Recover` must never be mistaken for N1
establishment authority.

The key separation runs in **both** directions. The device generates its own key and never
receives the continuity or root authority key — and genesis never receives the device's signing
key either. The authorize event is signed by the Subject's generation-0 authority, so only the
device's **public** Principal is required, and that is all the construction API accepts.

#### The bootstrap separation invariant

```text
initial_device_principal MUST NOT be a member of the inception body's initial_authority set
```

That set is the generation-0 establishment authority — in N1's terms, the `PrincipalSet` carried
as `initial_authority` in the `Inception` body, whose sole member is also the inception `signer`.

A capability set attenuates what a principal may *do*; it cannot attenuate what that principal
already *is*. If the device principal is the establishment authority, the narrow
`{Sign, Present}` credential is simultaneously the authority-log writer key, and whoever holds it
can mint arbitrary authorize and revoke events — the grant's narrowness is illusory.

**This is where GEN-A is deliberately stricter than N1.** Such a history is entirely valid N1:
the authorize event is admissible, its signer genuinely holds authority, and N1's fold yields a
live device grant. Nothing in N1 objects. The Alpha profile refuses it anyway.

```text
N1-valid authority history  ≠  valid GEN-A profile
```

Scope: this is the GEN-A **bootstrap** rule, about the position-1 genesis grant. It is not a
universal claim about every future device, nor about whether some later profile might authorize a
principal that holds establishment authority under different semantics.

### 6.4 Semantic references (witness-independent)

```text
subject_context_ref_preimage :=
      LP(SUBJECT_CONTEXT_REF_DOMAIN)
   || u16be(SUBJECT_CONTEXT_REF_VERSION)
   || u8(context_kind)
   || LP(context_id_utf8)
   || b32(context_salt)
   || b32(inception_event_id)

subject_context_ref := H(subject_context_ref_preimage)

initial_device_binding_ref_preimage :=
      LP(INITIAL_DEVICE_BINDING_REF_DOMAIN)
   || u16be(INITIAL_DEVICE_BINDING_REF_VERSION)
   || b32(subject_context_ref)
   || b32(initial_authorize_event_id)

initial_device_binding_ref := H(initial_device_binding_ref_preimage)
```

`subject_context_ref` is the basis a later recognition fact should name. It says *which
context-bound Subject inception* is referred to and does not depend on the device grant, so
replacing or revoking a device never disturbs it. Neither preimage contains witness bytes, so
re-signing either body leaves both references unchanged — which matters because N1 explicitly
permits several valid witnesses over one canonical body.

## 7. Verification

Given a bundle and an independently supplied `(claimed_kind, claimed_context_id)`:

1. Require `version == 1` and `kind == claimed_kind`.
2. Require `context_id` non-empty and **byte-identical** to `claimed_context_id`.
3. Recompute `context_nonce` from the bundle's context material (§6.1).
4. Admit `inception_body_bytes` + `inception_witness` through N1's existing state-independent
   `admissible_bytes`.
5. Require the body is an `Inception` whose `context_nonce` equals the recomputed bytes.
6. Derive `SubjectId` by N1's digest rule.
7. Admit `authorize_body_bytes` + `authorize_witness` the same way.
8. Require the authorize body names the derived Subject, is at position `1`, has
   `prev_digest == inception_event_id`, grants exactly `{Sign, Present}`, and has no validity
   span.
9. Require the authorized device is **not** a member of the decoded inception body's
    `initial_authority` set (the bootstrap separation invariant). Read from the inception body,
    never from a bundle-supplied assertion — a bundle may be assembled by anyone, so the
    constructor's refusal cannot be relied on here.
10. Ingest both events into a **fresh** `AuthorityStore` and run N1's existing `derive(subject)`.
11. Require a clean `AuthorityView::Live` at frontier `2` carrying exactly one device grant, for
    the device the authorize body names, with exactly the expected capabilities, no validity
    span, and `granted_at == 1`; and require the device is absent from the derived authority set.
12. Recompute both references (§6.4) from body/event references, witnesses excluded.

**Which check is load-bearing.** Step 9 is the one that enforces the separation: it holds
before any derivation runs and reads the authority set straight out of the decoded inception
body. The derived-authority clause in step 11 is *equivalent* for a two-event genesis bundle and
cannot fire on its own — no establishment event can occupy position 1, so the fold never advances
past generation 0 and the derived authority set is still the inception's `initial_authority`. It
is kept because it states the invariant against the structure a reader cares about ("the device
is not a log writer in the derived history") and would keep holding if a later profile ever
admitted a bundle whose prefix contains a rotation.

Step 10 is load-bearing and not redundant with step 7. Admission proves only *"some key signed
these bytes"* — an authorize event signed by an arbitrary key is perfectly admissible. Only the
derived fold answers *"did that key hold authority in this Subject's history?"*

Any parse, signature, context, parent, position, capability, authority or derivation mismatch
**fails closed**. There is no partial success and no "verified except" state.

Verification returns: `SubjectId`, device Principal, both event ids, both reference hashes, the
derived authority state, and the frontier.

## 8. Test vectors

Inputs, stated in full:

| Input | Value |
|---|---|
| `context_kind` | `GovernanceDomainV1` (`0x01`) |
| `context_id` | `coop.example.governance` (23 ASCII bytes) |
| `context_salt` | `salt[i] = i` for i in 0..32 → `000102…1e1f` |
| continuity secret | `secret[i] = 0x80 + i` → `8081…9e9f` |
| establishment plan | `[Rotate; 4]` (horizon 4) |
| device seed | `seed[i] = 0x40 + i` → `4041…5e5f` |

Outputs (hex):

| Value | Bytes |
|---|---|
| `context_preimage` (89 B) | `0000001769636e2e67656e2e7375626a6563742d636f6e7465787400010100000017636f6f702e6578616d706c652e676f7665726e616e6365000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f` |
| `context_nonce` | `208c2b372130c8479460ddc56a0d581caa5016a47df54ed08f1357c45041e3cf` |
| `initial_authority` pubkey | `6438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636ba` |
| `next_commitment` (C₁) | `d04408dd67899efb069fbb5930860464f34dde35816d5d469dcafc2639fcfa1a` |
| inception body (158 B) | `0000001169636e2e617574686f726974792d6c6f67000101016438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636ba208c2b372130c8479460ddc56a0d581caa5016a47df54ed08f1357c45041e3cf00000001016438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636bad04408dd67899efb069fbb5930860464f34dde35816d5d469dcafc2639fcfa1a` |
| **`SubjectId`** = inception `event_id` | `3b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a` |
| device pubkey | `2543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d` |
| authorize body (169 B) | `0000001169636e2e617574686f726974792d6c6f670001033b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a00000000000000013b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a016438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636ba012543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d00000002010300` |
| authorize `event_id` | `e600a622c3a727fd48b55f31ee7deb3d91bc1be518ee38f0211b4d575f61af40` |
| **`subject_context_ref`** | `5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86` |
| **`initial_device_binding_ref`** | `b2ff12700d18618947fdc6a6b02310580dd95c7b5e3b91f4cc7de338a197d3ad` |

These values were produced by an **independent reference implementation written from this
document** (Python, sharing no code, library or constant definition with `icn-identity`) and are
pinned as literals in `gen_subject_context.rs`. The reference is preserved and self-checking:

```bash
python3 icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py
```

It re-derives all eleven values from this document, reads the `EXPECT_*` literals out of the Rust test
purely as a comparison target, and exits non-zero on any disagreement. Both implementations agree
byte-for-byte. Witness signatures are
deterministic per RFC 8032 but are **not** pinned and are **not** part of semantic identity.

### 8.1 Negative cases covered

Empty context id (construction and verification); unsupported version; wrong context claim;
relabelling a bundle into another context; mutated salt; trailing bytes on a canonical body; bad
inception witness; bad authorize witness; authorize naming another Subject; wrong position; wrong
parent; extra capability; missing capability; `Recover` capability; validity span present;
inception body in the authorize slot and vice versa; an authorize event that is **admissible but
unauthorized**; a continuity root built with a foreign nonce; **the generation-0 establishment
authority named as the initial device — refused both at construction and, for an externally
assembled bundle, at verification, with the test first proving N1 itself accepts that history**.

Plus the positive structural obligations: byte-identical reproduction from the same full
configuration; different context, salt, or plan/horizon all change the Subject; two Subjects in
one context publish unlinkable nonces; changing the device leaves `subject_context_ref` unchanged
while changing the device-binding reference; a second valid witness changes neither reference;
GEN domains are pairwise distinct from each other and from N1's; no DID-string spelling enters
canonical bytes.

## 9. Privacy and correlation

The unlinkability claim is **relative to what a party has been given**, and collapsing the two
cases would overstate it. `context_salt` is never secret; it is public material that happens not
to be disclosed to everyone.

### 9.1 Observer A — holds only N1 inception material

Sees `context_nonce` and nothing else about the context. For this observer:

- The nonce is 32 bytes indistinguishable from uniform, and **names nothing**.
- The governance domain is **not recoverable**: doing so means guessing the 32-byte salt.
- Two Subjects in one context are **unlinkable**, because their salts are independent draws.
- Two Subjects of one *person* across contexts are unlinkable for the same reason.

### 9.2 Observer B — has been handed a `SubjectContextGenesisV1` bundle

Receives `context_id` and `context_salt` explicitly, and can therefore confirm the context and
recompute the nonce. That is **the point of the evidence**, not a leak: a relying verifier cannot
check a context claim it was not told. For this observer:

- The bundle's **own** context is revealed, deliberately.
- Nothing about the person's **other** contexts is revealed. Each context has an independent
  salt, so holding one bundle does not help recognize or link a Subject in another context.
- A bundle is bearer-showable: whoever holds it can present it. It still confers no authority —
  it contains no private key — but disclosure is a decision, so clients should treat the set of
  parties given a bundle as the set that knows that context binding.

### 9.3 Holding across both

- `context_nonce` must never be presented as a Subject identifier or a context identifier.
- `context_salt` is **public, not secret**, and must nonetheless never be globally indexed;
  publishing a salt directory would hand Observer A the capability §9.1 denies them.
- No protocol party receives a cross-context mapping of one human's Subjects.
- No global person registry, registrar, or institution-controlled human master identifier is
  introduced.

## 10. Deliberate scope boundaries

Not in this slice, by design: bundle transport/serialization; durable or daemon-persistent
authority storage; gateway routes; `SubjectRecognitionV1`; membership, standing or governance
integration; session claims; N3 replication; legacy DID or member migration; institution/entity
genesis; recovery protocol design; production cutover.

`SubjectContextGenesisV1` has **no serde impl and no wire encoding**. That is intentional: it
makes *"serde/JSON bytes are not the cryptographic identity of an N1 event"* structurally true
rather than merely tested. A strict deterministic container may be specified separately;
verification will still consume the canonical N1 bytes carried in the bundle, never a
re-serialization of decoded fields.

The next child of #2694 is the local N1 durability slice.
