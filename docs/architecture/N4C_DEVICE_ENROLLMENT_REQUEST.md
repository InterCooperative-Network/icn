---
Status: normative
Canonical: no
Last Reviewed: 2026-10-04
---

# N4-C — the enrollment request, the approval step, and the ceremony a client can implement

**Companion to:** `N4A_DEVICE_AUTHORITY_EVALUATION.md` (evaluation), `N4B_DEVICE_AUTHORITY_BUNDLE.md`
(carrying facts), `IDENTITY_SEMANTICS.md` §2.7 (the Device Principal contract),
`HUMAN_IDENTITY_ARCHITECTURE.md` §11–§12 (per-context device keys; custody of the root),
`HOME_RUNTIME_IDENTITY_PROFILE.md` (who consumes this) · **Issue:** #2599 (N4) ·
**Code:** `icn/crates/icn-identity/src/device_enrollment.rs`, `tests/device_enrollment.rs`

> **Truth status.** Normative for the request bytes and the approval rules; **LIB-TESTED** (10
> tests, including the ceremony end to end in a fixture: request → possession check → attenuated
> approval → act accepted inside the grant, refused outside it → revoke → refused; Subject
> unchanged). Transport, approval UI, persistence of pending requests, delivery of the resulting
> fact, rotation, replacement and recovery are **not** built. Fixture identities only.

---

## 1. What was missing

N1 defines the fact that authorizes a device (`Authorize`, `construct::authorize_event`); N4-A
decides whether an act is covered by it; N4-B carries facts to a relying party. Nothing defined
the one object a device presents **before it holds any authority**: the request. Without it, a
client could only be enrolled by a process that already held the person's establishment key and
constructed the `Authorize` fact itself — i.e. only inside the person's own client. This document
defines the request and the approval step, so the ceremony is a protocol between two parties:
the device and the person's authority edge.

## 2. The ceremony

| Step | Party | Produces | Class |
|---|---|---|---|
| 1. generate a device secret locally (hardware-backed where available) | device | nothing ICN-visible | — (device's own) |
| 2. derive the **per-context** device Principal for the context it wants to join (HIA §11.1; not from the `SubjectId`) | device | `PrincipalKey` | LIB-TESTED type; derivation DOC-ONLY, client-side |
| 3. build and self-sign an [`EnrollmentRequestV1`](#3-canonical-request-bytes): the Subject it asks to join, its Principal, the capabilities it asks for, a human label, a nonce | device | `SignedEnrollmentRequest` | **LIB-TESTED** (this slice) |
| 4. carry the request to the authority edge (QR, link, local network) | carrier | — | transport: not built |
| 5. `verify_enrollment_request`: the presenter holds the key it names. **Possession, not authority.** | edge | `Ok(())` or `BadSignature` | **LIB-TESTED** |
| 6. show the human: label, device key fingerprint, requested capabilities, the context | edge UI | — | not built |
| 7. the human approves with an **attenuated** grant (`granted ⊆ requested`, non-empty, never `Recover`), optionally a position span | edge | decision | — |
| 8. `approve_enrollment`: refuses a request for another Subject, a wider-than-requested grant, an empty grant, `Recover`, or the establishment key as device; otherwise authors the N1 `Authorize` fact with the edge's establishment key at the next log position | edge | `SignedAuthorityEvent` | **LIB-TESTED** |
| 9. the fact becomes durable at the edge (N1-D store, #2800) and reaches the device and relying parties | transport | facts in stores | N1-D PROPOSED; delivery not built |
| 10. a relying party evaluates any later act with N4-A over the facts (carried by N4-B) | relying party | `DeviceAuthorityEvidence` or a refusal | LIB-TESTED + `icnctl` |
| 11. the device is lost or replaced: the edge authors `Revoke`; a fresh install presents a **new** request with a **new** key | edge, device | `SignedAuthorityEvent` | LIB-TESTED (`revoke_event`) |

**Rejection** is silence: no fact is produced, and an unapproved request is never durable.
**Expiry** is the edge's policy: N1 has no clock, so the request carries a nonce, not a deadline;
an edge that wants requests to lapse enforces that where the request is shown, and a device whose
request lapsed builds a new one. Both are pinned in the module's contract.

## 3. Canonical request bytes

N1 framing, reused: principals and capability sets are encoded exactly as in an N1 body.

```text
enrollment_request_v1 :=
      LP("icn.n4.enrollment-request")
   || u16be(1)
   || b32(subject)                         -- the Subject the device ASKS to join
   || u8(0x01) || b32(device_key)          -- the device's per-context Principal
   || u32be(n) || n × u8(capability_tag)   -- requested, strictly ascending; never 0x04 (Recover)
   || LP(label)                            -- UTF-8, ≤ 64 bytes, for the approving human only
   || b32(nonce)                           -- device-chosen

request_id := SHA-256(enrollment_request_v1)
signature  := Ed25519(device_key, enrollment_request_v1)
```

Strict decoding refuses: wrong domain/version, a non-principal in the device slot, unordered or
unknown capability tags, `Recover`, an empty ask, an over-long or non-UTF-8 label, trailing bytes.
The domain is distinct from every N1, GEN, N4-A and N4-B separator (pinned).

## 4. What the request is not

- **Not a certificate.** It binds no name to no key with no authority. It is an *ask*.
- **Not a claim about the human.** It names a `SubjectId` because the edge must know which log
  to append to; naming it proves nothing, and the edge refuses a request for a Subject it does
  not act for.
- **Not reusable across contexts.** The Principal is per-context; a request for another context
  is another request with another key. A device that presented one key in two contexts would
  link the person's Subjects to any observer that saw both (IS §2.1); the request shape does
  not prevent a misbehaving device from doing that, and nothing can — it makes the correct
  behaviour the natural one.
- **Not an invitation, token or code.** Possession of request bytes confers nothing; only the
  edge's `Authorize` fact does, and only a relying party that holds it concludes anything.

## 5. Contradictions settled by the executable ceremony

| Question | Answer, as pinned by tests |
|---|---|
| one key per endpoint vs per-context device keys | the request carries one per-context Principal; a device joining three contexts presents three requests with three keys. The earlier "one operational key per endpoint" assumption is withdrawn. |
| can an approver grant more than was asked? | no: `GrantExceedsRequest`. Attenuation is structural, not advisory. |
| can enrollment ever confer recovery? | no: `RecoverRequested` at construction, `RecoverNeverGranted` at approval. Recovery stays with the person's establishment authority (N7). |
| can the phone enroll its own establishment key as a device? | no: `DeviceIsEstablishmentAuthority`. The edge's key authors facts; it is never a bounded device. |
| does a verified request grant anything? | no: a stranger with a valid request is refused by N4-A (`DeviceNotAuthorized` / `PrefixIncomplete`) until a fact exists. |
| does revoking a device touch the human? | no: identifier, authority set and generation are identical before and after (pinned). |

## 6. The contract a Home runtime or deployment may build to

In the vocabulary of `HOME_RUNTIME_IDENTITY_PROFILE.md` §6, with the objects that now exist:

**First boot.** The device generates a device secret; derives one Principal per context it will
join; builds, for each, an `EnrollmentRequestV1` and signs it. It presents **only** the signed
request bytes and a human-readable label. What survives reinstall: nothing — a reinstalled device
is a new Principal and presents a new request; the person's identity and facts are untouched.

**Enrollment.** The edge runs `verify_enrollment_request`, shows the human the label,
fingerprint and ask, and on approval runs `approve_enrollment` with the attenuated grant. The
relationship is the resulting N1 `Authorize` fact and nothing else. The bounded authority is the
grant's capability set and optional span.

**Runtime.** The Home runtime (a device Principal itself) stores public N1 facts in the N1-D
layout and its own device keys; it asks "is this act covered?" by building an N4-B bundle and
running the N4-A verifier (`icnctl device-authority verify`, or the library). The device itself
needs locally: its secret, its per-context Principals, and the facts it has received.

**Revocation.** Lost, stolen or reflashed: the edge authors `Revoke`; every relying party that
retains it refuses the device at every position from the revoke on. A reflashed device holds a
new secret and is a new Principal; the old one stays revoked. Effectiveness is per relying party,
as far as its facts go (N3 owns propagation).

**Recovery.** Out of scope here (N7, #2603). The request path never grants `Recover`, so losing
every device that was enrolled through it loses no recovery authority; losing the *edge* is an
establishment-authority event, handled by N1 pre-rotation, not by enrollment.

**Contexts.** One request per `(device, context)`; each context's Subject is a different
`SubjectId` the device learns from the edge for that context; the device's Principals differ per
context. Nothing links them except the person's own `ContinuityRoot`, which never appears in
any request, fact or bundle.

**Trust / transport.** TLS, pairing codes, QR scanning and local networks carry request and
fact bytes; none of them is identity or authority. A connection that "is device `K`" needs a
device binding object under N4 (`HOME_RUNTIME_IDENTITY_PROFILE.md` §6.5), not built here.

## 7. Scope boundaries

Not in this slice: request transport and display; persistence of pending requests; delivery of
the `Authorize` fact to the device and to relying parties; rotation and replacement flows beyond
`Revoke` + new request; recovery; a device-binding object for transports; any new capability
beyond N1's four; any new context kind (GEN, #2602); an `icnctl` verb for requests or approvals
(an approval needs the establishment key, which belongs on the person's client, not in a server
CLI). `EnrollmentRequestV1` has **no serde impl and no wire encoding beyond its canonical bytes**.
