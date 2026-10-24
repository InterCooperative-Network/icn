---
Status: proposed
Canonical: no
Last Reviewed: 2026-10-04
---

# Home runtime identity profile — decision record for the first personal-domain vertical slice

**Companion to:** `N4A_DEVICE_AUTHORITY_EVALUATION.md` (the primitive), `IDENTITY_SEMANTICS.md`
(the canonical contract), `GEN_SUBJECT_CONTEXT_GENESIS.md` (genesis) ·
**Issues:** #2599 (N4), #2602 (GEN), #2694 (ladder), #2800 (N1-D), #2801 (naming), #2806 (ADR-0083 drift) ·
**PR:** #2807

> **Truth status.** This is a **decision record and consumption profile**, not a new identity
> model and not an implementation claim. Every primitive it names is classified below as
> PRODUCTION / LIB-TESTED / FIXTURE-ONLY / EXPERIMENTAL / DOC-ONLY / MISSING against the live
> checkout at `main` `7ef8a670a` plus PR #2807. Where a class says MISSING, nothing downstream may
> build a substitute. The motivating deployment (a thin client to a hosted workstation) is a
> **fixture for requirements**; it is not part of ICN and no real person's identity material exists
> or may be created from this document.

---

## 0. The question and the one-line answer

> Which ICN primitives does a person's *Home* runtime consume so that a phone, a small desk
> computer, a PC and a hosted workstation can be **replaceable device Principals** under **one
> durable human**, in **several contexts**, without any device, host or operator becoming the human?

**Answer.** The human is an N1 Subject per context (GEN-A inception, client-held `ContinuityRoot`);
each device is an ordinary Principal holding a per-context key of its own; authority is an N1
`Authorize` grant in the Subject's log, bounded by capability set and optional position span;
revocation is an N1 `Revoke`; a relying party decides any act with N4-A `verify_device_act` at a
position it chose. All of that is **LIB-TESTED** today. What is **MISSING** is the enrollment
ceremony that moves a grant request from a new device to the phone and the event back (N4), a
transport container for facts and acts (next slice), a GEN context kind for personal and household
contexts (#2602), and recovery (N7). The Home runtime consumes; it never defines.

On naming: per #2801, *Mutualware* is the intended public name of ICN itself, public-brand-first
and protocol-stable. There is no "Mutualware identity" and no "contract layer" above ICN. A
downstream document that frames a Home runtime as a *contract layer* should read it as **a
consumption profile of ICN identity** — this document — and nothing more.

---

## 1. Primitive map (acceptance item 1)

Classes: **PRODUCTION** reachable from a shipped binary/route · **LIB-TESTED** implemented and
tested, no runtime caller · **FIXTURE-ONLY** exists only inside tests · **EXPERIMENTAL** present
but unreachable, stubbed or unsafe · **DOC-ONLY** · **MISSING**.

| Responsibility | Primitive | Class | Canonical owner | Evidence |
|---|---|---|---|---|
| (a) the human Subject, per context | `authority_log::SubjectId` = inception event id; GEN-A `incept_subject_context_v1` | LIB-TESTED | `IDENTITY_SEMANTICS.md` §2.2; `GEN_SUBJECT_CONTEXT_GENESIS.md` | `subject_context.rs`; `tests/gen_subject_context.rs` + Python reference; zero runtime callers |
| (a′) continuity across contexts | `ContinuityRoot` (secret + plan + per-context nonce), client-held only | LIB-TESTED (construction), DOC-ONLY (custody) | IS §2.5, §10; HIA §12.2 | `authority_log/construct.rs:82`; custody rule R1.2 [HARD] HIA L202 |
| (b) device Principals under the Subject | `PrincipalKey`; an `Authorize` body naming the device | LIB-TESTED | IS §2.7 | `body.rs` `AuthorizeBody`; `construct::authorize_event` |
| (b′) per-context device keys | deterministic per-context key from the device's own secret | DOC-ONLY | HIA §11.1 L1614–1616 | no derivation helper exists in code; client-side, trivial, and must not take `SubjectId` as input (HIA L1618) |
| (c) bounded delegation | `CapabilitySet` {Sign, Encrypt, Present, Recover} + `Option<ValiditySpan>`; N4-A evaluation | LIB-TESTED | IS §2.7 (attenuation mandatory, I5); N4-A doc | `device_authority.rs`; 25 tests (#2807). No caveat/resource field: richer scope is #2599 |
| (c′) relying-party verification of a device act | `verify_device_act` at a relying-party position | LIB-TESTED | N4-A doc §7 | `tests/device_authority.rs` |
| (d) revocation | N1 `Revoke` + `derive_prefix` | LIB-TESTED (semantics); MISSING (propagation/effectiveness) | IS §2.7; #2599 invariant 5; N3 #2598 | `derive.rs:304` removes the grant; prefix evaluation makes "at E" answerable |
| (e) recovery | N1 pre-rotation `Rotate`/`Recover` establishment | LIB-TESTED (establishment transitions); MISSING (guardian/threshold protocol, backup) | IS §2.5; N7 #2603 | `construct::establish`; `Recover` clears device grants (`derive.rs:288`) |
| (e′) legacy recovery | `recovery.rs`, RPC `recovery.*`, SDIS recovery routes | EXPERIMENTAL — **unsafe**: node key signs the trustee attestation; SDIS complete is a stub | HIA F13; #2591, #2448 | `icn-rpc/src/handler/recovery.rs:138`; `api/sdis/recovery.rs:306` |
| (f) durable local facts | `AuthorityFactStore` + sled adapter (N1-D) | PROPOSED (PR #2800 open) | #2799/#2694 | record = `event_id ‖ signature → canonical body`; `icnd` wiring deferred to #2777 |
| (g) external facts+act container | — | **MISSING** | N4-A doc §9.2 item 2 | nothing carries N1 facts across a process boundary |
| (h) stateless CLI consumer | — | **MISSING** | N4-A doc §9.2 item 3 | `icnctl` has `id`/`device`/`recovery` verbs over legacy objects only |
| (i) context kinds | GEN-A `GovernanceDomainV1` only | LIB-TESTED (one kind); MISSING (personal/household) | GEN doc §5; #2602 | `subject_context.rs:180` |
| (j) legacy device path | `multi_device.rs`, `/v1/devices/*` | EXPERIMENTAL — unreachable: no DID document is ever created | HIA F8; #2588/#2590; superseded by N4 | `identity_mgr.rs:156` |
| (k) sessions | gateway HS256 JWT + `jti` revocation | PRODUCTION — **not identity** | `AUTHORITY_SPINE.md` | a session proves a request may enter a handler; never authorship |
| (l) node transport identity | DID-TLS `BindingInfo` in Hello | PRODUCTION — **node Principal only** | IS §9; `network-session-identity-binding.md` | `bundle.rs:181`; `handlers/hello.rs` |

**What a Home runtime consumes today, truthfully:** rows (a), (b), (c), (c′), (d) as a Rust
library. Nothing else. Rows (g)–(i) are the seam it is waiting on.

---

## 2. The fixture test (acceptance item 2) — done

`icn/crates/icn-identity/tests/device_authority.rs` (PR #2807): Subject `S`; `A` authorized at 1,
`B` at 2, `A` revoked at 3. `A`'s act at `E = 3` → `Refused(DeviceNotAuthorized)`; `B`'s →
evidence `{granted_at: 2, generation: 0}`; `S`'s identifier, authority set, generation and next
commitment identical before and after; `A ∉ devices`, `B ∈ devices`, `B ∉ authority`; device bytes ≠
Subject bytes. `A`'s act at `E = 2` still verifies. 25 tests; independent reference 4/4.

---

## 3. The external verification boundary (acceptance item 3)

| Form | Status |
|---|---|
| library API `icn_identity::device_authority` | LIB-TESTED |
| documented canonical act bytes + cross-implementation vectors | done — N4-A doc §6, §8 |
| deterministic container for **(N1 facts, act)** that a non-Rust client can carry | **MISSING — the exact missing piece** |
| stateless `icnctl` verb consuming that container, no `icnd` | MISSING (follows the container) |
| gateway route | deliberately not planned as an authority; at most a hosted relying party (N4-A §9.2) |

The container's record layout should align with #2800's `event_id ‖ signature → canonical body`
so that "what the daemon persists" and "what a client carries" are one framing. Until it lands,
*external systems consume ICN truth* stops at the Rust API, and the Home runtime has no
identity-bearing thing to build.

---

## 4. GEN stance for Private / Household / a person's own project (acceptance item 4)

**Can a personal or household context exist today without a `GovernanceDomain`?**

Mechanically, yes: GEN-A accepts any non-empty UTF-8 `context_id` and never checks that a
`GovernanceDomain` exists (`subject_context.rs:293–305`). Semantically, **no**: the only kind tag
is `GovernanceDomainV1` (`0x01`), so a Subject incepted for `"household.smith"` is a Subject *in a
governance domain called household.smith*. That is the institutional collapse the contract leaves
to GEN to avoid (IS §2.2: "a context may later be bilateral, informal, community-level … GEN
defines").

Three routes were weighed:

| Route | Verdict | Why |
|---|---|---|
| **Individual-owned `InstitutionalDomain`** (ADR-0083 L88–92 admits `Individual` as an owning entity class) | **rejected for personal/household use** | It makes a person an *owning entity class* and so extends the person-as-Entity debt IS §8 L538–544 dispositions for retirement. Declaration is mandate-gated and reached through a `governance:write` bearer (PRINCIPAL_MODEL §2.8), i.e. a registrar in the human's genesis path, against R1.1 [HARD]. A household is not a tiny cooperative and must not need a charter, a steward or a `GovernanceDomain` row to exist. |
| **`GovernanceDomainV1` with a household/personal string** | **rejected** | Correct bytes, wrong meaning; nothing could later distinguish a household from a domain; no object exists for members to recognize each other against. |
| **New GEN context kinds** | **PROPOSED — route to #2602** | The kind tag is inside every GEN preimage, N1 is unchanged, and N4-A evaluates identically whatever the kind. The gap is GEN's alone. |

**Proposed shape, for #2602 to decide (not decided here):**

- `PersonalContextV1` — one human, no other party. `context_id` is an opaque 32-byte descriptor
  digest the person's own client mints and keeps; it is never published beyond the person's own
  clients and devices. No registrar, no founder, no charter. *Private* and *a person's own small
  project* are both this kind; if a project later takes on members it becomes a household or an
  institution through genesis, not by relabelling.
- `HouseholdContextV1` — two or more humans, no governance object. `context_id` is the genesis
  digest of a household descriptor the founding members' clients agree on; membership is **mutual
  recognition between members' Subjects** (the GEN-B recognition shape, bilateral), never an
  `AuthorityGrant`, never a steward. A household has no treasury, no charter and no office; if it
  ever needs institutional authority it undergoes institutional genesis and becomes an
  Institution — it is not promoted in place.

Each human still holds **one Subject per context**, so *Katie-in-Household* and *Matt-in-Household*
are two Subjects, unlinkable to their Private Subjects except through each person's own
`ContinuityRoot`.

**What this means for the vertical slice now.** The device-authority proof is context-agnostic,
so it is testable today against a **fixture** `GovernanceDomainV1` context. No *production*
Private or Household context may be created until a GEN kind exists; the deployment track is not
blocked by that, because it does not create any context.

---

## 5. Contradictions, settled against the text

| Contradiction | Resolution | Basis |
|---|---|---|
| per-context device keys vs one key per endpoint | **per-context.** A device holds one key per (device, context); the same key in two contexts links the person's Subjects to any observer that sees both (a Principal is "fully correlatable across contexts by construction", IS §2.1). The Pi, with three contexts, holds three device Principals derived from one local device secret. The derivation is client-side and must not take `SubjectId` as input. "One key per endpoint" stays withdrawn. | HIA §11.1 L1614–1620; IS §2.1 |
| where the `ContinuityRoot` may live vs a VM-hosted Home | **on the person's own client — the phone is the authority edge — with pre-rotation material backed up off-device at onboarding (HIA §12.2 default (a)).** A VM-hosted Home runtime holds *public* N1 facts and the VM's own device keys; it is a **device Principal** (the "hosted/client device"), not a custodian. A Home process may hold the root only if the person has chosen that process as their own client, which makes its operator a custodian of the person's identity; the default is **refuse**, and a host operated for someone else never qualifies. | HIA R1.2 [HARD] L202–203, §12.2 L1741–1745; IS §5 L337–339, §10 |
| `AuthorityGrant` grantor must be chartered vs household authority | **two different objects.** An N1 `DeviceGrant` is a fact in a *Subject's* log and needs no entity, charter or grantor beyond the Subject's own establishment authority (IS §2.7). ADR-0014's `AuthorityGrant` is *institutional* authority and its grantor rule is correct for institutions. A household mints no `AuthorityGrant`; its members delegate to their own devices in their own logs. Household *shared* decisions are recognition and app-layer policy, not grants. | ADR-0014 L191–196; IS §2.7, §8 |
| Mutualware naming vs a network-ops "contract layer" | **Mutualware = ICN's public name; no layer.** Rename the framing to "Home runtime consumption profile of ICN"; protocol identifiers (`icn-*`, `did:icn:`, domains) do not change. | #2801; PR #2802 claim ledger C-ID-01 / C-BRAND-01 |

---

## 6. What a Home runtime may implement against today (acceptance item 5)

This is the specification a deployment track may build to **without inventing identity**. Every
step says which ICN object it produces or consumes and whether that object exists. Steps marked
MISSING may be *designed around* but **must not be substituted** by a conventional mechanism that
pretends to be the ICN one.

### 6.1 First boot of a device (any endpoint: desk computer, PC, hosted client)

| Step | Produces | Class | Rule |
|---|---|---|---|
| generate a **device secret** locally, hardware-backed where available | nothing ICN-visible | — | never leaves the device; never a human key |
| derive one **device Principal per context** from that secret and the context descriptor | `PrincipalKey` (32-byte Ed25519) | LIB-TESTED (`PrincipalKey::try_from_bytes`); DOC-ONLY (derivation) | no `SubjectId` in the derivation |
| **present** to the authority edge: the per-context device public key, spelled `did:icn:<multibase>` or raw 32 bytes, plus a human-readable device label | an enrollment *request* — not an ICN object yet | **MISSING** (N4 request shape) | the request carries **no** certificate, no Subject, no claim of authority |

The device presents a *key*, nothing more. Possession of the device proves nothing about the human.

### 6.2 Enrollment approval

| Step | Produces | Class | Rule |
|---|---|---|---|
| the authority edge (the person's phone, holding the context's establishment key) decides | — | — | a human decision on the person's own client |
| it authors an N1 **`Authorize`** event: `subject`, `position`, `prev_digest`, `device`, `capabilities ⊆ {Sign, Present, Encrypt}`, `validity: Option<ValiditySpan>` | `SignedAuthorityEvent` | LIB-TESTED (`construct::authorize_event`) | `Recover` is never granted to a device; the establishment key is never the device |
| the event reaches the device and every relying party | facts in their stores | **MISSING** — the N4 ceremony/transport | until it exists the only path is in-process construction with the root present, i.e. fixture-only |

**Forbidden substitute:** a bearer token, invite code, OS account, SSH key, RDP login or TLS client
certificate standing in for the `Authorize` event. These may *gate a transport*; none of them is
device authority.

### 6.3 What the Home runtime stores

| Stored | Class | Rule |
|---|---|---|
| per Subject it serves: the **public** N1 facts — canonical bodies and witnesses — using the N1-D record layout `event_id ‖ signature → canonical body` | PROPOSED (#2800) | re-admit every record through `admissible_bytes` on load; storage never selects branches |
| device acts (`DeviceActV1` bytes + signature) and the `DeviceAuthorityEvidence` the runtime computed for them | LIB-TESTED | evidence is reproducible from facts + act + `E`; it is a log, not an oracle |
| the Home's **own** per-context device keys (the VM-as-device) | — | the VM is a device Principal |
| **never:** any `ContinuityRoot`, any other device's secret, any human establishment key, any cross-context mapping of one person's Subjects | — | R1.2 [HARD]; IS §5.4; I10 |

### 6.4 What revocation does

1. The authority edge authors N1 **`Revoke { device }`** at the next position.
2. Any relying party that has retained that fact evaluates every later act at `E ≥` the revoke
   position and refuses the device (`DeviceNotAuthorized`). Acts at earlier `E` remain verifiable:
   history is not rewritten.
3. **Effectiveness is per relying party**, exactly as far as its retained facts go. There is no
   global instant revocation and no claim of one (#2599 invariant 5; N3 owns propagation).
4. Replacing a device is `Revoke(old)` + `Authorize(new)`; the Subject is untouched. Reflashing a
   device is the same two events; a reflashed device with a fresh secret is a **new Principal**.
5. A Home runtime that loses contact with the authority edge cannot revoke anything; it can only
   stop *relaying*. Stopping relay is a hosting decision and confers no authority.

### 6.5 What the TLS trust root is

ICN binds a Principal to a TLS certificate **only for nodes**, through `BindingInfo` in the network
Hello (`bundle.rs:181`; IS §9), and that binding is per connection and node-scoped. There is **no**
device-Principal-to-TLS binding for Home transports, and `BindingInfo` must not be repurposed for
humans or devices. Therefore:

- the thin-client transport (remote desktop to a hosted workstation) keeps its **conventional**
  PKI, owned and operated by the deployment track; it is a transport fact, not an identity fact;
- a future Home transport that wants "this connection is device Principal `K`" needs a device
  binding object designed under N4, with the same three-fact shape (key signs the cert digest;
  current cert matches) — **MISSING**, routed with #2599;
- nothing about a TLS root ever identifies the human.

### 6.6 The MAY / MUST NOT list

A Home runtime or deployment **MAY** rely on: the N4-A verdict and evidence; `act_id` as an act's
identity; the refusal classes; the rule that a revoked device is refused at every `E` at or after
the revoke and unaffected before it; the N1-D record layout once merged.

It **MUST NOT**: reimplement encodings, the fold, grant semantics or refusal logic in another
language; treat any account, key, token, VM or machine as the Subject or derive a `SubjectId` from
one; hold or derive a `ContinuityRoot` on infrastructure; mint a Subject or a context on a
person's behalf; choose `E` from the act or fall back to the frontier when the act's `E` is not
retained; build an enrollment, revocation or recovery mechanism of its own; promote fixture
identities into anything real.

---

## 7. Acceptance scenario status

| Item | Status |
|---|---|
| 1. decision record with classes and owners | **this document §1** |
| 2. fixture-backed test | **done** — PR #2807 |
| 3. external verification boundary | **library + canonical bytes + vectors done; the (facts, act) container is the named missing piece** |
| 4. GEN stance | **§4** — new GEN context kinds, routed to #2602; Individual-owned `InstitutionalDomain` rejected for this use |
| 5. implementable spec | **§6** — with every MISSING step marked and its forbidden substitute named |

## 8. Routing

#2694 (ladder; container as the shared missing boundary) · #2599 (device case; ceremony, device
TLS binding) · #2602 (context kinds) · #2806 (ADR-0083 status drift) · PR #2807 (this branch).
