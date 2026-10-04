---
Status: normative
Canonical: no
Last Reviewed: 2026-10-04
---

# N4-D — binding a transport connection to a device Principal, by composition

**Companion to:** `N4A_DEVICE_AUTHORITY_EVALUATION.md` (the verifier), `N4B_PORTABLE_EVIDENCE_BUNDLE.md`
(carrying facts), `N4C_DEVICE_ENROLLMENT_REQUEST.md` (how the device got its grant),
`IDENTITY_SEMANTICS.md` §9 (node transport identity), `HOME_RUNTIME_IDENTITY_PROFILE.md` §6.5 ·
**Issue:** #2599 (N4) · **Code:** `icn/crates/icn-identity/src/device_channel_binding.rs`,
`tests/device_channel_binding.rs`

> **Truth status.** Normative for the payload bytes and the verification order; **LIB-TESTED**
> (7 tests, run on icn-dev at `f7151ae37`). No runtime, route or transport profile calls it; it
> is not CLI-reachable and not PRODUCTION. It adds **no kernel primitive**: everything below is a
> `DeviceActV1` evaluated by N4-A unchanged, plus one equality check. Fixture identities only.

---

## 1. The question

A Home runtime that terminates a connection — a hosted workstation receiving a desk computer,
a phone, a PC — must be able to conclude "this connection is acting as device Principal `K` for
Subject `S`" **without** treating any of the following as identity or authority: an IP address,
a hostname, an operating-system login, a remote-desktop login, ownership of a TLS certificate,
or possession of an enrollment request. All of those gate a transport. None of them is a device
grant.

## 2. Why node `BindingInfo` is not reused

ICN already binds a Principal to a TLS certificate: `BindingInfo` in the network Hello
(`icn/crates/icn-identity/src/bundle.rs`, `handlers/hello.rs`; `IDENTITY_SEMANTICS.md` §9;
`network-session-identity-binding.md`). That object binds a **node** Principal, per connection,
inside the node handshake, and the profile already rules that it "must not be repurposed for
humans or devices" (`HOME_RUNTIME_IDENTITY_PROFILE.md` §6.5). Three reasons hold:

1. A node Principal is a durable infrastructure identity; a device Principal is a bounded,
   revocable grant in a *human's* log. Binding the latter through the node object would make a
   device look like infrastructure to every relying party.
2. `BindingInfo` is verified against node state; a device binding must be verified against the
   Subject's retained N1 facts at a relying-party position (N4-A), or it says nothing about
   authority.
3. The node object carries no Subject and no capability. A device binding is meaningful only as
   "device `K`, for Subject `S`, with capability `Present`, at position `E`".

So the device binding is an *act*, not a handshake field.

## 3. The object

A channel binding is a `DeviceActV1` (N4-A) with:

- `capability = Present`;
- `payload = channel_binding_v1`, a domain-separated canonical object:

```text
channel_binding_v1 :=
      LP("icn.n4.channel-binding")   -- distinct from every N1, GEN, N4-A, N4-B, N4-C separator (pinned)
   || u16be(1)
   || b32(channel)                   -- the digest of the channel the device believes it is on
   || b32(nonce)                     -- device-chosen or relying-party challenge; see §6
```

Strict decoding refuses a wrong domain or version, short input and trailing bytes. The act's own
canonical bytes, signature, `act_id` and bundle carriage are exactly N4-A/N4-B: nothing new is
signed, hashed or framed.

### 3.1 What `channel` means — and what N4-D does not define

`channel` is 32 bytes both ends compute for the same channel and different bytes for different
channels. **N4-D does not say what it is the digest of.** That belongs to the transport profile
that adopts this binding (for TLS: the server certificate the client saw, or a `tls-exporter`
value; for a pairing transport: the pairing transcript). N4-D requires only the two properties
above. It says nothing about ciphers, certificate chains, trust roots, hostnames, ports or
reconnection; those are the transport's, and none of them becomes authority.

### 3.2 Why `Present`

Of N1's four device capabilities, `Present` is "the device may present credentials on the
subject's behalf" (`body.rs`); presenting *itself* on a channel is the smallest such act. It is
the capability the Alpha genesis profile grants an initial device and the one an enrollment
typically asks for, so a binding needs no new grant. `Sign` is reserved for acts that *do*
something; a connection asserting its own identity does nothing yet. The act class (N4-A §7.2)
is 2, immediately settled: the relying party evaluates once, at admission, at its own position.

## 4. Verification, in order, each fail-closed

`verify_channel_binding(signed, subject, store, position, expected_channel)`:

| step | check | refusal |
|---|---|---|
| 1 | the act's capability is `Present` | `NotAPresentation` — a `Sign` act carrying binding bytes is a signed act, not a binding; it must not be accepted as one by accident |
| 2 | the payload decodes as `channel_binding_v1` | `Payload(codec)` — an arbitrary `Present` payload is not automatically a binding |
| 3 | N4-A `verify_device_act` at the relying party's position: authorship under the device key, then authorization over the retained facts | `Act(…)`: `BadSignature`, `ActSubjectMismatch`, `EvaluationPositionMismatch`, or `Refused(DeviceNotAuthorized | PrefixIncomplete | …)` |
| 4 | the bound `channel` equals the one this relying party sees | `ChannelMismatch` |

Pinned by the tests:

- an authorized device binding the channel it is actually on → evidence (a grant, not a bool);
- **an authorized device binding another channel → refused** (the man-in-the-middle case: the
  signature is genuine, the grant is in force, and the connection is still refused — a
  transport fact, not an authority fact);
- a device that was never authorized → refused by N4-A before the channel is compared;
- a device authorized and then revoked → `DeviceNotAuthorized` at the post-revocation position;
- a `Sign` act with binding bytes → `NotAPresentation`;
- a `Present` act with a non-binding payload → `Payload`;
- the payload bytes are strict and round-trip, hand-framed from §3;
- the domain is distinct from every other separator.

## 5. What this keeps apart

| layer | answers | never answers |
|---|---|---|
| transport (TLS, pairing, local network) | is this channel private, and the same channel at both ends? | who is acting, under what authority |
| N4-D binding | the device Principal that signed this is on *this* channel | whether that device is authorized |
| N4-A over retained facts | is that device authorized for `Present` by `S` at `E`? | whether the human approved anything today |
| N4-C, at the edge | the human's approvals | — |

No layer may stand in for another. "The TLS handshake succeeded" never means "this is device
`K`"; "this is device `K`" never means "`K` is authorized"; "`K` is authorized" never means "the
human is present".

## 6. Replay and the nonce

N4-D fixes no replay rule, because N1 has no clock and N4-A §9 assigns replay slots to the action
family. A transport profile that adopts the binding chooses one of two sound rules and states it:

- **challenge**: the relying party issues the `nonce` and accepts a binding only against the
  challenge it issued for this connection; or
- **channel-unique digest**: `channel` itself is unique per connection (a `tls-exporter` value),
  so a replayed binding names a channel that no longer exists.

Either way, replaying a binding onto a *different* channel is already refused by step 4; the
nonce decides only whether the *same* channel may be re-bound.

## 7. Evaluation position

The relying party supplies `position` exactly as for any act (N4-A §7.1). For a live connection
that is a current admission (class 2); the current-admission rule and helper are defined in
`N4E_CURRENT_ADMISSION.md`. A binding bound to a position the relying party does not hold is
`PrefixIncomplete` / `EvaluationPositionMismatch`, never "probably fine".

## 8. Scope boundaries

Not defined here: the transport profile (what `channel` digests; TLS trust roots; pairing);
replay policy beyond §6; session lifetime or re-binding cadence; a gateway or runtime that calls
this (none exists); any binding for humans or Subjects (a Subject has no key and binds nothing);
any reuse of node `BindingInfo`. `ChannelBindingV1` has **no serde impl and no wire encoding
beyond its canonical bytes**.
