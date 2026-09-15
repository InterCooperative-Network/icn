---
Status: design-direction
Authority: architecture (forward-direction; normative only where it restates ADR-0032 or icn-civic-shell-v0)
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-09-15
Last Updated: 2026-09-15
Purpose: The trust-boundary decision for icn.zone join routes — which responsibilities may sit at the edge, which must remain ICN-authoritative, what possession of a join code confers, and the smallest safe implementation slices.
---

# `icn.zone/j/<code>` — trust boundary and join-route contract

> **One sentence.** A join code identifies an admission context; it does not
> grant membership — and every architectural decision below follows from
> refusing to collapse those two things.

**This document builds nothing.** It exists so that the next implementation
session can build Slice 1 without rediscovering the boundary. Companion to
[PUBLIC_SURFACE_DESIGN_CONTRACT.md](PUBLIC_SURFACE_DESIGN_CONTRACT.md), which
owns the shared *design* language; this document owns the *authority* question.

**Operating principle.** Infrastructure convenience must not decide
institutional authority. DNS, Cloudflare, KV, redirects, and short codes are
transport and discovery. They may help a person *find* an institution. They do
not decide who belongs to it.

---

## 1 · The central invariant

> **A join code identifies an admission context. It does not grant membership.**

A valid `/j/<code>` is permitted to mean:

> "This code refers to the worker cooperative Brightworks and opens its
> membership flow."

It must never mean:

> "Possession of this URL makes you a member."

### What possession actually confers

These are seven distinct things and collapsing any two of them is the failure
mode. Possession of a valid code confers **only the first two**:

| # | Category | Conferred by possession? |
|---|---|---|
| 1 | Knowledge of a public join route | **Yes** — that is the code's entire job |
| 2 | Ability to *begin* an admission flow | **Yes** — it opens a form, nothing more |
| 3 | Ability to *present* an invitation | **No** — presentation requires an identity to present it *as* |
| 4 | Invitation to *request* membership | **No** — anyone holding the URL may request; the code does not personalize the request |
| 5 | Eligibility evidence | **No** — eligibility is evaluated against the institution's admission rules, not against URL possession |
| 6 | Actual authorization | **No** — authorization is a mandate-gated decision |
| 7 | Actual standing | **No** — standing is governed state and arrives with a receipt |

Rows 3–7 are the ones an implementation will be tempted to fold into row 2
because it is convenient. Each fold moves an authority decision outward toward
the edge. A URL that conveys any of rows 3–7 is a **bearer credential**, and a
bearer credential resolved by third-party infrastructure is an authorization
decision delegated outside the trust boundary.

### Consequence

`/j/<code>` **resolves to a page, never to an outcome.** It must not grant
standing, mint a session, create a record, or mutate institutional state.
Admission happens afterwards, inside ICN, under a mandate, producing a receipt.

---

## 2 · Responsibility matrix

Classifications:

- **EDGE-SAFE** — may be performed entirely by edge infrastructure without
  becoming part of ICN institutional authority.
- **EDGE-CACHEABLE / ICN-AUTHORITATIVE** — the edge may relay or cache an
  answer, but ICN originates the state and the edge may never mutate it.
- **ICN-ONLY** — must happen inside ICN's governed path; may not be delegated to
  Cloudflare, DNS, KV, or any external routing infrastructure.

### Transport

| Responsibility | Class | Why · authoritative source · leak · failure if edge down/compromised |
|---|---|---|
| HTTP termination | **EDGE-SAFE** | No ICN semantics. Source: none. Leak: IP, UA, timing — already visible to any network path. Down: route unreachable; ICN state unaffected. Compromised: attacker can serve arbitrary content at the route — mitigated only by the fact that no authority decision happens here. |
| TLS | **EDGE-SAFE** | Cloudflare already terminates TLS for `icn.zone`. Leak: none beyond termination itself. Compromised: attacker sees codes in transit → treat every code as potentially disclosed, which is why codes confer only rows 1–2. |
| Path matching (`/j/*` vs fallthrough) | **EDGE-SAFE** | Pure routing. Down: falls through to root redirect. |
| Rate limiting | **EDGE-SAFE** *(and ICN must also enforce)* | Wants to be at the edge; carries no authority. **ICN must independently rate-limit** — an edge-only limit is bypassed the moment the gateway is reachable directly. Down: ICN limit still holds. |
| Abuse throttling / bot mitigation | **EDGE-SAFE** | Same. Compromised: throttling disabled → ICN-side limit is the real control. |

### The code itself

| Responsibility | Class | Why · authoritative source · leak · failure |
|---|---|---|
| Join-code generation | **ICN-ONLY** | A code is a handle on governed state; whoever mints it decides what admission contexts exist. Source: `icn-gateway` `InviteManager::create_invite` (admin-authenticated). Compromised edge cannot mint. |
| Entropy requirements | **ICN-ONLY** | Property of the generator. Current: 12 chars over a 32-symbol unambiguous alphabet = **60 bits**. Adequate; see §5. |
| Storage (record of record) | **ICN-ONLY** | The set of live codes is institutional state. **Today this is an in-memory `RwLock<HashMap>` — see §3, this is a blocker.** |
| Lookup / resolution | **EDGE-CACHEABLE / ICN-AUTHORITATIVE** | ICN answers "does this code refer to an open admission context, and for which institution." The edge may relay. See §4 for whether it may cache. Leak: which institutions exist and which codes are live. Down: route fails closed (§6). |
| Expiry | **ICN-ONLY** | Time-bound validity is part of the record. Source: `Invite::expires_at`, checked in `validate_invite`. An edge cache with TTL > remaining lifetime would serve an expired code. |
| **Revocation** | **ICN-ONLY** | The most authority-bearing property: it is the ability to withdraw an admission context. **Does not exist today — blocker, §3.** Revocation latency is the single strongest argument against an edge cache (§4). |
| Rotation | **ICN-ONLY** | Rotation = revoke + issue. Inherits both. |
| One-time vs multi-use semantics | **ICN-ONLY** | Determines whether a forwarded URL is reusable. Source: `Invite::used` / `mark_used`. Never an edge decision — an edge that could mark a code used could also *not* mark it used. |

### Destination

| Responsibility | Class | Why · leak · failure |
|---|---|---|
| Institution resolution (code → which institution) | **EDGE-CACHEABLE / ICN-AUTHORITATIVE** | The mapping is ICN state. Leak: institution membership of the code-holder's interest. |
| Institution display metadata (name, brand) | **EDGE-CACHEABLE / ICN-AUTHORITATIVE** | Public-facing and low-sensitivity, but it is what the visitor *trusts*. An edge that can alter displayed institution identity can phish. Cache only with short TTL. |
| Join-page destination selection | **EDGE-CACHEABLE / ICN-AUTHORITATIVE** | Must come from ICN. Edge-originated destination selection is redirect poisoning by design. |
| Redirect generation (302) | **EDGE-SAFE** *once the destination is ICN-supplied* | Mechanical. The authority is in *choosing* the destination, not emitting the header. |
| Redirect caching | **EDGE-CACHEABLE, constrained** | See §4. Default: **do not cache** until revocation exists. |

### Identity

| Responsibility | Class | Why |
|---|---|---|
| Identity collection | **ICN-ONLY** | Personal data crossing an edge boundary in a form the edge can retain is an unnecessary disclosure. Collected on the ICN-served join page, not at the edge. |
| Authentication | **ICN-ONLY** | An IdP may carry browser-session state. Per [`icn-civic-shell-v0.md`](../spec/icn-civic-shell-v0.md), the permitted direction is `DID / standing / mandate → short-lived session claim`; **forbidden: `IdP group → ICN authority`**. |
| Credential presentation | **ICN-ONLY** | ZKP / SDIS presentation paths are kernel-adjacent. |
| Member recognition | **ICN-ONLY** | "Is this person already a member here" is governed state. |
| Standing lookup | **ICN-ONLY** | Standing is scoped governed state; also privacy-sensitive across institutions. |

### Decision

| Responsibility | Class | Why |
|---|---|---|
| Admission-rule evaluation | **ICN-ONLY** | The institution's rules, evaluated against its own state. |
| Eligibility evaluation | **ICN-ONLY** | Same. |
| Approval workflow | **ICN-ONLY** | |
| Human approval | **ICN-ONLY** | Today the only always-on admission path is the steward vouch/reject surface (§3). |
| Governance approval | **ICN-ONLY** | |
| Mandate evaluation | **ICN-ONLY** | Mandates are the authority primitive. Delegating this *is* the landlord failure. |

### Mutation and proof

| Responsibility | Class | Why |
|---|---|---|
| Membership creation | **ICN-ONLY** | |
| Standing creation / change | **ICN-ONLY** | |
| Institutional-state mutation | **ICN-ONLY** | |
| Receipt creation | **ICN-ONLY** | ADR-0026 envelope; kernel path. |
| Receipt signing | **ICN-ONLY** | Signing keys never leave ICN custody. |
| Receipt persistence | **ICN-ONLY** | |
| Receipt verification | **ICN-ONLY** *(third parties may verify independently)* | Verification is open by design, but the edge is not a verifier in this flow. |
| Audit / provenance records | **ICN-ONLY** | |
| Revocation records | **ICN-ONLY** | |
| Security-event records | **ICN-ONLY** for authority-bearing events; edge logs are operational telemetry only and **must not record codes** (§5). |

**Everything below the "Transport" block that is not explicitly EDGE-SAFE is
ICN-ONLY or ICN-authoritative. The edge's entire job is: terminate, match, throttle,
relay, redirect.**

---

## 3 · Existing primitives — inventory

Searched for opaque invitation codes, enrollment tokens, one-time codes,
expiring/revocable capabilities, invitation objects, enrollment ceremonies,
bootstrap/join links, QR/pairing codes, magic-link flows, capability URLs, and
nonce-backed admission.

### Primitive 1 — gateway invite code *(the only real candidate)*

| Property | Value |
|---|---|
| Lives in | `icn/crates/icn-gateway/src/invite.rs` (266 lines), struct `Invite` |
| HTTP | `POST /v1/invites` (create, **admin-auth**), `GET /v1/invites`, `POST /v1/invites/join` (`invites.rs`, handler `join_via_invite`, body `JoinRequest{invite_code, did}`) |
| Currently authorizes | Redemption mints a **session JWT** scoped to `(coop_id, role)` |
| Entropy | 12 chars × 32-symbol alphabet `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` = **60 bits**. Alphabet deliberately excludes `I O 0 1` — correct for a code read aloud, typed, or printed on a card |
| Expiry | **Yes** — `expires_at`, caller-supplied `expires_in_seconds`, enforced in `validate_invite` and `mark_used` |
| Revocability | **No.** No `revoke_invite` exists anywhere in `icn/crates`. Live until expiry or redemption |
| Single/multi use | Single — `used` / `used_by` / `used_at`; `mark_used` rejects reuse |
| Persisted | **No.** `RwLock<HashMap<String, Invite>>`, in-memory. **Every outstanding invite is lost on restart** |
| Produces a receipt | **No** |
| Read-only resolution | **Yes** — `validate_invite(code)` checks exists / not-used / not-expired and mutates nothing. This is exactly the operation `/j/` needs |

### Primitive 2 — `EnrollmentSession` *(not a fit)*

`icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs` — `enrollment_id` plus
a `VERIFY-NNNN` code (~9,000 values), fixed 24h expiry, with
`rejected`/`rejection_reason`/`rejected_by`. Low entropy, not single-use, not a
bearer capability. Its rejection path is the closest thing to revocation in the
codebase, but the code itself is a human-verification step inside an already-open
session, not an entry point.

**Route status matters here:** the public self-serve enrollment tree is
**flag-gated off and returns 404 on every ordinary profile** — the code is
explicit that it is "unauthenticated by construction and terminates in a
credential mint, so on every ordinary profile the route tree is absent (404)
rather than present-and-guarded." The always-on admission surface is the
**steward vouch/reject** path (`POST /v1/sdis/vouch/{id}`, `/reject/{id}`),
behind JWT auth plus `authorize_steward_act`.

### Not admission primitives

- `icn-steward/src/ceremony/enrollment.rs` (417 lines) — a pure state machine. **No HTTP routes.**
- `icn-gateway/src/api/sdis/enrollment.rs` (543 lines) — **dead**, registration commented out.
- `icn-identity/src/authority_log/admission.rs`, `icn-net/src/preauth_admission.rs` — **node/peer** admission, not human membership. Do not conflate.

### Nothing else exists

No capability-URL, magic-link, or one-time-token primitive. The only other
workspace hits are per-peer replay nonces in `icn-gossip` (wrong layer).

### Verdict: **extend, then wrap** — do not replace, do not invent a second code

The code *format* is already right: 60 bits, unambiguous alphabet, expiring,
single-use, with a read-only validator. Inventing a parallel "join code" concept
beside `Invite` would give the project two overlapping bearer-token vocabularies,
which is the duplication failure this repo keeps paying for elsewhere.

But the existing primitive **cannot back a public URL as it stands**, and these
are requirements discovered by this analysis rather than work to do inside it:

1. **Durability** — in-memory storage means a gateway restart silently
   invalidates every outstanding join link. A printed QR code that stops working
   after a deploy is not a join mechanism.
2. **Revocation** — a forwarded or leaked URL cannot be withdrawn. This is the
   property that makes a public link defensible, and it is absent.
3. **A read-only public resolution endpoint** — `validate_invite` is the right
   operation but is not exposed unauthenticated, and must not be without
   ICN-side rate limiting.
4. **Separation of resolution from redemption** — `join_via_invite` currently
   *mints a session JWT*. `/j/` must reach resolution without touching that path,
   or the invariant in §1 is violated at the first endpoint.

Both (1) and (2) belong to the invite primitive itself and are scoped as a
separate implementation task (§8). (3) and (4) are the wrapper, and are Slice 1.

---

## 4 · The Cloudflare KV decision

[`ICN_ZONE_ROUTING.md`](../deployment/ICN_ZONE_ROUTING.md) currently leaves this
as a disjunction inside a runbook step — "looks up code in Cloudflare KV **or**
queries the ICN gateway API." Those are not variants of one design.

| | **A — Cloudflare-authoritative** | **B — ICN-authoritative** | **C — ICN-authoritative + bounded edge cache** |
|---|---|---|---|
| Authority | KV holds `code → destination`. Cloudflare is *in* the trust boundary | ICN owns the record; Worker relays | ICN owns the record; edge caches the answer |
| Revocation latency | Unbounded — requires a KV write ICN does not control | **Immediate** | Bounded by TTL — a revoked code keeps working for the TTL window |
| Privacy | Codes and institution mapping resident at a third party | Codes transit the edge; nothing resident | Codes and answers resident for TTL |
| Enumerability | KV is a complete enumerable list of live codes | Enumeration must go through ICN rate limiting | Partial list resident at edge |
| Offline behavior | Works while ICN is down — **which is the danger**: it admits into an institution whose state cannot be consulted | Fails closed | Serves stale answers while ICN is down |
| Compromise impact | Full: attacker redirects any join code, mints new ones, enumerates institutions | Attacker sees codes in transit and can deny service; cannot alter ICN state | Attacker can serve stale/poisoned answers for TTL |
| Operational complexity | Two stores to keep consistent | One store | One store plus invalidation |
| Cloudflare dependency | **Authority-level** | Availability-level only | Availability + freshness |
| Provenance | Resolution has no ICN-side record | Every resolution observable by ICN | Cache hits invisible to ICN |
| Disaster recovery | KV must be restored *and reconciled* | Restore ICN | Restore ICN; cache self-heals |
| Custom-domain compatibility | Poor — an institution on its own DNS would need a parallel mechanism | **Good** — resolution is an ICN API any front door can call | Good |

### Recommendation: **Model B**, and not C yet

Two reasons, in order.

**Revocation latency is the deciding factor, and revocation does not exist yet.**
Adopting C today would bake a revocation-latency floor into the architecture
*before* the revocation mechanism it is supposed to be a tradeoff against has
been built. That is the wrong order: you cannot reason about an acceptable
staleness window for withdrawing an invitation when withdrawal is not
implemented. Build B, build revocation, measure, and only then decide whether a
cache buys enough to be worth a window in which a revoked invitation still opens.

**Model A is disqualified on authority, not on performance.** Its "works while
ICN is down" property is not a benefit here — a join route that resolves while
the institution's state cannot be consulted is precisely the failure the §1
invariant exists to prevent. And it is the concrete form of the landlord
problem: Cloudflare would hold the authoritative answer to "which institution
does this admission context belong to."

The runbook's stated benefit — "Worker deployment does not require changes to the
ICN Rust codebase" — is true and welcome for a dumb redirect. For anything that
resolves a code it is a warning: it means resolution logic would be unversioned
relative to the kernel it fronts, outside review, and outside `just website-verify`.

**If C is later adopted**, the constraints are: TTL ≤ 60s; cache keyed on the
code; negative results not cached; the cached payload carries only *public join
context* (institution display name and join-page URL), never eligibility,
standing, or identity; and ICN can purge. Do not adopt C to reduce ICN load — a
join route's traffic is human-scale.

### Required doc change

`ICN_ZONE_ROUTING.md` Phase 2 step 2 must stop saying "Cloudflare KV **or**
queries the ICN gateway API" and say the chosen model. That is a one-line
runbook edit, listed in §8.

---

## 5 · Threat model

Only threats with a plausible path, each tied to a mitigation.

| Threat | Plausible? | Mitigation |
|---|---|---|
| **Code guessing / brute force** | Yes, cheap to attempt | 60 bits is already sufficient against guessing; the real control is **ICN-side rate limiting** on the resolution endpoint, plus edge throttling. Edge-only limiting is bypassable |
| **Enumeration** | Yes | Rate limit; uniform response shape and timing for invalid / expired / used / revoked so probing cannot distinguish them; never return institution metadata for an invalid code |
| **Leaked URL** (forwarded mail, shared chat) | **Very likely — treat as the default case** | §1 is the mitigation: possession confers only rows 1–2. Plus expiry, single-use, and revocation |
| **Browser history / screenshots** | Yes | Unavoidable; same answer as above. Do not put the code in the final join-page URL — redirect to a URL that does not contain it |
| **Referrer leakage** | Yes | `Referrer-Policy: no-referrer` on the resolution response and the join page |
| **Search indexing** | Yes | `X-Robots-Tag: noindex, nofollow` on `/j/*`; `Disallow: /j/` in `icn.zone` robots.txt. A code in a search index is a permanently leaked invitation |
| **Replay of a redeemed code** | Yes | Already handled — `used` flag, `mark_used` rejects reuse |
| **Stale / expired invite** | Yes, routine | Already handled — `expires_at` |
| **Revoked invite** | Yes | **Not currently mitigable — no revocation exists.** Blocker |
| **Institution impersonation** | Yes | Institution display metadata must come from ICN per resolution, never from the code or a URL parameter |
| **Phishing via lookalike path** (`icn.zone/J/…`, `icn-zone.com/j/…`) | Yes | Cannot be solved at this layer. Reduce by keeping `/j/` the *only* code-bearing route and by the join page displaying institution identity from ICN-resolved data |
| **Cloudflare account compromise** | Low probability, high impact | Model B bounds it: attacker can deny service and see codes in transit, but cannot alter ICN state or admit anyone |
| **KV compromise** | N/A under Model B | Eliminated by the choice |
| **Gateway compromise** | High impact | Outside this boundary; in scope for ICN's own threat model |
| **Denial of service** | Yes | Edge throttling; ICN rate limit; route **fails closed** |
| **Redirect / cache poisoning** | Yes under A/C | Eliminated under B (no cached authority). If C is adopted, short TTL + purge |

### Derived requirements

- **Entropy:** ≥ 60 bits. Current generator already satisfies this — do not lower it.
- **Alphabet:** keep the existing 32-symbol unambiguous set; it is correct for printed and spoken codes.
- **Expiry:** required, caller-set, enforced server-side. Already present.
- **Revocation:** required before public exposure. **Missing.**
- **Rate limiting:** ICN-side, mandatory. Edge-side, additional.
- **Cache-control:** `no-store` on resolution responses under Model B.
- **`Referrer-Policy`:** `no-referrer`.
- **Indexing:** `X-Robots-Tag: noindex, nofollow` + robots.txt `Disallow: /j/`.
- **Logging:** codes are secrets. **Never log a raw code** — at the edge or in ICN. Log a truncated salted hash if correlation is needed.
- **Analytics:** no third-party analytics on `/j/*`. The URL *is* the secret; sending it to an analytics vendor discloses it.
- **Observability:** count resolutions by outcome class (valid / invalid / expired / used / revoked) and by institution. That is sufficient for operating the route without recording a single code.

---

## 6 · Layered flow

Preferred flow, with the boundary each transition crosses.

```text
GET https://icn.zone/j/<code>
  │  boundary: public internet → edge
  │  actor: anonymous visitor   data: code, IP, UA   read-only   no mandate   no receipt
  ▼
EDGE — terminate, match /j/*, throttle, relay
  │  boundary: edge → ICN gateway (Model B: no cache, no KV)
  │  actor: Worker (unprivileged relay)   data: code   read-only   no mandate   no receipt
  ▼
ICN — resolve code (read-only; `validate_invite` semantics)
  │  answers: is there an open admission context, and for which institution
  │  MUST NOT: mint a session, mark used, create state
  │  returns: public join context (institution display identity + join-page URL)
  ▼
PUBLIC JOIN CONTEXT  ← served by ICN, not the edge
  │  the page a human sees. No identity collected yet
  │  boundary: anonymous → identified
  ▼
IDENTITY / CREDENTIAL STEP        ICN-ONLY
  │  actor: prospective member   data: DID / credential presentation   mandate: none yet
  ▼
INSTITUTION ADMISSION POLICY      ICN-ONLY
  │  eligibility evaluated against governed state   read-only
  ▼
AUTHORIZED APPROVAL / GOVERNANCE  ICN-ONLY
  │  actor: steward or governance path   mandate: REQUIRED
  │  today the only always-on path is steward vouch/reject
  ▼
STANDING / MEMBERSHIP MUTATION    ICN-ONLY
  │  MUTATION — the first one in the entire flow
  │  mandate: REQUIRED   card_kind: MembershipApproval
  ▼
RECEIPT + PROVENANCE              ICN-ONLY
     ADR-0026 envelope, signed, persisted
```

**The single most important property of this diagram: the first mutation is
seven steps below the edge.** Everything above it is read-only. That is what
makes delegating the top two layers to third-party infrastructure defensible.

The confirm step before the mutation inherits the ten-step pre-confirm contract
in [`member-shell-v0.md`](../spec/member-shell-v0.md) §"Signing / confirmation
flow" — including the **named receipt class** stated before confirmation.

---

## 7 · Why `/n/<name>` is deferred, and is not a short-link variant

`/j/` and `/i/` are handles on *bounded, revocable, replaceable* records.
`/n/<name>` is a handle on an institution's **identity**, and identity is
permanent, contested, and transferable. It is a domain-binding problem wearing a
short-link costume.

Unresolved questions it opens, none of which `/j/` opens:

- **Namespace ownership** — who decides that `/n/brightworks` means Brightworks?
- **Collisions** — two cooperatives with the same common name.
- **Squatting** — names are valuable precisely because they are memorable.
- **Renames and transfers** — institutions merge, split, dissolve, rename. Does the old name redirect? Forever? Who decides?
- **Persistence expectation** — a printed `/n/` name is expected to work indefinitely; a `/j/` code is expected to expire.
- **Apparent legitimacy** — an `icn.zone/n/<name>` route reads as ICN *conferring* recognition, which is the landlord failure in its purest form.
- **Custom-domain parity** — an institution on its own DNS must not be second-class. `/n/` inherently privileges ICN-hosted names.
- **Requires `DnsBinding` + `PublicationReceipt`** — neither exists in code. `DOMAIN_ROUTING_AND_DNS_BINDINGS.md` models `/n/` as a `DnsBinding` of purpose `short_route`, with a TXT-challenge verification flow and a binding receipt. That is the correct home for it.

**`/n/<name>` is blocked on `DnsBinding` existing. `/j/<code>` is not.**

---

## 8 · Slices

### Slice 0 — infrastructure only, implementable now, independent

```text
icn.zone/            → 301 https://intercooperative.network/  (path-preserving)
/j/  /i/  /n/        → reserved; do NOT fall through to the redirect
```

No join implementation, no UI, no ICN authority semantics, no DNS change beyond
the redirect rule.

**Can this be done immediately?** Yes. It touches no ICN code and resolves no
codes, so none of this document's blockers apply. Exact change set:

1. Cloudflare Redirect Rule on the `icn.zone` zone: match `hostname eq "icn.zone"`, dynamic redirect to `https://intercooperative.network${uri.path}`, 301. (Runbook steps already written in `ICN_ZONE_ROUTING.md` Phase 1.)
2. A rule ahead of it that returns **404 for `/j/*`, `/i/*`, `/n/*`** so reserved prefixes do not silently redirect to a canonical-site 404 and become indexed.
3. `icn.zone` `robots.txt`: `Disallow: /j/`, `/i/`, `/n/`.
4. **Repo change:** edit `ICN_ZONE_ROUTING.md` Phase 2 step 2 to state Model B instead of the "KV or gateway" disjunction, and register the file in `docs/registry.toml` (it is currently unregistered, which is why it is the weakest doc in this stack).
5. Verify: `curl -I https://icn.zone/anything` → 301 preserving path; `curl -I https://icn.zone/j/x` → 404.

Items 1–3 are Cloudflare console operations; item 4 is the only code-review-able change.

### Slice 1 — `/j/<code>` resolution. **Blocked.**

Smallest honest vertical path: edge relays to an ICN read-only resolution
endpoint, which returns a public join context; the join page is served by ICN.

**Do not start it yet.** Two required primitives do not exist (§3): invite
**durability** and invite **revocation**. A public join URL backed by in-memory,
non-revocable state is not defensible — a deploy silently voids every
outstanding invitation, and a leaked link cannot be withdrawn.

Prerequisite task, scoped separately and sized small:

> **Make the gateway invite record durable and revocable.** Extend
> `icn/crates/icn-gateway/src/invite.rs`: persist the `Invite` record instead of
> `RwLock<HashMap>`, and add revocation (`revoked_at` / `revoked_by` / reason)
> honored by `validate_invite` and `mark_used`. Keep the existing 12-char
> 32-symbol code format and expiry semantics unchanged. Do **not** change
> `join_via_invite`'s redemption behaviour in this task. Extend the primitive —
> do not introduce a second code concept.

Then Slice 1 proper:

> **Add an ICN-authoritative, read-only join-code resolution endpoint and route
> `icn.zone/j/<code>` to it.** The endpoint answers only: is there an open
> admission context for this code, and which institution does it belong to —
> returning public join context (institution display identity + join-page URL).
> It must not mint a session, mark the code used, or create state; it must not
> reuse `join_via_invite`. Uniform response shape and timing across invalid /
> expired / used / revoked. ICN-side rate limiting, `no-store`,
> `Referrer-Policy: no-referrer`, `X-Robots-Tag: noindex, nofollow`. Never log a
> raw code. Cloudflare Worker relays only — Model B, no KV, no cache.

### Explicitly not in Slice 0 or 1

`/i/<code>` · `/n/<name>` · DNS-binding UI · institution-name registry ·
general-purpose shortener · learning site · second marketing site · any
production claim not backed by current implementation.

---

## 9 · Blockers

Genuine blockers only.

| # | Blocker | Blocks | Status |
|---|---|---|---|
| 1 | Invite records are in-memory (`RwLock<HashMap>`) | Slice 1 | Real. Needs the scoped task in §8 |
| 2 | No invite revocation anywhere in `icn/crates` | Slice 1 | Real. Same task |
| 3 | No read-only public resolution endpoint separated from redemption | Slice 1 | Real. Is Slice 1 |
| 4 | `ICN_ZONE_ROUTING.md` still says "KV **or** gateway" | Slice 0 (doc half) | One-line edit, §8 item 4 |
| 5 | `DnsBinding` / `PublicationReceipt` do not exist | `/n/<name>` only | Deferred by design, §7 |

**Not blockers:**

- **RFC-0015's `draft` status.** The canonical-surface question is settled by
  ADR-0032, and the readiness audit's prescribed amendment was applied on
  2026-04-28. Its remaining open questions are all about `learn.icn.zone` and
  adjacent surfaces. See [PUBLIC_SURFACE_DESIGN_CONTRACT.md](PUBLIC_SURFACE_DESIGN_CONTRACT.md) §7.
- **ADR-0033's `proposed` status.** Its linter is unbuilt, so evidence discipline
  is editorial today. It does not gate a route that makes no maturity claims.
- **Absence of a member-shell join surface.** Real, but it is *work*, not a
  blocker on Slice 0 or on the resolution endpoint.

---

## Related

- [PUBLIC_SURFACE_DESIGN_CONTRACT.md](PUBLIC_SURFACE_DESIGN_CONTRACT.md) — the shared design language and surface boundary
- [../spec/icn-civic-shell-v0.md](../spec/icn-civic-shell-v0.md) — domain/route doctrine, authority direction
- [../spec/member-shell-v0.md](../spec/member-shell-v0.md) — the ten-step pre-confirm contract
- [../adr/ADR-0032-website-truth-boundary.md](../adr/ADR-0032-website-truth-boundary.md)
- [../adr/ADR-0027-action-card-contract.md](../adr/ADR-0027-action-card-contract.md) — card fields, `MembershipApproval` kind
- [../adr/ADR-0026-receipt-and-provenance-proof-envelope.md](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)
- [../architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md](../architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md) — where `/n/<name>` belongs
- [../deployment/ICN_ZONE_ROUTING.md](../deployment/ICN_ZONE_ROUTING.md) — the runbook, needs the Model B edit
