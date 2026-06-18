---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-06-18
---

# Abuse-case hardening strategy

> **Receipts prove events. They do not prove legitimacy.**
>
> **Authority shortcuts must label themselves as shortcuts.**
>
> **Unresolved standing is not standing in production.**

This document codifies the durable hardening doctrine that prevents ICN from becoming an administrative panel with receipts. It sits alongside [`ARCHITECTURE_DUE_DILIGENCE.md`](ARCHITECTURE_DUE_DILIGENCE.md) (twin doctrines: convenience-vs-authority, participation-access-is-architecture) and extends that reflex into the institutional-legitimacy layer.

The doctrine is not a security checklist for transport encryption, key management, or replay defense. Those live in [`../security/production-hardening.md`](../security/production-hardening.md). This is the upstream layer that asks whether a procedurally clean record actually corresponds to an institutionally legitimate outcome — and what the substrate must do, in code and in surfaces, so the answer cannot be "no" while the receipt still looks like "yes."

<!-- truth: descriptive -->

## 0. Status and non-claims

- **Status**: descriptive strategy. No runtime behavior is changed by this document.
- **No new ADR.** The doctrine extends `ARCHITECTURE_DUE_DILIGENCE.md`; the operational decisions are deferred to follow-up PRs that may file ADRs as the design firms up.
- **No new contract URN, no new schema field, no new wire format.** Candidate vocabulary in §6 and §10 is **draft** and explicitly marked as such.
- **No production-readiness claim, no live-federation claim, no formal-NYCN-pilot claim, no Phase 2 completion claim.** Phase 2 (see [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md)) remains ⏳ and partner-bound; this document sequences hardening work that must precede a partner-formalization step that does not exist yet.
- **No K3s, DNS, Forgejo, gateway-deploy, or NYCN partner-data mutation.**
- **Code anchors below are pinned to `main` at session start (`d57ff1d6e`).** They may drift; re-verify before citing in follow-up PRs. The §2.11 / §3.11 / §4.11 additions (2026-06-18) are pinned to `main @ 040e44f0`.

<!-- truth: descriptive -->

## 1. Why this exists

The strongest current red-team finding against ICN is not that someone might corrupt the receipt store or forge a signature. Those failure modes are already addressed by the opaque hash-keyed receipt store, the `SignedEnvelope` replay-guard pattern, the QUIC/TLS DID-binding layer, and the regular audit verification CLI.

The stronger finding is that someone can use ICN's primitives **correctly** — issue a capability token that has the right scope, call a route that the gateway is happy to authorize, file a proposal whose decision hash binds cleanly, watch the effect dispatch chain produce an evidence record — and end up with procedurally clean machinery that proves an institutionally illegitimate outcome.

A worked example, written from code currently in `main`:

1. A single broad capability string `governance:write` (`icn/crates/icn-rpc/src/auth.rs:947`) gates roughly twelve mutation endpoints in `icn/apps/governance/src/http/handlers.rs` — among them `create_domain` (l. 386), `activate_charter` (l. 694), `create_proposal` (l. 751), `cast_vote` (l. 1695), `add_domain_member` (l. 548), `remove_domain_member` (l. 593), `create_structure` (l. 3415), `create_meeting` (l. 3678), and `create_action_item` (l. 2550).
2. A capability token carrying that scope can create a plausible domain, activate a permissive charter via the direct path, seed a member list, file proposals against rules it just authored, mutate membership directly between proposals, and produce procedurally clean receipts the whole way through.
3. Each individual step has a corresponding receipt; each receipt is structurally valid; the audit-verify CLI says everything checks out. The institution that emerged from the sequence may already be captured.

This document writes the doctrine, invariants, and follow-up tracks that prevent that sequence from ever being procedurally clean — without widening the meaning firewall, without importing domain semantics into the kernel, and without inventing a SaaS-style RBAC pyramid that ignores ICN's mandate / capability model.

<!-- truth: descriptive -->

## 2. Core doctrine

> The danger is not only that someone might break the machinery. The danger is that someone might use the machinery correctly to prove the wrong thing.

The doctrine below is the upstream version of the more specific rules in §3–§13. Each line is a sentence reviewers are expected to be able to quote at design time.

### 2.1 Receipts prove events, not legitimacy

A receipt establishes that some specific event occurred under some specific authority basis. It does not, on its own, establish that the event was institutionally legitimate. A reviewer reading a receipt must always be able to recover the authority basis, the actor identity, the standing at the time, and any administrative-shortcut flag — and must be able to challenge the receipt without challenging the substrate.

### 2.2 Authority shortcuts must label themselves as shortcuts

Some paths in the substrate are intentionally not democratic: direct charter activation, direct membership mutation, bootstrap onboarding, fixture-mode seeding. These exist for legitimate reasons. They must never look like ordinary governance in any surface — not in receipts, not in member shell, not in the steward cockpit, not in API responses. The convenience of the shortcut does not buy the legitimacy of the long path.

### 2.3 Unresolved standing is not standing in production

If the substrate cannot resolve whether an actor has standing, the production answer is not "yes by default." Trust-threshold membership, suspension state, steward authority, delegation eligibility — each of these has a resolver or checker today. In production, the absence of a resolver, an unreachable resolver, or a stale resolver answer must not cause the substrate to act as though standing is established.

### 2.4 Accepted is not applied

A proposal can be `Accepted` and still have produced no effect. The effect dispatch chain (decision → mandate → effect plan → dispatch → application) has many ways to halt or partially complete, several of which are honestly recorded as `EmittedOnly`, `ExecutionEvidenced`, or `ExecutionFailed` in `icn/apps/governance/src/dispatch_evidence.rs:131–169`. A surface that renders `Accepted` as `done` is laundering uncertainty into confidence.

### 2.5 Convenience paths must not become authority paths

A surface introduced for convenience — a direct admin endpoint, a fixture-mode shortcut, a debug bypass — must not silently become the path institutional decisions travel through. This is the legitimacy-layer analogue of `ARCHITECTURE_DUE_DILIGENCE.md` §2.1; that doctrine names DNS and SaaS; this one names admin scopes and bootstrap flags.

### 2.6 Bootstrap is not democracy

The substrate must be bootstrappable. The first charter, the first member set, the first steward have to come from somewhere outside the governance loop. That somewhere is administrative, by definition. Bootstrap artifacts must be visibly bootstrap artifacts, not retroactively dressed as democratic outputs.

### 2.7 A token is not a mandate

Capability scopes (`governance:write`, etc.) are kernel-side enforcement primitives. They are not, on their own, institutional mandates. The institution authorizes by mandate; the substrate authorizes by capability. Where the two diverge, the institution's view wins, and the surface must show the divergence.

### 2.8 A UI must not launder uncertainty into confidence

Member shell and steward cockpit are designed to render real states: degraded, partial, stale, disputed, emitted-only, challenge-window-open, awaiting-evidence, reversed. A surface that renders any of these as a clean green checkmark fails the doctrine — even if every receipt under it is real.

### 2.9 Privacy posture is not private content

`ScopedVault` and `PrivateEvidence` define what bodies a surface may read. Existence, scope, access path, authority basis, access receipts — these are renderable. Body bytes are not. Debug convenience cannot lift a body byte into a rendering layer; if it has, the surface has become a surveillance panel by accident.

### 2.10 Index absence is not record absence

A receipt may exist in the primary store and be missing from a secondary index. In the opaque receipt store this is impossible by construction (atomic primary + index + bind writes). On older typed-receipt paths it is currently possible. Absence from index must never silently become absence from truth.

### 2.11 A global id is not a grant

An object reachable by a globally-keyed id must still be proven to belong to the institutional context named by the request before it is returned. Knowing — or guessing — an object's id is not standing over the object. A correct check on the *caller's* context (path coop, token scope) does not bind the *object* to that context; the object carries its own context and must be matched to the request's. Where they differ, the answer is not-found, and a foreign object must be reported exactly as a missing one, so the endpoint cannot be used to enumerate or confirm objects in contexts the caller has no standing in.

<!-- truth: descriptive -->

## 3. Abuse stories

Each story below is short on purpose. Each maps to a hardening track in §4. Each cites at least one code anchor so a follow-up PR has somewhere to start.

### 3.1 The broad-scope institution factory

A member with `governance:write` mints a new `LocalDomain` via `create_domain` (`handlers.rs:386`), activates a permissive charter directly via `activate_charter` (`handlers.rs:694`), seeds a member list with `add_domain_member` (`handlers.rs:548`), files and closes proposals under rules they just authored, and produces a complete trail of clean receipts. Anchor: `icn/crates/icn-rpc/src/auth.rs:947` (single broad scope).

### 3.2 Membership coup by direct mutation

A member with `governance:write` calls `add_domain_member` to add allies and `remove_domain_member` (`handlers.rs:593`) to remove opposition, then files a decisive proposal against the altered electorate. Both endpoints fail-open on `TrustThreshold` membership domains (`handlers.rs:572` for add, `:617` for remove); the resolver, if any, is consulted only at proposal-close time (`:1269`), never at mutation time.

### 3.3 Resolver-outage democracy

A `TrustThreshold` domain comes up with `MemberStandingChecker` (`configure.rs:197-201`), `SuspensionChecker` (`configure.rs:214-218`), or `MembershipResolver` (`configure.rs:264`) unwired, miswired, or degraded. Write paths short-circuit to `true` (`handlers.rs:78, 572, 617, 1111, 1203`). Suspended or non-eligible actors influence governance, and the evidence trail is internally consistent but institutionally wrong. The existing inline comment at `handlers.rs:1267–1268` ("Fail-open on resolver errors") is a known trade-off that has not yet been re-evaluated against a production threat model.

### 3.4 Admin monarchy with YAML

A privileged operator calls `POST /charters` → `activate_charter` (`handlers.rs:694–744`). The handler constructs a synthetic provenance string `direct-activation:{charter_id}` and proceeds without the ratified-proposal decision hash, without an `InstitutionalEffectRecord`, and without `EffectDispatchEvidence`. The charter activates; the substrate begins enforcing it; the authority basis is "someone with `governance:write` did it."

### 3.5 The receipt halo effect

A member opens the member shell, sees a row labeled "Accepted," and concludes the institution decided the matter. In fact the proposal may be `Accepted` and `EmittedOnly` (no evidence yet), `Accepted` and `ExecutionFailed` (anchor: `dispatch_evidence.rs:146`), `Accepted` and partial, `Accepted` and pending a challenge window, or `Accepted` and reversed. The current API surface and the rendering layers above it do not consistently expose this distinction.

### 3.6 Retry / restart / partial-effect ambiguity

A high-impact effect partially applies before a restart. The substrate retries. One subsystem no-ops correctly; another mutates a second time; a third returns opaque evidence. The current idempotency key for institutional effects is at institutional-effect granularity, not per-effect granularity; the stronger `(decision_hash, manifest_hash, effect_index)` key named by [`../spec/effect-dispatch-contract.md`](../spec/effect-dispatch-contract.md) is future schema work and is not yet in the dispatch path.

### 3.7 The fake-domain factory

A domain is created with technically valid but institutionally garbage governance parameters — quorum 1, approval threshold 1, voting period zero, no challenge window, no notification requirement. Current CCL governance validation (`icn/crates/icn-ccl/src/governance.rs:146-163`) is semantic-only; there are no pilot-readiness sanity bounds and no escape-hatch labeling for fixture-mode permissive configs.

### 3.8 Visual greenwashing

Implementation drift between [`../spec/member-shell-v0.md`](../spec/member-shell-v0.md) and [`../spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) lets a degraded, partial, stale, or disputed state render as a clean green checkmark in member shell while the cockpit shows red. The v0 spec already flags this as a failure-table violation; the fixture matrix that catches it before merge does not yet exist.

### 3.9 Surveillance by debug

A rendering layer in the cockpit, or a "debug preview" in the shell, dereferences a `PrivateEvidence` body when it should be displaying only existence + scope + authority basis + access receipts. The current digest-only rendering boundary in `icn/crates/icn-kernel-api/src/proofs.rs:646-661` (`ArtifactDigest::scoped_vault_reference`) is clean; the abuse case is the regression test that doesn't yet exist.

### 3.10 Index-skew invisibility

A crash mid-write on an older typed-receipt path leaves the primary record durable but the secondary index missing. The opaque receipt store is atomic by construction; `icn/apps/governance/src/receipt_backend.rs:173-210` documents that the **default** `put_mandate_with_grants` is **not atomic** and depends on the sled-backed gateway override. Other typed write paths have not been inventoried for the same invariant.

### 3.11 Cross-context read by global id

A member holding a valid `coop-a` token requests `GET /treasury/coop-a/budgets/{id}` with a `coop-b` budget id they knew or guessed. The flat guard passes (token coop == path coop), the budget is fetched by its globally-keyed id, and — before #2087 — returned, leaking `coop-b`'s treasury DID, purpose, and amount across the institutional boundary. The leak survived a *correct subject check* because the loaded object was never bound to the named context. Closed for this endpoint by #2087 (`icn/crates/icn-gateway/src/api/treasury.rs:699-702`, the `treasury_did` ownership match in `get_budget`; tests `test_get_budget_cross_coop_read_denied`, `_same_coop_read_ok`, `_path_coop_without_treasury_does_not_reveal_foreign_budget`). The class is older than this instance — caller-context siblings hardened in #2052 / #2056 / #2058 — and the cross-gateway generalization is still open as the enumeration-safe-404 audit **#1642**. Anchor: `icn/crates/icn-gateway/src/api/treasury.rs:647` (`get_budget`; generic `"Budget not found"` at `:670`, ownership match at `:699-702`).

<!-- truth: descriptive -->

## 4. Hardening tracks

Each subsection has the same shape: problem, hardening goal, doctrine line for future PRs to quote, draft design (explicitly marked draft), and code anchors.

### 4.1 Narrow production authority scopes

**Problem.** `governance:write` is a single broad capability across ~12 mutation endpoints; one capability token is an institution factory.

**Hardening goal.** Production write authority is decomposed so the surface that issues a capability can describe what the capability actually authorizes — at the level of an institutional act, not "all of governance."

**Doctrine.** *A capability scope must describe an act the institution can authorize, not a directory of routes the gateway can dispatch.*

**Draft design.** Two complementary approaches; both are draft.

- **Scope decomposition (the SaaS-shaped option, retained for kernel-side enforcement).** Candidate scopes, names not minted:
  - `governance:domain:create`
  - `governance:charter:activate` (bootstrap-only, see §4.4)
  - `governance:membership:mutate` (bootstrap-only, see §4.2)
  - `governance:proposal:create`
  - `governance:proposal:lifecycle`
  - `governance:vote:cast`
  - `governance:action-item:mutate`
  - `governance:meeting:mutate`
  - `governance:structure:mutate`
  - `governance:federation:propose`
  - `governance:sdis:propose`
- **Mandate-bundle gating (the ICN-native option, preferred where the model supports it).** Where the act being authorized is institutional rather than purely technical, the gate is a mandate or authority grant from the governance app, not a kernel-side capability string. The kernel still enforces; the gate is just "does the actor hold a mandate that authorizes this act in this domain at this height" — which is constraint enforcement, not domain semantics inside the kernel.

The two are not in tension: the kernel-side enforcement primitive is the capability; the institutional gate that produces the capability is the mandate. The split is **where the institution authorizes** versus **how the kernel enforces**.

**Anchors.** `icn/crates/icn-rpc/src/auth.rs:947` (scope string), `icn/apps/governance/src/http/handlers.rs:283–2342` (route handlers gated by it).

### 4.2 Direct membership mutation as bootstrap-only

**Problem.** `add_domain_member` and `remove_domain_member` mutate the electorate without proposal → decision → mandate → effect → evidence, and fail-open on `TrustThreshold` domains.

**Hardening goal.** Production membership mutation flows through the governance loop. Direct mutation, if it remains in production, is classified as a bootstrap / administrative shortcut and labels itself as one in every surface it touches.

**Doctrine.** *Direct membership mutation is not a normal democratic act. Authority shortcuts must label themselves as shortcuts.*

**Draft design.** Direct mutation, where retained, emits an administrative receipt that includes:

- signer DID and capability used,
- authority basis (text, e.g. "bootstrap charter activation" / "ratified administrative carve-out"),
- reason (free-text, required),
- previous-membership snapshot hash,
- resulting-membership snapshot hash,
- affected-member set,
- notification path (which subjects were notified, by which channel),
- challenge / review path,
- `is_administrative_shortcut: true` flag,
- cockpit rendering directive (`degraded-action`),
- member-shell rendering directive (`administrative-action-applied`),
- production-allowed flag (driven by deployment posture, see §5),
- post-pilot-allowed flag.

Membership mutation triggered by a ratified proposal does not emit this artifact; it emits the ordinary `EffectDispatchEvidence` chain.

**Anchors.** `icn/apps/governance/src/http/handlers.rs:548-635` (handlers), `:572, :617` (fail-open TrustThreshold), `:1245-1294` (resolver-aware delegation exclusion that already exists at close-time, as positive precedent).

### 4.3 Fail-closed resolver / checker policy in production

**Problem.** `MemberStandingChecker`, `SuspensionChecker`, and `MembershipResolver` are `Option<...>`; nothing in production startup currently asserts they are wired and healthy.

**Hardening goal.** Production runtime refuses to serve write paths whose institutional correctness depends on an unwired or unhealthy resolver. Test and fixture builds carve explicit, visibly labeled exceptions.

**Doctrine.** *Unresolved standing is not standing in production.*

**Draft design.** Startup invariants in §5 plus runtime state that maps to existing `SyncDegradedStatus` precedent in `icn/crates/icn-kernel-api/src/proofs.rs:1699-1739`. Where the institutional check cannot be resolved, the write path either:

- **fail-closed** — refuses to act and emits a `governance-degraded` artifact;
- **fail-degraded with banner** — performs the act but emits the act under `Accepted/AdministrativeShortcut` semantics and marks the surface;
- **fail-blocked** — refuses to act and routes the actor to the institutional path that does not require the missing resolver.

The choice per call site is design work in the follow-up PR; this strategy lists the menu and the invariants, not the per-site policy.

**Anchors.** `icn/apps/governance/src/http/configure.rs:197-264` (optional checker types), `handlers.rs:78, 572, 617, 1111, 1203` (fail-open sites), `handlers.rs:1267–1268` (existing inline comment acknowledging the trade-off).

### 4.4 Direct charter activation as bootstrap-only

**Problem.** `POST /charters` activates a charter outside the ratified proposal path, with synthetic provenance `direct-activation:{charter_id}`, no institutional-effect record, and no dispatch evidence.

**Hardening goal.** Direct activation is classified as bootstrap-only. Where retained, it emits a clearly distinct administrative artifact and is gated by a separate, narrow scope (or, preferably, a bootstrap mandate).

**Doctrine.** *A charter that bypasses governance must never look like one ratified by governance.*

**Draft design.** A candidate administrative artifact — **name draft, not minted** — `BootstrapCharterActivationReceipt` or the equivalent extension of an existing administrative receipt class, carrying:

- signer DID,
- capability used (e.g. `governance:charter:activate`),
- authority basis,
- activation mode (`bootstrap` / `administrative-override` / `replay-after-incident` / etc.),
- previous-charter hash (if any),
- new-charter hash,
- `activated_at`,
- scope (domain / federation / institution),
- `is_administrative_shortcut: true`,
- review / challenge path,
- production-allowed flag,
- post-pilot-allowed flag,
- cockpit and member-shell rendering directives.

Whether the artifact is a new receipt class or a discriminator on an existing one is an **open question** (§15).

**Anchors.** `handlers.rs:694-744` (handler), `:647, :725` (synthetic provenance string).

### 4.5 Closed lifecycle vocabulary for accepted-vs-applied

**Problem.** The current dispatch evidence layer produces `EmittedOnly`, `ExecutionEvidenced`, and `ExecutionFailed` (`dispatch_evidence.rs:131-169`), but surfaces (member shell, cockpit, API consumers) can flatten these to "Accepted" and lose the distinction.

**Hardening goal.** A closed lifecycle vocabulary is defined once and adopted consistently across API, member shell, and cockpit. "Accepted" never carries the implicit meaning "applied and reconciled."

**Doctrine.** *Accepted is not applied. Receipt exists is not complete.*

**Draft set, not minted.**

- `Draft`
- `Open`
- `Accepted/PendingEffect`
- `Accepted/EmittedOnly`
- `Accepted/EvidencePending`
- `Accepted/AppliedWithEvidence`
- `Accepted/PartialEffect`
- `Accepted/DispatchFailed`
- `Accepted/Challenged`
- `Accepted/Reversed`
- `Accepted/AdministrativeShortcut` (the §4.2/§4.4 case)
- `Rejected`
- `NoQuorum`
- `Degraded/UnresolvedStanding` (the §4.3 case)

Where this vocabulary lives — kernel-api proofs vs governance-app types — is an **open question** (§15).

**Anchors.** `icn/apps/governance/src/dispatch_evidence.rs:131-169` (existing reconciliation status).

### 4.6 Per-effect idempotency

**Problem.** Effect dispatch can partially apply, retries on restart, and currently keys idempotency at institutional-effect granularity. The stronger key `(decision_hash, manifest_hash, effect_index)` is named in the spec but not yet in code.

**Hardening goal.** Every effect carries a stable per-effect idempotency key. Subsystems contract for duplicate dispatch behavior: a duplicate dispatch returns the prior outcome, never mutates a second time, and emits a `Duplicate` evidence record rather than a fresh applied-evidence record.

**Doctrine.** *Retries are governance events when effects are not provably idempotent.*

**Draft design.** Candidate key `decision_hash + manifest_hash + effect_index`. Per-subsystem contract: idempotent / read-after-write idempotent / not idempotent (requires governance decision before retry). Evidence records distinguish first application from duplicate.

**Anchors.** `icn/apps/governance/src/dispatch_evidence.rs:34-87` (`EffectDispatchEvidence` struct, current granularity), [`../spec/effect-dispatch-contract.md`](../spec/effect-dispatch-contract.md) (spec calling for the stronger key).

### 4.7 Governance parameter sanity bounds

**Problem.** CCL governance parameter validation is semantic-only; pilot-readiness sanity bounds do not exist and there is no explicit fixture-mode escape hatch.

**Hardening goal.** Governance configs are classified into bands: `technically-valid`, `fixture-valid`, `pilot-valid`, `production-valid`. Member-facing institutions whose configs fail the band test render as `Degraded/InsufficientGovernanceConfig` and refuse member-facing operation until corrected.

**Doctrine.** *A technically valid institution can still be democratically fake.*

**Draft design.** Per-band bounds (numbers explicitly draft):

- minimum quorum > 0; pilot-valid ≥ a small positive fraction; production-valid bound set by the institution's CCL.
- minimum approval threshold > 0; pilot-valid ≥ a non-trivial fraction.
- minimum voting period > some pilot-band floor (must allow member notification + response).
- maximum voting period bounded to prevent indefinite "open" proposals.
- explicit challenge / review window required for high-impact effects.
- bootstrap-member threshold required for a domain to graduate from `bootstrap` band to `pilot-valid` band.
- notification requirement: at least one notification path bound for each band above `fixture-valid`.

Fixture / dev / test bands are explicit and labeled in every surface that touches them.

**Anchors.** `icn/crates/icn-ccl/src/governance.rs:146-163` (current quorum check).

### 4.8 Shell / cockpit fixture matrix (no greenwashing)

**Problem.** Implementation drift between member shell and cockpit can let degraded / partial / stale / disputed states render as clean.

**Hardening goal.** A fixture matrix asserts member-shell rendering against cockpit rendering for every degraded state, every administrative shortcut, every pending challenge, every emitted-only effect, every stale resolver answer. If cockpit shows degraded, member shell must show degraded — in member vocabulary.

**Doctrine.** *The UI must not launder uncertainty into confidence.*

**Draft matrix axes.**

- cockpit-degraded / shell-degraded parity,
- cockpit-evidence-pending / shell-evidence-pending parity,
- cockpit-resolver-down / shell-action-paused parity,
- direct-bootstrap-path / shell-shortcut-banner parity,
- partial-effect-applied / shell-partial-applied parity,
- challenge-window-open / shell-challenge-banner parity,
- private-evidence-exists / shell-body-bytes-absent (regression test),
- irreversible-action / shell-explicit-confirmation parity,
- signed-but-unacknowledged / shell-says-sent-not-confirmed parity.

**Anchors.** [`../spec/member-shell-v0.md`](../spec/member-shell-v0.md) (design principle 9 + failure-table row), [`../spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) (cockpit-degraded → shell-degraded rule).

### 4.9 PrivateEvidence non-rendering regression

**Problem.** Debug convenience can promote a `PrivateEvidence` body byte into a rendering layer.

**Hardening goal.** A regression test asserts that no rendering layer (member shell fixture, cockpit fixture) ever receives a private body. Rendering layers receive existence, scope, authority basis, access path, access-holder identity (where policy permits), access / export receipts — and nothing else.

**Doctrine.** *Privacy posture is not private content. Debug convenience cannot become surveillance authority.*

**Anchors.** `icn/crates/icn-kernel-api/src/proofs.rs:646-661` (`ArtifactDigest::scoped_vault_reference`, current digest-only boundary).

### 4.10 Typed-receipt atomicity inventory

**Problem.** Opaque receipt storage is atomic by construction; typed receipt paths are not uniformly atomic.

**Hardening goal.** Every typed receipt write path is classified and remediated. The classification matrix below is filled in by a follow-up PR; the doctrine here is that no typed write path is allowed to silently fall outside the atomicity guarantees of the opaque store without an explicit named exception.

**Classification axes** (one row per typed write entry point):

- atomic primary + index,
- non-atomic primary + index,
- write-once-by-hash,
- overwrite risk,
- index-skew risk,
- repair-scan availability,
- silent-absence-from-index treated as absence-from-truth.

**Doctrine.** *Index absence must not silently become record absence.*

**Anchors.** `icn/apps/governance/src/receipt_backend.rs:42, 80, 106, 146, 178-210, 248` (typed write entry points; default `put_mandate_with_grants` is documented as **not atomic** at lines 178-190).

### 4.11 Object-context binding on global-id reads

**Problem.** Endpoints shaped `/{context}/{collection}/{object_id}` authorize on the path / token context but fetch by a globally-keyed object id. If the loaded object's own context is not matched to the named one, a caller with standing in context A can read an object in context B by id (#2087 in treasury budgets; the class also covers ledger, compute, storage, governance records, federation provenance, and the escrow / invite / recurring objects of #2058).

**Hardening goal.** Every global-id read binds the loaded object to the institutional context named by the request before returning it, and reports a foreign object identically to a missing one (enumeration-safe, per #1642).

**Doctrine.** *A record reached by a globally-keyed id must be proven to belong to the institutional context named by the request before it is returned; a foreign object is reported exactly as a missing one.*

**Draft design.** Object-fetch checklist (the worked shape, from `get_budget`):

1. resolve the institutional context named by the request path (e.g. path coop → its treasury, yielding the context's DID);
2. load the object by its global id;
3. prove the object's **own** context binding against that context (e.g. `object.treasury_did == path_treasury.treasury_did`) — not merely the path / token check;
4. on mismatch, missing, or no-context, return one generic not-found shape carrying no id, owner, DID, or content (no enumeration oracle);
5. keep any observe-mode / divergence recording firing **before** the enforced denial, so cross-context attempts stay visible as divergence rather than vanishing;
6. this is the **object axis** and composes with — does not replace — the **subject axis** entity guard (RFC-0018 / ADR-0035, "may this caller act on this entity"). Both must hold.

This is not RBAC and not tenant isolation: it adds no roles and no per-tenant ACL, only a generic context-DID equality between a loaded object and the request's named context. The meaning firewall is untouched.

**Anchors.** `icn/crates/icn-gateway/src/api/treasury.rs:647-705` (`get_budget`, #2087 — the proven example); #1642 (open enumeration-safe-404 audit, the generalization); #2052 / #2056 / #2058 (caller-context siblings). Unbound families to sweep next: ledger, compute, storage, governance records, federation provenance.

<!-- truth: descriptive -->

## 5. Production invariants

The substrate refuses to serve the listed write paths unless the listed resolver / checker / config is wired and healthy. Fixture, dev, and test builds carve explicit exceptions; the exception is visibly labeled in every surface it touches.

- **TrustThreshold write paths require `MemberStandingChecker`.** No write to a `TrustThreshold` domain proceeds unless `ctx.member_checker` is `Some` and recent. Affected sites: `handlers.rs:78, 572, 617, 1111, 1203`.
- **Suspension-sensitive paths require `SuspensionChecker`.** No proposal close, vote tally, or delegation exclusion proceeds without it for domains whose CCL declares suspension-sensitivity. Affected site: `handlers.rs:1245-1294`.
- **TrustThreshold close-time delegation exclusion requires `MembershipResolver`.** The fail-open-on-error inline trade-off at `handlers.rs:1267-1268` is re-evaluated in production posture: production resolver errors are fail-blocked, not fail-open.
- **Steward authority paths require steward checker.** Pending the steward authority wiring; named here so the invariant is in place when the checker lands.
- **No domain transitions from `bootstrap` band to `pilot-valid` band without §4.7 sanity bounds passing.**
- **No charter activates via the direct path in production without the bootstrap shortcut artifact (§4.4) and the `production-allowed` flag set.**
- **No direct membership mutation in production without the bootstrap shortcut artifact (§4.2) and the `production-allowed` flag set.**

The production / fixture / dev classification is driven by an explicit deployment posture (env var, config field, or build feature) that is visible at startup. Surfaces render the posture; a "production posture but fixture wiring" combination is itself a degraded state.

<!-- truth: descriptive -->

## 6. Closed lifecycle vocabulary (draft)

The vocabulary is draft. The intent is one closed set adopted by the API surface, member shell, and steward cockpit so consumers do not invent their own collapses of "Accepted" or "Applied."

| State | Member-shell rendering | Cockpit rendering | Does NOT imply |
|---|---|---|---|
| `Draft` | "Being prepared" | "Draft" | governance has seen it |
| `Open` | "Open for input" | "Open" | a quorum exists |
| `Accepted/PendingEffect` | "Accepted, effect pending" | "Accepted, effect-plan emitted" | the effect has run |
| `Accepted/EmittedOnly` | "Accepted, no evidence yet" | "EmittedOnly" | the effect has applied |
| `Accepted/EvidencePending` | "Accepted, awaiting evidence" | "Evidence pending" | application succeeded |
| `Accepted/AppliedWithEvidence` | "Applied" | "Applied with evidence" | the act is final or non-reversible |
| `Accepted/PartialEffect` | "Partially applied" | "Partial effect" | the institution is in a clean state |
| `Accepted/DispatchFailed` | "Failed to apply" | "ExecutionFailed" | the proposal was rejected |
| `Accepted/Challenged` | "Challenged" | "Challenge window open" | the proposal is final |
| `Accepted/Reversed` | "Reversed" | "Reversed by subsequent decision" | the original act left no trace |
| `Accepted/AdministrativeShortcut` | "Administrative action applied" | "Administrative shortcut" | a ratified governance loop occurred |
| `Degraded/UnresolvedStanding` | "Action paused until standing resolves" | "Standing resolver degraded" | the actor has been judged ineligible |
| `Rejected` | "Rejected" | "Rejected" | the act cannot be re-proposed |
| `NoQuorum` | "No quorum" | "No quorum" | the institution is dysfunctional |

The hard rule remains: a receipt of any kind never means "legitimate and complete" by default. Receipts prove specific events under specific authority bases; the surface vocabulary states which event.

<!-- truth: descriptive -->

## 7. Authority shortcut policy

### 7.1 What is an authority shortcut

A surface is an authority shortcut if it produces an institutional effect without traversing the proposal → decision → mandate → effect → evidence chain. Current shortcuts in the substrate:

- `add_domain_member` / `remove_domain_member` (`handlers.rs:548-635`)
- `activate_charter` direct path (`handlers.rs:694-744`)
- The single broad `governance:write` capability (`auth.rs:947`) when used to exercise any of the above

Future shortcuts must be evaluated against this list and either added (with labeling) or rejected.

### 7.2 Labeling requirements

Every shortcut emits an administrative receipt (the artifact draft in §4.2 / §4.4). The receipt is rendered as `Accepted/AdministrativeShortcut` in surfaces (§6). API consumers receive a structured discriminator, not a free-text annotation; the surface is responsible for translating the discriminator to member-readable text.

### 7.3 Production allow-list

Each shortcut carries two flags in its administrative receipt: `production-allowed` and `post-pilot-allowed`. Deployment posture (§5) governs which flags are honored. A shortcut whose `production-allowed: false` flag is set refuses to execute in production deployments and surfaces an explicit "this path is bootstrap-only" error.

<!-- truth: descriptive -->

## 8. Resolver / checker fail-closed policy

Production semantics, per §3.3 abuse story:

| Resolver / checker | Production absence | Production unhealthy | Production stale |
|---|---|---|---|
| `MemberStandingChecker` | fail-closed (write paths refuse) | fail-degraded with banner | fail-degraded with banner |
| `SuspensionChecker` | fail-closed (suspension-sensitive paths refuse) | fail-blocked (route to non-suspension-sensitive path) | fail-degraded |
| `MembershipResolver` | fail-closed for `TrustThreshold` write + close | fail-blocked | fail-degraded |
| Steward authority checker | fail-closed (steward authority paths refuse) | fail-blocked | fail-degraded |

The current inline "fail-open on resolver errors" trade-off (`handlers.rs:1267-1268`) is a known production gap. The follow-up PR that lands per-site policy resolves it explicitly.

<!-- truth: descriptive -->

## 9. Receipt semantics policy

Per §2.1 doctrine, receipts state what they prove. Receipt rendering layers in member shell and cockpit must distinguish:

- **Application receipts** — the effect ran and applied.
- **Failure receipts** — the effect ran and did not apply.
- **Partial receipts** — the effect ran and applied to a known subset.
- **Emitted-only receipts** — the effect plan exists; no subsystem has yet reported back.
- **Duplicate receipts** — a retry produced a `Duplicate` outcome, not a fresh apply.
- **Administrative receipts** — an authority shortcut ran; this is the §4.2 / §4.4 artifact.
- **Privacy-preserving existence receipts** — a `PrivateEvidence` body exists; its scope, authority basis, and access path are renderable; its body is not.
- **Challenge receipts** — a challenge has been filed; the original decision is not yet final.
- **Reversal receipts** — a subsequent decision reversed an earlier one; the original receipts remain in the trail.

No surface flattens any of these into a single "Receipt issued" affirmation.

<!-- truth: descriptive -->

## 10. Governance parameter sanity strategy

Per §4.7. The follow-up PR fills in the per-band bounds; this section names the bands and the gate.

- `fixture-valid` — anything that parses. Explicitly labeled in every surface.
- `pilot-valid` — meets the minimum sanity bounds; safe for partner-bound rehearsal.
- `production-valid` — meets pilot-valid plus the post-pilot tightening the institution declared in its CCL.

A domain whose config is below its declared band refuses member-facing operation and renders as `Degraded/InsufficientGovernanceConfig` in both surfaces.

<!-- truth: descriptive -->

## 11. Shell / cockpit fixture matrix

Per §4.8. Implementation lives in the testkit; the matrix here is the design contract.

For every state listed in §6 and every shortcut listed in §7, a fixture asserts:

- the cockpit rendering name,
- the member-shell rendering name (member vocabulary),
- the receipt class actually emitted,
- the API discriminator returned,
- the parity invariant (if cockpit is degraded / shortcut / partial / pending / challenged, the shell is correspondingly so).

The matrix runs as part of the testkit. A v0 violation (cockpit and shell disagree) fails the build.

<!-- truth: descriptive -->

## 12. Privacy / ScopedVault abuse prevention

Per §4.9 and §2.9 doctrine. The regression test set covers:

- no `PrivateEvidence` body byte reaches any rendering layer,
- no "debug preview" path in any surface dereferences a private body,
- no API response includes a private body unless the caller holds the corresponding access mandate,
- access / export receipts are themselves first-class records and render as such.

The existing digest-only rendering in `icn/crates/icn-kernel-api/src/proofs.rs:646-661` is the positive-precedent anchor.

<!-- truth: descriptive -->

## 13. Typed-receipt atomicity inventory

Per §4.10. The follow-up PR fills in the matrix; this section lists the entry points to inventory.

Entry points (`icn/apps/governance/src/receipt_backend.rs`):

- `put_governance` (l. 42) — `GovernanceDecisionReceipt`.
- `put_institutional_effect` (l. 80) — `InstitutionalEffectRecord`; default no-op (line 80 onward).
- `put_effect_dispatch_evidence` (l. 106) — `EffectDispatchEvidence`; default no-op.
- `put_mandate` (l. 146) — `Mandate`.
- `put_authority_grant` (l. 248) — `AuthorityGrant`.
- `put_mandate_with_grants` (l. 195) — composite; **default is NOT atomic** per the comment at lines 178-190; sled-backed gateway must override.

For each: classify, document the gap (or lack of gap), and either migrate to opaque-store-grade atomicity, add a repair scan, or accept and document the weaker guarantee with an explicit reason.

<!-- truth: descriptive -->

## 14. Implementation issue roster

Candidate follow-up issues, grouped by priority. **Names are descriptive; no issue numbers are minted by this document.** Each row names the track section, a code anchor, and a candidate label set (existing labels in the repo include `epic:trust-hardening`, `tier:1-correctness`, etc., per `.github/ISSUE_POLICY.md`).

### P0 — production invariants and authority shortcuts

| Candidate title | Track | Anchor |
|---|---|---|
| `feat(rpc): split governance:write into per-act scopes (or mandate gating)` | §4.1 | `icn/crates/icn-rpc/src/auth.rs:947` |
| `feat(governance): emit BootstrapMembershipMutationReceipt on direct add/remove` | §4.2 | `icn/apps/governance/src/http/handlers.rs:548-635` |
| `feat(governance): emit BootstrapCharterActivationReceipt on /charters POST` | §4.4 | `handlers.rs:694-744` |
| `feat(runtime): production startup guard for optional checkers / resolvers` | §4.3, §5 | `icn/apps/governance/src/http/configure.rs:197-264` |

### P1 — evidence and execution correctness

| Candidate title | Track | Anchor |
|---|---|---|
| `feat(governance): per-effect idempotency key (decision_hash, manifest_hash, effect_index)` | §4.6 | `icn/apps/governance/src/dispatch_evidence.rs:34-87` |
| `feat(governance): closed lifecycle vocabulary across API / shell / cockpit` | §4.5, §6 | `dispatch_evidence.rs:131-169` |
| `feat(governance): emitted_only / partial / failed API surface treatment` | §4.5, §9 | `dispatch_evidence.rs:131-169` |
| `feat(governance): resolver-degraded governance lifecycle (Degraded/UnresolvedStanding)` | §4.3, §6, §8 | `handlers.rs:1267-1268` |

### P2 — surface regression

| Candidate title | Track | Anchor |
|---|---|---|
| `test(testkit): member shell / cockpit parity fixture matrix` | §4.8, §11 | `docs/spec/member-shell-v0.md`, `docs/spec/steward-cockpit-v0.md` |
| `test(testkit): no-greenwashing degraded-state assertions` | §4.8, §11 | same |
| `test(testkit): PrivateEvidence non-rendering regression` | §4.9, §12 | `icn/crates/icn-kernel-api/src/proofs.rs:646-661` |
| `test(testkit): stale / degraded / offline rendering parity` | §4.8, §11 | same |

### P3 — governance sanity and receipt-store consistency

| Candidate title | Track | Anchor |
|---|---|---|
| `feat(ccl): pilot-ready governance profile bands and gate` | §4.7, §10 | `icn/crates/icn-ccl/src/governance.rs:146-163` |
| `chore(receipts): typed receipt write-path atomicity inventory` | §4.10, §13 | `icn/apps/governance/src/receipt_backend.rs:42-248` |
| `feat(receipts): repair scans for index skew on typed paths` | §4.10, §13 | same |
| `refactor(receipts): migrate typed paths toward opaque-store-grade atomicity` | §4.10, §13 | same |

<!-- truth: descriptive -->

## 15. Open questions

The questions below are genuinely open; the strategy doc is descriptive of what production must look like, not prescriptive of how to encode the answer.

- **Scope decomposition vs mandate gating.** Where does the line fall between kernel-side capability strings and ICN-native mandate / authority-grant gating? The doctrine in §4.1 keeps both available; the follow-up design PR picks per call site.
- **Administrative receipt class.** Is `BootstrapCharterActivationReceipt` a new receipt class, or a discriminator on an existing administrative receipt type? The decision touches §4.2 (membership) as well as §4.4 (charters).
- **Closed lifecycle vocabulary owner.** Does the vocabulary in §6 live in `icn-kernel-api` proofs (kernel-side, with apps emitting these states) or in the governance app (app-side, kernel sees only `Accepted` / `Rejected`)? The kernel/app boundary in [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) argues both ways depending on whether the state is a generic constraint or a domain semantic.
- **Degraded-state record class.** Does resolver-degraded governance use the existing `SyncDegradedStatus` precedent (`icn/crates/icn-kernel-api/src/proofs.rs:1699-1739`) or introduce a separate `GovernanceDegradedStatus`?
- **Direct-mutation lifetime.** After pilot formalization, does direct membership mutation remain as a labeled bootstrap path indefinitely, or is it removed entirely? The doctrine in §4.2 supports either; the institution that adopts ICN decides.

<!-- truth: descriptive -->

## 16. Recommended sequencing

Implementation order, designed so the production-safety floor is in place before the surface-regression and governance-sanity layers land on top.

### Stage A — documentation / control plane (this PR)

1. Strategy document (this file).
2. Abuse-case review checklist embedded in the strategy doc (sections §3, §4).
3. Closed lifecycle vocabulary (§6, draft).
4. Production invariants (§5).
5. Authority shortcut policy (§7).

### Stage B — safety rails

1. Production config guard for optional checker / resolver wiring (§4.3, §5).
2. Scope split design or mandate-gating design (§4.1) — design, then PR.
3. Direct charter activation marked bootstrap-only (§4.4).
4. Direct membership mutation marked bootstrap-only (§4.2).
5. Fixture / dev / test exceptions made explicit and visibly labeled (§5).

### Stage C — evidence correctness

1. Per-effect idempotency key (§4.6).
2. Dispatch lifecycle status — closed vocabulary adoption (§4.5, §6).
3. Emitted-only / partial / failed rendering requirements wired through the API (§9).
4. Typed receipt atomicity inventory (§4.10, §13).
5. Repair-scan strategy where needed.

### Stage D — surface regression

1. Shell / cockpit fixture matrix (§4.8, §11).
2. No-greenwashing degraded-state tests.
3. PrivateEvidence non-rendering tests (§4.9, §12).
4. Stale / degraded / offline parity tests.
5. Bootstrap / direct-path labeling tests (§7.2).

### Stage E — governance sanity

1. Pilot / member-facing governance profile gate (§4.7, §10).
2. Challenge / review window requirement for high-impact effects.
3. Minimum quorum / approval / voting-period sanity bands.
4. Bootstrap-member threshold.
5. Notification / accessibility requirements (cross-link to `ARCHITECTURE_DUE_DILIGENCE.md` §3.B).

<!-- truth: descriptive -->

## 17. Non-claims preserved

This document does not imply or claim:

- production deployment, Phase 2 completion, or formal pilot status for any partner,
- live federation, live K3s / DNS / Forgejo mutation,
- live NYCN partner-data handling,
- new contract URN minting,
- new ADR or RFC creation,
- new runtime behavior, new schema fields, new wire formats,
- security-checklist coverage that overlaps `production-hardening.md` (transport, key management, replay defense),
- RBAC redesign in isolation from ICN's mandate / authority-grant model.

NYCN remains the **intended first cooperative partner**, not a committed formal pilot.

<!-- truth: descriptive -->

## 18. See also

<!-- truth: descriptive -->

- [`ARCHITECTURE_DUE_DILIGENCE.md`](ARCHITECTURE_DUE_DILIGENCE.md) — twin upstream doctrines (convenience-vs-authority, participation-access); this document is the institutional-legitimacy-layer companion.
- [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) — meaning firewall; this document does not widen it.
- [`../spec/effect-dispatch-contract.md`](../spec/effect-dispatch-contract.md) — five-stage chain referenced by §4.5 and §4.6.
- [`../spec/member-shell-v0.md`](../spec/member-shell-v0.md) — member rendering vocabulary referenced by §6, §8, §11.
- [`../spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) — cockpit rendering and the cockpit-degraded → shell-degraded rule referenced by §4.8 and §11.
- [`../security/production-hardening.md`](../security/production-hardening.md) — transport / crypto / replay layer; complementary, not overlapping.
- [`../STATE.md`](../STATE.md) — living state; the sync block records this session's landing.
- [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) — Phase 2 status remains ⏳ and partner-bound.
- [`.github/ISSUE_POLICY.md`](../../.github/ISSUE_POLICY.md) — issue label taxonomy; the §14 roster uses these labels descriptively, not as a filing instruction.
