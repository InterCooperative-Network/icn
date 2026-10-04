---
Status: normative
Canonical: no
Last Reviewed: 2026-10-04
---

# N4-B — the `(N1 facts, device act)` bundle and the stateless `icnctl` relying party

**Companion to:** `N4A_DEVICE_AUTHORITY_EVALUATION.md` (the verifier this carries input for),
`IDENTITY_SEMANTICS.md` (the canonical contract), `HOME_RUNTIME_IDENTITY_PROFILE.md` (who consumes
this and why) · **Issues:** #2599 (N4), #2694 (ladder; the shared missing boundary), #2800 (N1-D
record layout) · **Code:** `icn/crates/icn-identity/src/device_authority_bundle.rs`,
`icn/bins/icnctl/src/device_authority.rs`

> **Truth status.** Normative for the bundle bytes and the verb's contract; **LIB-TESTED** for the
> container (14 tests + an independent Python framing vector) and **reachable from the `icnctl`
> binary** for the verb (5 end-to-end tests). It changes nothing in N1 or N4-A: the bundle adds no
> semantics and removes none. No transport, persistence, replication, enrollment, revocation
> propagation, recovery or gateway route is claimed. Fixture identities only in every test.

---

## 1. The question this answers

N4-A decides, for one relying party that already holds an `AuthorityStore`, whether one signed
device act is covered by a Subject's delegation at a position the relying party chose. It left
exactly one boundary unbuilt (`N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.2 item 2): *how does a
relying party that is not this Rust process come to hold the facts and the act?*

The answer is a container, not a service: **a strict, deterministic byte string carrying the
Subject's retained canonical facts and one signed act**, which any relying party rebuilds a store
from, through the ordinary N1 admission gate, and then verifies with N4-A unchanged. The relying
party still supplies the Subject and the position; the bundle supplies evidence only.

This is the first point at which a system outside Rust can obtain ICN device-authority truth
without a running daemon: `icnctl device-authority verify` reads a bundle file, takes
`(subject, position)` from its caller, and prints the N4-A verdict. It is a *relying party*, not an
oracle: it can refuse, or return a grant that already exists in the log, and nothing else.

## 2. Canonical bundle bytes

N1 framing, reused and not reimplemented (`LP(x) := u32be(len(x)) || x`; `b32`/`b64` raw bytes;
integers big-endian):

```text
bundle_v1 :=
      LP("icn.n4.device-authority-bundle")
   || u16be(1)
   || u32be(fact_count)
   || fact_count × fact
   || LP(device_act_v1)          -- the act's canonical bytes, exactly as N4-A defines them
   || b64(act_signature)

fact :=
      b32(event_id)              -- SHA-256(canonical_body)
   || b64(witness_signature)     -- one Ed25519 witness over the body's signature preimage
   || LP(canonical_body)         -- the N1 canonical body, exactly

bundle_id := SHA-256(bundle_v1)
```

**Strictness (every one is a decode error):**

| Rule | Why |
|---|---|
| domain and version are exactly as above | no other object can be mistaken for a bundle; the domain is distinct from every N1, GEN and N4-A separator (pinned) |
| `fact_count ≤ 65 536`, and never more than the remaining input can hold | bounds what an untrusted bundle can make a verifier allocate, before any allocation |
| facts strictly ascending by `(event_id, witness_signature)` | one set of facts has one encoding; duplicates and reorderings are refused, so `bundle_id` identifies what was carried |
| `event_id == SHA-256(canonical_body)` | the id is checked, never believed |
| `canonical_body` re-encodes to itself | the record carries the canonical bytes, nothing lossy |
| the act decodes as a strict `DeviceActV1` | N4-A's own rules |
| no trailing bytes | — |

The fact record is laid out as `event_id ‖ witness → canonical body`, the N1-D (#2800) record
layout, so *what a replica persists* and *what a client carries* are one framing.

**What the bytes do not carry, on purpose:** the Subject the relying party evaluates for, the
evaluation position, any clock, any locator, any claim of authority. A bundle that "says" it is
authorized would be meaningless; only the fold says that.

## 3. Verification over a bundle

`verify_bundle(bundle, subject, position)`:

1. **Admit every fact, fail-closed, in order.** Each fact passes `AuthorityStore::ingest`, i.e.
   the N1 admission rules (canonical re-encode, inline signer verifies, position bound). The
   first inadmissible fact refuses the **whole** bundle, naming its index
   (`InadmissibleFact { index, error }`). A replica may drop a bad event and continue; a verifier
   handed bytes by an untrusted party must not, because dropping would let the sender choose
   which facts the relying party evaluates against.
2. **Run N4-A unchanged:** `verify_device_act(act, subject, store, position)`.

Consequences, each pinned by a test:

- **Equivalence.** Verifying through a bundle yields exactly the evidence verifying the same
  facts in a local store yields.
- **Withholding shrinks, never extends.** A bundle missing the revocation at position 3 cannot
  make the revoked device authorized at 3: the prefix is incomplete (`PrefixIncomplete {frontier:
  3, required: 3}`). It still proves the device at 2, because history is not rewritten.
- **The relying party chooses.** A bundle verified for another Subject is `ActSubjectMismatch`; at
  another position, `EvaluationPositionMismatch`. The bytes never pick either.
- **No clock, no environment.** The module names no time, filesystem, network or randomness
  source (pinned by inspecting its source).

## 4. `icnctl device-authority`

Stateless. Reads the bundle file and nothing else: no data directory is opened, no daemon or
network is contacted, no N2-A gate applies (nothing persistent is touched). stdout is one JSON
document; the exit code is the verdict class.

```text
icnctl device-authority verify  --bundle FILE --subject HEX64 --position N
icnctl device-authority inspect --bundle FILE
```

| exit | `verdict` | when |
|---|---|---|
| 0 | `authorized` | the act is covered; the document carries the N4-A evidence |
| 1 | `refused` | all facts admissible and N4-A refused (`class` = one of the N4-A refusal names: `ActSubjectMismatch`, `EvaluationPositionMismatch`, `BadSignature`, `PositionOutOfRange`, `SubjectUnknown`, `AuthorityHalted`, `PrefixIncomplete`, `DeviceIsEstablishmentAuthority`, `DeviceNotAuthorized`, `GrantNotInForce`, `CapabilityNotGranted`), **or** one fact was inadmissible (`class` = `InadmissibleFact`, with `fact_index`) |
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

`inspect` lists each fact (`index`, `kind_tag`, `subject`, `position`, `event_id`) and the act
(`subject`, `device`, `capability`, `evaluation_position`, `payload_len`, `act_id`) and **decides
nothing**: a bundle whose act would be refused inspects with exit 0.

`--subject` and `--position` are mandatory and are never read from the bundle. A caller that
takes them from the bundle has made itself the signer's relying party, which is the one thing
N4 invariant 6 forbids.

## 5. Cross-implementation evidence

- `tests/reference/device_authority_bundle_reference.py` re-derives, from this document's prose
  alone, the bytes and `bundle_id` of a zero-fact bundle over the N4-A act vector, including the
  deterministic Ed25519 act signature, and compares them with the literals pinned in
  `tests/device_authority_bundle.rs`: **agrees.**
- The fact-record layout is pinned by a hand-written framer in the Rust test (written from §2,
  not from the implementation), which produces byte-identical output for the canonical order and
  is used to construce every malformed case (reordered, duplicated, wrong `event_id`,
  non-canonical body, impossible count).

## 6. What a downstream consumer MAY rely on / MUST NOT do

Everything in `N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.3, plus:

**MAY:** carry a bundle over any transport; store a bundle as the record of what was evaluated;
treat `bundle_id` as the identity of *what was carried* (it is witness-dependent, unlike `act_id`);
run `icnctl device-authority verify` as its relying party and gate on exit code and `class`;
re-verify the same bundle at a later `position` once it has retained more facts, by building a
new bundle.

**MUST NOT:** take `subject` or `position` from the bundle; cache a verdict across positions;
treat `inspect` output as a verdict; treat exit 1 with `InadmissibleFact` as "some facts were
fine"; reimplement the framing, the admission rules or the fold in another language (the Python
file is an *audit*, not a client); put anything in a bundle that is not an N1 fact or the act.

## 7. What remains open

- **Where a client gets the facts.** This document assumes the client holds them. The N1-D fact
  store (#2800) is how a replica persists them; replication between replicas is N3 (#2598); how
  a *device* first receives its Subject's facts is part of the N4 enrollment ceremony (#2599,
  `N4C_DEVICE_ENROLLMENT_REQUEST.md`).
- **Which position a relying party should pin** (G1-A, #2694 §6). The verb takes the caller's
  answer; it does not supply one.
- **A hosted relying party** (gateway route) — only after this, and only as a relying party
  returning evidence plus the facts it evaluated, never as an authority (N4-A §9.2 item 4).

## 8. Scope boundaries

Not in this slice, by design: transport; persistence; replication; enrollment, rotation,
replacement, lost/stolen lifecycle (N4C begins the request shape only); recovery (N7); any
capability beyond N1's four; any new context kind (GEN, #2602); a gateway route; SDK bindings;
production wiring beyond the `icnctl` verb. `DeviceAuthorityBundleV1` has **no serde impl and no
wire encoding beyond its canonical bytes**, as with N4-A and GEN-A: the canonical bytes *are* the
bundle.
